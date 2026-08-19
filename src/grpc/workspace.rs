//! gRPC WorkSessionService implementation (Switchboard-backed).

use crate::grpc::arp_proto::work_session_service_server::WorkSessionService;
use crate::grpc::arp_proto::{self, *};
use crate::grpc::convert::work_session_view_to_proto;
use crate::model::lifecycle::WorkSessionState;
use crate::model::work_session::{CreateWorkSessionRequest as AppCreate, RealizationOptions};
use crate::service_access;
use tonic::{Request, Response, Status};

#[derive(Default, Clone, Copy)]
pub struct WorkSessionServiceImpl;

/// Backward-compat alias.
pub type WorkspaceServiceImpl = WorkSessionServiceImpl;

impl WorkSessionServiceImpl {
    pub fn new() -> Self {
        Self
    }
}

fn map_proto_state(s: i32) -> Result<WorkSessionState, Status> {
    use arp_proto::WorkSessionState as Ps;
    match Ps::try_from(s) {
        Ok(Ps::Proposed) => Ok(WorkSessionState::Proposed),
        Ok(Ps::Open) => Ok(WorkSessionState::Open),
        Ok(Ps::Paused) => Ok(WorkSessionState::Paused),
        Ok(Ps::Closed) => Ok(WorkSessionState::Closed),
        Ok(Ps::Aborted) => Ok(WorkSessionState::Aborted),
        _ => Err(Status::invalid_argument(format!("invalid state: {s}"))),
    }
}

#[tonic::async_trait]
impl WorkSessionService for WorkSessionServiceImpl {
    async fn create_work_session(
        &self,
        request: Request<CreateWorkSessionRequest>,
    ) -> Result<Response<WorkSession>, Status> {
        let req = request.into_inner();
        let svc = service_access::service().await;
        let profile = if req.work_profile_id.is_empty() {
            None
        } else {
            Some(req.work_profile_id)
        };
        let resp = svc
            .create_work_session(AppCreate {
                work_session_id: req.work_session_id,
                project_id: req.project_id,
                work_profile_id: profile,
                display_name: if req.display_name.is_empty() {
                    None
                } else {
                    Some(req.display_name)
                },
                realization: RealizationOptions {
                    create_tag: !req.headless,
                    launch_apps: !req.headless,
                    headless: req.headless,
                    no_wm: req.headless,
                },
            })
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        let view = crate::model::work_session::WorkSessionView {
            work_session: resp.work_session,
            runtime: resp.runtime,
        };
        Ok(Response::new(work_session_view_to_proto(&view)))
    }

    async fn list_work_sessions(
        &self,
        request: Request<ListWorkSessionsRequest>,
    ) -> Result<Response<ListWorkSessionsResponse>, Status> {
        let req = request.into_inner();
        let svc = service_access::service().await;
        let project = if req.project_id.is_empty() {
            None
        } else {
            Some(req.project_id.as_str())
        };
        let list = svc
            .list_work_sessions(None, project)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(ListWorkSessionsResponse {
            work_sessions: list.iter().map(work_session_view_to_proto).collect(),
        }))
    }

    async fn get_work_session(
        &self,
        request: Request<GetWorkSessionRequest>,
    ) -> Result<Response<WorkSession>, Status> {
        let id = request.into_inner().work_session_id;
        let svc = service_access::service().await;
        let v = svc
            .get_work_session(&id)
            .await
            .map_err(|e| Status::not_found(e.to_string()))?;
        Ok(Response::new(work_session_view_to_proto(&v)))
    }

    async fn transition_work_session(
        &self,
        request: Request<TransitionWorkSessionRequest>,
    ) -> Result<Response<WorkSession>, Status> {
        let req = request.into_inner();
        let to = map_proto_state(req.state)?;
        let svc = service_access::service().await;
        let v = svc
            .transition(&req.work_session_id, to)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        Ok(Response::new(work_session_view_to_proto(&v)))
    }

    async fn destroy_work_session(
        &self,
        request: Request<DestroyWorkSessionRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        let svc = service_access::service().await;
        svc.destroy(&req.work_session_id, req.keep_worktree)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        Ok(Response::new(()))
    }

    async fn list_work_profiles(
        &self,
        _request: Request<ListWorkProfilesRequest>,
    ) -> Result<Response<ListWorkProfilesResponse>, Status> {
        let svc = service_access::service().await;
        let list = svc
            .list_work_profiles()
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(ListWorkProfilesResponse {
            work_profiles: list
                .into_iter()
                .map(|p| WorkProfile {
                    work_profile_id: p.work_profile_id,
                    display_name: p.display_name.unwrap_or_default(),
                    description: p.description.unwrap_or_default(),
                })
                .collect(),
        }))
    }
}
