use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default, rename = "workspace")]
    pub workspaces: Vec<Workspace>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct Defaults {
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub branch: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Workspace {
    pub name: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub gui: Vec<String>,
    #[serde(default)]
    pub layout: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct WorkspaceState {
    pub tag_index: i32,
    pub dir: String,
    pub active: bool,
}

pub type State = HashMap<String, WorkspaceState>;

pub const TAG_OFFSET: i32 = 10;

pub fn config_path() -> PathBuf {
    dirs_home().join(".config/workspaces.toml")
}

pub fn state_path() -> PathBuf {
    dirs_home().join(".local/state/workspaces.json")
}

pub fn worktree_base() -> PathBuf {
    dirs_home().join("worktrees")
}

fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

pub fn load_config() -> Result<Config, String> {
    let path = config_path();
    let data = fs::read_to_string(&path).map_err(|e| format!("load config: {e}"))?;
    toml::from_str(&data).map_err(|e| format!("parse config: {e}"))
}

pub fn load_state() -> Result<State, String> {
    let path = state_path();
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("load state: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse state: {e}"))
}

pub fn save_state(state: &State) -> Result<(), String> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create state dir: {e}"))?;
    }
    let data = serde_json::to_string_pretty(state).map_err(|e| format!("serialize state: {e}"))?;
    fs::write(&path, data).map_err(|e| format!("write state: {e}"))
}

impl Config {
    pub fn find_workspace(&self, name: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.name == name)
    }

    pub fn all_names(&self) -> Vec<String> {
        self.workspaces.iter().map(|w| w.name.clone()).collect()
    }

    pub fn active_names(&self, state: &State) -> Vec<String> {
        self.workspaces
            .iter()
            .filter(|w| state.get(&w.name).is_some_and(|s| s.active))
            .map(|w| w.name.clone())
            .collect()
    }
}

impl Workspace {
    pub fn resolve_dir(&self) -> PathBuf {
        if !self.path.is_empty() {
            return expand_home(&self.path);
        }
        if !self.branch.is_empty() {
            let safe = self.name.replace('/', "-");
            return worktree_base().join(safe);
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    pub fn resolve_repo(&self, defaults: &Defaults) -> String {
        let r = if !self.repo.is_empty() {
            &self.repo
        } else {
            &defaults.repo
        };
        expand_home(r).to_string_lossy().into_owned()
    }

    pub fn resolve_branch(&self, defaults: &Defaults) -> String {
        if !self.branch.is_empty() {
            self.branch.clone()
        } else {
            defaults.branch.clone()
        }
    }

    pub fn resolve_layout(&self) -> &str {
        if !self.layout.is_empty() {
            &self.layout
        } else {
            "tile"
        }
    }
}

fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        dirs_home().join(rest)
    } else {
        PathBuf::from(p)
    }
}

pub fn allocate_tag_index(name: &str, state: &State) -> i32 {
    if let Some(s) = state.get(name) {
        return s.tag_index;
    }
    let used: std::collections::HashSet<i32> = state.values().map(|s| s.tag_index).collect();
    let mut i = TAG_OFFSET;
    while used.contains(&i) {
        i += 1;
    }
    i
}

pub fn append_to_config(name: &str, repo: &str, branch: &str) -> Result<(), String> {
    let path = config_path();
    let entry = format!("\n[[workspace]]\nname = \"{name}\"\nrepo = \"{repo}\"\nbranch = \"{branch}\"\n");
    let mut data = fs::read_to_string(&path).unwrap_or_default();
    data.push_str(&entry);
    fs::write(&path, data).map_err(|e| format!("write config: {e}"))
}

pub fn remove_from_config(name: &str) -> Result<(), String> {
    let path = config_path();
    let data = fs::read_to_string(&path).map_err(|e| format!("read config: {e}"))?;
    let lines: Vec<&str> = data.split('\n').collect();
    let mut out: Vec<&str> = Vec::new();
    let mut skip = false;
    let target = format!("name = \"{name}\"");

    for line in &lines {
        let trimmed = line.trim();
        if trimmed == "[[workspace]]" {
            skip = false;
            out.push(line);
            continue;
        }
        if skip {
            continue;
        }
        if trimmed == target {
            while out.last().is_some_and(|l| {
                let t = l.trim();
                t == "[[workspace]]" || t.is_empty()
            }) {
                out.pop();
            }
            skip = true;
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    let result = out.join("\n") + "\n";
    fs::write(&path, result).map_err(|e| format!("write config: {e}"))
}

pub fn list_repos() -> Vec<PathBuf> {
    let work_dir = dirs_home().join("work");
    let Ok(entries) = fs::read_dir(&work_dir) else {
        return vec![];
    };
    let mut repos: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && e.path().join(".git").exists())
        .map(|e| e.path())
        .collect();
    repos.sort();
    repos
}

pub fn list_branches(repo: &Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["-C", &repo.to_string_lossy(), "branch", "-a", "--format=%(refname:short)"])
        .output();
    let Ok(output) = output else { return vec![] };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut seen = std::collections::HashSet::new();
    for line in stdout.lines() {
        let clean = line.strip_prefix("origin/").unwrap_or(line);
        if !clean.is_empty() && clean != "HEAD" {
            seen.insert(clean.to_string());
        }
    }
    let mut branches: Vec<String> = seen.into_iter().collect();
    branches.sort();
    branches
}
