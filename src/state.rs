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
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".config/awesometree")
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
