use crate::agent_supervisor;
use crate::auth::{
    Permission, TokenScope,
    create_child_token, create_scoped_token, encode_scoped_token,
    ensure_session, localhost_admin_token,
};
use crate::mcp::ArpServer;
use crate::state::{self, AgentInstanceState, AgentStatus};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::tool;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, JsonSchema)]
pub struct AgentSpawnParams {
    pub workspace: String,
    pub template: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub prompt: Option<String>,
    /// Narrow the spawned agent's project scope.
    /// Use "*" for global or an array of project names.
    /// Must be subset of caller's scope. Omit to inherit caller's full scope.
    #[serde(default)]
    pub scope: Option<serde_json::Value>,
    /// Permission level for spawned agent ("session", "project", or "admin").
    /// Must be <= caller's permission. Omit to inherit caller's permission.
    #[serde(default)]
    pub permission: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AgentListParams {
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AgentStatusParams {
    pub agent_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct AgentMessageParams {
    pub agent_id: String,
    pub message: String,
    #[serde(default)]
    pub context_id: Option<String>,
    #[serde(default)]
    pub blocking: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AgentTaskParams {
    pub agent_id: String,
    pub message: String,
    #[serde(default)]
    pub context_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AgentTaskStatusParams {
    pub agent_id: String,
    pub task_id: String,
    #[serde(default)]
    pub history_length: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AgentStopParams {
    pub agent_id: String,
    #[serde(default)]
    pub grace_period_ms: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AgentRestartParams {
    pub agent_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct TokenCreateParams {
    /// Who this token represents
    pub subject: String,
    /// Project scope: "*" for global, or array of project names
    pub scope: serde_json::Value,
    /// Permission level: "session", "project", or "admin"
    pub permission: String,
    /// Token TTL in seconds (optional, omit for non-expiring)
    #[serde(default)]
    pub expires_in_seconds: Option<u64>,
}

fn agent_to_json(ws_name: &str, agent: &AgentInstanceState, project: &str) -> serde_json::Value {
    let mut json = serde_json::json!({
        "id": agent.id,
        "template": agent.template,
        "name": agent.name,
        "workspace": ws_name,
        "project": project,
        "status": agent.status.to_string(),
        "port": agent.port,
        "direct_url": agent.base_url(),
        "proxy_url": format!("http://localhost:9099/a2a/agents/{}", agent.id),
        "pid": agent.pid,
        "started_at": agent.started_at,
    });
    if let Some(ref tid) = agent.token_id {
        json["token_id"] = serde_json::Value::String(tid.clone());
    }
    if let Some(ref sid) = agent.session_id {
        json["session_id"] = serde_json::Value::String(sid.clone());
    }
    if let Some(ref sb) = agent.spawned_by {
        json["spawned_by"] = serde_json::Value::String(sb.clone());
    }
    json
}

/// Parse a scope from a JSON value ("*" or array of strings).
fn parse_scope(value: &serde_json::Value) -> Result<TokenScope, String> {
    match value {
        serde_json::Value::String(s) if s == "*" => Ok(TokenScope::Global),
        serde_json::Value::Array(arr) => {
            let projects: Result<Vec<String>, _> = arr
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(|s| s.to_string())
                        .ok_or_else(|| "scope array must contain strings".to_string())
                })
                .collect();
            Ok(TokenScope::Projects(projects?))
        }
        _ => Err("scope must be \"*\" or an array of project names".to_string()),
    }
}

/// Parse a permission string.
fn parse_permission(s: &str) -> Result<Permission, String> {
    match s {
        "session" => Ok(Permission::Session),
        "project" => Ok(Permission::Project),
        "admin" => Ok(Permission::Admin),
        _ => Err(format!("invalid permission: {s} (must be session|project|admin)")),
    }
}

#[rmcp::tool_router(router = tool_router_agent, vis = "pub")]
impl ArpServer {
    #[tool(name = "agent/spawn", description = "Spawn a new A2A agent in an existing workspace. Each agent gets its own port, context_id space, and AgentCard.")]
    pub fn agent_spawn(
        &self,
        Parameters(params): Parameters<AgentSpawnParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let ws = st.workspace(&params.workspace).ok_or_else(|| {
            ErrorData::invalid_params(format!("workspace not found: {}", params.workspace), None)
        })?;
        if !ws.active {
            return Err(ErrorData::invalid_params(
                format!("workspace not active: {}", params.workspace),
                None,
            ));
        }
        let project = ws.project.clone();
        let name = params.name.unwrap_or_else(|| params.template.clone());

        let port = st.allocate_agent_port().ok_or_else(|| {
            ErrorData::internal_error("no ports available", None)
        })?;

        // --- Scoped token integration ---
        // Use a default admin/* parent token (since we don't have MCP-level auth context yet)
        let mut parent_token = localhost_admin_token();

        // Parse optional scope narrowing
        let child_scope = if let Some(ref scope_val) = params.scope {
            Some(parse_scope(scope_val).map_err(|e| ErrorData::invalid_params(e, None))?)
        } else {
            None
        };

        // Parse optional permission narrowing
        let child_perm = if let Some(ref perm_str) = params.permission {
            Some(parse_permission(perm_str).map_err(|e| ErrorData::invalid_params(e, None))?)
        } else {
            None
        };

        // Ensure parent has a session
        let session_id = ensure_session(&mut parent_token);

        // Create child token
        let child_token = create_child_token(&parent_token, child_scope, child_perm)
            .map_err(|e| ErrorData::invalid_params(e, None))?;

        // Encode the child token for injection
        let mut child_token_with_session = child_token;
        child_token_with_session.session_id = Some(session_id.clone());
        let token_str = encode_scoped_token(&child_token_with_session);

        let mut env = params.env.unwrap_or_default();
        env.insert("ARP_TOKEN".to_string(), token_str);

        let command = format!("{} serve", params.template);

        let opts = agent_supervisor::SpawnOptions {
            workspace: params.workspace.clone(),
            dir: ws.dir.clone(),
            template: params.template.clone(),
            name: name.clone(),
            port,
            command,
            env,
        };

        let result = agent_supervisor::spawn_agent(opts).ok_or_else(|| {
            ErrorData::internal_error("agent supervisor not initialized", None)
        })?;

        let agent = AgentInstanceState {
            id: result.agent_id.clone(),
            template: params.template,
            name,
            workspace: params.workspace.clone(),
            status: AgentStatus::Starting,
            port: result.port,
            host: None,
            pid: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            token_id: Some(child_token_with_session.id.clone()),
            session_id: Some(session_id),
            spawned_by: Some(parent_token.id.clone()),
        };

        state::modify(|st| {
            st.add_agent(&params.workspace, agent.clone());
        })
        .map_err(|e| ErrorData::internal_error(e, None))?;

        let json = agent_to_json(&params.workspace, &agent, &project);
        let out = serde_json::to_string_pretty(&json)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }

    #[tool(name = "agent/list", description = "List agent instances across all workspaces or filtered by workspace/status.")]
    pub fn agent_list(
        &self,
        Parameters(params): Parameters<AgentListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let mut results: Vec<serde_json::Value> = Vec::new();

        for (ws_name, ws) in &st.workspaces {
            if let Some(ref filter_ws) = params.workspace {
                if ws_name != filter_ws {
                    continue;
                }
            }
            for agent in &ws.agents {
                if let Some(ref filter_status) = params.status {
                    if agent.status.to_string() != *filter_status {
                        continue;
                    }
                }
                if let Some(ref filter_template) = params.template {
                    if &agent.template != filter_template {
                        continue;
                    }
                }
                results.push(agent_to_json(ws_name, agent, &ws.project));
            }
        }
        results.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(name = "agent/status", description = "Get full status of an agent instance including health, resolved AgentCard, both access URLs, and resource usage.")]
    pub fn agent_status(
        &self,
        Parameters(params): Parameters<AgentStatusParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let (ws_name, agent) = st.find_agent(&params.agent_id).ok_or_else(|| {
            ErrorData::invalid_params(format!("agent not found: {}", params.agent_id), None)
        })?;
        let ws = st.workspace(ws_name).unwrap();
        let json = agent_to_json(ws_name, agent, &ws.project);
        let out = serde_json::to_string_pretty(&json)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }

    #[tool(name = "agent/message", description = "Send an A2A SendMessage to an agent (proxied through ARP). For long-running tasks, use agent/task instead.")]
    pub async fn agent_message(
        &self,
        Parameters(params): Parameters<AgentMessageParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let (_, agent) = st.find_agent(&params.agent_id).ok_or_else(|| {
            ErrorData::invalid_params(format!("agent not found: {}", params.agent_id), None)
        })?;
        let base_url = agent.base_url();

        let a2a_body = serde_json::json!({
            "message": {
                "role": "ROLE_USER",
                "parts": [{ "text_part": { "text": params.message } }],
                "context_id": params.context_id,
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{base_url}/message:send"))
            .json(&a2a_body)
            .send()
            .await
            .map_err(|e| ErrorData::internal_error(format!("A2A request failed: {e}"), None))?;

        let body = resp
            .text()
            .await
            .map_err(|e| ErrorData::internal_error(format!("A2A read body: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(body)]))
    }

    #[tool(name = "agent/task", description = "Send a message to an agent via A2A SendMessage and return the Task for async tracking.")]
    pub async fn agent_task(
        &self,
        Parameters(params): Parameters<AgentTaskParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let (_, agent) = st.find_agent(&params.agent_id).ok_or_else(|| {
            ErrorData::invalid_params(format!("agent not found: {}", params.agent_id), None)
        })?;
        let base_url = agent.base_url();

        let a2a_body = serde_json::json!({
            "message": {
                "role": "ROLE_USER",
                "parts": [{ "text_part": { "text": params.message } }],
                "context_id": params.context_id,
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{base_url}/message:send"))
            .json(&a2a_body)
            .send()
            .await
            .map_err(|e| ErrorData::internal_error(format!("A2A request failed: {e}"), None))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ErrorData::internal_error(format!("A2A parse response: {e}"), None))?;

        let task = if body.get("id").is_some() {
            body
        } else {
            serde_json::json!({
                "id": format!("synthetic-{}", uuid::Uuid::new_v4()),
                "status": { "state": "TASK_STATE_COMPLETED" },
                "artifacts": [],
                "message": body,
            })
        };

        let out = serde_json::to_string_pretty(&task)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }

    #[tool(name = "agent/task_status", description = "Get the current status of an A2A Task via GetTask.")]
    pub async fn agent_task_status(
        &self,
        Parameters(params): Parameters<AgentTaskStatusParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let (_, agent) = st.find_agent(&params.agent_id).ok_or_else(|| {
            ErrorData::invalid_params(format!("agent not found: {}", params.agent_id), None)
        })?;
        let base_url = agent.base_url();

        let mut url = format!("{base_url}/tasks/{}", params.task_id);
        if let Some(len) = params.history_length {
            url = format!("{url}?history_length={len}");
        }

        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ErrorData::internal_error(format!("A2A GetTask failed: {e}"), None))?;

        let body = resp
            .text()
            .await
            .map_err(|e| ErrorData::internal_error(format!("A2A read body: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(body)]))
    }

    #[tool(name = "agent/stop", description = "Gracefully stop an agent. Sends SIGTERM, waits for grace period, then SIGKILL.")]
    pub fn agent_stop(
        &self,
        Parameters(params): Parameters<AgentStopParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let _ = st.find_agent(&params.agent_id).ok_or_else(|| {
            ErrorData::invalid_params(format!("agent not found: {}", params.agent_id), None)
        })?;

        agent_supervisor::stop_agent(&params.agent_id);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Stop signal sent to agent: {}",
            params.agent_id
        ))]))
    }

    #[tool(name = "agent/restart", description = "Restart an agent instance. Preserves the same template and configuration.")]
    pub fn agent_restart(
        &self,
        Parameters(params): Parameters<AgentRestartParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let (ws_name, agent) = st.find_agent(&params.agent_id).ok_or_else(|| {
            ErrorData::invalid_params(format!("agent not found: {}", params.agent_id), None)
        })?;

        let ws = st.workspace(ws_name).unwrap();
        let template = agent.template.clone();
        let name = agent.name.clone();
        let workspace = ws_name.to_string();
        let dir = ws.dir.clone();
        let old_token_id = agent.token_id.clone();
        let old_session_id = agent.session_id.clone();
        let old_spawned_by = agent.spawned_by.clone();

        agent_supervisor::stop_agent(&params.agent_id);
        std::thread::sleep(std::time::Duration::from_millis(500));

        let port = st.allocate_agent_port().ok_or_else(|| {
            ErrorData::internal_error("no ports available", None)
        })?;

        let opts = agent_supervisor::SpawnOptions {
            workspace: workspace.clone(),
            dir,
            template: template.clone(),
            name: name.clone(),
            port,
            command: format!("{template} serve"),
            env: HashMap::new(),
        };

        let result = agent_supervisor::spawn_agent(opts).ok_or_else(|| {
            ErrorData::internal_error("agent supervisor not initialized", None)
        })?;

        let new_agent = AgentInstanceState {
            id: result.agent_id.clone(),
            template,
            name,
            workspace: workspace.clone(),
            status: AgentStatus::Starting,
            port: result.port,
            host: None,
            pid: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            token_id: old_token_id,
            session_id: old_session_id,
            spawned_by: old_spawned_by,
        };

        state::modify(|st| {
            st.remove_agent(&params.agent_id);
            st.add_agent(&workspace, new_agent.clone());
        })
        .map_err(|e| ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Restarted agent: {} -> {}",
            params.agent_id, result.agent_id
        ))]))
    }

    #[tool(name = "token/create", description = "Create an ARP token with specified scope and permission. Requires admin permission.")]
    pub fn token_create(
        &self,
        Parameters(params): Parameters<TokenCreateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let scope = parse_scope(&params.scope)
            .map_err(|e| ErrorData::invalid_params(e, None))?;
        let permission = parse_permission(&params.permission)
            .map_err(|e| ErrorData::invalid_params(e, None))?;

        let token = create_scoped_token(
            &params.subject,
            scope,
            permission,
            params.expires_in_seconds,
        );
        let token_str = encode_scoped_token(&token);

        let result = serde_json::json!({
            "token": token_str,
            "token_id": token.id,
            "subject": token.subject,
            "scope": token.scope,
            "permission": token.permission,
            "issued_at": token.issued_at,
            "expires_at": token.expires_at,
        });

        let out = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }
}
