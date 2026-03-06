use crate::paths;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_dir: Option<String>,
}

const EXT_KEY: &str = "dev.awesometree";

pub fn base_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("project-interop")
    } else {
        paths::home_dir().join(".config/project-interop")
    }
}

pub fn projects_dir() -> PathBuf {
    base_dir().join("projects")
}

pub fn context_dir(name: &str) -> PathBuf {
    base_dir().join("context").join(name)
}

pub fn worktree_base() -> PathBuf {
    paths::home_dir().join("worktrees")
}

pub fn expand_home(p: &str) -> PathBuf {
    paths::expand_home(p)
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

pub const DEFAULT_SCHEMA: &str = "https://project-interop.dev/schemas/v1/project.schema.json";

impl Project {
    pub fn new(name: impl Into<String>, repo: impl Into<String>, branch: impl Into<String>) -> Self {
        Self {
            schema: Some(DEFAULT_SCHEMA.into()),
            version: "1".into(),
            name: name.into(),
            repo: Some(repo.into()),
            branch: Some(branch.into()),
            ..Default::default()
        }
    }

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
    let work_dir = paths::home_dir().join("work");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_new_sets_defaults() {
        let p = Project::new("myproj", "/repos/myproj", "main");
        assert_eq!(p.name, "myproj");
        assert_eq!(p.repo.as_deref(), Some("/repos/myproj"));
        assert_eq!(p.branch.as_deref(), Some("main"));
        assert_eq!(p.version, "1");
        assert_eq!(p.schema.as_deref(), Some(DEFAULT_SCHEMA));
    }

    #[test]
    fn interpolate_replaces_placeholders() {
        let result = interpolate("zeditor -n {dir} --project {project}", "curri", "/tmp/ws");
        assert_eq!(result, "zeditor -n /tmp/ws --project curri");
    }

    #[test]
    fn interpolate_no_placeholders() {
        let result = interpolate("echo hello", "proj", "/dir");
        assert_eq!(result, "echo hello");
    }

    #[test]
    fn interpolate_multiple_same_placeholder() {
        let result = interpolate("{dir}/{dir}", "p", "/x");
        assert_eq!(result, "/x//x");
    }

    #[test]
    fn project_repo_path_some() {
        let p = Project::new("p", "/home/user/repo", "main");
        let path = p.repo_path().unwrap();
        assert_eq!(path, PathBuf::from("/home/user/repo"));
    }

    #[test]
    fn project_repo_path_none() {
        let p = Project::default();
        assert!(p.repo_path().is_none());
    }

    #[test]
    fn branch_or_default_with_branch() {
        let p = Project::new("p", "/r", "develop");
        assert_eq!(p.branch_or_default(), "develop");
    }

    #[test]
    fn branch_or_default_without_branch() {
        let p = Project::default();
        assert_eq!(p.branch_or_default(), "master");
    }

    #[test]
    fn awesometree_ext_default() {
        let p = Project::default();
        let ext = p.awesometree_ext();
        assert!(ext.mcp.is_none());
        assert!(ext.apps.is_empty());
        assert!(ext.layout.is_empty());
        assert!(ext.worktree_dir.is_none());
    }

    #[test]
    fn awesometree_ext_roundtrip() {
        let mut p = Project::new("p", "/r", "main");
        let ext = AwesometreeExt {
            mcp: Some("http://localhost:8080".into()),
            apps: vec!["zeditor -n {dir}".into()],
            layout: "max".into(),
            worktree_dir: Some("~/wt".into()),
        };
        p.set_awesometree_ext(&ext);

        let restored = p.awesometree_ext();
        assert_eq!(restored.mcp.as_deref(), Some("http://localhost:8080"));
        assert_eq!(restored.apps, vec!["zeditor -n {dir}"]);
        assert_eq!(restored.layout, "max");
        assert_eq!(restored.worktree_dir.as_deref(), Some("~/wt"));
    }

    #[test]
    fn resolved_mcp_url_interpolates() {
        let mut p = Project::new("proj", "/r", "main");
        let ext = AwesometreeExt {
            mcp: Some("http://localhost/{project}".into()),
            ..Default::default()
        };
        p.set_awesometree_ext(&ext);
        let url = p.resolved_mcp_url("/ws/dir").unwrap();
        assert_eq!(url, "http://localhost/proj");
    }

    #[test]
    fn resolved_mcp_url_none_when_no_ext() {
        let p = Project::new("proj", "/r", "main");
        assert!(p.resolved_mcp_url("/ws/dir").is_none());
    }

    #[test]
    fn expand_home_tilde() {
        let result = expand_home("~/projects/foo");
        assert!(result.to_string_lossy().ends_with("projects/foo"));
        assert!(!result.to_string_lossy().starts_with("~"));
    }

    #[test]
    fn expand_home_absolute() {
        let result = expand_home("/absolute/path");
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn project_serialization_roundtrip() {
        let mut p = Project::new("test", "/repo", "main");
        p.launch = Some(Launch {
            prompt: Some("hello".into()),
            ..Default::default()
        });
        let json = serde_json::to_string(&p).unwrap();
        let p2: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(p2.name, "test");
        assert_eq!(p2.launch.unwrap().prompt.unwrap(), "hello");
    }

    #[test]
    fn merge_basic_overlay() {
        let base = Project::new("proj", "/base/repo", "master");
        let overlay = Project {
            name: "proj".into(),
            branch: Some("develop".into()),
            ..Default::default()
        };
        let merged = merge(base, overlay).unwrap();
        assert_eq!(merged.repo.as_deref(), Some("/base/repo"));
        assert_eq!(merged.branch.as_deref(), Some("develop"));
    }

    #[test]
    fn merge_name_mismatch_errors() {
        let base = Project::new("proj-a", "/r", "main");
        let overlay = Project::new("proj-b", "/r", "main");
        assert!(merge(base, overlay).is_err());
    }

    #[test]
    fn merge_empty_overlay_name_ok() {
        let base = Project::new("proj", "/r", "main");
        let overlay = Project::default();
        assert!(merge(base, overlay).is_ok());
    }

    #[test]
    fn merge_launch_overlay() {
        let base = Project {
            launch: Some(Launch {
                prompt: Some("base prompt".into()),
                ..Default::default()
            }),
            ..Project::new("p", "/r", "main")
        };
        let overlay = Project {
            name: "p".into(),
            launch: Some(Launch {
                prompt: Some("override prompt".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge(base, overlay).unwrap();
        assert_eq!(
            merged.launch.unwrap().prompt.unwrap(),
            "override prompt"
        );
    }

    #[test]
    fn merge_context_extends() {
        let base = Project {
            context: Some(ContextConfig {
                files: Some(vec!["a.md".into()]),
                ..Default::default()
            }),
            ..Project::new("p", "/r", "main")
        };
        let overlay = Project {
            name: "p".into(),
            context: Some(ContextConfig {
                files: Some(vec!["b.md".into()]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge(base, overlay).unwrap();
        let files = merged.context.unwrap().files.unwrap();
        assert_eq!(files, vec!["a.md", "b.md"]);
    }

    #[test]
    fn merge_extensions_overlay() {
        let mut base = Project::new("p", "/r", "main");
        let mut ext_map = HashMap::new();
        ext_map.insert("key1".into(), serde_json::json!("val1"));
        base.extensions = Some(ext_map);

        let mut overlay = Project {
            name: "p".into(),
            ..Default::default()
        };
        let mut overlay_ext = HashMap::new();
        overlay_ext.insert("key2".into(), serde_json::json!("val2"));
        overlay.extensions = Some(overlay_ext);

        let merged = merge(base, overlay).unwrap();
        let exts = merged.extensions.unwrap();
        assert_eq!(exts.get("key1"), Some(&serde_json::json!("val1")));
        assert_eq!(exts.get("key2"), Some(&serde_json::json!("val2")));
    }

    #[test]
    fn merge_json_objects_both_objects() {
        let base = serde_json::json!({"a": 1, "b": 2});
        let overlay = serde_json::json!({"b": 3, "c": 4});
        let result = merge_json_objects(base, overlay);
        assert_eq!(result, serde_json::json!({"a": 1, "b": 3, "c": 4}));
    }

    #[test]
    fn merge_json_objects_overlay_wins_non_object() {
        let base = serde_json::json!({"a": 1});
        let overlay = serde_json::json!([1, 2, 3]);
        let result = merge_json_objects(base, overlay.clone());
        assert_eq!(result, overlay);
    }

    #[test]
    fn context_dir_path() {
        let dir = context_dir("myproj");
        assert!(dir.to_string_lossy().contains("context/myproj"));
    }

    #[test]
    fn projects_dir_path() {
        let dir = projects_dir();
        assert!(dir.to_string_lossy().contains("projects"));
    }

    #[test]
    fn assemble_context_bundle_empty() {
        let p = Project::default();
        let result = assemble_context_bundle(&p).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn assemble_context_bundle_no_context() {
        let p = Project::new("p", "/r", "main");
        let result = assemble_context_bundle(&p).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn assemble_launch_prompt_no_launch() {
        let p = Project::default();
        let result = assemble_launch_prompt(&p, "/dir").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn assemble_launch_prompt_with_inline() {
        let p = Project {
            launch: Some(Launch {
                prompt: Some("Hello {project} in {dir}".into()),
                ..Default::default()
            }),
            ..Project::new("myproj", "/r", "main")
        };
        let result = assemble_launch_prompt(&p, "/my/dir").unwrap();
        assert_eq!(result, "Hello myproj in /my/dir");
    }

    #[test]
    fn merge_launch_env() {
        let base = Project {
            launch: Some(Launch {
                env: Some(HashMap::from([("A".into(), "1".into())])),
                ..Default::default()
            }),
            ..Project::new("p", "/r", "main")
        };
        let overlay = Project {
            name: "p".into(),
            launch: Some(Launch {
                env: Some(HashMap::from([
                    ("A".into(), "2".into()),
                    ("B".into(), "3".into()),
                ])),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge(base, overlay).unwrap();
        let env = merged.launch.unwrap().env.unwrap();
        assert_eq!(env.get("A"), Some(&"2".to_string()));
        assert_eq!(env.get("B"), Some(&"3".to_string()));
    }

    #[test]
    fn merge_tools_overlay() {
        let base = Project {
            tools: Some(serde_json::json!({"lint": "eslint"})),
            ..Project::new("p", "/r", "main")
        };
        let overlay = Project {
            name: "p".into(),
            tools: Some(serde_json::json!({"test": "jest"})),
            ..Default::default()
        };
        let merged = merge(base, overlay).unwrap();
        let tools = merged.tools.unwrap();
        assert_eq!(tools["lint"], "eslint");
        assert_eq!(tools["test"], "jest");
    }

    #[test]
    fn merge_agents_overlay() {
        let base = Project {
            agents: Some(serde_json::json!({"claude": {}})),
            ..Project::new("p", "/r", "main")
        };
        let overlay = Project {
            name: "p".into(),
            agents: Some(serde_json::json!({"codex": {}})),
            ..Default::default()
        };
        let merged = merge(base, overlay).unwrap();
        let agents = merged.agents.unwrap();
        assert!(agents["claude"].is_object());
        assert!(agents["codex"].is_object());
    }

    #[test]
    fn merge_context_max_bytes_override() {
        let base = Project {
            context: Some(ContextConfig {
                max_bytes: Some(1000),
                ..Default::default()
            }),
            ..Project::new("p", "/r", "main")
        };
        let overlay = Project {
            name: "p".into(),
            context: Some(ContextConfig {
                max_bytes: Some(500),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge(base, overlay).unwrap();
        assert_eq!(merged.context.unwrap().max_bytes, Some(500));
    }

    #[test]
    fn merge_context_repo_includes() {
        let base = Project {
            context: Some(ContextConfig {
                repo_includes: Some(vec!["*.md".into()]),
                ..Default::default()
            }),
            ..Project::new("p", "/r", "main")
        };
        let overlay = Project {
            name: "p".into(),
            context: Some(ContextConfig {
                repo_includes: Some(vec!["*.rs".into()]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge(base, overlay).unwrap();
        let includes = merged.context.unwrap().repo_includes.unwrap();
        assert_eq!(includes, vec!["*.md", "*.rs"]);
    }

    #[test]
    fn project_skip_empty_serialization() {
        let p = Project::default();
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("\"repo\""));
        assert!(!json.contains("\"branch\""));
        assert!(!json.contains("\"launch\""));
    }

    #[test]
    fn awesometree_ext_skip_empty_serialization() {
        let ext = AwesometreeExt::default();
        let json = serde_json::to_string(&ext).unwrap();
        assert!(!json.contains("\"apps\""));
        assert!(!json.contains("\"layout\""));
    }

    #[test]
    fn project_with_all_fields() {
        let p = Project {
            schema: Some(DEFAULT_SCHEMA.into()),
            version: "1".into(),
            name: "full".into(),
            repo: Some("/r".into()),
            branch: Some("main".into()),
            launch: Some(Launch {
                prompt: Some("hello".into()),
                prompt_file: Some("prompt.md".into()),
                env: Some(HashMap::from([("KEY".into(), "val".into())])),
            }),
            tools: Some(serde_json::json!({"test": "jest"})),
            context: Some(ContextConfig {
                files: Some(vec!["a.md".into()]),
                repo_includes: Some(vec!["*.rs".into()]),
                max_bytes: Some(1000),
            }),
            agents: Some(serde_json::json!({"claude": {}})),
            extensions: Some(HashMap::new()),
        };
        let json = serde_json::to_string_pretty(&p).unwrap();
        let p2: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(p2.name, "full");
        assert!(p2.launch.is_some());
        assert!(p2.tools.is_some());
        assert!(p2.context.is_some());
        assert!(p2.agents.is_some());
    }

    #[test]
    fn base_dir_default() {
        let dir = base_dir();
        assert!(dir.to_string_lossy().contains("project-interop"));
    }

    #[test]
    fn worktree_base_path() {
        let dir = worktree_base();
        assert!(dir.to_string_lossy().ends_with("worktrees"));
    }

    #[test]
    fn load_nonexistent_project_errors() {
        let result = load("definitely-not-a-project-12345");
        assert!(result.is_err());
    }

    #[test]
    fn delete_nonexistent_project_errors() {
        let result = delete("definitely-not-a-project-12345");
        assert!(result.is_err());
    }

    #[test]
    fn list_empty_or_valid() {
        let result = list();
        assert!(result.is_ok());
    }
}
