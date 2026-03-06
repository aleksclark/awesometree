use crate::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Store {
    #[serde(default)]
    pub workspaces: HashMap<String, WorkspaceState>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct WorkspaceState {
    pub project: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub tag_index: i32,
    #[serde(default)]
    pub dir: String,
}

pub const TAG_OFFSET: i32 = 10;

fn state_dir() -> PathBuf {
    paths::home_dir().join(".config/awesometree")
}

fn state_path() -> PathBuf {
    state_dir().join("state.json")
}

pub fn load() -> Result<Store, String> {
    let path = state_path();
    if !path.exists() {
        return Ok(Store::default());
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("read state: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse state: {e}"))
}

pub fn save(store: &Store) -> Result<(), String> {
    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create state dir: {e}"))?;
    let data = serde_json::to_string_pretty(store).map_err(|e| format!("serialize state: {e}"))?;
    let path = state_path();
    let tmp = dir.join(".state.json.tmp");
    fs::write(&tmp, &data).map_err(|e| format!("write tmp: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
    Ok(())
}

impl Store {
    pub fn workspace(&self, name: &str) -> Option<&WorkspaceState> {
        self.workspaces.get(name)
    }

    pub fn set_active(&mut self, name: &str, project: &str, tag_index: i32, dir: &str) {
        let ws = self.workspaces.entry(name.to_string()).or_insert_with(|| {
            WorkspaceState {
                project: project.to_string(),
                ..Default::default()
            }
        });
        ws.project = project.to_string();
        ws.active = true;
        ws.tag_index = tag_index;
        ws.dir = dir.to_string();
    }

    pub fn set_inactive(&mut self, name: &str) {
        if let Some(ws) = self.workspaces.get_mut(name) {
            ws.active = false;
            ws.tag_index = 0;
            ws.dir.clear();
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.workspaces.remove(name);
    }

    pub fn active_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .workspaces
            .iter()
            .filter(|(_, ws)| ws.active)
            .map(|(name, _)| name.clone())
            .collect();
        names.sort();
        names
    }

    pub fn all_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.workspaces.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn workspaces_for_project(&self, project: &str) -> Vec<(String, &WorkspaceState)> {
        let mut result: Vec<_> = self
            .workspaces
            .iter()
            .filter(|(_, ws)| ws.project == project)
            .map(|(name, ws)| (name.clone(), ws))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    pub fn allocate_tag_index(&self, name: &str) -> i32 {
        if let Some(ws) = self.workspaces.get(name) {
            if ws.tag_index > 0 {
                return ws.tag_index;
            }
        }
        let used: std::collections::HashSet<i32> = self
            .workspaces
            .values()
            .filter(|ws| ws.active)
            .map(|ws| ws.tag_index)
            .collect();
        let mut i = TAG_OFFSET;
        while used.contains(&i) {
            i += 1;
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> Store {
        Store::default()
    }

    #[test]
    fn empty_store() {
        let s = make_store();
        assert!(s.workspace("foo").is_none());
        assert!(s.active_names().is_empty());
        assert!(s.all_names().is_empty());
    }

    #[test]
    fn set_active_creates_workspace() {
        let mut s = make_store();
        s.set_active("feat-1", "myproject", 10, "/tmp/feat-1");
        let ws = s.workspace("feat-1").unwrap();
        assert_eq!(ws.project, "myproject");
        assert!(ws.active);
        assert_eq!(ws.tag_index, 10);
        assert_eq!(ws.dir, "/tmp/feat-1");
    }

    #[test]
    fn set_active_updates_existing() {
        let mut s = make_store();
        s.set_active("feat-1", "proj-a", 10, "/tmp/a");
        s.set_active("feat-1", "proj-b", 11, "/tmp/b");
        let ws = s.workspace("feat-1").unwrap();
        assert_eq!(ws.project, "proj-b");
        assert_eq!(ws.tag_index, 11);
    }

    #[test]
    fn set_inactive_clears_fields() {
        let mut s = make_store();
        s.set_active("feat-1", "proj", 10, "/tmp/feat-1");
        s.set_inactive("feat-1");
        let ws = s.workspace("feat-1").unwrap();
        assert!(!ws.active);
        assert_eq!(ws.tag_index, 0);
        assert!(ws.dir.is_empty());
        assert_eq!(ws.project, "proj");
    }

    #[test]
    fn set_inactive_nonexistent_noop() {
        let mut s = make_store();
        s.set_inactive("ghost");
        assert!(s.workspace("ghost").is_none());
    }

    #[test]
    fn remove_workspace() {
        let mut s = make_store();
        s.set_active("feat-1", "proj", 10, "/tmp");
        s.remove("feat-1");
        assert!(s.workspace("feat-1").is_none());
    }

    #[test]
    fn active_names_sorted() {
        let mut s = make_store();
        s.set_active("charlie", "p", 10, "/tmp");
        s.set_active("alice", "p", 11, "/tmp");
        s.set_active("bob", "p", 12, "/tmp");
        s.set_inactive("bob");
        assert_eq!(s.active_names(), vec!["alice", "charlie"]);
    }

    #[test]
    fn all_names_sorted() {
        let mut s = make_store();
        s.set_active("charlie", "p", 10, "/tmp");
        s.set_active("alice", "p", 11, "/tmp");
        s.set_inactive("alice");
        assert_eq!(s.all_names(), vec!["alice", "charlie"]);
    }

    #[test]
    fn workspaces_for_project_filters() {
        let mut s = make_store();
        s.set_active("feat-1", "proj-a", 10, "/tmp");
        s.set_active("feat-2", "proj-b", 11, "/tmp");
        s.set_active("feat-3", "proj-a", 12, "/tmp");
        let result = s.workspaces_for_project("proj-a");
        let names: Vec<_> = result.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["feat-1", "feat-3"]);
    }

    #[test]
    fn workspaces_for_project_empty() {
        let s = make_store();
        assert!(s.workspaces_for_project("nope").is_empty());
    }

    #[test]
    fn allocate_tag_index_returns_existing() {
        let mut s = make_store();
        s.set_active("feat-1", "p", 15, "/tmp");
        assert_eq!(s.allocate_tag_index("feat-1"), 15);
    }

    #[test]
    fn allocate_tag_index_starts_at_offset() {
        let s = make_store();
        assert_eq!(s.allocate_tag_index("new"), TAG_OFFSET);
    }

    #[test]
    fn allocate_tag_index_skips_used() {
        let mut s = make_store();
        s.set_active("a", "p", TAG_OFFSET, "/tmp");
        s.set_active("b", "p", TAG_OFFSET + 1, "/tmp");
        assert_eq!(s.allocate_tag_index("c"), TAG_OFFSET + 2);
    }

    #[test]
    fn allocate_tag_index_ignores_inactive() {
        let mut s = make_store();
        s.set_active("a", "p", TAG_OFFSET, "/tmp");
        s.set_inactive("a");
        assert_eq!(s.allocate_tag_index("b"), TAG_OFFSET);
    }

    #[test]
    fn allocate_tag_index_zero_gets_new() {
        let mut s = make_store();
        s.workspaces.insert("ws".into(), WorkspaceState {
            project: "p".into(),
            tag_index: 0,
            ..Default::default()
        });
        assert_eq!(s.allocate_tag_index("ws"), TAG_OFFSET);
    }

    #[test]
    fn serialization_roundtrip() {
        let mut s = make_store();
        s.set_active("feat-1", "proj", 10, "/tmp/feat-1");
        let json = serde_json::to_string(&s).unwrap();
        let s2: Store = serde_json::from_str(&json).unwrap();
        let ws = s2.workspace("feat-1").unwrap();
        assert_eq!(ws.project, "proj");
        assert!(ws.active);
        assert_eq!(ws.tag_index, 10);
    }

    #[test]
    fn deserialize_missing_fields() {
        let json = r#"{"workspaces":{"ws1":{"project":"p"}}}"#;
        let s: Store = serde_json::from_str(json).unwrap();
        let ws = s.workspace("ws1").unwrap();
        assert_eq!(ws.project, "p");
        assert!(!ws.active);
        assert_eq!(ws.tag_index, 0);
        assert!(ws.dir.is_empty());
    }
}
