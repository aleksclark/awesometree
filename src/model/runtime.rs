use crate::model::resource::{ResourceBinding, WorkspaceResourceRef};
use serde::{Deserialize, Serialize};

/// Host-local realization status (not authoritative WorkSession lifecycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RealizationStatus {
    #[default]
    Pending,
    Ready,
    Degraded,
    Failed,
    Cleaned,
}

impl RealizationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
            Self::Cleaned => "cleaned",
        }
    }
}

impl std::fmt::Display for RealizationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Host-local runtime realization keyed by work_session_id.
///
/// Must NOT store Project/WorkProfile/WorkSession definitions or lifecycle truth.
/// May hold foreign keys only as needed to query Switchboard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WorkSessionRuntime {
    pub work_session_id: String,
    /// Stable host identity for multi-host reconciliation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceResourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_binding: Option<ResourceBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_name: Option<String>,
    #[serde(default)]
    pub headless: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bezalel_port: Option<u16>,
    /// Secret reference only — raw bearer never belongs in Switchboard records.
    /// Host-local store may hold the token under a separate secrets map; this field
    /// is a reference key (e.g. "bezalel:{work_session_id}").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bezalel_token_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub process_ids: Vec<u32>,
    #[serde(default)]
    pub realization_status: RealizationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Host-local secrets store entry (never serialized to Switchboard).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeSecrets {
    /// work_session_id → bezalel bearer token
    #[serde(default)]
    pub bezalel_tokens: std::collections::HashMap<String, String>,
}
