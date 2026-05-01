//! HTTP/1.1 bridge for gRPC services on port 9099.
//!
//! Provides axum route handlers that translate JSON ↔ proto and call the gRPC
//! service implementations directly, implementing the HTTP transcoding defined
//! by the google.api.http annotations in the proto files.

use axum::extract::{Path, Query};
use axum::response::{IntoResponse, Json};
use axum::http::StatusCode;
use serde::Deserialize;
use std::collections::HashMap;

use crate::grpc::arp_proto;
use crate::grpc::arp_proto::project_service_server::ProjectService;
use crate::grpc::arp_proto::workspace_service_server::WorkspaceService;
use crate::grpc::arp_proto::agent_service_server::AgentService;
use crate::grpc::arp_proto::discovery_service_server::DiscoveryService;
use crate::grpc::arp_proto::token_service_server::TokenService;
use crate::grpc::{
    ProjectServiceImpl, WorkspaceServiceImpl, AgentServiceImpl,
    DiscoveryServiceImpl, TokenServiceImpl,
};

// ---- Helper: convert tonic Status to axum response ----

fn tonic_to_axum(status: tonic::Status) -> (StatusCode, Json<serde_json::Value>) {
    let http_code = match status.code() {
        tonic::Code::Ok => StatusCode::OK,
        tonic::Code::InvalidArgument => StatusCode::BAD_REQUEST,
        tonic::Code::NotFound => StatusCode::NOT_FOUND,
        tonic::Code::AlreadyExists => StatusCode::CONFLICT,
        tonic::Code::PermissionDenied => StatusCode::FORBIDDEN,
        tonic::Code::Unauthenticated => StatusCode::UNAUTHORIZED,
        tonic::Code::FailedPrecondition => StatusCode::PRECONDITION_FAILED,
        tonic::Code::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
        tonic::Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        http_code,
        Json(serde_json::json!({
            "error": status.message(),
            "code": status.code() as i32,
        })),
    )
}

// ---- Projects ----

// GET /v1/projects
pub async fn list_projects() -> impl IntoResponse {
    let svc = ProjectServiceImpl;
    let req = tonic::Request::new(arp_proto::ListProjectsRequest {});
    match svc.list_projects(req).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            let projects: Vec<serde_json::Value> = inner
                .projects
                .iter()
                .map(project_to_json)
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "projects": projects }))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// POST /v1/projects
pub async fn register_project(
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let agents = parse_agent_templates_from_json(&body);

    let req = arp_proto::RegisterProjectRequest {
        name: body.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        repo: body.get("repo").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        branch: body.get("branch").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        agents,
    };

    let svc = ProjectServiceImpl;
    match svc.register_project(tonic::Request::new(req)).await {
        Ok(resp) => {
            let p = resp.into_inner();
            (StatusCode::OK, Json(project_to_json(&p))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// DELETE /v1/projects/:name
pub async fn unregister_project(
    Path(name): Path<String>,
) -> impl IntoResponse {
    let svc = ProjectServiceImpl;
    let req = arp_proto::UnregisterProjectRequest { name };
    match svc.unregister_project(tonic::Request::new(req)).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// ---- Workspaces ----

#[derive(Deserialize, Default)]
pub struct ListWorkspacesQuery {
    #[serde(default)]
    project: String,
}

// GET /v1/workspaces
pub async fn list_workspaces(
    Query(q): Query<ListWorkspacesQuery>,
) -> impl IntoResponse {
    let svc = WorkspaceServiceImpl;
    let req = arp_proto::ListWorkspacesRequest {
        project: q.project,
        status: 0,
    };
    match svc.list_workspaces(tonic::Request::new(req)).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            let workspaces: Vec<serde_json::Value> = inner
                .workspaces
                .iter()
                .map(workspace_to_json)
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "workspaces": workspaces }))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// POST /v1/workspaces
pub async fn create_workspace(
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let auto_agents = body
        .get("auto_agents")
        .or_else(|| body.get("autoAgents"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let req = arp_proto::CreateWorkspaceRequest {
        name: body.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        project: body.get("project").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        branch: body.get("branch").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        auto_agents,
    };

    let svc = WorkspaceServiceImpl;
    match svc.create_workspace(tonic::Request::new(req)).await {
        Ok(resp) => {
            let ws = resp.into_inner();
            (StatusCode::CREATED, Json(workspace_to_json(&ws))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// GET /v1/workspaces/:name
pub async fn get_workspace(
    Path(name): Path<String>,
) -> impl IntoResponse {
    let svc = WorkspaceServiceImpl;
    let req = arp_proto::GetWorkspaceRequest { name };
    match svc.get_workspace(tonic::Request::new(req)).await {
        Ok(resp) => {
            let ws = resp.into_inner();
            (StatusCode::OK, Json(workspace_to_json(&ws))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// DELETE /v1/workspaces/:name
pub async fn destroy_workspace(
    Path(name): Path<String>,
) -> impl IntoResponse {
    let svc = WorkspaceServiceImpl;
    let req = arp_proto::DestroyWorkspaceRequest {
        name,
        keep_worktree: false,
    };
    match svc.destroy_workspace(tonic::Request::new(req)).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// ---- Agents ----

#[derive(Deserialize, Default)]
pub struct ListAgentsQuery {
    #[serde(default)]
    workspace: String,
    #[serde(default)]
    template: String,
    #[serde(default)]
    status: i32,
}

// GET /v1/agents
pub async fn list_agents(
    Query(q): Query<ListAgentsQuery>,
) -> impl IntoResponse {
    let svc = AgentServiceImpl;
    let req = arp_proto::ListAgentsRequest {
        workspace: q.workspace,
        template: q.template,
        status: q.status,
    };
    match svc.list_agents(tonic::Request::new(req)).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            let agents: Vec<serde_json::Value> = inner
                .agents
                .iter()
                .map(agent_instance_to_json)
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "agents": agents }))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// POST /v1/agents
pub async fn spawn_agent(
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let env: HashMap<String, String> = body
        .get("env")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let scope = body.get("scope").map(|s| arp_proto::Scope {
        global: s.get("global").and_then(|v| v.as_bool()).unwrap_or(false),
        projects: s
            .get("projects")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    });

    let req = arp_proto::SpawnAgentRequest {
        workspace: body.get("workspace").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        template: body.get("template").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        name: body.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        env,
        prompt: body.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        scope,
        permission: body.get("permission").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
    };

    let svc = AgentServiceImpl;
    match svc.spawn_agent(tonic::Request::new(req)).await {
        Ok(resp) => {
            let agent = resp.into_inner();
            (StatusCode::CREATED, Json(agent_instance_to_json(&agent))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// GET /v1/agents/:agent_id
pub async fn get_agent_status(
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let svc = AgentServiceImpl;
    let req = arp_proto::GetAgentStatusRequest { agent_id };
    match svc.get_agent_status(tonic::Request::new(req)).await {
        Ok(resp) => {
            let agent = resp.into_inner();
            (StatusCode::OK, Json(agent_instance_to_json(&agent))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// POST /v1/agents/:agent_id/messages
pub async fn send_agent_message(
    Path(agent_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let req = arp_proto::SendAgentMessageRequest {
        agent_id,
        message: body.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        context_id: body.get("context_id").or_else(|| body.get("contextId"))
            .and_then(|v| v.as_str()).unwrap_or("").to_string(),
        blocking: body.get("blocking").and_then(|v| v.as_bool()).unwrap_or(false),
    };

    let svc = AgentServiceImpl;
    match svc.send_agent_message(tonic::Request::new(req)).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            let result = match inner.result {
                Some(arp_proto::send_agent_message_response::Result::Task(s)) => {
                    serde_json::json!({ "task": prost_struct_to_json(&s) })
                }
                Some(arp_proto::send_agent_message_response::Result::Message(s)) => {
                    serde_json::json!({ "message": prost_struct_to_json(&s) })
                }
                None => serde_json::json!({}),
            };
            (StatusCode::OK, Json(result)).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// POST /v1/agents/:agent_id/tasks
pub async fn create_agent_task(
    Path(agent_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let req = arp_proto::CreateAgentTaskRequest {
        agent_id,
        message: body.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        context_id: body.get("context_id").or_else(|| body.get("contextId"))
            .and_then(|v| v.as_str()).unwrap_or("").to_string(),
    };

    let svc = AgentServiceImpl;
    match svc.create_agent_task(tonic::Request::new(req)).await {
        Ok(resp) => {
            let s = resp.into_inner();
            (StatusCode::OK, Json(prost_struct_to_json(&s))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// GET /v1/agents/:agent_id/tasks/:task_id
pub async fn get_agent_task_status(
    Path((agent_id, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let svc = AgentServiceImpl;
    let req = arp_proto::GetAgentTaskStatusRequest {
        agent_id,
        task_id,
        history_length: 0,
    };
    match svc.get_agent_task_status(tonic::Request::new(req)).await {
        Ok(resp) => {
            let s = resp.into_inner();
            (StatusCode::OK, Json(prost_struct_to_json(&s))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// POST /v1/agents/:agent_id:stop
pub async fn stop_agent(
    Path(agent_id): Path<String>,
    body: Option<Json<serde_json::Value>>,
) -> impl IntoResponse {
    let grace = body
        .as_ref()
        .and_then(|b| b.get("grace_period_ms").or_else(|| b.get("gracePeriodMs")))
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;

    let svc = AgentServiceImpl;
    let req = arp_proto::StopAgentRequest {
        agent_id,
        grace_period_ms: grace,
    };
    match svc.stop_agent(tonic::Request::new(req)).await {
        Ok(resp) => {
            let agent = resp.into_inner();
            (StatusCode::OK, Json(agent_instance_to_json(&agent))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// POST /v1/agents/:agent_id:restart
pub async fn restart_agent(
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let svc = AgentServiceImpl;
    let req = arp_proto::RestartAgentRequest { agent_id };
    match svc.restart_agent(tonic::Request::new(req)).await {
        Ok(resp) => {
            let agent = resp.into_inner();
            (StatusCode::OK, Json(agent_instance_to_json(&agent))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// ---- Discovery ----

#[derive(Deserialize, Default)]
pub struct DiscoverQuery {
    #[serde(default)]
    scope: i32,
    #[serde(default)]
    capability: String,
}

// GET /v1/discover
pub async fn discover_agents(
    Query(q): Query<DiscoverQuery>,
) -> impl IntoResponse {
    let svc = DiscoveryServiceImpl;
    let req = arp_proto::DiscoverAgentsRequest {
        scope: q.scope,
        capability: q.capability,
        urls: Vec::new(),
    };
    match svc.discover_agents(tonic::Request::new(req)).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            let cards: Vec<serde_json::Value> = inner
                .agent_cards
                .iter()
                .map(prost_struct_to_json)
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "agent_cards": cards }))).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// ---- Tokens ----

// POST /v1/tokens
pub async fn create_token(
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let scope = body.get("scope").map(|s| arp_proto::Scope {
        global: s.get("global").and_then(|v| v.as_bool()).unwrap_or(false),
        projects: s
            .get("projects")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    });

    let req = arp_proto::CreateTokenRequest {
        subject: body.get("subject").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        scope,
        permission: body.get("permission").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        expires_in_seconds: body.get("expires_in_seconds")
            .or_else(|| body.get("expiresInSeconds"))
            .and_then(|v| v.as_i64()).unwrap_or(0) as i32,
    };

    let svc = TokenServiceImpl;
    match svc.create_token(tonic::Request::new(req)).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            let mut result = serde_json::json!({
                "bearer_token": inner.bearer_token,
            });
            if let Some(token) = &inner.token {
                result["token"] = token_to_json(token);
            }
            (StatusCode::OK, Json(result)).into_response()
        }
        Err(e) => tonic_to_axum(e).into_response(),
    }
}

// ---- Build the axum router for /v1/* ----

pub fn router() -> axum::Router {
    use axum::routing::{delete, get, post};
    axum::Router::new()
        // Projects
        .route("/v1/projects", get(list_projects).post(register_project))
        .route("/v1/projects/{name}", delete(unregister_project))
        // Workspaces
        .route("/v1/workspaces", get(list_workspaces).post(create_workspace))
        .route("/v1/workspaces/{name}", get(get_workspace).delete(destroy_workspace))
        // Agents
        .route("/v1/agents", get(list_agents).post(spawn_agent))
        .route("/v1/agents/{agent_id}", get(get_agent_status))
        .route("/v1/agents/{agent_id}/messages", post(send_agent_message))
        .route("/v1/agents/{agent_id}/tasks", post(create_agent_task))
        .route("/v1/agents/{agent_id}/tasks/{task_id}", get(get_agent_task_status))
        .route("/v1/agents/{agent_id}/stop", post(stop_agent))
        .route("/v1/agents/{agent_id}/restart", post(restart_agent))
        // Discovery
        .route("/v1/discover", get(discover_agents))
        // Tokens
        .route("/v1/tokens", post(create_token))
}

// ---- JSON serialization helpers ----

fn project_to_json(p: &arp_proto::Project) -> serde_json::Value {
    let agents: Vec<serde_json::Value> = p.agents.iter().map(agent_template_to_json).collect();
    let mut obj = serde_json::json!({
        "name": p.name,
        "repo": p.repo,
        "branch": p.branch,
        "agents": agents,
    });
    if let Some(ctx) = &p.context {
        obj["context"] = serde_json::json!({
            "files": ctx.files,
            "repo_includes": ctx.repo_includes,
            "max_bytes": ctx.max_bytes,
        });
    }
    obj
}

fn agent_template_to_json(t: &arp_proto::AgentTemplate) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "name": t.name,
        "command": t.command,
    });
    if !t.port_env.is_empty() {
        obj["port_env"] = serde_json::json!(t.port_env);
    }
    if let Some(hc) = &t.health_check {
        obj["health_check"] = serde_json::json!({
            "path": hc.path,
            "interval_ms": hc.interval_ms,
            "timeout_ms": hc.timeout_ms,
            "retries": hc.retries,
        });
    }
    if !t.env.is_empty() {
        obj["env"] = serde_json::json!(t.env);
    }
    if !t.capabilities.is_empty() {
        obj["capabilities"] = serde_json::json!(t.capabilities);
    }
    obj
}

fn workspace_to_json(ws: &arp_proto::Workspace) -> serde_json::Value {
    let agents: Vec<serde_json::Value> = ws.agents.iter().map(agent_instance_to_json).collect();
    serde_json::json!({
        "name": ws.name,
        "project": ws.project,
        "dir": ws.dir,
        "status": ws.status,
        "agents": agents,
    })
}

fn agent_instance_to_json(a: &arp_proto::AgentInstance) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "id": a.id,
        "template": a.template,
        "workspace": a.workspace,
        "status": a.status,
        "port": a.port,
        "direct_url": a.direct_url,
        "proxy_url": a.proxy_url,
        "pid": a.pid,
        "context_id": a.context_id,
        "token_id": a.token_id,
        "session_id": a.session_id,
        "spawned_by": a.spawned_by,
    });
    if let Some(ts) = &a.started_at {
        obj["started_at"] = serde_json::json!({
            "seconds": ts.seconds,
            "nanos": ts.nanos,
        });
    }
    obj
}

fn token_to_json(t: &arp_proto::Token) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "id": t.id,
        "subject": t.subject,
        "permission": t.permission,
        "session_id": t.session_id,
        "parent_token_id": t.parent_token_id,
    });
    if let Some(scope) = &t.scope {
        obj["scope"] = serde_json::json!({
            "global": scope.global,
            "projects": scope.projects,
        });
    }
    if let Some(ts) = &t.issued_at {
        obj["issued_at"] = serde_json::json!({ "seconds": ts.seconds, "nanos": ts.nanos });
    }
    if let Some(ts) = &t.expires_at {
        obj["expires_at"] = serde_json::json!({ "seconds": ts.seconds, "nanos": ts.nanos });
    }
    obj
}

fn prost_struct_to_json(s: &prost_types::Struct) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, v) in &s.fields {
        map.insert(k.clone(), prost_value_to_json(v));
    }
    serde_json::Value::Object(map)
}

fn prost_value_to_json(v: &prost_types::Value) -> serde_json::Value {
    match &v.kind {
        Some(prost_types::value::Kind::NullValue(_)) => serde_json::Value::Null,
        Some(prost_types::value::Kind::NumberValue(n)) => serde_json::json!(n),
        Some(prost_types::value::Kind::StringValue(s)) => serde_json::json!(s),
        Some(prost_types::value::Kind::BoolValue(b)) => serde_json::json!(b),
        Some(prost_types::value::Kind::ListValue(list)) => {
            serde_json::Value::Array(list.values.iter().map(prost_value_to_json).collect())
        }
        Some(prost_types::value::Kind::StructValue(s)) => prost_struct_to_json(s),
        None => serde_json::Value::Null,
    }
}

/// Parse agent templates from JSON request body for RegisterProject.
fn parse_agent_templates_from_json(body: &serde_json::Value) -> Vec<arp_proto::AgentTemplate> {
    let Some(agents) = body.get("agents") else {
        return Vec::new();
    };

    // Support array of agent templates
    if let Some(arr) = agents.as_array() {
        return arr
            .iter()
            .filter_map(|v| {
                let name = v.get("name")?.as_str()?.to_string();
                let command = v
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let port_env = v
                    .get("port_env")
                    .or_else(|| v.get("portEnv"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let health_check = v.get("health_check").or_else(|| v.get("healthCheck")).map(|hc| {
                    arp_proto::HealthCheckConfig {
                        path: hc.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        interval_ms: hc.get("interval_ms").or_else(|| hc.get("intervalMs")).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                        timeout_ms: hc.get("timeout_ms").or_else(|| hc.get("timeoutMs")).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                        retries: hc.get("retries").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    }
                });

                let env: HashMap<String, String> = v
                    .get("env")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let capabilities: Vec<String> = v
                    .get("capabilities")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                Some(arp_proto::AgentTemplate {
                    name,
                    command,
                    port_env,
                    health_check,
                    env,
                    capabilities,
                    a2a_card_config: None,
                })
            })
            .collect();
    }

    Vec::new()
}
