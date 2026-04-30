//! gRPC WorkspaceService implementation.

use crate::agent_supervisor;
use crate::auth;
use crate::grpc::arp_proto::*;
use crate::grpc::arp_proto::workspace_service_server::WorkspaceService;
use crate::grpc::convert;
use crate::grpc::extract_token;
use crate::interop;
use crate::state;
use crate::workspace;
use tonic::{Request, Response, Status};

/// Implements the `WorkspaceService` gRPC trait.
#[derive(Debug, Default)]
pub struct WorkspaceServiceImpl;

#[tonic::async_trait]
impl WorkspaceService for WorkspaceServiceImpl {
    async fn create_workspace(
        &self,
        request: Request<CreateWorkspaceRequest>,
    ) -> Result<Response<Workspace>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        // Require project-level permission
        if !auth::permission_allows(&token.permission, &auth::Permission::Project) {
            return Err(Status::permission_denied("project permission required"));
        }

        if req.name.is_empty() {
            return Err(Status::invalid_argument("workspace name is required"));
        }
        if req.project.is_empty() {
            return Err(Status::invalid_argument("project name is required"));
        }

        // Check scope includes this project
        if !auth::scope_includes_project(&token.scope, &req.project) {
            return Err(Status::permission_denied(format!(
                "token scope does not include project '{}'",
                req.project
            )));
        }

        // Load the project
        let mut project = interop::load(&req.project)
            .map_err(|e| Status::not_found(format!("project not found: {e}")))?;

        // Override branch if provided
        if !req.branch.is_empty() {
            project.branch = Some(req.branch);
        }

        // Resolve directory and ensure worktree
        let dir = workspace::resolve_dir(&req.name, &project);
        workspace::ensure_worktree(&req.name, &project, &dir)
            .map_err(|e| Status::internal(format!("failed to create worktree: {e}")))?;

        // Allocate tag and port, persist state
        let dir_str = dir.to_string_lossy().into_owned();

        let ws_state = {
            let mut store = state::load()
                .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

            let tag_idx = store.allocate_tag_index(&req.name);
            let acp_port = store.allocate_acp_port(&req.name);
            store.set_active(&req.name, &req.project, tag_idx, &dir_str, acp_port, None);

            state::save(&store)
                .map_err(|e| Status::internal(format!("failed to save state: {e}")))?;

            store.workspace(&req.name).cloned().ok_or_else(|| {
                Status::internal("workspace not found after creation")
            })?
        };

        // Auto-spawn agents if requested
        for template_name in &req.auto_agents {
            let store = state::load()
                .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

            let port = store.allocate_agent_port().ok_or_else(|| {
                Status::resource_exhausted("no available ports for agent")
            })?;

            let command = format!("{template_name} serve");
            let opts = agent_supervisor::SpawnOptions {
                workspace: req.name.clone(),
                dir: dir_str.clone(),
                template: template_name.clone(),
                name: template_name.clone(),
                port,
                command,
                env: std::collections::HashMap::new(),
            };

            if let Some(result) = agent_supervisor::spawn_agent(opts) {
                let agent = state::AgentInstanceState {
                    id: result.agent_id,
                    template: template_name.clone(),
                    name: template_name.clone(),
                    workspace: req.name.clone(),
                    status: state::AgentStatus::Starting,
                    port: result.port,
                    host: None,
                    pid: None,
                    started_at: chrono::Utc::now().to_rfc3339(),
                    token_id: None,
                    session_id: None,
                    spawned_by: Some(token.id.clone()),
                };

                state::modify(|st| {
                    st.add_agent(&req.name, agent);
                })
                .map_err(|e| Status::internal(format!("failed to persist agent: {e}")))?;
            }
        }

        // Re-load the workspace state to include any spawned agents
        let final_store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;
        let final_ws = final_store.workspace(&req.name).unwrap_or(&ws_state);

        Ok(Response::new(convert::workspace_to_proto(&req.name, final_ws)))
    }

    async fn list_workspaces(
        &self,
        request: Request<ListWorkspacesRequest>,
    ) -> Result<Response<ListWorkspacesResponse>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let workspaces: Vec<Workspace> = store
            .workspaces
            .iter()
            .filter(|(_, ws)| {
                // Filter by project scope
                if !auth::scope_includes_project(&token.scope, &ws.project) {
                    return false;
                }
                // Filter by project name if specified
                if !req.project.is_empty() && ws.project != req.project {
                    return false;
                }
                // Filter by status if specified
                if req.status != WorkspaceStatus::Unspecified as i32 {
                    let ws_active = ws.active;
                    let want_active = req.status == WorkspaceStatus::Active as i32;
                    if ws_active != want_active {
                        return false;
                    }
                }
                true
            })
            .map(|(name, ws)| convert::workspace_to_proto(name, ws))
            .collect();

        Ok(Response::new(ListWorkspacesResponse { workspaces }))
    }

    async fn get_workspace(
        &self,
        request: Request<GetWorkspaceRequest>,
    ) -> Result<Response<Workspace>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        if req.name.is_empty() {
            return Err(Status::invalid_argument("workspace name is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let ws = store.workspace(&req.name).ok_or_else(|| {
            Status::not_found(format!("workspace '{}' not found", req.name))
        })?;

        // Check scope
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied(format!(
                "token scope does not include project '{}'",
                ws.project
            )));
        }

        Ok(Response::new(convert::workspace_to_proto(&req.name, ws)))
    }

    async fn destroy_workspace(
        &self,
        request: Request<DestroyWorkspaceRequest>,
    ) -> Result<Response<()>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        // Require project-level permission
        if !auth::permission_allows(&token.permission, &auth::Permission::Project) {
            return Err(Status::permission_denied("project permission required"));
        }

        if req.name.is_empty() {
            return Err(Status::invalid_argument("workspace name is required"));
        }

        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let ws = store.workspace(&req.name).ok_or_else(|| {
            Status::not_found(format!("workspace '{}' not found", req.name))
        })?;

        // Check scope
        if !auth::scope_includes_project(&token.scope, &ws.project) {
            return Err(Status::permission_denied(format!(
                "token scope does not include project '{}'",
                ws.project
            )));
        }

        // Stop all agents in this workspace
        agent_supervisor::stop_workspace_agents(&req.name);

        // Load project for worktree removal
        let maybe_project = interop::load(&ws.project);
        let dir = std::path::PathBuf::from(&ws.dir);

        // Remove workspace from state
        state::modify(|st| {
            st.remove(&req.name);
        })
        .map_err(|e| Status::internal(format!("failed to update state: {e}")))?;

        // Optionally remove the worktree directory
        if !req.keep_worktree {
            if let Ok(project) = maybe_project {
                if let Err(e) = workspace::remove_worktree(&project, &dir) {
                    // Log but don't fail — the state is already cleaned up
                    eprintln!("warning: failed to remove worktree: {e}");
                }
            }
        }

        Ok(Response::new(()))
    }
}
