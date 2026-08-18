//! gRPC ProjectService backed by Switchboard.

use crate::grpc::arp_proto::project_service_server::ProjectService;
use crate::grpc::arp_proto::*;
use crate::model::project::definition_for_create;
use crate::service_access;
use tonic::{Request, Response, Status};

pub struct ProjectServiceImpl;

impl ProjectServiceImpl {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProjectServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[tonic::async_trait]
impl ProjectService for ProjectServiceImpl {
    async fn list_projects(
        &self,
        _request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        let svc = service_access::service().await;
        let list = svc
            .list_projects(None)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        let projects = list
            .into_iter()
            .map(|p| Project {
                name: p.project_id,
                repo: String::new(),
                branch: String::new(),
                agents: vec![],
                context: None,
            })
            .collect();
        Ok(Response::new(ListProjectsResponse { projects }))
    }

    async fn register_project(
        &self,
        request: Request<RegisterProjectRequest>,
    ) -> Result<Response<Project>, Status> {
        let req = request.into_inner();
        let svc = service_access::service().await;
        let def = definition_for_create(
            &req.name,
            None,
            if req.repo.is_empty() {
                None
            } else {
                Some(&req.repo)
            },
            if req.branch.is_empty() {
                None
            } else {
                Some(&req.branch)
            },
            None,
        );
        let sum = svc
            .create_project(def)
            .await
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        Ok(Response::new(Project {
            name: sum.project_id,
            repo: req.repo,
            branch: req.branch,
            agents: req.agents,
            context: None,
        }))
    }

    async fn unregister_project(
        &self,
        request: Request<UnregisterProjectRequest>,
    ) -> Result<Response<()>, Status> {
        let name = request.into_inner().name;
        let svc = service_access::service().await;
        let env = svc
            .get_project(&name)
            .await
            .map_err(|e| Status::not_found(e.to_string()))?;
        svc.delete_project(&name, &env.source_revision)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        Ok(Response::new(()))
    }
}
