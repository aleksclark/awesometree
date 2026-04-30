pub mod tools_project;
pub mod tools_workspace;
pub mod tools_agent;
pub mod tools_discovery;
pub mod prompts;
pub mod resources;

use crate::auth::{self, ScopedToken, Permission, scope_includes_project, permission_allows, session_matches};
use crate::state::AgentInstanceState;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler, ServiceExt};

// ---------------------------------------------------------------------------
// Scope enforcement helpers for MCP tools
// ---------------------------------------------------------------------------

/// Returns the caller's ScopedToken for MCP tool calls.
///
/// MCP is stdio-based so there's no HTTP request context with a bearer token.
/// For now all MCP calls are treated as local/admin. This is a placeholder
/// until MCP auth context (e.g. session tokens negotiated during initialize)
/// is available.
pub(crate) fn caller_token() -> ScopedToken {
    auth::localhost_admin_token()
}

/// Check whether `token` is allowed to access a specific agent in `project`.
///
/// Returns `Ok(())` if:
///   1. The token scope includes the project, AND
///   2. For session-scoped tokens, the agent's session_id matches the token's.
///
/// Returns an appropriate `ErrorData` on failure.
pub(crate) fn check_agent_access(
    token: &ScopedToken,
    agent: &AgentInstanceState,
    project: &str,
) -> Result<(), ErrorData> {
    if !scope_includes_project(&token.scope, project) {
        return Err(ErrorData::invalid_params(
            format!("token scope does not include project: {project}"),
            None,
        ));
    }
    if !session_matches(token, agent) {
        return Err(ErrorData::invalid_params(
            format!(
                "session-scoped token cannot access agent {} (session mismatch)",
                agent.id
            ),
            None,
        ));
    }
    Ok(())
}

/// Check whether `token` has the required permission and scope for a project.
///
/// Returns `Ok(())` if:
///   1. The token permission is >= `required`, AND
///   2. The token scope includes `project`.
pub(crate) fn check_project_access(
    token: &ScopedToken,
    project: &str,
    required: &Permission,
) -> Result<(), ErrorData> {
    if !permission_allows(&token.permission, required) {
        return Err(ErrorData::invalid_params(
            format!(
                "insufficient permission: {:?} required, have {:?}",
                required, token.permission
            ),
            None,
        ));
    }
    if !scope_includes_project(&token.scope, project) {
        return Err(ErrorData::invalid_params(
            format!("token scope does not include project: {project}"),
            None,
        ));
    }
    Ok(())
}

#[derive(Clone)]
pub struct ArpServer {
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl Default for ArpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl ArpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router_project()
                + Self::tool_router_workspace()
                + Self::tool_router_agent()
                + Self::tool_router_discovery(),
            prompt_router: Self::prompt_router(),
        }
    }
}

#[rmcp::tool_handler(
    router = self.tool_router,
    name = "awesometree-arp",
    version = "0.3.0",
    instructions = "Agent Registry Protocol (ARP) MCP server. Manages A2A agent lifecycle within workspaces."
)]
#[rmcp::prompt_handler(router = self.prompt_router)]
impl ServerHandler for ArpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .enable_resources_subscribe()
                .build(),
        )
        .with_server_info(
            Implementation::new("awesometree-arp", "0.3.0"),
        )
        .with_instructions(
            "Agent Registry Protocol (ARP) — manages A2A agent lifecycle within workspaces. \
             Use project/ tools to register repos, workspace/ tools to create isolated environments, \
             and agent/ tools to spawn, message, and manage A2A agents.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        resources::list_resources()
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        resources::read_resource(&request.uri)
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        resources::list_resource_templates()
    }
}

pub async fn run_stdio() -> anyhow::Result<()> {
    let server = ArpServer::new();
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
