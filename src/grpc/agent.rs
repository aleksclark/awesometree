//! gRPC AgentService implementation.

use crate::agent_supervisor;
use crate::auth;
use crate::grpc::arp_proto::*;
use crate::grpc::arp_proto::agent_service_server::AgentService;
use crate::grpc::convert;
use crate::grpc::extract_token;
use crate::state;
use tonic::{Request, Response, Status};

/// Implements the `AgentService` gRPC trait.
#[derive(Debug, Default)]
pub struct AgentServiceImpl;

#[tonic::async_trait]
impl AgentService for AgentServiceImpl {
    async fn spawn_agent(
        &self,
        request: Request<SpawnAgentRequest>,
    ) -> Result<Response<AgentInstance>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        // Session-scoped tokens can spawn agents within scoped projects
        if !auth::permission_allows(&token.permission, &auth::Permission::Session) {
            return Err(Status::permission_denied("insufficient permission"));
        }

        if req.workspace.is_empty() {
            return Err(Status::invalid_argument("workspace is required"));
        }
        if req.template.is_empty() {
            return Err(Status::invalid_argument("template is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let ws = store.workspace(&req.workspace).ok_or_else(|| {
            Status::not_found(format!("workspace '{}' not found", req.workspace))
        })?;

        // Check scope includes this workspace's project
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied(format!(
                "token scope does not include project '{}'",
                ws.project
            )));
        }

        let port = store.allocate_agent_port().ok_or_else(|| {
            Status::resource_exhausted("no available ports for agent")
        })?;

        let name = if req.name.is_empty() {
            req.template.clone()
        } else {
            req.name.clone()
        };

        // Create child token for the agent
        let child_token = auth::create_child_token(&token, None, None)
            .map_err(|e| Status::internal(format!("failed to create child token: {e}")))?;
        let mut child_token_with_session = child_token;
        let session_id = auth::ensure_session(&mut child_token_with_session);
        let token_str = auth::encode_scoped_token(&child_token_with_session);

        let mut env = req.env;
        env.insert("ARP_TOKEN".to_string(), token_str);

        let command = format!("{} serve", req.template);
        let opts = agent_supervisor::SpawnOptions {
            workspace: req.workspace.clone(),
            dir: ws.dir.clone(),
            template: req.template.clone(),
            name: name.clone(),
            port,
            command,
            env,
        };

        let result = agent_supervisor::spawn_agent(opts).ok_or_else(|| {
            Status::internal("agent supervisor not initialized")
        })?;

        let agent = state::AgentInstanceState {
            id: result.agent_id.clone(),
            template: req.template,
            name,
            workspace: req.workspace.clone(),
            status: state::AgentStatus::Starting,
            port: result.port,
            host: None,
            pid: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            token_id: Some(child_token_with_session.id.clone()),
            session_id: Some(session_id),
            spawned_by: Some(token.id.clone()),
        };

        let proto_agent = convert::agent_instance_to_proto(&agent);

        state::modify(|st| {
            st.add_agent(&req.workspace, agent);
        })
        .map_err(|e| Status::internal(format!("failed to persist agent: {e}")))?;

        Ok(Response::new(proto_agent))
    }

    async fn list_agents(
        &self,
        request: Request<ListAgentsRequest>,
    ) -> Result<Response<ListAgentsResponse>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let mut agents: Vec<AgentInstance> = Vec::new();

        for (ws_name, ws) in &store.workspaces {
            // Filter by project scope
            if !auth::scope_includes_project(&token.scope, &ws.project) {
                continue;
            }

            // Filter by workspace
            if !req.workspace.is_empty() && ws_name.as_str() != req.workspace {
                continue;
            }

            for agent in &ws.agents {
                // For session-scoped tokens, only show own-session agents
                if token.permission == auth::Permission::Session
                    && !auth::session_matches(&token, agent)
                {
                    continue;
                }

                // Filter by status
                if req.status != AgentStatus::Unspecified as i32 {
                    let agent_status = convert::agent_status_to_proto(&agent.status);
                    if agent_status != req.status {
                        continue;
                    }
                }

                // Filter by template
                if !req.template.is_empty() && agent.template != req.template {
                    continue;
                }

                agents.push(convert::agent_instance_to_proto(agent));
            }
        }

        Ok(Response::new(ListAgentsResponse { agents }))
    }

    async fn get_agent_status(
        &self,
        request: Request<GetAgentStatusRequest>,
    ) -> Result<Response<AgentInstance>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let (_ws_name, ws_state) = store
            .find_agent(&req.agent_id)
            .map(|(ws_name, agent)| {
                let ws = store.workspace(ws_name).unwrap();
                (ws_name, (ws, agent))
            })
            .ok_or_else(|| {
                Status::not_found(format!("agent '{}' not found", req.agent_id))
            })?;

        let (ws, agent) = ws_state;

        // Check scope
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied("token scope mismatch"));
        }

        // Session check
        if token.permission == auth::Permission::Session
            && !auth::session_matches(&token, agent)
        {
            return Err(Status::permission_denied("session mismatch"));
        }

        Ok(Response::new(convert::agent_instance_to_proto(agent)))
    }

    async fn send_agent_message(
        &self,
        request: Request<SendAgentMessageRequest>,
    ) -> Result<Response<SendAgentMessageResponse>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }
        if req.message.is_empty() {
            return Err(Status::invalid_argument("message is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let (ws_name, agent) = store
            .resolve_agent_flexible(&req.agent_id)
            .ok_or_else(|| {
                Status::not_found(format!("agent '{}' not found", req.agent_id))
            })?;

        let ws = store.workspace(ws_name).unwrap();

        // Check scope
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied("token scope mismatch"));
        }

        // Session check
        if token.permission == auth::Permission::Session
            && !auth::session_matches(&token, agent)
        {
            return Err(Status::permission_denied("session mismatch"));
        }

        // Agent readiness check — must be Ready or Busy to accept messages
        if agent.status != state::AgentStatus::Ready && agent.status != state::AgentStatus::Busy {
            return Err(Status::failed_precondition(format!(
                "agent '{}' is not ready (status: {})",
                req.agent_id, agent.status
            )));
        }

        // NOTE: The `blocking` field (proto3 bool, default false) conflicts with the spec
        // default of true. Since we cannot distinguish "not set" from "set to false" in
        // proto3, we always block (current behavior matches spec default). A non-blocking
        // fire-and-forget path can be added when the proto is updated to `optional bool`.

        let base_url = agent.base_url();

        // Build A2A SendMessage JSON-RPC request
        let context_id = if req.context_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            req.context_id.clone()
        };

        let a2a_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": uuid::Uuid::new_v4().to_string(),
            "method": "message/send",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{"kind": "text", "text": req.message}],
                    "contextId": context_id
                }
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{base_url}/"))
            .json(&a2a_body)
            .send()
            .await
            .map_err(|e| Status::unavailable(format!("failed to reach agent: {e}")))?;

        if !resp.status().is_success() {
            let status_code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Status::internal(format!(
                "agent returned {status_code}: {body}"
            )));
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Status::internal(format!("failed to parse agent response: {e}")))?;

        // Extract "result" from JSON-RPC response
        let result_val = resp_json.get("result").unwrap_or(&resp_json);

        // Determine if it's a task or message based on presence of "id" field (tasks have ids)
        let prost_struct = convert::json_to_prost_struct(result_val)
            .unwrap_or_default();

        let result = if result_val.get("id").is_some() {
            // Looks like a task
            Some(send_agent_message_response::Result::Task(prost_struct))
        } else {
            // Treat as a message
            Some(send_agent_message_response::Result::Message(prost_struct))
        };

        Ok(Response::new(SendAgentMessageResponse { result }))
    }

    async fn create_agent_task(
        &self,
        request: Request<CreateAgentTaskRequest>,
    ) -> Result<Response<prost_types::Struct>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }
        if req.message.is_empty() {
            return Err(Status::invalid_argument("message is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let (ws_name, agent) = store
            .resolve_agent_flexible(&req.agent_id)
            .ok_or_else(|| {
                Status::not_found(format!("agent '{}' not found", req.agent_id))
            })?;

        let ws = store.workspace(ws_name).unwrap();

        // Check scope
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied("token scope mismatch"));
        }

        if token.permission == auth::Permission::Session
            && !auth::session_matches(&token, agent)
        {
            return Err(Status::permission_denied("session mismatch"));
        }

        // Agent readiness check — must be Ready or Busy to accept tasks
        if agent.status != state::AgentStatus::Ready && agent.status != state::AgentStatus::Busy {
            return Err(Status::failed_precondition(format!(
                "agent '{}' is not ready (status: {})",
                req.agent_id, agent.status
            )));
        }

        let base_url = agent.base_url();

        let context_id = if req.context_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            req.context_id.clone()
        };

        // A2A SendMessage — same as above, but we always return a task
        let a2a_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": uuid::Uuid::new_v4().to_string(),
            "method": "message/send",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{"kind": "text", "text": req.message}],
                    "contextId": context_id
                }
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{base_url}/"))
            .json(&a2a_body)
            .send()
            .await
            .map_err(|e| Status::unavailable(format!("failed to reach agent: {e}")))?;

        if !resp.status().is_success() {
            let status_code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Status::internal(format!(
                "agent returned {status_code}: {body}"
            )));
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Status::internal(format!("failed to parse agent response: {e}")))?;

        let result_val = resp_json.get("result").unwrap_or(&resp_json);

        // If the result already has an "id", it's a task. Otherwise, wrap it.
        let task_json = if result_val.get("id").is_some() {
            result_val.clone()
        } else {
            // Wrap in a synthetic completed task
            serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "contextId": context_id,
                "status": {"state": "completed"},
                "result": result_val
            })
        };

        let prost_struct = convert::json_to_prost_struct(&task_json)
            .unwrap_or_default();

        Ok(Response::new(prost_struct))
    }

    async fn get_agent_task_status(
        &self,
        request: Request<GetAgentTaskStatusRequest>,
    ) -> Result<Response<prost_types::Struct>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let (ws_name, agent) = store
            .resolve_agent_flexible(&req.agent_id)
            .ok_or_else(|| {
                Status::not_found(format!("agent '{}' not found", req.agent_id))
            })?;

        let ws = store.workspace(ws_name).unwrap();

        // Check scope
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied("token scope mismatch"));
        }

        if token.permission == auth::Permission::Session
            && !auth::session_matches(&token, agent)
        {
            return Err(Status::permission_denied("session mismatch"));
        }

        let base_url = agent.base_url();

        // A2A tasks/get JSON-RPC request
        let a2a_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": uuid::Uuid::new_v4().to_string(),
            "method": "tasks/get",
            "params": {
                "id": req.task_id,
                "historyLength": req.history_length
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{base_url}/"))
            .json(&a2a_body)
            .send()
            .await
            .map_err(|e| Status::unavailable(format!("failed to reach agent: {e}")))?;

        if !resp.status().is_success() {
            let status_code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Status::internal(format!(
                "agent returned {status_code}: {body}"
            )));
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Status::internal(format!("failed to parse agent response: {e}")))?;

        let result_val = resp_json.get("result").unwrap_or(&resp_json);
        let prost_struct = convert::json_to_prost_struct(result_val)
            .unwrap_or_default();

        Ok(Response::new(prost_struct))
    }

    async fn stop_agent(
        &self,
        request: Request<StopAgentRequest>,
    ) -> Result<Response<AgentInstance>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let (ws_name, agent) = store
            .find_agent(&req.agent_id)
            .ok_or_else(|| {
                Status::not_found(format!("agent '{}' not found", req.agent_id))
            })?;

        let ws = store.workspace(ws_name).unwrap();

        // Check scope
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied("token scope mismatch"));
        }

        // Session-scoped tokens can only stop own-session agents
        if token.permission == auth::Permission::Session && !auth::session_matches(&token, agent) {
            return Err(Status::permission_denied("session-scoped token can only stop own-session agents"));
        }

        // TODO: Cancel working A2A tasks before stopping. Task tracking is not yet in
        // AgentInstanceState, so we skip best-effort cancel for now.

        if req.grace_period_ms > 0 {
            crate::log::log(format!(
                "StopAgent: grace_period_ms={} requested (using supervisor default)",
                req.grace_period_ms
            ));
        }
        // TODO: Pass grace_period_ms through to supervisor. Currently the supervisor
        // uses a hardcoded 5-second grace period in its graceful_stop task.

        // Stop via supervisor
        agent_supervisor::stop_agent(&req.agent_id);

        // Update state to Stopping
        let mut updated_agent = agent.clone();
        updated_agent.status = state::AgentStatus::Stopping;

        state::modify(|st| {
            st.update_agent_status(&req.agent_id, state::AgentStatus::Stopping);
        })
        .map_err(|e| Status::internal(format!("failed to update state: {e}")))?;

        Ok(Response::new(convert::agent_instance_to_proto(&updated_agent)))
    }

    async fn restart_agent(
        &self,
        request: Request<RestartAgentRequest>,
    ) -> Result<Response<AgentInstance>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let (ws_name, agent) = store
            .find_agent(&req.agent_id)
            .ok_or_else(|| {
                Status::not_found(format!("agent '{}' not found", req.agent_id))
            })?;

        let ws = store.workspace(ws_name).unwrap();

        // Check scope
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied("token scope mismatch"));
        }

        // Session-scoped tokens can only restart own-session agents
        if token.permission == auth::Permission::Session && !auth::session_matches(&token, agent) {
            return Err(Status::permission_denied("session-scoped token can only restart own-session agents"));
        }

        // Capture the agent config and identity before stopping
        let template = agent.template.clone();
        let name = agent.name.clone();
        let workspace = agent.workspace.clone();
        let ws_dir = ws.dir.clone();
        let old_token_id = agent.token_id.clone();
        let old_session_id = agent.session_id.clone();
        let old_spawned_by = agent.spawned_by.clone();

        // Reconstruct env with ARP_TOKEN from the old agent's token
        let mut old_env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        if let Some(ref tid) = old_token_id {
            if let Some(old_token) = auth::get_token_by_id(tid) {
                old_env.insert("ARP_TOKEN".to_string(), auth::encode_scoped_token(&old_token));
            }
        }

        // Stop the old agent
        agent_supervisor::stop_agent(&req.agent_id);
        state::modify(|st| {
            st.remove_agent(&req.agent_id);
        })
        .map_err(|e| Status::internal(format!("failed to remove old agent: {e}")))?;

        // Allocate new port and re-spawn
        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let port = store.allocate_agent_port().ok_or_else(|| {
            Status::resource_exhausted("no available ports for agent")
        })?;

        let command = format!("{template} serve");
        let opts = agent_supervisor::SpawnOptions {
            workspace: workspace.clone(),
            dir: ws_dir,
            template: template.clone(),
            name: name.clone(),
            port,
            command,
            env: old_env,
        };

        let result = agent_supervisor::spawn_agent(opts).ok_or_else(|| {
            Status::internal("agent supervisor not initialized")
        })?;

        let new_agent = state::AgentInstanceState {
            id: result.agent_id.clone(),
            template,
            name,
            workspace: workspace.clone(),
            status: state::AgentStatus::Starting,
            port: result.port,
            host: None,
            pid: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            token_id: old_token_id,
            session_id: old_session_id,
            spawned_by: old_spawned_by,
        };

        let proto_agent = convert::agent_instance_to_proto(&new_agent);

        state::modify(|st| {
            st.add_agent(&workspace, new_agent);
        })
        .map_err(|e| Status::internal(format!("failed to persist new agent: {e}")))?;

        Ok(Response::new(proto_agent))
    }
}
