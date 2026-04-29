use crate::auth::{Permission, scope_includes_project};
use crate::interop;
use crate::mcp::{caller_token, ArpServer};
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
        let token = caller_token();
        let projects = interop::list().map_err(|e| ErrorData::internal_error(e, None))?;

        // Filter projects by token scope
        let filtered: Vec<_> = projects
            .into_iter()
            .filter(|p| scope_includes_project(&token.scope, &p.name))
            .collect();

        let json = serde_json::to_string_pretty(&filtered)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(name = "project/register", description = "Register a project repository. Defines what agents are available and how they're configured.")]
    pub fn project_register(
        &self,
        Parameters(params): Parameters<ProjectRegisterParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();

        // project/register requires admin permission
        if !crate::auth::permission_allows(&token.permission, &Permission::Admin) {
            return Err(ErrorData::invalid_params(
                "project/register requires admin permission",
                None,
            ));
        }
        // Scope must include the project being registered
        if !scope_includes_project(&token.scope, &params.name) {
            return Err(ErrorData::invalid_params(
                format!("token scope does not include project: {}", params.name),
                None,
            ));
        }

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
        let token = caller_token();

        // project/unregister requires admin permission
        if !crate::auth::permission_allows(&token.permission, &Permission::Admin) {
            return Err(ErrorData::invalid_params(
                "project/unregister requires admin permission",
                None,
            ));
        }
        // Scope must include the project being unregistered
        if !scope_includes_project(&token.scope, &params.name) {
            return Err(ErrorData::invalid_params(
                format!("token scope does not include project: {}", params.name),
                None,
            ));
        }

        interop::delete(&params.name).map_err(|e| ErrorData::internal_error(e, None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Unregistered project: {}",
            params.name
        ))]))
    }
}
