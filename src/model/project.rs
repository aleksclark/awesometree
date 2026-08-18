use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Lightweight project row from Switchboard `project_list`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub project_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
}

/// Full project envelope from Switchboard `project_get` (definition + CAS tokens).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnvelope {
    pub project_id: String,
    pub revision: String,
    pub source_revision: String,
    /// Open definition object (may include resources, launch, extensions, …).
    pub definition: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<ProjectSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

impl ProjectEnvelope {
    pub fn name(&self) -> &str {
        self.definition
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.project_id)
    }

    pub fn description(&self) -> Option<&str> {
        self.definition.get("description").and_then(|v| v.as_str())
    }

    /// Primary git repo path if present on the definition (legacy field or resources).
    pub fn primary_repo(&self) -> Option<String> {
        if let Some(r) = self.definition.get("repo").and_then(|v| v.as_str()) {
            return Some(r.to_string());
        }
        let resources = self.definition.get("resources")?.as_object()?;
        for (_id, res) in resources {
            let ty = res.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ty == "repo" || ty == "git" {
                if let Some(path) = res
                    .get("path")
                    .or_else(|| res.get("root"))
                    .or_else(|| res.get("repo"))
                    .and_then(|v| v.as_str())
                {
                    return Some(path.to_string());
                }
            }
        }
        None
    }

    pub fn branch(&self) -> Option<String> {
        self.definition
            .get("branch")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Awesometree extension blob under definition.extensions["dev.awesometree"].
    pub fn awesometree_ext(&self) -> AwesometreeExt {
        self.definition
            .get("extensions")
            .and_then(|e| e.get("dev.awesometree"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    pub fn policy(&self) -> Option<&Value> {
        self.definition.get("policy")
    }

    pub fn snapshot_ref(&self) -> ProjectSnapshotRef {
        ProjectSnapshotRef {
            project_id: self.project_id.clone(),
            project_revision: self.revision.clone(),
            project_snapshot_id: format!(
                "project://registry/projects/{}/revisions/{}",
                self.project_id, self.revision
            ),
        }
    }
}

/// Immutable pin of a Project revision/snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSnapshotRef {
    pub project_id: String,
    pub project_revision: String,
    pub project_snapshot_id: String,
}

/// Host-local launch / worktree settings carried in Project definition extensions.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwesometreeExt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub layout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_dir: Option<String>,
}


/// Build a minimal Switchboard project definition for create.
pub fn definition_for_create(
    project_id: &str,
    description: Option<&str>,
    repo: Option<&str>,
    branch: Option<&str>,
    ext: Option<&AwesometreeExt>,
) -> Value {
    let mut def = serde_json::json!({
        "version": "1",
        "name": project_id,
        "resources": {}
    });
    if let Some(d) = description {
        def["description"] = Value::String(d.into());
    }
    if let Some(r) = repo {
        def["repo"] = Value::String(r.into());
        let mut resources = serde_json::Map::new();
        resources.insert(
            "primary".into(),
            serde_json::json!({"type": "repo", "path": r}),
        );
        def["resources"] = Value::Object(resources);
    }
    if let Some(b) = branch {
        def["branch"] = Value::String(b.into());
    }
    if let Some(e) = ext {
        let mut extensions = HashMap::new();
        extensions.insert(
            "dev.awesometree".to_string(),
            serde_json::to_value(e).unwrap_or(Value::Null),
        );
        def["extensions"] = serde_json::to_value(extensions).unwrap();
    }
    def
}
