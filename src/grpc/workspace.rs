//! gRPC WorkSessionService implementation (AWM WorkSession).

use crate::auth;
use crate::grpc::arp_proto::work_session_service_server::WorkSessionService;
use crate::grpc::arp_proto::{self, *};
use crate::grpc::extract_token;
use crate::model::lifecycle::WorkSessionState as ModelState;
use crate::model::work_session::RealizationOptions;
use crate::service_access;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct WorkSessionServiceImpl;

fn map_err(e: crate::model::SwitchboardError) -> Status {
    match e.code {
        crate::model::ErrorCode::NotFound | crate::model::ErrorCode::MissingDefaultProfile => {
            Status::not_found(e.to_string())
        }
        crate::model::ErrorCode::AlreadyExists | crate::model::ErrorCode::Conflict => {
            Status::already_exists(e.to_string())
        }
        crate::model::ErrorCode::InvalidInput
        | crate::model::ErrorCode::InvalidReference
        | crate::model::ErrorCode::InvalidTransition
        | crate::model::ErrorCode::PolicyBroadening
        | crate::model::ErrorCode::Referenced => Status::invalid_argument(e.to_string()),
        crate::model::ErrorCode::Unauthorized => Status::permission_denied(e.to_string()),
        crate::model::ErrorCode::Unavailable => Status::unavailable(e.to_string()),
        _ => Status::internal(e.to_string()),
    }
}

fn to_proto_state(s: ModelState) -> i32 {
    match s {
        ModelState::Proposed => WorkSessionState::Proposed as i32,
        ModelState::Open => WorkSessionState::Open as i32,
        ModelState::Paused => WorkSessionState::Paused as i32,
        ModelState::Closed => WorkSessionState::Closed as i32,
        ModelState::Aborted => WorkSessionState::Aborted as i32,
    }
}

fn from_proto_state(s: i32) -> Result<ModelState, Status> {
    match WorkSessionState::try_from(s) {
        Ok(WorkSessionState::Proposed) => Ok(ModelState::Proposed),
        Ok(WorkSessionState::Open) => Ok(ModelState::Open),
        Ok(WorkSessionState::Paused) => Ok(ModelState::Paused),
        Ok(WorkSessionState::Closed) => Ok(ModelState::Closed),
        Ok(WorkSessionState::Aborted) => Ok(ModelState::Aborted),
        _ => Err(Status::invalid_argument("invalid work session state")),
    }
}

fn view_to_proto(view: crate::model::work_session::WorkSessionView) -> WorkSession {
    let path = view
        .runtime
        .as_ref()
        .and_then(|r| r.workspace.as_ref())
        .map(|w| w.path.clone())
        .unwrap_or_default();
    let realization = view
        .runtime
        .as_ref()
        .map(|r| r.realization_status.as_str().to_string())
        .unwrap_or_default();
    WorkSession {
        work_session_id: view.work_session.work_session_id,
        project_id: view.work_session.project_id.unwrap_or_default(),
        work_profile_id: view.work_session.work_profile_id.unwrap_or_default(),
        project_revision: view.work_session.project_revision.unwrap_or_default(),
        project_snapshot_id: view.work_session.project_snapshot_id.unwrap_or_default(),
        state: to_proto_state(view.work_session.state),
        display_name: view.work_session.display_name.unwrap_or_default(),
        workspace_path: path,
        realization_status: realization,
        created_at: None,
    }
}

#[tonic::async_trait]
impl WorkSessionService for WorkSessionServiceImpl {
    async fn create_work_session(
        &self,
        request: Request<arp_proto::CreateWorkSessionRequest>,
    ) -> Result<Response<WorkSession>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();
        if !auth::permission_allows(&token.permission, &auth::Permission::Session) {
            return Err(Status::permission_denied("session permission required"));
        }
        if req.work_session_id.is_empty() || req.project_id.is_empty() {
            return Err(Status::invalid_argument(
                "work_session_id and project_id are required",
            ));
        }
        if !auth::scope_includes_project(&token.scope, &req.project_id) {
            return Err(Status::permission_denied("project not in scope"));
        }
        let headless = req.headless;
        let create = crate::model::work_session::CreateWorkSessionRequest {
            work_session_id: req.work_session_id,
            project_id: req.project_id,
            work_profile_id: if req.work_profile_id.is_empty() {
                None
            } else {
                Some(req.work_profile_id)
            },
            display_name: if req.display_name.is_empty() {
                None
            } else {
                Some(req.display_name)
            },
            realization: RealizationOptions {
                create_tag: !headless,
                launch_apps: !headless,
                headless,
                no_wm: headless,
            },
        };
        let svc = service_access::service().await;
        let resp = svc.create_work_session(create).await.map_err(map_err)?;
        let view = crate::model::work_session::WorkSessionView {
            work_session: resp.work_session,
            runtime: resp.runtime,
        };
        Ok(Response::new(view_to_proto(view)))
    }

    async fn list_work_sessions(
        &self,
        request: Request<ListWorkSessionsRequest>,
    ) -> Result<Response<ListWorkSessionsResponse>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();
        let state = if req.state == 0 {
            None
        } else {
            Some(from_proto_state(req.state)?.as_str().to_string())
        };
        let project = if req.project_id.is_empty() {
            None
        } else {
            Some(req.project_id)
        };
        let svc = service_access::service().await;
        let list = svc
            .list_work_sessions(state.as_deref(), project.as_deref())
            .await
            .map_err(map_err)?;
        let work_sessions = list
            .into_iter()
            .filter(|v| {
                let pid = v.work_session.project_id.as_deref().unwrap_or("");
                auth::scope_includes_project(&token.scope, pid)
            })
            .map(view_to_proto)
            .collect();
        Ok(Response::new(ListWorkSessionsResponse { work_sessions }))
    }

    async fn get_work_session(
        &self,
        request: Request<GetWorkSessionRequest>,
    ) -> Result<Response<WorkSession>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();
        let svc = service_access::service().await;
        let view = svc
            .get_work_session(&req.work_session_id)
            .await
            .map_err(map_err)?;
        if let Some(pid) = &view.work_session.project_id {
            if !auth::scope_includes_project(&token.scope, pid) {
                return Err(Status::permission_denied("project not in scope"));
            }
        }
        Ok(Response::new(view_to_proto(view)))
    }

    async fn transition_work_session(
        &self,
        request: Request<TransitionWorkSessionRequest>,
    ) -> Result<Response<WorkSession>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();
        let state = from_proto_state(req.state)?;
        let svc = service_access::service().await;
        let current = svc
            .get_work_session(&req.work_session_id)
            .await
            .map_err(map_err)?;
        if let Some(pid) = &current.work_session.project_id {
            if !auth::scope_includes_project(&token.scope, pid) {
                return Err(Status::permission_denied("project not in scope"));
            }
        }
        let view = svc
            .transition(&req.work_session_id, state)
            .await
            .map_err(map_err)?;
        Ok(Response::new(view_to_proto(view)))
    }

    async fn destroy_work_session(
        &self,
        request: Request<DestroyWorkSessionRequest>,
    ) -> Result<Response<()>, Status> {
        let token = extract_token(&request);
        let req = request.into_inner();
        let svc = service_access::service().await;
        if let Ok(view) = svc.get_work_session(&req.work_session_id).await {
            if let Some(pid) = &view.work_session.project_id {
                if !auth::scope_includes_project(&token.scope, pid) {
                    return Err(Status::permission_denied("project not in scope"));
                }
            }
        }
        svc.destroy(&req.work_session_id, req.keep_worktree)
            .await
            .map_err(map_err)?;
        Ok(Response::new(()))
    }

    async fn list_work_profiles(
        &self,
        request: Request<ListWorkProfilesRequest>,
    ) -> Result<Response<ListWorkProfilesResponse>, Status> {
        let _token = extract_token(&request);
        let svc = service_access::service().await;
        let list = svc.list_work_profiles().await.map_err(map_err)?;
        let work_profiles = list
            .into_iter()
            .map(|p| WorkProfile {
                work_profile_id: p.work_profile_id,
                display_name: p.display_name.unwrap_or_default(),
                description: p.description.unwrap_or_default(),
            })
            .collect();
        Ok(Response::new(ListWorkProfilesResponse { work_profiles }))
    }
}

// Backward-compatible type alias for module wiring.
pub type WorkspaceServiceImpl = WorkSessionServiceImpl;
