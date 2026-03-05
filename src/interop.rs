use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Project {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub version: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<Launch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContextConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_includes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AwesometreeExt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub layout: String,
}

const EXT_KEY: &str = "dev.awesometree";

fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

pub fn base_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("project-interop")
    } else {
        home_dir().join(".config/project-interop")
    }
}

pub fn projects_dir() -> PathBuf {
    base_dir().join("projects")
}

pub fn context_dir(name: &str) -> PathBuf {
    base_dir().join("context").join(name)
}

pub fn worktree_base() -> PathBuf {
    home_dir().join("worktrees")
}

pub fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(p)
    }
}

pub fn load(name: &str) -> Result<Project, String> {
    let path = projects_dir().join(format!("{name}.project.json"));
    if !path.exists() {
        return Err(format!("project not found: {name}"));
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&data).map_err(|e| format!("parse {}: {e}", path.display()))
}

pub fn load_merged(name: &str, repo_path: Option<&Path>) -> Result<Project, String> {
    let user = load(name)?;
    let repo_local = repo_path.and_then(|rp| {
        let p = rp.join(".project.json");
        if p.exists() {
            let data = fs::read_to_string(&p).ok()?;
            serde_json::from_str::<Project>(&data).ok()
        } else {
            None
        }
    });
    match repo_local {
        Some(local) => merge(user, local),
        None => Ok(user),
    }
}

pub fn save(project: &Project) -> Result<(), String> {
    let dir = projects_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create projects dir: {e}"))?;
    let data =
        serde_json::to_string_pretty(project).map_err(|e| format!("serialize project: {e}"))?;
    let path = dir.join(format!("{}.project.json", project.name));
    let tmp = dir.join(format!(".{}.project.json.tmp", project.name));
    fs::write(&tmp, &data).map_err(|e| format!("write tmp: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
    Ok(())
}

pub fn list() -> Result<Vec<Project>, String> {
    let dir = projects_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut projects = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("read projects dir: {e}"))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".project.json") {
            continue;
        }
        let data = match fs::read_to_string(entry.path()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if let Ok(proj) = serde_json::from_str::<Project>(&data) {
            if proj.version == "1" && !proj.name.is_empty() {
                projects.push(proj);
            }
        }
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}

pub fn delete(name: &str) -> Result<(), String> {
    let path = projects_dir().join(format!("{name}.project.json"));
    if !path.exists() {
        return Err(format!("project not found: {name}"));
    }
    fs::remove_file(&path).map_err(|e| format!("delete {}: {e}", path.display()))
}

pub fn interpolate(template: &str, project_name: &str, dir: &str) -> String {
    template
        .replace("{project}", project_name)
        .replace("{dir}", dir)
}

impl Project {
    pub fn repo_path(&self) -> Option<PathBuf> {
        self.repo.as_ref().map(|r| expand_home(r))
    }

    pub fn branch_or_default(&self) -> &str {
        self.branch.as_deref().unwrap_or("master")
    }

    pub fn awesometree_ext(&self) -> AwesometreeExt {
        self.extensions
            .as_ref()
            .and_then(|exts| exts.get(EXT_KEY))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    pub fn set_awesometree_ext(&mut self, ext: &AwesometreeExt) {
        let val = serde_json::to_value(ext).unwrap_or_default();
        self.extensions
            .get_or_insert_with(HashMap::new)
            .insert(EXT_KEY.to_string(), val);
    }

    pub fn resolved_mcp_url(&self, dir: &str) -> Option<String> {
        let ext = self.awesometree_ext();
        ext.mcp.map(|url| interpolate(&url, &self.name, dir))
    }
}

pub fn assemble_launch_prompt(project: &Project, dir: &str) -> Result<String, String> {
    let launch = match &project.launch {
        Some(l) => l,
        None => return Ok(String::new()),
    };

    let mut parts = Vec::new();

    if let Some(prompt) = &launch.prompt {
        parts.push(interpolate(prompt, &project.name, dir));
    }

    if let Some(prompt_file) = &launch.prompt_file {
        let ctx_dir = context_dir(&project.name);
        let path = ctx_dir.join(prompt_file);
        match fs::read_to_string(&path) {
            Ok(content) => parts.push(interpolate(&content, &project.name, dir)),
            Err(e) => eprintln!("warning: prompt file {}: {e}", path.display()),
        }
    }

    Ok(parts.join("\n"))
}

pub fn assemble_context_bundle(
    project: &Project,
) -> Result<Vec<(String, String)>, String> {
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut total_bytes: usize = 0;

    let max_bytes = project
        .context
        .as_ref()
        .and_then(|c| c.max_bytes)
        .unwrap_or(usize::MAX);

    let ctx = match &project.context {
        Some(c) => c,
        None => return Ok(entries),
    };

    if let (Some(includes), Some(repo)) = (&ctx.repo_includes, &project.repo) {
        let repo_root = expand_home(repo);
        for pattern in includes {
            let full = repo_root.join(pattern);
            let full_str = full.to_string_lossy();
            let matches = glob::glob(&full_str).map_err(|e| format!("glob: {e}"))?;
            for path in matches.flatten() {
                if !path.is_file() {
                    continue;
                }
                let rel = path
                    .strip_prefix(&repo_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                if seen.contains(&rel) {
                    continue;
                }
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        total_bytes += content.len();
                        if total_bytes > max_bytes {
                            entries.push((
                                "__truncated__".into(),
                                format!("Context truncated at {max_bytes} bytes"),
                            ));
                            return Ok(entries);
                        }
                        seen.insert(rel.clone());
                        entries.push((rel, content));
                    }
                    Err(e) => eprintln!("warning: {}: {e}", path.display()),
                }
            }
        }
    }

    if let Some(files) = &ctx.files {
        let ctx_dir = context_dir(&project.name);
        for file in files {
            if seen.contains(file) {
                continue;
            }
            let path = ctx_dir.join(file);
            match fs::read_to_string(&path) {
                Ok(content) => {
                    total_bytes += content.len();
                    if total_bytes > max_bytes {
                        entries.push((
                            "__truncated__".into(),
                            format!("Context truncated at {max_bytes} bytes"),
                        ));
                        return Ok(entries);
                    }
                    seen.insert(file.clone());
                    entries.push((file.clone(), content));
                }
                Err(e) => eprintln!("warning: context/{}/{}: {e}", project.name, file),
            }
        }
    }

    Ok(entries)
}

fn merge(base: Project, overlay: Project) -> Result<Project, String> {
    if !overlay.name.is_empty() && overlay.name != base.name {
        return Err(format!(
            "project name mismatch: user-level '{}' vs repo-local '{}'",
            base.name, overlay.name
        ));
    }

    let mut merged = base;

    if overlay.repo.is_some() {
        merged.repo = overlay.repo;
    }
    if overlay.branch.is_some() {
        merged.branch = overlay.branch;
    }

    match (&mut merged.launch, overlay.launch) {
        (_, None) => {}
        (target, Some(over)) => {
            let t = target.get_or_insert_with(Launch::default);
            if over.prompt.is_some() {
                t.prompt = over.prompt;
            }
            if over.prompt_file.is_some() {
                t.prompt_file = over.prompt_file;
            }
            if let Some(over_env) = over.env {
                let env = t.env.get_or_insert_with(HashMap::new);
                for (k, v) in over_env {
                    env.insert(k, v);
                }
            }
        }
    }

    if let Some(over_tools) = overlay.tools {
        merged.tools = Some(match merged.tools {
            Some(base_tools) => merge_json_objects(base_tools, over_tools),
            None => over_tools,
        });
    }

    match (&mut merged.context, overlay.context) {
        (_, None) => {}
        (target, Some(over)) => {
            let t = target.get_or_insert_with(ContextConfig::default);
            if let Some(over_files) = over.files {
                let files = t.files.get_or_insert_with(Vec::new);
                files.extend(over_files);
            }
            if let Some(over_includes) = over.repo_includes {
                let includes = t.repo_includes.get_or_insert_with(Vec::new);
                includes.extend(over_includes);
            }
            if over.max_bytes.is_some() {
                t.max_bytes = over.max_bytes;
            }
        }
    }

    if let Some(over_agents) = overlay.agents {
        merged.agents = Some(match merged.agents {
            Some(base_agents) => merge_json_objects(base_agents, over_agents),
            None => over_agents,
        });
    }

    if let Some(over_exts) = overlay.extensions {
        let exts = merged.extensions.get_or_insert_with(HashMap::new);
        for (k, v) in over_exts {
            exts.insert(k, v);
        }
    }

    Ok(merged)
}

fn merge_json_objects(base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    match (base, overlay) {
        (serde_json::Value::Object(mut b), serde_json::Value::Object(o)) => {
            for (k, v) in o {
                b.insert(k, v);
            }
            serde_json::Value::Object(b)
        }
        (_, overlay) => overlay,
    }
}

pub fn list_repos() -> Vec<PathBuf> {
    let work_dir = home_dir().join("work");
    let Ok(outer) = fs::read_dir(&work_dir) else {
        return vec![];
    };
    let mut repos: Vec<PathBuf> = Vec::new();
    for entry in outer.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join(".git").exists() {
                repos.push(path.clone());
            }
            if let Ok(inner) = fs::read_dir(&path) {
                for sub in inner.flatten() {
                    let sp = sub.path();
                    if sp.is_dir() && sp.join(".git").exists() {
                        repos.push(sp);
                    }
                }
            }
        }
    }
    repos.sort();
    repos
}

pub fn list_branches(repo: &Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args([
            "-C",
            &repo.to_string_lossy(),
            "branch",
            "-a",
            "--format=%(refname:short)",
        ])
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
