use serde::{Deserialize, Serialize};

/// Independently identified material Workspace Resource (git worktree/environment).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceResourceRef {
    pub workspace_id: String,
    pub resource_id: String,
    /// e.g. "git-worktree"
    pub environment_kind: String,
    /// Non-secret resolved filesystem locator.
    pub path: String,
}

/// WorkSession-owned binding of a Resource into the episode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceBinding {
    pub work_session_id: String,
    pub resource_id: String,
    /// Non-secret resolved locator (path/URI).
    pub locator: String,
    /// Grant that narrows session policy (never credentials).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<serde_json::Value>,
}
