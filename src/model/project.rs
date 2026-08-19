use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
        if let Some(b) = self.definition.get("branch").and_then(|v| v.as_str()) {
            return Some(b.to_string());
        }
        let resources = self.definition.get("resources")?.as_object()?;
        for (_id, res) in resources {
            let ty = res.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ty == "repo" || ty == "git" {
                if let Some(b) = res.get("branch").and_then(|v| v.as_str()) {
                    return Some(b.to_string());
                }
            }
        }
        None
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

/// Overlay form fields onto an existing Switchboard definition without dropping
/// unknown catalog fields (tools, agents, additional resources, …).
///
/// The result is an update payload: immutable `name` is omitted because
/// Switchboard rejects patches that include `/name` even when unchanged.
pub fn merge_form_into_definition(
    existing: &Value,
    _project_id: &str,
    repo: Option<&str>,
    branch: Option<&str>,
    ext: &AwesometreeExt,
) -> Value {
    let mut def = match existing {
        Value::Object(_) => existing.clone(),
        _ => json!({ "version": "1" }),
    };
    if !def.is_object() {
        def = json!({ "version": "1" });
    }
    def["version"] = json!("1");
    if let Value::Object(obj) = &mut def {
        obj.remove("name");
    }

    let has_repo_resources = def
        .get("resources")
        .and_then(Value::as_object)
        .map(|m| {
            m.values().any(|res| {
                matches!(res.get("type").and_then(|v| v.as_str()), Some("repo" | "git"))
            })
        })
        .unwrap_or(false);
    let has_top_level_repo = def.get("repo").and_then(|v| v.as_str()).is_some();
    let has_top_level_branch = def.get("branch").and_then(|v| v.as_str()).is_some();

    if let Some(r) = repo.filter(|s| !s.is_empty()) {
        if has_top_level_repo || !has_repo_resources {
            def["repo"] = json!(r);
        }
        if has_repo_resources {
            update_repo_resources(&mut def, Some(r), None);
        } else {
            let mut resources = def
                .get("resources")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            resources.insert("primary".into(), json!({"type": "repo", "path": r}));
            def["resources"] = Value::Object(resources);
        }
    }

    if let Some(b) = branch.filter(|s| !s.is_empty()) {
        if has_repo_resources {
            update_repo_resources(&mut def, None, Some(b));
        }
        if has_top_level_branch || !has_repo_resources {
            def["branch"] = json!(b);
        }
    }

    let ext_val = serde_json::to_value(ext).unwrap_or(Value::Null);
    let ext_empty = ext_val.as_object().map(|o| o.is_empty()).unwrap_or(true);
    if !ext_empty {
        let mut extensions = def
            .get("extensions")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        extensions.insert("dev.awesometree".into(), ext_val);
        def["extensions"] = Value::Object(extensions);
    }
    def
}

fn update_repo_resources(def: &mut Value, path: Option<&str>, branch: Option<&str>) {
    let Some(resources) = def.get_mut("resources").and_then(Value::as_object_mut) else {
        return;
    };
    for (_id, res) in resources.iter_mut() {
        let ty = res.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty != "repo" && ty != "git" {
            continue;
        }
        if let Some(p) = path {
            res["path"] = json!(p);
        }
        if let Some(b) = branch {
            res["branch"] = json!(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_preserves_unrelated_fields() {
        let existing = json!({
            "version": "1",
            "name": "curri",
            "description": "keep me",
            "tools": {"github": {"allow": ["*"]}},
            "repo": "/old",
            "resources": {"docs": {"type": "path", "path": "/docs"}}
        });
        let ext = AwesometreeExt {
            apps: vec!["zeditor -n {dir}".into()],
            ..Default::default()
        };
        let merged = merge_form_into_definition(
            &existing,
            "curri",
            Some("/new/repo"),
            Some("master"),
            &ext,
        );
        assert_eq!(merged["description"], "keep me");
        assert_eq!(merged["tools"]["github"]["allow"][0], "*");
        assert_eq!(merged["repo"], "/new/repo");
        assert_eq!(merged["branch"], "master");
        assert_eq!(merged["resources"]["docs"]["path"], "/docs");
        assert_eq!(merged["resources"]["primary"]["path"], "/new/repo");
        assert_eq!(
            merged["extensions"]["dev.awesometree"]["apps"][0],
            "zeditor -n {dir}"
        );
        assert!(merged.get("name").is_none(), "updates must omit immutable name");
    }

    #[test]
    fn merge_updates_named_repo_resource_branch_without_name() {
        let existing = json!({
            "version": "1",
            "name": "audiobook",
            "resources": {
                "audiobook": {
                    "type": "repo",
                    "path": "/home/aleks/work/projects/audiobook_reader/repo",
                    "branch": "master"
                }
            }
        });
        let merged = merge_form_into_definition(
            &existing,
            "audiobook",
            Some("/home/aleks/work/projects/audiobook_reader/repo"),
            Some("feat/chapters"),
            &AwesometreeExt::default(),
        );
        assert!(merged.get("name").is_none());
        assert!(merged.get("branch").is_none(), "do not invent top-level branch");
        assert!(merged.get("repo").is_none(), "do not invent top-level repo");
        assert_eq!(
            merged["resources"]["audiobook"]["branch"],
            "feat/chapters"
        );
        assert_eq!(
            merged["resources"]["audiobook"]["path"],
            "/home/aleks/work/projects/audiobook_reader/repo"
        );
    }
}
