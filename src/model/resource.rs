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

/// Persist a host worktree binding in Switchboard WorkSession.policy.workspace.
/// Policy is the mutable field Switchboard accepts; new keys only-narrow.
pub fn workspace_binding_policy(binding: &ResourceBinding, kind: &str) -> serde_json::Value {
    serde_json::json!({
        "workspace": {
            "resource_id": binding.resource_id,
            "kind": kind,
            "locator": binding.locator,
        }
    })
}

/// Merge a workspace binding into an existing policy object without dropping other keys.
pub fn merge_workspace_binding(
    existing: Option<&serde_json::Value>,
    binding: &ResourceBinding,
    kind: &str,
) -> serde_json::Value {
    let mut policy = match existing {
        Some(serde_json::Value::Object(_)) => existing.cloned().unwrap(),
        _ => serde_json::json!({}),
    };
    if let Some(obj) = policy.as_object_mut() {
        obj.insert(
            "workspace".into(),
            workspace_binding_policy(binding, kind)["workspace"].clone(),
        );
    }
    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_preserves_other_policy_keys() {
        let existing = serde_json::json!({"network": false});
        let binding = ResourceBinding {
            work_session_id: "ws".into(),
            resource_id: "workspace:ws".into(),
            locator: "/tmp/wt".into(),
            grant: None,
        };
        let merged = merge_workspace_binding(Some(&existing), &binding, "git-worktree");
        assert_eq!(merged["network"], false);
        assert_eq!(merged["workspace"]["locator"], "/tmp/wt");
        assert_eq!(merged["workspace"]["kind"], "git-worktree");
    }
}
