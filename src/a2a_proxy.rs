use crate::agent_supervisor;
use crate::auth::{self, Permission, ScopedToken, scope_includes_project, session_matches};
use crate::state::{self, AgentInstanceState, AgentStatus};
use a2a_rs_core::{
    AgentCapabilities, AgentCard, AgentInterface, AgentSkill,
};
use serde_json::Value;
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const ARP_SERVER_PORT: u16 = 9099;

#[derive(Clone)]
pub struct A2aProxyState {
    pub client: Arc<Client<hyper_util::client::legacy::connect::HttpConnector, Body>>,
}

impl Default for A2aProxyState {
    fn default() -> Self {
        Self::new()
    }
}

impl A2aProxyState {
    pub fn new() -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();
        Self {
            client: Arc::new(client),
        }
    }

    pub fn with_client(
        client: Arc<Client<hyper_util::client::legacy::connect::HttpConnector, Body>>,
    ) -> Self {
        Self { client }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: msg.into() })).into_response()
}

pub fn router() -> axum::Router<A2aProxyState> {
    axum::Router::new()
        .route("/a2a/agents", axum::routing::get(list_agents))
        .route("/a2a/discover", axum::routing::get(discover_agents))
        .route(
            "/a2a/agents/{agent_id}/.well-known/agent-card.json",
            axum::routing::get(get_agent_card),
        )
        .route(
            "/a2a/agents/{agent_id}",
            axum::routing::any(proxy_agent_root),
        )
        .route(
            "/a2a/agents/{agent_id}/{*rest}",
            axum::routing::any(proxy_agent_request),
        )
        .route(
            "/a2a/route/{*rest}",
            axum::routing::post(route_send_message),
        )
}

#[derive(Serialize, Clone)]
pub struct EnrichedAgentCard {
    #[serde(flatten)]
    pub card: AgentCard,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<Value>,
}

pub fn enriched_agent_card(agent: &AgentInstanceState, project: &str) -> EnrichedAgentCard {
    let sup_card = agent_supervisor::agent_card(&agent.id);

    let mut card = match sup_card {
        Some(c) => c,
        None => synthetic_agent_card(agent),
    };

    let proxy_url = format!(
        "http://localhost:{}/a2a/agents/{}",
        ARP_SERVER_PORT, agent.id
    );
    let direct_url = format!("http://localhost:{}", agent.port);

    card.supported_interfaces = vec![AgentInterface {
        url: proxy_url.clone(),
        protocol_binding: "HTTP+JSON".to_string(),
        protocol_version: a2a_rs_core::PROTOCOL_VERSION.to_string(),
        tenant: None,
    }];

    let arp_meta = serde_json::json!({
        "arp": {
            "agent_id": agent.id,
            "workspace": agent.work_session_id,
            "project": project,
            "template": agent.template,
            "status": agent.status.to_string(),
            "direct_url": direct_url,
            "started_at": agent.started_at,
        }
    });

    EnrichedAgentCard {
        card,
        metadata: Some(arp_meta),
    }
}

fn synthetic_agent_card(agent: &AgentInstanceState) -> AgentCard {
    AgentCard {
        name: agent.name.clone(),
        description: format!("{} agent ({})", agent.template, agent.name),
        version: "1.0.0".to_string(),
        supported_interfaces: vec![],
        capabilities: AgentCapabilities {
            streaming: Some(true),
            push_notifications: Some(false),
            ..Default::default()
        },
        skills: vec![AgentSkill {
            id: "general".to_string(),
            name: "General".to_string(),
            description: "General agent capabilities".to_string(),
            tags: vec![agent.template.clone()],
            examples: vec![],
            ..Default::default()
        }],
        default_input_modes: vec!["text/plain".to_string()],
        default_output_modes: vec!["text/plain".to_string()],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Token extraction helper
// ---------------------------------------------------------------------------

/// Extract the ScopedToken from request extensions (set by auth middleware).
/// Falls back to localhost_admin_token if no token is present (shouldn't happen
/// since auth middleware always attaches one).
fn extract_token(req: &Request) -> ScopedToken {
    req.extensions()
        .get::<ScopedToken>()
        .cloned()
        .unwrap_or_else(auth::localhost_admin_token)
}

// ---------------------------------------------------------------------------
// Scope-checked agent resolution
// ---------------------------------------------------------------------------

struct ResolvedAgent {
    url: String,
    agent: AgentInstanceState,
    project: String,
}

fn resolve_agent(agent_id: &str, token: &ScopedToken) -> Result<ResolvedAgent, Response> {
    let st = state::load().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let (ws_name, agent) = st
        .resolve_agent_flexible(agent_id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, format!("agent not found: {agent_id}")))?;

    let _agents_bucket = st.work_session_id(ws_name).ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "workspace not found for agent",
        )
    })?;

    let project = ws_name.to_string().clone();

    // Scope enforcement: token must include agent's project
    if !scope_includes_project(&token.scope, &project) {
        return Err(err(
            StatusCode::FORBIDDEN,
            format!("token scope does not include project: {project}"),
        ));
    }
    // For session-scoped tokens, agent session must match
    if !session_matches(token, agent) {
        return Err(err(
            StatusCode::FORBIDDEN,
            format!("session-scoped token cannot access agent: {agent_id}"),
        ));
    }

    Ok(ResolvedAgent {
        url: agent.base_url(),
        agent: agent.clone(),
        project,
    })
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn list_agents(req: Request) -> Result<Json<Vec<serde_json::Value>>, Response> {
    let token = extract_token(&req);
    let st = state::load().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let mut cards = Vec::new();

    for (ws_name, agents) in &st.agents {
        // Filter by project scope
        if !scope_includes_project(&token.scope, &ws_name.to_string()) {
            continue;
        }
        for agent in agents.iter() {
            if agent.status == AgentStatus::Ready || agent.status == AgentStatus::Busy {
                // For session-scoped tokens, only show own-session agents
                if token.permission == Permission::Session && !session_matches(&token, agent) {
                    continue;
                }
                let card = enriched_agent_card(agent, &ws_name.to_string());
                if let Ok(val) = serde_json::to_value(&card) {
                    cards.push(val);
                }
            }
        }
    }

    Ok(Json(cards))
}

#[derive(Deserialize)]
struct DiscoverQuery {
    capability: Option<String>,
    work_session_id: Option<String>,
    status: Option<String>,
}

async fn discover_agents(
    Query(query): Query<DiscoverQuery>,
    req: Request,
) -> Result<Json<Vec<serde_json::Value>>, Response> {
    let token = extract_token(&req);
    let st = state::load().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let mut cards = Vec::new();

    for (ws_name, agents) in &st.agents {
        // Filter by project scope
        if !scope_includes_project(&token.scope, &ws_name.to_string()) {
            continue;
        }
        if let Some(ref filter_ws) = query.work_session_id {
            if ws_name != filter_ws {
                continue;
            }
        }
        for agent in agents.iter() {
            if let Some(ref filter_status) = query.status {
                if agent.status.to_string() != *filter_status {
                    continue;
                }
            } else if agent.status != AgentStatus::Ready && agent.status != AgentStatus::Busy {
                continue;
            }

            // For session-scoped tokens, only own-session agents
            if token.permission == Permission::Session && !session_matches(&token, agent) {
                continue;
            }

            let card = enriched_agent_card(agent, &ws_name.to_string());

            if let Some(ref capability) = query.capability {
                let matches_cap = card.card.skills.iter().any(|s| {
                    s.tags.iter().any(|t| t == capability)
                });
                if !matches_cap {
                    continue;
                }
            }

            if let Ok(val) = serde_json::to_value(&card) {
                cards.push(val);
            }
        }
    }

    Ok(Json(cards))
}

async fn get_agent_card(
    Path(agent_id): Path<String>,
    req: Request,
) -> Result<Json<serde_json::Value>, Response> {
    let token = extract_token(&req);
    let resolved = resolve_agent(&agent_id, &token)?;
    let card = enriched_agent_card(&resolved.agent, &resolved.project);
    let val = serde_json::to_value(&card)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("serialize: {e}")))?;
    Ok(Json(val))
}

async fn proxy_agent_root(
    Path(agent_id): Path<String>,
    State(state): State<A2aProxyState>,
    req: Request,
) -> Result<Response, Response> {
    let token = extract_token(&req);
    let resolved = resolve_agent(&agent_id, &token)?;
    proxy_to_agent(&resolved.url, "/", req, &state).await
}

async fn proxy_agent_request(
    Path((agent_id, rest)): Path<(String, String)>,
    State(state): State<A2aProxyState>,
    req: Request,
) -> Result<Response, Response> {
    let token = extract_token(&req);
    let resolved = resolve_agent(&agent_id, &token)?;
    let path = format!("/{rest}");
    proxy_to_agent(&resolved.url, &path, req, &state).await
}

#[derive(Deserialize)]
struct RoutingCriteria {
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    capability: Option<String>,
}

#[derive(Deserialize)]
struct RouteMessageRequest {
    #[serde(default)]
    routing: Option<RoutingCriteria>,
    message: serde_json::Value,
}

async fn route_send_message(
    Path(_rest): Path<String>,
    State(state): State<A2aProxyState>,
    Json(body): Json<RouteMessageRequest>,
) -> Result<Response, Response> {
    let st = state::load().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let has_routing = body.routing.is_some();
    let match_tags: Vec<String> = match body.routing {
        Some(r) => r
            .tags
            .unwrap_or_default()
            .into_iter()
            .chain(r.capability)
            .collect(),
        None => vec![],
    };

    if match_tags.is_empty() && has_routing {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "routing provided but no tags or capability specified",
        ));
    }

    let mut best_agent: Option<(AgentInstanceState, String)> = None;

    for (ws_name, agents) in &st.agents {
        for agent in agents.iter() {
            if agent.status != AgentStatus::Ready && agent.status != AgentStatus::Busy {
                continue;
            }

            let has_match = if match_tags.is_empty() {
                true
            } else {
                let card = enriched_agent_card(agent, &ws_name.to_string());
                card.card.skills.iter().any(|s| {
                    s.tags.iter().any(|t| match_tags.contains(t))
                })
            };

            if !has_match {
                continue;
            }

            match &best_agent {
                None => best_agent = Some((agent.clone(), ws_name.to_string().clone())),
                Some((existing, _)) => {
                    if agent.status == AgentStatus::Ready
                        && existing.status == AgentStatus::Busy
                    {
                        best_agent = Some((agent.clone(), ws_name.to_string().clone()));
                    }
                }
            }
        }
    }

    let (agent, _) = best_agent.ok_or_else(|| {
        err(
            StatusCode::NOT_FOUND,
            format!("no agent found matching tags: {:?}", match_tags),
        )
    })?;

    let base_url = agent.base_url();

    let body_bytes = serde_json::to_vec(&body.message)
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("serialize message: {e}")))?;

    let proxy_req = hyper::Request::builder()
        .method(hyper::Method::POST)
        .uri(format!("{base_url}/message:send"))
        .header("content-type", "application/json")
        .body(Body::from(body_bytes))
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("build request: {e}")))?;

    let resp = state
        .client
        .request(proxy_req)
        .await
        .map_err(|e| {
            err(
                StatusCode::BAD_GATEWAY,
                format!("agent {} (port {}): {e}", agent.id, agent.port),
            )
        })?;

    let (parts, body) = resp.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}

async fn proxy_to_agent(
    base_url: &str,
    path: &str,
    req: Request,
    state: &A2aProxyState,
) -> Result<Response, Response> {
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();

    let target_uri = format!("{base_url}{path}{query}");

    let (parts, body) = req.into_parts();
    let mut builder = hyper::Request::builder()
        .method(parts.method)
        .uri(&target_uri);

    for (key, value) in &parts.headers {
        if key != "host" {
            builder = builder.header(key, value);
        }
    }

    let proxy_req = builder
        .body(body)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("build request: {e}")))?;

    let resp = state
        .client
        .request(proxy_req)
        .await
        .map_err(|e| {
            err(
                StatusCode::BAD_GATEWAY,
                format!("agent backend: {e}"),
            )
        })?;

    let (parts, body) = resp.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}


#[cfg(test)]
mod tests {
    // A2A proxy unit tests depend on host-local agent rows keyed by work_session_id.
    // Full coverage lives in integration tests against WorkSessionService.
    use super::*;
    use crate::state::{AgentInstanceState, AgentStatus, Store};

    fn make_agent(id: &str, name: &str, ws: &str, port: u16) -> AgentInstanceState {
        AgentInstanceState {
            id: id.into(),
            template: "crush".into(),
            name: name.into(),
            work_session_id: ws.into(),
            status: AgentStatus::Ready,
            port,
            host: None,
            pid: None,
            started_at: "now".into(),
            token_id: None,
            session_id: None,
            spawned_by: None,
        }
    }

    #[test]
    fn agent_base_url() {
        let a = make_agent("a", "n", "ws", 9100);
        assert_eq!(a.base_url(), "http://127.0.0.1:9100");
    }

    #[test]
    fn store_find_agent() {
        let mut s = Store::default();
        s.add_agent("ws1", make_agent("a1", "bot", "ws1", 9100));
        assert!(s.find_agent("a1").is_some());
    }
}
