//! Agent Work Model contracts shared by all awesometree transports.
//!
//! Terminology mapping (canonical AWM → awesometree):
//! - Switchboard `Project` + immutable revision/snapshot ← former `interop::Project`
//! - `WorkSession` ← former workspace-as-episode
//! - material `Workspace` Resource ← git worktree / runtime environment
//! - agent process records remain agent instances (not WorkSession/Workspace)

pub mod error;
pub mod lifecycle;
pub mod policy;
pub mod project;
pub mod resource;
pub mod runtime;
pub mod work_profile;
pub mod work_session;

pub use error::{ErrorCode, SwitchboardError};
pub use lifecycle::WorkSessionState;
pub use project::{ProjectEnvelope, ProjectSnapshotRef, ProjectSummary};
pub use resource::{ResourceBinding, WorkspaceResourceRef};
pub use runtime::{RealizationStatus, WorkSessionRuntime};
pub use work_profile::{eligible_for_project, WorkProfile};
pub use work_session::{
    CreateWorkSessionRequest, CreateWorkSessionResponse, RealizationOptions, WorkSession,
    DEFAULT_WORK_PROFILE_ID,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn work_session_state_roundtrip() {
        for s in [
            WorkSessionState::Proposed,
            WorkSessionState::Open,
            WorkSessionState::Paused,
            WorkSessionState::Closed,
            WorkSessionState::Aborted,
        ] {
            let v = serde_json::to_value(s).unwrap();
            let back: WorkSessionState = serde_json::from_value(v).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn create_request_defaults_profile_omission() {
        let req: CreateWorkSessionRequest = serde_json::from_value(json!({
            "work_session_id": "ws-1",
            "project_id": "demo"
        }))
        .unwrap();
        assert!(req.work_profile_id.is_none());
        assert_eq!(req.resolved_work_profile_id(), DEFAULT_WORK_PROFILE_ID);
    }

    #[test]
    fn create_request_explicit_profile() {
        let req = CreateWorkSessionRequest {
            work_session_id: "ws-1".into(),
            project_id: "demo".into(),
            work_profile_id: Some("review".into()),
            display_name: None,
            realization: RealizationOptions::default(),
        };
        assert_eq!(req.resolved_work_profile_id(), "review");
    }

    #[test]
    fn lifecycle_transitions() {
        assert!(WorkSessionState::Proposed.can_transition_to(WorkSessionState::Open));
        assert!(WorkSessionState::Proposed.can_transition_to(WorkSessionState::Aborted));
        assert!(!WorkSessionState::Closed.can_transition_to(WorkSessionState::Open));
        assert!(!WorkSessionState::Aborted.can_transition_to(WorkSessionState::Open));
        assert!(WorkSessionState::Open.can_transition_to(WorkSessionState::Paused));
        assert!(WorkSessionState::Paused.can_transition_to(WorkSessionState::Open));
    }

    #[test]
    fn policy_narrowing() {
        let parent = json!({"network": false, "write": true});
        let ok_child = json!({"write": false});
        assert!(policy::policy_narrows(&parent, &ok_child).is_ok());
        let bad = json!({"network": true});
        assert!(policy::policy_narrows(&parent, &bad).is_err());
    }

    #[test]
    fn switchboard_error_serde() {
        let e = SwitchboardError {
            code: ErrorCode::MissingDefaultProfile,
            message: "default WorkProfile is missing".into(),
            operation: Some("create_work_session".into()),
            entity_kind: Some("work_profile".into()),
            entity_id: Some("default".into()),
            project_id: None,
            work_profile_id: Some("default".into()),
            work_session_id: None,
            expected: None,
            current: None,
            cause: None,
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "missing_default_profile");
        let back: SwitchboardError = serde_json::from_value(v).unwrap();
        assert_eq!(back.code, ErrorCode::MissingDefaultProfile);
    }

    #[test]
    fn runtime_excludes_lifecycle_truth() {
        // WorkSessionRuntime must not carry authoritative lifecycle; only realization.
        let rt = WorkSessionRuntime {
            work_session_id: "ws".into(),
            host_id: "host-1".into(),
            workspace: Some(WorkspaceResourceRef {
                workspace_id: "ws-res".into(),
                resource_id: "res-1".into(),
                environment_kind: "git-worktree".into(),
                path: "/tmp/wt".into(),
            }),
            resource_binding: None,
            tag_index: Some(11),
            tag_name: Some("demo/ws".into()),
            headless: false,
            bezalel_port: None,
            bezalel_token_ref: None,
            process_ids: vec![],
            realization_status: RealizationStatus::Ready,
            last_error: None,
        };
        let s = serde_json::to_string(&rt).unwrap();
        assert!(!s.contains("\"state\""));
        assert!(!s.contains("project_revision"));
    }
}
