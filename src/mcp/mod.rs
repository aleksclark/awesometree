pub mod tools_project;
pub mod tools_workspace;
pub mod tools_agent;
pub mod tools_discovery;
pub mod prompts;
pub mod resources;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler, ServiceExt};

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
