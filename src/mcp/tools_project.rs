use crate::interop;
use crate::mcp::ArpServer;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::tool;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct ProjectRegisterParams {
    pub name: String,
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ProjectUnregisterParams {
    pub name: String,
}

#[rmcp::tool_router(router = tool_router_project, vis = "pub")]
impl ArpServer {
    #[tool(name = "project/list", description = "List all registered projects with their agent templates.")]
    pub fn project_list(&self) -> Result<CallToolResult, ErrorData> {
        let projects = interop::list().map_err(|e| ErrorData::internal_error(e, None))?;
        let json = serde_json::to_string_pretty(&projects)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(name = "project/register", description = "Register a project repository. Defines what agents are available and how they're configured.")]
    pub fn project_register(
        &self,
        Parameters(params): Parameters<ProjectRegisterParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let branch = params.branch.unwrap_or_else(|| "main".into());
        let project = interop::Project::new(&params.name, &params.repo, &branch);
        interop::save(&project).map_err(|e| ErrorData::internal_error(e, None))?;
        let json = serde_json::to_string_pretty(&project)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(name = "project/unregister", description = "Unregister a project. Running workspaces for this project are not affected.")]
    pub fn project_unregister(
        &self,
        Parameters(params): Parameters<ProjectUnregisterParams>,
    ) -> Result<CallToolResult, ErrorData> {
        interop::delete(&params.name).map_err(|e| ErrorData::internal_error(e, None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Unregistered project: {}",
            params.name
        ))]))
    }
}
