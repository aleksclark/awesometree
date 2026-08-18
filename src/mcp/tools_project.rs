use crate::auth::{Permission, scope_includes_project};
use crate::mcp::{caller_token, ArpServer};
use crate::model::project::definition_for_create;
use crate::service_access;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::tool;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct ProjectRegisterParams {
    pub project_id: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ProjectUnregisterParams {
    pub project_id: String,
    pub expected_source_revision: String,
}

fn block_on_svc<F, T>(f: F) -> Result<T, ErrorData>
where
    F: std::future::Future<Output = Result<T, crate::model::SwitchboardError>>,
{
    let result = match tokio::runtime::Handle::try_current() {
        Ok(h) => tokio::task::block_in_place(|| h.block_on(f)),
        Err(_) => {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
            rt.block_on(f)
        }
    };
    result.map_err(|e| ErrorData::invalid_params(e.to_string(), None))
}

#[rmcp::tool_router(router = tool_router_project, vis = "pub")]
impl ArpServer {
    #[tool(
        name = "project/list",
        description = "List Projects from Switchboard Project Catalog."
    )]
    pub fn project_list(&self) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();
        let svc = service_access::service_blocking();
        let projects = block_on_svc(svc.list_projects(None))?;
        let filtered: Vec<_> = projects
            .into_iter()
            .filter(|p| scope_includes_project(&token.scope, &p.project_id))
            .collect();
        let json = serde_json::to_string_pretty(&filtered)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "project/register",
        description = "Create a Project in Switchboard (authoritative catalog)."
    )]
    pub fn project_register(
        &self,
        Parameters(params): Parameters<ProjectRegisterParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();
        if !matches!(token.permission, Permission::Admin | Permission::Project) {
            return Err(ErrorData::invalid_params("admin or project permission required", None));
        }
        let def = definition_for_create(
            &params.project_id,
            params.description.as_deref(),
            params.repo.as_deref(),
            params.branch.as_deref(),
            None,
        );
        let svc = service_access::service_blocking();
        let summary = block_on_svc(svc.create_project(def))?;
        let json = serde_json::to_string_pretty(&summary)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "project/unregister",
        description = "Delete a Project from Switchboard when no WorkSession references it."
    )]
    pub fn project_unregister(
        &self,
        Parameters(params): Parameters<ProjectUnregisterParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();
        if !matches!(token.permission, Permission::Admin | Permission::Project) {
            return Err(ErrorData::invalid_params("admin or project permission required", None));
        }
        let svc = service_access::service_blocking();
        block_on_svc(svc.delete_project(&params.project_id, &params.expected_source_revision))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "deleted {}",
            params.project_id
        ))]))
    }
}
