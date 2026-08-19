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

/// Profiles a create form may offer for `project_id`.
///
/// Empty `project_id` yields no profiles — the field stays disabled until a
/// Project is chosen. A profile with empty `project_ids` is global.
pub fn eligible_for_project<'a>(
    profiles: &'a [WorkProfile],
    project_id: &str,
) -> Vec<&'a WorkProfile> {
    if project_id.is_empty() {
        return Vec::new();
    }
    profiles
        .iter()
        .filter(|p| p.applies_to(project_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, projects: &[&str]) -> WorkProfile {
        WorkProfile {
            version: "1".into(),
            work_profile_id: id.into(),
            display_name: Some(id.into()),
            description: None,
            project_ids: projects.iter().map(|s| (*s).to_string()).collect(),
            intended_resources: vec![],
            default_policy: None,
        }
    }

    #[test]
    fn no_project_yields_no_profiles() {
        let all = vec![profile("default", &[]), profile("curri-only", &["curri"])];
        assert!(eligible_for_project(&all, "").is_empty());
    }

    #[test]
    fn global_and_matching_project_profiles() {
        let all = vec![
            profile("default", &[]),
            profile("curri-only", &["curri"]),
            profile("other-only", &["other"]),
        ];
        let ids: Vec<&str> = eligible_for_project(&all, "curri")
            .iter()
            .map(|p| p.work_profile_id.as_str())
            .collect();
        assert_eq!(ids, vec!["default", "curri-only"]);
        assert!(!ids.contains(&"other-only"));
    }

    #[test]
    fn foreign_project_profile_excluded() {
        let all = vec![profile("other-only", &["other"])];
        assert!(eligible_for_project(&all, "curri").is_empty());
    }
}
