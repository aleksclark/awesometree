//! gRPC ProjectService implementation.

use crate::auth;
use crate::grpc::arp_proto::*;
use crate::grpc::arp_proto::project_service_server::ProjectService;
use crate::grpc::convert;
use crate::grpc::extract_token;
use crate::interop;
use crate::state;
use tonic::{Request, Response, Status};

/// Implements the `ProjectService` gRPC trait.
#[derive(Debug, Default)]
pub struct ProjectServiceImpl;

#[tonic::async_trait]
impl ProjectService for ProjectServiceImpl {
    async fn list_projects(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        let token = extract_token(&request);

        let projects = interop::list()
            .map_err(|e| Status::internal(format!("failed to list projects: {e}")))?;

        let filtered: Vec<Project> = projects
            .iter()
            .filter(|p| auth::scope_includes_project(&token.scope, &p.name))
            .map(convert::interop_project_to_proto)
            .collect();

        Ok(Response::new(ListProjectsResponse { projects: filtered }))
    }

    async fn register_project(
        &self,
        request: Request<RegisterProjectRequest>,
    ) -> Result<Response<Project>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        // Require admin permission
        if !auth::permission_allows(&token.permission, &auth::Permission::Admin) {
            return Err(Status::permission_denied("admin permission required"));
        }

        // Check scope includes this project name
        if !auth::scope_includes_project(&token.scope, &req.name) {
            return Err(Status::permission_denied(format!(
                "token scope does not include project '{}'",
                req.name
            )));
        }

        if req.name.is_empty() {
            return Err(Status::invalid_argument("project name is required"));
        }
        if req.repo.is_empty() {
            return Err(Status::invalid_argument("repo is required"));
        }

        let branch = if req.branch.is_empty() {
            "master".to_string()
        } else {
            req.branch.clone()
        };

        let project = interop::Project::new(&req.name, &req.repo, &branch);

        interop::save(&project)
            .map_err(|e| Status::internal(format!("failed to save project: {e}")))?;

        Ok(Response::new(convert::interop_project_to_proto(&project)))
    }

    async fn unregister_project(
        &self,
        request: Request<UnregisterProjectRequest>,
    ) -> Result<Response<()>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();

        // Require admin permission
        if !auth::permission_allows(&token.permission, &auth::Permission::Admin) {
            return Err(Status::permission_denied("admin permission required"));
        }

        // Check scope includes this project name
        if !auth::scope_includes_project(&token.scope, &req.name) {
            return Err(Status::permission_denied(format!(
                "token scope does not include project '{}'",
                req.name
            )));
        }

        if req.name.is_empty() {
            return Err(Status::invalid_argument("project name is required"));
        }

        // Verify no active workspaces reference this project
        let store = state::load()
            .map_err(|e| Status::internal(format!("failed to load state: {e}")))?;

        let active_ws = store.workspaces_for_project(&req.name);
        let has_active = active_ws.iter().any(|(_, ws)| ws.active);
        if has_active {
            return Err(Status::failed_precondition(format!(
                "project '{}' has active workspaces; destroy them first",
                req.name
            )));
        }

        interop::delete(&req.name)
            .map_err(|e| Status::not_found(format!("failed to delete project: {e}")))?;

        Ok(Response::new(()))
    }
}
