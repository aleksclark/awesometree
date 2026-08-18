use crate::model::lifecycle::WorkSessionState;
use crate::model::runtime::{RealizationStatus, WorkSessionRuntime};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Exact WorkProfile ID used when callers omit a profile choice.
pub const DEFAULT_WORK_PROFILE_ID: &str = "default";

/// Authoritative WorkSession record (Switchboard).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkSession {
    #[serde(default = "version_one")]
    pub version: String,
    pub work_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_profile_ids: Vec<String>,
    pub state: WorkSessionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
}

fn version_one() -> String {
    "1".into()
}

/// Host-local realization inputs — not WorkSession identity.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RealizationOptions {
    #[serde(default)]
    pub create_tag: bool,
    #[serde(default)]
    pub launch_apps: bool,
    #[serde(default)]
    pub headless: bool,
    /// When true, skip window-manager operations (CI / headless hosts).
    #[serde(default)]
    pub no_wm: bool,
}

/// Public creation request shared by CLI/REST/MCP/gRPC/UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkSessionRequest {
    pub work_session_id: String,
    pub project_id: String,
    /// Omission resolves only to exact ID `default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub realization: RealizationOptions,
}

impl CreateWorkSessionRequest {
    pub fn resolved_work_profile_id(&self) -> &str {
        self.work_profile_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_WORK_PROFILE_ID)
    }
}

/// Creation response with authoritative session + safe local realization status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkSessionResponse {
    pub work_session: WorkSession,
    pub work_profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_snapshot_id: Option<String>,
    pub realization_status: RealizationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<WorkSessionRuntime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Combined view for list/detail APIs (authoritative + safe runtime facts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSessionView {
    pub work_session: WorkSession,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<WorkSessionRuntime>,
}
