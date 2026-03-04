use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub projects: Vec<Project>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    pub name: String,
    pub repo: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceEntry>,
    #[serde(default)]
    pub gui: Vec<String>,
    #[serde(default)]
    pub layout: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceEntry {
    pub name: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub tag_index: i32,
    #[serde(default)]
    pub dir: String,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub name: String,
    pub project: String,
    pub repo: String,
    pub branch: String,
    pub gui: Vec<String>,
    pub layout: String,
    pub active: bool,
    pub tag_index: i32,
    pub dir: String,
}

pub const TAG_OFFSET: i32 = 10;

pub fn config_dir() -> PathBuf {
    dirs_home().join(".config/awesometree")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn worktree_base() -> PathBuf {
    dirs_home().join("worktrees")
}

fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

pub fn load_config() -> Result<Config, String> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("load config: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse config: {e}"))
}

pub fn save_config(cfg: &Config) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
    let data = serde_json::to_string_pretty(cfg).map_err(|e| format!("serialize config: {e}"))?;
    fs::write(config_path(), data).map_err(|e| format!("write config: {e}"))
}

impl Config {
    pub fn all_workspaces(&self) -> Vec<Workspace> {
        self.projects
            .iter()
            .flat_map(|p| {
                p.workspaces.iter().map(move |ws| Workspace {
                    name: ws.name.clone(),
                    project: p.name.clone(),
                    repo: p.repo.clone(),
                    branch: p.branch.clone(),
                    gui: p.gui.clone(),
                    layout: p.layout.clone(),
                    active: ws.active,
                    tag_index: ws.tag_index,
                    dir: ws.dir.clone(),
                })
            })
            .collect()
    }

    pub fn find_workspace(&self, name: &str) -> Option<Workspace> {
        self.all_workspaces().into_iter().find(|w| w.name == name)
    }

    pub fn find_project(&self, name: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.name == name)
    }

    pub fn all_names(&self) -> Vec<String> {
        self.all_workspaces().iter().map(|w| w.name.clone()).collect()
    }

    pub fn active_names(&self) -> Vec<String> {
        self.all_workspaces()
            .iter()
            .filter(|w| w.active)
            .map(|w| w.name.clone())
            .collect()
    }

    pub fn project_names(&self) -> Vec<String> {
        self.projects.iter().map(|p| p.name.clone()).collect()
    }

    pub fn set_workspace_active(&mut self, name: &str, active: bool, tag_index: i32, dir: &str) {
        for p in &mut self.projects {
            for ws in &mut p.workspaces {
                if ws.name == name {
                    ws.active = active;
                    ws.tag_index = tag_index;
                    ws.dir = dir.to_string();
                    return;
                }
            }
        }
    }

    pub fn set_workspace_inactive(&mut self, name: &str) {
        for p in &mut self.projects {
            for ws in &mut p.workspaces {
                if ws.name == name {
                    ws.active = false;
                    ws.tag_index = 0;
                    ws.dir.clear();
                    return;
                }
            }
        }
    }

    pub fn append_workspace_to_project(&mut self, project_name: &str, ws_name: &str) -> Result<(), String> {
        for p in &mut self.projects {
            if p.name == project_name {
                p.workspaces.push(WorkspaceEntry {
                    name: ws_name.to_string(),
                    active: false,
                    tag_index: 0,
                    dir: String::new(),
                });
                return Ok(());
            }
        }
        Err(format!("project not found: {project_name}"))
    }

    pub fn remove_workspace(&mut self, ws_name: &str) {
        for p in &mut self.projects {
            p.workspaces.retain(|ws| ws.name != ws_name);
        }
    }

    pub fn add_project(&mut self, name: &str, repo: &str, branch: &str) {
        self.projects.push(Project {
            name: name.to_string(),
            repo: repo.to_string(),
            branch: branch.to_string(),
            workspaces: vec![],
            gui: vec![],
            layout: String::new(),
        });
    }
}

impl Workspace {
    pub fn resolve_dir(&self) -> PathBuf {
        if !self.branch.is_empty() {
            let safe = self.name.replace('/', "-");
            return worktree_base().join(&self.project).join(safe);
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    pub fn resolve_repo(&self) -> String {
        expand_home(&self.repo).to_string_lossy().into_owned()
    }

    pub fn resolve_branch(&self) -> String {
        self.branch.clone()
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

pub fn allocate_tag_index(name: &str, cfg: &Config) -> i32 {
    for p in &cfg.projects {
        for ws in &p.workspaces {
            if ws.name == name && ws.tag_index > 0 {
                return ws.tag_index;
            }
        }
    }
    let used: std::collections::HashSet<i32> = cfg
        .projects
        .iter()
        .flat_map(|p| p.workspaces.iter())
        .filter(|ws| ws.active)
        .map(|ws| ws.tag_index)
        .collect();
    let mut i = TAG_OFFSET;
    while used.contains(&i) {
        i += 1;
    }
    i
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
