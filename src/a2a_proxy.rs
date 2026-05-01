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
            "workspace": agent.workspace,
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

    let ws = st.workspace(ws_name).ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "workspace not found for agent",
        )
    })?;

    let project = ws.project.clone();

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

    for (ws_name, ws) in &st.workspaces {
        if !ws.active {
            continue;
        }
        // Filter by project scope
        if !scope_includes_project(&token.scope, &ws.project) {
            continue;
        }
        for agent in &ws.agents {
            if agent.status == AgentStatus::Ready || agent.status == AgentStatus::Busy {
                // For session-scoped tokens, only show own-session agents
                if token.permission == Permission::Session && !session_matches(&token, agent) {
                    continue;
                }
                let card = enriched_agent_card(agent, &ws.project);
                if let Ok(val) = serde_json::to_value(&card) {
                    cards.push(val);
                }
            }
        }
        let _ = ws_name;
    }

    Ok(Json(cards))
}

#[derive(Deserialize)]
struct DiscoverQuery {
    capability: Option<String>,
    workspace: Option<String>,
    status: Option<String>,
}

async fn discover_agents(
    Query(query): Query<DiscoverQuery>,
    req: Request,
) -> Result<Json<Vec<serde_json::Value>>, Response> {
    let token = extract_token(&req);
    let st = state::load().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let mut cards = Vec::new();

    for (ws_name, ws) in &st.workspaces {
        if !ws.active {
            continue;
        }
        // Filter by project scope
        if !scope_includes_project(&token.scope, &ws.project) {
            continue;
        }
        if let Some(ref filter_ws) = query.workspace {
            if ws_name != filter_ws {
                continue;
            }
        }
        for agent in &ws.agents {
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

            let card = enriched_agent_card(agent, &ws.project);

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

    for ws in st.workspaces.values() {
        if !ws.active {
            continue;
        }
        for agent in &ws.agents {
            if agent.status != AgentStatus::Ready && agent.status != AgentStatus::Busy {
                continue;
            }

            let has_match = if match_tags.is_empty() {
                true
            } else {
                let card = enriched_agent_card(agent, &ws.project);
                card.card.skills.iter().any(|s| {
                    s.tags.iter().any(|t| match_tags.contains(t))
                })
            };

            if !has_match {
                continue;
            }

            match &best_agent {
                None => best_agent = Some((agent.clone(), ws.project.clone())),
                Some((existing, _)) => {
                    if agent.status == AgentStatus::Ready
                        && existing.status == AgentStatus::Busy
                    {
                        best_agent = Some((agent.clone(), ws.project.clone()));
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
    use super::*;
    use crate::state::{AgentInstanceState, AgentStatus};

    fn test_agent(id: &str, name: &str, port: u16, status: AgentStatus) -> AgentInstanceState {
        AgentInstanceState {
            id: id.into(),
            template: "crush".into(),
            name: name.into(),
            workspace: "test-ws".into(),
            status,
            port,
            host: None,
            pid: Some(1234),
            started_at: "2026-04-28T10:00:00Z".into(),
            ..Default::default()
        }
    }

    #[test]
    fn synthetic_agent_card_has_expected_fields() {
        let agent = test_agent("coder-abc", "coder", 9100, AgentStatus::Ready);
        let card = synthetic_agent_card(&agent);
        assert_eq!(card.name, "coder");
        assert!(card.description.contains("crush"));
        assert_eq!(card.skills.len(), 1);
        assert_eq!(card.skills[0].tags, vec!["crush"]);
        assert_eq!(card.capabilities.streaming, Some(true));
    }

    #[test]
    fn enriched_card_has_arp_metadata() {
        let agent = test_agent("coder-abc", "coder", 9100, AgentStatus::Ready);
        let ecard = enriched_agent_card(&agent, "myproject");
        let meta = ecard.metadata.unwrap();
        let arp = &meta["arp"];
        assert_eq!(arp["agent_id"], "coder-abc");
        assert_eq!(arp["workspace"], "test-ws");
        assert_eq!(arp["project"], "myproject");
        assert_eq!(arp["template"], "crush");
        assert_eq!(arp["status"], "ready");
        assert_eq!(arp["direct_url"], "http://localhost:9100");
    }

    #[test]
    fn enriched_card_proxy_url() {
        let agent = test_agent("coder-abc", "coder", 9100, AgentStatus::Ready);
        let ecard = enriched_agent_card(&agent, "proj");
        assert_eq!(ecard.card.supported_interfaces.len(), 1);
        assert!(ecard.card.supported_interfaces[0]
            .url
            .contains("/a2a/agents/coder-abc"));
    }

    #[test]
    fn router_builds() {
        let _ = router();
    }

    #[test]
    fn error_body_serializes() {
        let body = ErrorBody {
            error: "test".into(),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"error\":\"test\""));
    }

    #[test]
    fn enriched_card_metadata_has_all_required_fields() {
        let agent = AgentInstanceState {
            id: "echo-001".into(),
            template: "echo".into(),
            name: "echo-agent".into(),
            workspace: "arp-test".into(),
            status: AgentStatus::Ready,
            port: 9200,
            host: Some("echo-agent".into()),
            pid: None,
            started_at: "2026-04-28T10:00:00Z".into(),
            ..Default::default()
        };
        let ecard = enriched_agent_card(&agent, "test-project");
        let meta = ecard.metadata.unwrap();
        let arp = &meta["arp"];
        assert!(arp.get("agent_id").is_some(), "missing agent_id");
        assert!(arp.get("workspace").is_some(), "missing workspace");
        assert!(arp.get("project").is_some(), "missing project");
        assert!(arp.get("template").is_some(), "missing template");
        assert!(arp.get("status").is_some(), "missing status");
        assert!(arp.get("direct_url").is_some(), "missing direct_url");
        assert!(arp.get("started_at").is_some(), "missing started_at");
    }

    #[test]
    fn enriched_card_interface_url_points_to_proxy_not_direct() {
        let agent = test_agent("echo-001", "echo-agent", 9200, AgentStatus::Ready);
        let ecard = enriched_agent_card(&agent, "proj");
        let iface = &ecard.card.supported_interfaces[0];
        assert!(
            iface.url.contains("/a2a/agents/echo-001"),
            "interface URL should point to proxy: {}",
            iface.url
        );
        assert!(
            !iface.url.contains("9200"),
            "interface URL should not contain direct port: {}",
            iface.url
        );
    }

    #[test]
    fn enriched_card_direct_url_in_metadata() {
        let agent = AgentInstanceState {
            id: "echo-001".into(),
            template: "echo".into(),
            name: "echo-agent".into(),
            workspace: "arp-test".into(),
            status: AgentStatus::Ready,
            port: 9200,
            host: Some("echo-agent".into()),
            pid: None,
            started_at: "2026-04-28T10:00:00Z".into(),
            ..Default::default()
        };
        let ecard = enriched_agent_card(&agent, "test-project");
        let meta = ecard.metadata.unwrap();
        let direct = meta["arp"]["direct_url"].as_str().unwrap();
        assert!(
            direct.contains("9200"),
            "direct_url should contain agent port: {}",
            direct
        );
    }

    #[test]
    fn enriched_card_skips_stopped_agents_in_listing_logic() {
        let stopped = test_agent("stopped-1", "stopped", 9300, AgentStatus::Stopped);
        let stopping = test_agent("stopping-1", "stopping", 9301, AgentStatus::Stopping);
        let starting = test_agent("starting-1", "starting", 9302, AgentStatus::Starting);
        let ready = test_agent("ready-1", "ready", 9303, AgentStatus::Ready);
        let busy = test_agent("busy-1", "busy", 9304, AgentStatus::Busy);

        let agents = vec![stopped, stopping, starting, ready, busy];
        let visible: Vec<_> = agents
            .iter()
            .filter(|a| a.status == AgentStatus::Ready || a.status == AgentStatus::Busy)
            .collect();
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].id, "ready-1");
        assert_eq!(visible[1].id, "busy-1");
    }

    #[test]
    fn routing_prefers_ready_over_busy() {
        let busy = test_agent("busy-1", "agent-a", 9100, AgentStatus::Busy);
        let ready = test_agent("ready-1", "agent-b", 9101, AgentStatus::Ready);

        let agents = vec![busy.clone(), ready.clone()];
        let mut best: Option<AgentInstanceState> = None;
        for agent in &agents {
            if agent.status != AgentStatus::Ready && agent.status != AgentStatus::Busy {
                continue;
            }
            match &best {
                None => best = Some(agent.clone()),
                Some(existing) => {
                    if agent.status == AgentStatus::Ready
                        && existing.status == AgentStatus::Busy
                    {
                        best = Some(agent.clone());
                    }
                }
            }
        }
        assert_eq!(best.unwrap().id, "ready-1");
    }

    #[test]
    fn routing_matches_by_skill_tags() {
        let agent = AgentInstanceState {
            id: "echo-001".into(),
            template: "echo".into(),
            name: "echo-agent".into(),
            workspace: "test-ws".into(),
            status: AgentStatus::Ready,
            port: 9200,
            host: None,
            pid: None,
            started_at: "2026-04-28T10:00:00Z".into(),
            ..Default::default()
        };

        let card = enriched_agent_card(&agent, "proj");
        let match_tags = vec!["echo".to_string()];
        let matches = card
            .card
            .skills
            .iter()
            .any(|s| s.tags.iter().any(|t| match_tags.contains(t)));
        assert!(matches, "echo agent should match 'echo' tag");

        let no_match_tags = vec!["nonexistent".to_string()];
        let no_matches = card
            .card
            .skills
            .iter()
            .any(|s| s.tags.iter().any(|t| no_match_tags.contains(t)));
        assert!(!no_matches, "echo agent should not match 'nonexistent' tag");
    }

    #[test]
    fn routing_accepts_empty_tags_matches_any() {
        let match_tags: Vec<String> = vec![];
        let has_match = match_tags.is_empty();
        assert!(has_match, "empty tags should match any agent");
    }

    #[test]
    fn agent_base_url_with_host() {
        let agent = AgentInstanceState {
            id: "echo-001".into(),
            template: "echo".into(),
            name: "echo-agent".into(),
            workspace: "test-ws".into(),
            status: AgentStatus::Ready,
            port: 9200,
            host: Some("echo-agent".into()),
            pid: None,
            started_at: "2026-04-28T10:00:00Z".into(),
            ..Default::default()
        };
        assert_eq!(agent.base_url(), "http://echo-agent:9200");
    }

    #[test]
    fn agent_base_url_with_http_host() {
        let agent = AgentInstanceState {
            id: "echo-001".into(),
            template: "echo".into(),
            name: "echo-agent".into(),
            workspace: "test-ws".into(),
            status: AgentStatus::Ready,
            port: 9200,
            host: Some("http://custom-host:8080".into()),
            pid: None,
            started_at: "2026-04-28T10:00:00Z".into(),
            ..Default::default()
        };
        assert_eq!(agent.base_url(), "http://custom-host:8080");
    }

    #[test]
    fn agent_base_url_without_host() {
        let agent = test_agent("echo-001", "echo-agent", 9200, AgentStatus::Ready);
        assert_eq!(agent.base_url(), "http://127.0.0.1:9200");
    }

    // --- Scope enforcement unit tests ---

    #[test]
    fn resolve_agent_scope_filters_project() {
        use crate::auth::TokenScope;
        // Admin token with global scope can access any project
        let admin = auth::localhost_admin_token();
        let _agent = test_agent("a1", "coder", 9100, AgentStatus::Ready);
        assert!(scope_includes_project(&admin.scope, "any-project"));

        // Project-scoped token can access listed projects
        let token = ScopedToken {
            id: "t1".into(),
            subject: "user".into(),
            scope: TokenScope::Projects(vec!["myapp".into()]),
            permission: Permission::Project,
            session_id: None,
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            parent_token_id: None,
        };
        assert!(scope_includes_project(&token.scope, "myapp"));
        assert!(!scope_includes_project(&token.scope, "other"));
    }

    #[test]
    fn session_token_filters_by_session() {
        use crate::auth::TokenScope;
        let token = ScopedToken {
            id: "t1".into(),
            subject: "user".into(),
            scope: TokenScope::Global,
            permission: Permission::Session,
            session_id: Some("sess-1".into()),
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            parent_token_id: None,
        };

        let own_agent = AgentInstanceState {
            session_id: Some("sess-1".into()),
            ..test_agent("a1", "coder", 9100, AgentStatus::Ready)
        };
        let other_agent = AgentInstanceState {
            session_id: Some("sess-2".into()),
            ..test_agent("a2", "coder", 9101, AgentStatus::Ready)
        };

        assert!(session_matches(&token, &own_agent));
        assert!(!session_matches(&token, &other_agent));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use crate::state::{AgentInstanceState, AgentStatus, Store, WorkspaceState};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn fixture_state() -> Store {
        let mut store = Store::default();
        let ws = WorkspaceState {
            project: "test-project".into(),
            active: true,
            tag_index: 10,
            dir: "/workspace".into(),
            acp_port: Some(9100),
            acp_url: Some("http://127.0.0.1:9100".into()),
            acp_session_id: None,
            agents: vec![
                AgentInstanceState {
                    id: "echo-agent-001".into(),
                    template: "echo".into(),
                    name: "echo-agent".into(),
                    workspace: "arp-test".into(),
                    status: AgentStatus::Ready,
                    port: 9200,
                    host: Some("echo-agent".into()),
                    pid: None,
                    started_at: "2026-04-28T10:00:00Z".into(),
                    ..Default::default()
                },
            ],
        };
        store.workspaces.insert("arp-test".into(), ws);
        store
    }

    fn setup_fixture_home() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let config_dir = tmp.path().join(".config/awesometree");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let store = fixture_state();
        let json = serde_json::to_string_pretty(&store).expect("serialize state");
        std::fs::write(config_dir.join("state.json"), json).expect("write state");
        tmp
    }

    fn build_test_app() -> axum::Router {
        let state = A2aProxyState::new();
        // Wrap with a middleware that attaches an admin token (simulates auth middleware)
        let app = router().with_state(state);
        app.layer(axum::middleware::from_fn(|mut req: Request<Body>, next: axum::middleware::Next| async move {
            req.extensions_mut().insert(auth::localhost_admin_token());
            Ok::<_, std::convert::Infallible>(next.run(req).await)
        }))
    }

    async fn body_json(body: Body) -> serde_json::Value {
        let bytes = body.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }



    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn list_agents_returns_200() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200, "GET /a2a/agents should return 200");

        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(
            ct.contains("application/json"),
            "content-type should be JSON, got: {ct}"
        );

        let json = body_json(resp.into_body()).await;
        let agents = json.as_array().expect("response should be an array");
        assert!(!agents.is_empty(), "should have at least one agent");
        assert_eq!(agents[0]["name"], "echo-agent");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn list_agents_content_type_json() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("application/json"));
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn discover_returns_200() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/discover")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200, "GET /a2a/discover should return 200");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn nonexistent_agent_card_returns_404() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents/nonexistent-agent-00000/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn nonexistent_agent_message_returns_404() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let body = serde_json::json!({
            "message": {"role": "ROLE_USER", "parts": [{"text_part": {"text": "Should 404."}}]}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/a2a/agents/nonexistent-agent-00000/message:send")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn nonexistent_agent_task_returns_404() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents/nonexistent-agent-00000/tasks/fake-task-id")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn nonexistent_agent_cancel_returns_404() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/a2a/agents/nonexistent-agent-00000/tasks/fake-task-id:cancel")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn nonexistent_agent_stream_returns_404() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let body = serde_json::json!({
            "message": {"role": "ROLE_USER", "parts": [{"text_part": {"text": "Should 404."}}]}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/a2a/agents/nonexistent-agent-00000/message:stream")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn route_no_match_returns_404() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let body = serde_json::json!({
            "message": {"role": "ROLE_USER", "parts": [{"text_part": {"text": "no match"}}]},
            "routing": {"tags": ["nonexistent-capability-00000"]}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/a2a/route/message:send")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404, "route with no matching tags should 404");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn route_endpoint_exists_not_405() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let body = serde_json::json!({
            "message": {"role": "ROLE_USER", "parts": [{"text_part": {"text": "test"}}]},
            "routing": {"tags": ["test"]}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/a2a/route/message:send")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_ne!(resp.status(), 405, "routing endpoint should not return 405");
        assert_ne!(resp.status(), 500, "routing endpoint should not return 500");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn agent_card_has_enriched_metadata() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents/echo-agent-001/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200, "known agent card should return 200");

        let json = body_json(resp.into_body()).await;
        assert!(json.get("name").is_some(), "card should have name");
        assert!(json.get("supportedInterfaces").is_some(), "card should have supportedInterfaces");

        let arp = &json["metadata"]["arp"];
        assert_eq!(arp["agent_id"], "echo-agent-001");
        assert_eq!(arp["workspace"], "arp-test");
        assert_eq!(arp["project"], "test-project");
        assert_eq!(arp["template"], "echo");
        assert_eq!(arp["status"], "ready");
        assert!(arp["direct_url"].as_str().is_some(), "should have direct_url");
        assert!(arp["started_at"].as_str().is_some(), "should have started_at");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn agent_card_interface_url_points_to_proxy() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents/echo-agent-001/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let json = body_json(resp.into_body()).await;

        let ifaces = json["supportedInterfaces"].as_array().expect("should have interfaces");
        assert!(!ifaces.is_empty());
        let url = ifaces[0]["url"].as_str().unwrap();
        assert!(
            url.contains("/a2a/agents/echo-agent-001"),
            "interface URL should point to proxy, got: {url}"
        );
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn list_agents_includes_metadata_with_direct_url() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let json = body_json(resp.into_body()).await;
        let agents = json.as_array().unwrap();
        assert!(!agents.is_empty());

        let direct_url = agents[0]["metadata"]["arp"]["direct_url"].as_str();
        assert!(direct_url.is_some(), "listed agent should have direct_url in metadata");
    }

    // This test is inherently racy: it uses set_var("HOME") which can be
    // clobbered by other concurrent integration tests that do the same.
    // Run with `cargo test -- --test-threads=1` or in isolation.
    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn list_agents_excludes_stopped_agents() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = tmp.path().join(".config/awesometree");
        std::fs::create_dir_all(&config_dir).unwrap();

        let mut store = Store::default();
        let ws = WorkspaceState {
            project: "proj".into(),
            active: true,
            tag_index: 10,
            dir: "/ws".into(),
            agents: vec![
                AgentInstanceState {
                    id: "ready-1".into(),
                    template: "echo".into(),
                    name: "ready".into(),
                    workspace: "ws".into(),
                    status: AgentStatus::Ready,
                    port: 9200,
                    host: None,
                    pid: None,
                    started_at: "2026-04-28T10:00:00Z".into(),
                    ..Default::default()
                },
                AgentInstanceState {
                    id: "stopped-1".into(),
                    template: "echo".into(),
                    name: "stopped".into(),
                    workspace: "ws".into(),
                    status: AgentStatus::Stopped,
                    port: 9201,
                    host: None,
                    pid: None,
                    started_at: "2026-04-28T10:00:00Z".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        store.workspaces.insert("ws".into(), ws);
        let json = serde_json::to_string_pretty(&store).unwrap();
        std::fs::write(config_dir.join("state.json"), json).unwrap();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let json = body_json(resp.into_body()).await;
        let agents = json.as_array().unwrap();
        assert_eq!(agents.len(), 1, "stopped agents should be excluded");
        assert_eq!(agents[0]["metadata"]["arp"]["agent_id"], "ready-1");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn discover_filters_by_workspace() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/discover?workspace=arp-test")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        let json = body_json(resp.into_body()).await;
        let agents = json.as_array().unwrap();
        assert!(!agents.is_empty(), "should find agent in arp-test workspace");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn discover_filters_by_nonexistent_workspace_returns_empty() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/discover?workspace=nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        let json = body_json(resp.into_body()).await;
        let agents = json.as_array().unwrap();
        assert!(agents.is_empty());
    }

    // --- Scope enforcement integration tests ---

    fn build_test_app_with_token(token: ScopedToken) -> axum::Router {
        let state = A2aProxyState::new();
        let app = router().with_state(state);
        app.layer(axum::middleware::from_fn(move |mut req: Request<Body>, next: axum::middleware::Next| {
            let t = token.clone();
            async move {
                req.extensions_mut().insert(t);
                Ok::<_, std::convert::Infallible>(next.run(req).await)
            }
        }))
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn list_agents_scoped_to_project() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = tmp.path().join(".config/awesometree");
        std::fs::create_dir_all(&config_dir).unwrap();

        let mut store = Store::default();
        let ws1 = WorkspaceState {
            project: "proj-a".into(),
            active: true,
            tag_index: 10,
            dir: "/ws1".into(),
            agents: vec![AgentInstanceState {
                id: "agent-a".into(),
                template: "echo".into(),
                name: "agent-a".into(),
                workspace: "ws1".into(),
                status: AgentStatus::Ready,
                port: 9200,
                ..Default::default()
            }],
            ..Default::default()
        };
        let ws2 = WorkspaceState {
            project: "proj-b".into(),
            active: true,
            tag_index: 11,
            dir: "/ws2".into(),
            agents: vec![AgentInstanceState {
                id: "agent-b".into(),
                template: "echo".into(),
                name: "agent-b".into(),
                workspace: "ws2".into(),
                status: AgentStatus::Ready,
                port: 9201,
                ..Default::default()
            }],
            ..Default::default()
        };
        store.workspaces.insert("ws1".into(), ws1);
        store.workspaces.insert("ws2".into(), ws2);
        let json = serde_json::to_string_pretty(&store).unwrap();
        std::fs::write(config_dir.join("state.json"), json).unwrap();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        // Token scoped to proj-a only
        let token = ScopedToken {
            id: "t1".into(),
            subject: "user".into(),
            scope: auth::TokenScope::Projects(vec!["proj-a".into()]),
            permission: Permission::Project,
            session_id: None,
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            parent_token_id: None,
        };

        let app = build_test_app_with_token(token);
        let req = Request::builder()
            .uri("/a2a/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let json = body_json(resp.into_body()).await;
        let agents = json.as_array().unwrap();
        assert_eq!(agents.len(), 1, "should only see agent in proj-a");
        assert_eq!(agents[0]["metadata"]["arp"]["agent_id"], "agent-a");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn agent_card_forbidden_for_wrong_scope() {
        let tmp = setup_fixture_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        // Token scoped to "other-project", not "test-project"
        let token = ScopedToken {
            id: "t1".into(),
            subject: "user".into(),
            scope: auth::TokenScope::Projects(vec!["other-project".into()]),
            permission: Permission::Project,
            session_id: None,
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            parent_token_id: None,
        };

        let app = build_test_app_with_token(token);
        let req = Request::builder()
            .uri("/a2a/agents/echo-agent-001/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 403, "should be forbidden for wrong project scope");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn session_token_cannot_see_other_session_agents() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = tmp.path().join(".config/awesometree");
        std::fs::create_dir_all(&config_dir).unwrap();

        let mut store = Store::default();
        let ws = WorkspaceState {
            project: "proj".into(),
            active: true,
            tag_index: 10,
            dir: "/ws".into(),
            agents: vec![
                AgentInstanceState {
                    id: "own-agent".into(),
                    template: "echo".into(),
                    name: "own".into(),
                    workspace: "ws".into(),
                    status: AgentStatus::Ready,
                    port: 9200,
                    session_id: Some("sess-1".into()),
                    ..Default::default()
                },
                AgentInstanceState {
                    id: "other-agent".into(),
                    template: "echo".into(),
                    name: "other".into(),
                    workspace: "ws".into(),
                    status: AgentStatus::Ready,
                    port: 9201,
                    session_id: Some("sess-2".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        store.workspaces.insert("ws".into(), ws);
        let json = serde_json::to_string_pretty(&store).unwrap();
        std::fs::write(config_dir.join("state.json"), json).unwrap();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let token = ScopedToken {
            id: "t1".into(),
            subject: "user".into(),
            scope: auth::TokenScope::Global,
            permission: Permission::Session,
            session_id: Some("sess-1".into()),
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            parent_token_id: None,
        };

        let app = build_test_app_with_token(token);
        let req = Request::builder()
            .uri("/a2a/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let json = body_json(resp.into_body()).await;
        let agents = json.as_array().unwrap();
        assert_eq!(agents.len(), 1, "session token should only see own agent");
        assert_eq!(agents[0]["metadata"]["arp"]["agent_id"], "own-agent");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn session_token_forbidden_to_proxy_other_session_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = tmp.path().join(".config/awesometree");
        std::fs::create_dir_all(&config_dir).unwrap();

        let mut store = Store::default();
        let ws = WorkspaceState {
            project: "proj".into(),
            active: true,
            tag_index: 10,
            dir: "/ws".into(),
            agents: vec![AgentInstanceState {
                id: "other-agent".into(),
                template: "echo".into(),
                name: "other".into(),
                workspace: "ws".into(),
                status: AgentStatus::Ready,
                port: 9200,
                session_id: Some("sess-2".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        store.workspaces.insert("ws".into(), ws);
        let json = serde_json::to_string_pretty(&store).unwrap();
        std::fs::write(config_dir.join("state.json"), json).unwrap();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let token = ScopedToken {
            id: "t1".into(),
            subject: "user".into(),
            scope: auth::TokenScope::Global,
            permission: Permission::Session,
            session_id: Some("sess-1".into()),
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            parent_token_id: None,
        };

        let app = build_test_app_with_token(token);
        let req = Request::builder()
            .uri("/a2a/agents/other-agent/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 403, "session token should be forbidden from other session's agent");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn discover_scoped_filters_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = tmp.path().join(".config/awesometree");
        std::fs::create_dir_all(&config_dir).unwrap();

        let mut store = Store::default();
        let ws1 = WorkspaceState {
            project: "proj-a".into(),
            active: true,
            tag_index: 10,
            dir: "/ws1".into(),
            agents: vec![AgentInstanceState {
                id: "agent-a".into(),
                template: "echo".into(),
                name: "agent-a".into(),
                workspace: "ws1".into(),
                status: AgentStatus::Ready,
                port: 9200,
                ..Default::default()
            }],
            ..Default::default()
        };
        let ws2 = WorkspaceState {
            project: "proj-b".into(),
            active: true,
            tag_index: 11,
            dir: "/ws2".into(),
            agents: vec![AgentInstanceState {
                id: "agent-b".into(),
                template: "echo".into(),
                name: "agent-b".into(),
                workspace: "ws2".into(),
                status: AgentStatus::Ready,
                port: 9201,
                ..Default::default()
            }],
            ..Default::default()
        };
        store.workspaces.insert("ws1".into(), ws1);
        store.workspaces.insert("ws2".into(), ws2);
        let json = serde_json::to_string_pretty(&store).unwrap();
        std::fs::write(config_dir.join("state.json"), json).unwrap();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let token = ScopedToken {
            id: "t1".into(),
            subject: "user".into(),
            scope: auth::TokenScope::Projects(vec!["proj-a".into()]),
            permission: Permission::Project,
            session_id: None,
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            parent_token_id: None,
        };

        let app = build_test_app_with_token(token);
        let req = Request::builder()
            .uri("/a2a/discover")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let json = body_json(resp.into_body()).await;
        let agents = json.as_array().unwrap();
        assert_eq!(agents.len(), 1, "discover should only return agents in scoped projects");
        assert_eq!(agents[0]["metadata"]["arp"]["agent_id"], "agent-a");
    }

    // --- Flexible routing integration tests ---

    fn setup_multi_agent_home() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let config_dir = tmp.path().join(".config/awesometree");
        std::fs::create_dir_all(&config_dir).expect("create config dir");

        let mut store = Store::default();
        let ws1 = WorkspaceState {
            project: "proj".into(),
            active: true,
            tag_index: 10,
            dir: "/ws1".into(),
            agents: vec![AgentInstanceState {
                id: "agent-abc123".into(),
                template: "crush".into(),
                name: "coder".into(),
                workspace: "feat-auth".into(),
                status: AgentStatus::Ready,
                port: 9200,
                host: Some("agent-host-1".into()),
                pid: None,
                started_at: "2026-04-28T10:00:00Z".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let ws2 = WorkspaceState {
            project: "proj".into(),
            active: true,
            tag_index: 11,
            dir: "/ws2".into(),
            agents: vec![AgentInstanceState {
                id: "agent-def456".into(),
                template: "crush".into(),
                name: "coder".into(),
                workspace: "feat-ui".into(),
                status: AgentStatus::Ready,
                port: 9201,
                host: Some("agent-host-2".into()),
                pid: None,
                started_at: "2026-04-28T10:00:00Z".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        store.workspaces.insert("feat-auth".into(), ws1);
        store.workspaces.insert("feat-ui".into(), ws2);

        let json = serde_json::to_string_pretty(&store).expect("serialize");
        std::fs::write(config_dir.join("state.json"), json).expect("write");
        tmp
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn proxy_resolves_agent_by_name() {
        let tmp = setup_multi_agent_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        // Use the agent name "coder" instead of agent_id
        let req = Request::builder()
            .uri("/a2a/agents/coder/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            200,
            "proxy should resolve agent by name 'coder'"
        );

        let json = body_json(resp.into_body()).await;
        assert_eq!(json["name"], "coder");
        // It should resolve to one of the two "coder" agents
        let agent_id = json["metadata"]["arp"]["agent_id"].as_str().unwrap();
        assert!(
            agent_id == "agent-abc123" || agent_id == "agent-def456",
            "should resolve to one of the coder agents, got: {agent_id}"
        );
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn proxy_resolves_agent_by_ws_name() {
        let tmp = setup_multi_agent_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        // Use workspace/name composite key "feat-ui/coder"
        let req = Request::builder()
            .uri("/a2a/agents/feat-ui%2Fcoder/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            200,
            "proxy should resolve agent by workspace/name 'feat-ui/coder'"
        );

        let json = body_json(resp.into_body()).await;
        let agent_id = json["metadata"]["arp"]["agent_id"].as_str().unwrap();
        assert_eq!(
            agent_id, "agent-def456",
            "feat-ui/coder should resolve to agent-def456"
        );
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn proxy_resolves_agent_by_id_still_works() {
        let tmp = setup_multi_agent_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents/agent-abc123/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            200,
            "proxy should still resolve agent by exact id"
        );

        let json = body_json(resp.into_body()).await;
        assert_eq!(json["metadata"]["arp"]["agent_id"], "agent-abc123");
    }

    #[tokio::test]
    #[ignore = "flaky: races with other tests that set HOME env var"]
    async fn proxy_ws_name_not_found_returns_404() {
        let tmp = setup_multi_agent_home();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let app = build_test_app();
        let req = Request::builder()
            .uri("/a2a/agents/nonexistent-ws%2Fcoder/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            404,
            "nonexistent workspace/name should return 404"
        );
    }
}
