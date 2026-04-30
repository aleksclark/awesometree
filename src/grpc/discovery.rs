//! gRPC DiscoveryService implementation.

use crate::a2a_proxy;
use crate::auth;
use crate::grpc::arp_proto::*;
use crate::grpc::arp_proto::discovery_service_server::DiscoveryService;
use crate::grpc::convert;
use crate::grpc::extract_token;
use crate::state;
use std::pin::Pin;
use tonic::{Request, Response, Status};

/// Implements the `DiscoveryService` gRPC trait.
#[derive(Debug, Default)]
pub struct DiscoveryServiceImpl;

#[tonic::async_trait]
impl DiscoveryService for DiscoveryServiceImpl {
    async fn discover_agents(
        &self,
        request: Request<DiscoverAgentsRequest>,
    ) -> Result<Response<DiscoverAgentsResponse>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        let scope = DiscoveryScope::try_from(req.scope).unwrap_or(DiscoveryScope::Local);

        let mut agent_cards: Vec<prost_types::Struct> = Vec::new();

        // --- Local discovery (default) ---
        if scope == DiscoveryScope::Unspecified || scope == DiscoveryScope::Local || scope == DiscoveryScope::Network {
            let store = state::load()
                .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

            for (_ws_name, ws) in &store.workspaces {
                if !ws.active {
                    continue;
                }

                // Filter by project scope
                if !auth::scope_includes_project(&token.scope, &ws.project) {
                    continue;
                }

                for agent in &ws.agents {
                    // Only Ready or Busy agents
                    if agent.status != state::AgentStatus::Ready
                        && agent.status != state::AgentStatus::Busy
                    {
                        continue;
                    }

                    // Session-scoped tokens only see own-session agents
                    if token.permission == auth::Permission::Session
                        && !auth::session_matches(&token, agent)
                    {
                        continue;
                    }

                    let enriched = a2a_proxy::enriched_agent_card(agent, &ws.project);

                    // Capability filtering
                    if !req.capability.is_empty() {
                        let matches_cap = enriched.card.skills.iter().any(|s| {
                            s.tags.iter().any(|t| t == &req.capability)
                        });
                        if !matches_cap {
                            continue;
                        }
                    }

                    // Convert to prost_types::Struct via JSON
                    if let Ok(val) = serde_json::to_value(&enriched) {
                        if let Some(prost_struct) = convert::json_to_prost_struct(&val) {
                            agent_cards.push(prost_struct);
                        }
                    }
                }
            }
        }

        // --- Network discovery ---
        if scope == DiscoveryScope::Network && !req.urls.is_empty() {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default();

            for url in &req.urls {
                let card_url = format!("{}/.well-known/agent-card.json", url.trim_end_matches('/'));
                match client.get(&card_url).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            if let Ok(json) = resp.json::<serde_json::Value>().await {
                                // Capability filtering for network cards
                                if !req.capability.is_empty() {
                                    let matches = json
                                        .get("skills")
                                        .and_then(|s| s.as_array())
                                        .map(|skills| {
                                            skills.iter().any(|skill| {
                                                skill
                                                    .get("tags")
                                                    .and_then(|t| t.as_array())
                                                    .map(|tags| {
                                                        tags.iter().any(|tag| {
                                                            tag.as_str() == Some(&req.capability)
                                                        })
                                                    })
                                                    .unwrap_or(false)
                                            })
                                        })
                                        .unwrap_or(false);
                                    if !matches {
                                        continue;
                                    }
                                }

                                if let Some(prost_struct) = convert::json_to_prost_struct(&json) {
                                    agent_cards.push(prost_struct);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Skip unreachable URLs silently
                    }
                }
            }
        }

        Ok(Response::new(DiscoverAgentsResponse { agent_cards }))
    }

    type WatchAgentStream = Pin<
        Box<dyn tokio_stream::Stream<Item = Result<AgentEvent, Status>> + Send>,
    >;

    async fn watch_agent(
        &self,
        request: Request<WatchAgentRequest>,
    ) -> Result<Response<Self::WatchAgentStream>, Status> {
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

        let ws = store.workspace(ws_name).ok_or_else(|| {
            Status::internal("workspace not found for agent")
        })?;

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

        // Send initial event with current status
        let initial_event = AgentEvent {
            event_type: AgentEventType::StatusChanged as i32,
            agent: Some(convert::agent_instance_to_proto(agent)),
            agent_card: None,
        };

        let stream = tokio_stream::once(Ok(initial_event));
        Ok(Response::new(Box::pin(stream)))
    }

    type WatchWorkspaceStream = Pin<
        Box<dyn tokio_stream::Stream<Item = Result<WorkspaceEvent, Status>> + Send>,
    >;

    async fn watch_workspace(
        &self,
        request: Request<WatchWorkspaceRequest>,
    ) -> Result<Response<Self::WatchWorkspaceStream>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        if req.workspace_name.is_empty() {
            return Err(Status::invalid_argument("workspace_name is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let ws = store.workspace(&req.workspace_name).ok_or_else(|| {
            Status::not_found(format!("workspace '{}' not found", req.workspace_name))
        })?;

        // Check scope
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied("token scope mismatch"));
        }

        // Send initial events for each existing agent in the workspace
        let events: Vec<Result<WorkspaceEvent, Status>> = ws
            .agents
            .iter()
            .filter(|agent| {
                // Session check
                if token.permission == auth::Permission::Session {
                    auth::session_matches(&token, agent)
                } else {
                    true
                }
            })
            .map(|agent| {
                Ok(WorkspaceEvent {
                    event_type: WorkspaceEventType::AgentStatusChanged as i32,
                    workspace: Some(convert::workspace_to_proto(&req.workspace_name, ws)),
                    agent: Some(convert::agent_instance_to_proto(agent)),
                })
            })
            .collect();

        let stream = tokio_stream::iter(events);
        Ok(Response::new(Box::pin(stream)))
    }
}
