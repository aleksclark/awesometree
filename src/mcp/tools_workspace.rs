use crate::interop;
use crate::mcp::ArpServer;
use crate::state;
use crate::workspace;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::tool;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct WorkspaceCreateParams {
    pub name: String,
    pub project: String,
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct WorkspaceListParams {
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct WorkspaceGetParams {
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct WorkspaceDestroyParams {
    pub name: String,
    #[serde(default)]
    pub keep_worktree: Option<bool>,
}

#[rmcp::tool_router(router = tool_router_workspace, vis = "pub")]
impl ArpServer {
    #[tool(name = "workspace/create", description = "Create a new isolated workspace for a project. Creates a git worktree.")]
    pub fn workspace_create(
        &self,
        Parameters(params): Parameters<WorkspaceCreateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let project = interop::load(&params.project)
            .map_err(|e| ErrorData::invalid_params(e, None))?;
        let dir = workspace::resolve_dir(&params.name, &project);
        workspace::ensure_worktree(&params.name, &project, &dir)
            .map_err(|e| ErrorData::internal_error(e, None))?;

        let mut st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let tag_idx = st.allocate_tag_index(&params.name);
        let acp_port = st.allocate_acp_port(&params.name);
        let dir_str = dir.to_string_lossy().into_owned();
        let acp_url = project.resolved_acp_url(&dir_str, acp_port);
        st.set_active(&params.name, &params.project, tag_idx, &dir_str, acp_port, acp_url);
        state::save(&st).map_err(|e| ErrorData::internal_error(e, None))?;

        let ws = st.workspace(&params.name).unwrap();
        let json = serde_json::to_string_pretty(&ws)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(name = "workspace/list", description = "List all workspaces with agent status. Filter by project or status.")]
    pub fn workspace_list(
        &self,
        Parameters(params): Parameters<WorkspaceListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let mut results: Vec<serde_json::Value> = Vec::new();
        for (name, ws) in &st.workspaces {
            if let Some(ref proj) = params.project {
                if &ws.project != proj {
                    continue;
                }
            }
            if let Some(ref status) = params.status {
                let active = ws.active;
                match status.as_str() {
                    "active" if !active => continue,
                    "inactive" if active => continue,
                    _ => {}
                }
            }
            results.push(serde_json::json!({
                "name": name,
                "project": ws.project,
                "dir": ws.dir,
                "status": if ws.active { "active" } else { "inactive" },
                "agents": ws.agents,
            }));
        }
        results.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(name = "workspace/get", description = "Get full details of a workspace including all agent instances and their status.")]
    pub fn workspace_get(
        &self,
        Parameters(params): Parameters<WorkspaceGetParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let ws = st.workspace(&params.name).ok_or_else(|| {
            ErrorData::invalid_params(format!("workspace not found: {}", params.name), None)
        })?;
        let result = serde_json::json!({
            "name": params.name,
            "project": ws.project,
            "dir": ws.dir,
            "status": if ws.active { "active" } else { "inactive" },
            "agents": ws.agents,
        });
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(name = "workspace/destroy", description = "Destroy a workspace. Stops all agents, removes the git worktree, and cleans up all state.")]
    pub fn workspace_destroy(
        &self,
        Parameters(params): Parameters<WorkspaceDestroyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let keep = params.keep_worktree.unwrap_or(false);

        let mut st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
        let ws = st.workspace(&params.name).ok_or_else(|| {
            ErrorData::invalid_params(format!("workspace not found: {}", params.name), None)
        })?;

        for agent in &ws.agents {
            crate::agent_supervisor::stop_agent(&agent.id);
        }

        let dir = ws.dir.clone();
        let project_name = ws.project.clone();
        st.remove(&params.name);
        state::save(&st).map_err(|e| ErrorData::internal_error(e, None))?;

        if !keep && !dir.is_empty() {
            if let Ok(project) = interop::load(&project_name) {
                let _ = workspace::remove_worktree(&project, &std::path::PathBuf::from(&dir));
            }
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Destroyed workspace: {}",
            params.name
        ))]))
    }
}
