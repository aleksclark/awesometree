use crate::auth::{Permission, scope_includes_project, session_matches};
use crate::mcp::{caller_token, ArpServer};
use crate::state::{self, AgentStatus};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::tool;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct AgentDiscoverParams {
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub urls: Option<Vec<String>>,
}

#[rmcp::tool_router(router = tool_router_discovery, vis = "pub")]
impl ArpServer {
    #[tool(name = "agent/discover", description = "Discover available agents. Returns AgentCards from managed workspaces (local) or by probing URLs (network).")]
    pub async fn agent_discover(
        &self,
        Parameters(params): Parameters<AgentDiscoverParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();
        let scope = params.scope.as_deref().unwrap_or("local");

        let mut cards: Vec<serde_json::Value> = Vec::new();

        if scope == "local" || scope == "all" {
            let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
            for (ws_name, ws) in &st.workspaces {
                if !ws.active {
                    continue;
                }
                // Filter by project scope
                if !scope_includes_project(&token.scope, &ws.project) {
                    continue;
                }
                for agent in &ws.agents {
                    if agent.status != AgentStatus::Ready && agent.status != AgentStatus::Busy {
                        continue;
                    }
                    // For session-scoped tokens, only show own-session agents (local discovery)
                    if token.permission == Permission::Session && !session_matches(&token, agent) {
                        continue;
                    }
                    if let Some(ref cap) = params.capability {
                        let card = crate::a2a_proxy::enriched_agent_card(agent, &ws.project);
                        let matches = card.card.skills.iter().any(|s| {
                            s.tags.iter().any(|t| t == cap)
                        });
                        if !matches {
                            continue;
                        }
                    }
                    let card = crate::a2a_proxy::enriched_agent_card(agent, &ws.project);
                    if let Ok(val) = serde_json::to_value(&card) {
                        cards.push(val);
                    }
                    let _ = ws_name;
                }
            }
        }

        if scope == "network" || scope == "all" {
            if let Some(urls) = &params.urls {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(5))
                    .build()
                    .unwrap_or_default();
                for url in urls {
                    let card_url = format!("{}/.well-known/agent-card.json", url.trim_end_matches('/'));
                    if let Ok(resp) = client.get(&card_url).send().await {
                        if resp.status().is_success() {
                            if let Ok(card) = resp.json::<serde_json::Value>().await {
                                cards.push(card);
                            }
                        }
                    }
                }
            }
        }

        let json = serde_json::to_string_pretty(&cards)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}
