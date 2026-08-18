use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Switchboard WorkProfile (session blueprint).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkProfile {
    #[serde(default = "version_one")]
    pub version: String,
    pub work_profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intended_resources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_policy: Option<Value>,
}

fn version_one() -> String {
    "1".into()
}

impl WorkProfile {
    pub fn display(&self) -> &str {
        self.display_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.work_profile_id)
    }

    /// Globally applicable when project_ids is empty; otherwise must list the project.
    pub fn applies_to(&self, project_id: &str) -> bool {
        self.project_ids.is_empty() || self.project_ids.iter().any(|p| p == project_id)
    }
}
