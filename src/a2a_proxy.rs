use crate::agent_supervisor;
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
            "/a2a/agents/{agent_id}/{*rest}",
            axum::routing::any(proxy_agent_request),
        )
        .route(
            "/a2a/route/{*rest}",
            axum::routing::post(route_send_message),
        )
}

#[derive(Serialize, Clone)]
struct EnrichedAgentCard {
    #[serde(flatten)]
    card: AgentCard,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<Value>,
}

fn enriched_agent_card(agent: &AgentInstanceState, project: &str) -> EnrichedAgentCard {
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

struct ResolvedAgent {
    url: String,
    agent: AgentInstanceState,
    project: String,
}

fn resolve_agent(agent_id: &str) -> Result<ResolvedAgent, Response> {
    let st = state::load().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let (ws_name, agent) = st
        .find_agent(agent_id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, format!("agent not found: {agent_id}")))?;

    if agent.status == AgentStatus::Stopped || agent.status == AgentStatus::Stopping {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("agent {} is {}", agent_id, agent.status),
        ));
    }

    let ws = st.workspace(ws_name).ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "workspace not found for agent",
        )
    })?;

    Ok(ResolvedAgent {
        url: agent.base_url(),
        agent: agent.clone(),
        project: ws.project.clone(),
    })
}

async fn list_agents() -> Result<Json<Vec<serde_json::Value>>, Response> {
    let st = state::load().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let mut cards = Vec::new();

    for (ws_name, ws) in &st.workspaces {
        if !ws.active {
            continue;
        }
        for agent in &ws.agents {
            if agent.status == AgentStatus::Ready || agent.status == AgentStatus::Busy {
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
) -> Result<Json<Vec<serde_json::Value>>, Response> {
    let st = state::load().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let mut cards = Vec::new();

    for (ws_name, ws) in &st.workspaces {
        if !ws.active {
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
) -> Result<Json<serde_json::Value>, Response> {
    let resolved = resolve_agent(&agent_id)?;
    let card = enriched_agent_card(&resolved.agent, &resolved.project);
    let val = serde_json::to_value(&card)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("serialize: {e}")))?;
    Ok(Json(val))
}

async fn proxy_agent_request(
    Path((agent_id, rest)): Path<(String, String)>,
    State(state): State<A2aProxyState>,
    req: Request,
) -> Result<Response, Response> {
    let resolved = resolve_agent(&agent_id)?;
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
            .chain(r.capability.into_iter())
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

    for (_, ws) in &st.workspaces {
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
}
