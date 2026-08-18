//! WorkSession realization façade. Authoritative Project/WorkProfile/WorkSession
//! CRUD lives in Switchboard; these tools call the shared application service.

use crate::auth::{Permission, scope_includes_project};
use crate::mcp::{caller_token, check_project_access, ArpServer};
use crate::model::lifecycle::WorkSessionState;
use crate::model::work_session::{CreateWorkSessionRequest, RealizationOptions};
use crate::service_access;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::tool;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct WorkSessionCreateParams {
    pub work_session_id: String,
    pub project_id: String,
    #[serde(default)]
    pub work_profile_id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub headless: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct WorkSessionListParams {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct WorkSessionGetParams {
    pub work_session_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct WorkSessionDestroyParams {
    pub work_session_id: String,
    #[serde(default)]
    pub keep_worktree: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct WorkSessionTransitionParams {
    pub work_session_id: String,
    pub state: String,
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

#[rmcp::tool_router(router = tool_router_workspace, vis = "pub")]
impl ArpServer {
    #[tool(
        name = "work_session/create",
        description = "Create a WorkSession via Switchboard and realize a local Workspace (git worktree). Omitting work_profile_id resolves exact ID default."
    )]
    pub fn work_session_create(
        &self,
        Parameters(params): Parameters<WorkSessionCreateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();
        check_project_access(&token, &params.project_id, &Permission::Session)?;

        let headless = params.headless.unwrap_or(false);
        let req = CreateWorkSessionRequest {
            work_session_id: params.work_session_id,
            project_id: params.project_id,
            work_profile_id: params.work_profile_id,
            display_name: params.display_name,
            realization: RealizationOptions {
                create_tag: !headless,
                launch_apps: !headless,
                headless,
                no_wm: headless,
            },
        };
        let svc = service_access::service_blocking();
        let resp = block_on_svc(svc.create_work_session(req))?;
        let json = serde_json::to_string_pretty(&resp)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "work_session/list",
        description = "List WorkSessions from Switchboard with local realization status."
    )]
    pub fn work_session_list(
        &self,
        Parameters(params): Parameters<WorkSessionListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();
        let svc = service_access::service_blocking();
        let list = block_on_svc(
            svc.list_work_sessions(params.state.as_deref(), params.project_id.as_deref()),
        )?;
        let filtered: Vec<_> = list
            .into_iter()
            .filter(|v| {
                let pid = v.work_session.project_id.as_deref().unwrap_or("");
                scope_includes_project(&token.scope, pid)
            })
            .collect();
        let json = serde_json::to_string_pretty(&filtered)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "work_session/get",
        description = "Get a WorkSession by work_session_id."
    )]
    pub fn work_session_get(
        &self,
        Parameters(params): Parameters<WorkSessionGetParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();
        let svc = service_access::service_blocking();
        let view = block_on_svc(svc.get_work_session(&params.work_session_id))?;
        if let Some(pid) = &view.work_session.project_id {
            check_project_access(&token, pid, &Permission::Session)?;
        }
        let json = serde_json::to_string_pretty(&view)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "work_session/transition",
        description = "Transition WorkSession lifecycle (open|paused|closed|aborted)."
    )]
    pub fn work_session_transition(
        &self,
        Parameters(params): Parameters<WorkSessionTransitionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();
        let state = WorkSessionState::parse(&params.state).ok_or_else(|| {
            ErrorData::invalid_params(format!("invalid state {}", params.state), None)
        })?;
        let svc = service_access::service_blocking();
        // Authz before side effects
        let current = block_on_svc(svc.get_work_session(&params.work_session_id))?;
        if let Some(pid) = &current.work_session.project_id {
            check_project_access(&token, pid, &Permission::Session)?;
        }
        let view = block_on_svc(svc.transition(&params.work_session_id, state))?;
        let json = serde_json::to_string_pretty(&view)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "work_session/destroy",
        description = "Close and delete a WorkSession; tear down local Workspace realization."
    )]
    pub fn work_session_destroy(
        &self,
        Parameters(params): Parameters<WorkSessionDestroyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let token = caller_token();
        let svc = service_access::service_blocking();
        if let Ok(view) = block_on_svc(svc.get_work_session(&params.work_session_id)) {
            if let Some(pid) = &view.work_session.project_id {
                check_project_access(&token, pid, &Permission::Session)?;
            }
        }
        block_on_svc(svc.destroy(
            &params.work_session_id,
            params.keep_worktree.unwrap_or(false),
        ))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "destroyed {}",
            params.work_session_id
        ))]))
    }

    #[tool(
        name = "work_profile/list",
        description = "List WorkProfiles from Switchboard (including exact-ID default)."
    )]
    pub fn work_profile_list(&self) -> Result<CallToolResult, ErrorData> {
        let svc = service_access::service_blocking();
        let list = block_on_svc(svc.list_work_profiles())?;
        let json = serde_json::to_string_pretty(&list)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}
