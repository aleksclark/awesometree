use crate::interop::{self, AwesometreeExt, Project};
use crate::state::{self, Store};
use crate::wm::Adapter;
use std::path::PathBuf;
use std::process::Command;

pub struct Manager {
    pub state: Store,
    pub wm: Box<dyn Adapter>,
}

pub struct UpOptions {
    pub create_tag: bool,
    pub launch_apps: bool,
}

pub struct DownOptions {
    pub manage_tag: bool,
    pub keep_worktree: bool,
}

pub struct ResolvedWorkspace {
    pub name: String,
    pub project: Project,
    pub ext: AwesometreeExt,
    pub active: bool,
    pub tag_index: i32,
    pub dir: PathBuf,
}

impl Manager {
    pub fn new(state: Store, wm: Box<dyn Adapter>) -> Self {
        Self { state, wm }
    }

    pub fn resolve_workspace(&self, ws_name: &str) -> Result<ResolvedWorkspace, String> {
        let ws_state = self
            .state
            .workspace(ws_name)
            .ok_or_else(|| format!("workspace not found: {ws_name}"))?;
        let project = interop::load(&ws_state.project)?;
        let ext = project.awesometree_ext();
        let dir = resolve_dir(ws_name, &project);
        Ok(ResolvedWorkspace {
            name: ws_name.to_string(),
            project,
            ext,
            active: ws_state.active,
            tag_index: ws_state.tag_index,
            dir,
        })
    }

    pub fn up(
        &mut self,
        ws_name: &str,
        project: &Project,
        opts: &UpOptions,
    ) -> Result<(), String> {
        eprintln!("  Creating workspace: {ws_name}");
        let ext = project.awesometree_ext();
        let dir = resolve_dir(ws_name, project);

        ensure_worktree(ws_name, project, &dir)?;

        let tag_idx = self.state.allocate_tag_index(ws_name);
        let layout = if ext.layout.is_empty() {
            "tile"
        } else {
            &ext.layout
        };

        if opts.create_tag {
            self.wm.create_tag(ws_name, tag_idx, layout)?;
        }

        if opts.launch_apps {
            let dir_str = dir.to_string_lossy();
            let apps = if ext.apps.is_empty() {
                vec!["zeditor -n {dir}".to_string()]
            } else {
                ext.apps.clone()
            };

            for app_cmd in &apps {
                let expanded = interop::interpolate(app_cmd, &project.name, &dir_str);
                let _ = Command::new("sh")
                    .args(["-c", &expanded])
                    .current_dir(&dir)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
        }

        let dir_str = dir.to_string_lossy().into_owned();
        self.state
            .set_active(ws_name, &project.name, tag_idx, &dir_str);
        state::save(&self.state)
    }

    pub fn down(&mut self, ws_name: &str, opts: &DownOptions) -> Result<(), String> {
        eprintln!("  Removing workspace: {ws_name}");
        let rw = self.resolve_workspace(ws_name)?;

        if opts.manage_tag {
            let _ = self.wm.kill_tag_clients(ws_name);
            let _ = self.wm.delete_tag(ws_name);
        }

        if !opts.keep_worktree {
            remove_worktree(&rw.project, &rw.dir);
        }

        self.state.set_inactive(ws_name);
        state::save(&self.state)
    }

    pub fn switch(&self, name: &str) -> Result<(), String> {
        self.wm.switch_tag(name)
    }

    pub fn is_dirty(&self, ws_name: &str) -> Result<bool, String> {
        let rw = self.resolve_workspace(ws_name)?;
        let output = Command::new("git")
            .args(["-C", &rw.dir.to_string_lossy(), "status", "--porcelain"])
            .output()
            .map_err(|e| format!("git status: {e}"))?;
        Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }

    pub fn launch_agent(
        &self,
        ws_name: &str,
        agent_host: &str,
    ) -> Result<(), String> {
        let rw = self.resolve_workspace(ws_name)?;
        let dir_str = rw.dir.to_string_lossy();
        let prompt = interop::assemble_launch_prompt(&rw.project, &dir_str)?;
        let mcp_url = rw.project.resolved_mcp_url(&dir_str);

        let launch_env = rw
            .project
            .launch
            .as_ref()
            .and_then(|l| l.env.as_ref())
            .cloned()
            .unwrap_or_default();

        match agent_host {
            "claude" => {
                let mut cmd = Command::new("claude");
                if !prompt.is_empty() {
                    cmd.args(["--append-system-prompt", &prompt]);
                }
                if let Some(url) = &mcp_url {
                    cmd.args(["--mcp-server", url]);
                }
                cmd.current_dir(&rw.dir);
                for (k, v) in &launch_env {
                    cmd.env(k, v);
                }
                cmd.status()
                    .map_err(|e| format!("launch claude: {e}"))?;
            }
            "codex" => {
                let mut cmd = Command::new("codex");
                if !prompt.is_empty() {
                    cmd.args(["--system-prompt", &prompt]);
                }
                cmd.current_dir(&rw.dir);
                for (k, v) in &launch_env {
                    cmd.env(k, v);
                }
                if let Some(url) = &mcp_url {
                    let mcp_json = serde_json::json!([{"url": url}]).to_string();
                    cmd.env("OPENAI_MCP_SERVERS", mcp_json);
                }
                cmd.status()
                    .map_err(|e| format!("launch codex: {e}"))?;
            }
            other => return Err(format!("unknown agent host: {other}")),
        }

        Ok(())
    }

    pub fn destroy(&mut self, ws_name: &str, opts: &DownOptions) -> Result<(), String> {
        self.down(ws_name, opts)?;
        self.state.remove(ws_name);
        state::save(&self.state)
    }
}

pub fn resolve_dir(ws_name: &str, project: &Project) -> PathBuf {
    if project.branch.is_some() {
        let safe = ws_name.replace('/', "-");
        interop::worktree_base().join(&project.name).join(safe)
    } else {
        project
            .repo_path()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

fn ensure_worktree(ws_name: &str, project: &Project, dir: &PathBuf) -> Result<(), String> {
    let branch = match &project.branch {
        Some(b) => b,
        None => return Ok(()),
    };
    if dir.exists() {
        return Ok(());
    }

    let repo = project
        .repo_path()
        .ok_or_else(|| "project has no repo path".to_string())?;
    let repo_str = repo.to_string_lossy();
    if !repo.exists() {
        return Err(format!("repo not found: {repo_str}"));
    }

    let base = interop::worktree_base();
    std::fs::create_dir_all(&base).map_err(|e| format!("create worktree base: {e}"))?;

    let _ = Command::new("git")
        .args(["-C", &repo_str, "fetch", "origin", branch])
        .output();

    let branch_exists = Command::new("git")
        .args(["-C", &repo_str, "rev-parse", "--verify", ws_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let dir_str = dir.to_string_lossy();
    let output = if branch_exists {
        Command::new("git")
            .args(["-C", &repo_str, "worktree", "add", &dir_str, ws_name])
            .output()
    } else {
        let tracking = format!("origin/{branch}");
        Command::new("git")
            .args([
                "-C", &repo_str, "worktree", "add", "-b", ws_name, &dir_str, &tracking,
            ])
            .output()
    };

    match output {
        Ok(o) if !o.status.success() => {
            return Err(format!(
                "worktree create failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ));
        }
        Err(e) => return Err(format!("worktree create: {e}")),
        _ => {}
    }

    let _ = Command::new("git")
        .args(["-C", &repo_str, "branch", "--unset-upstream", ws_name])
        .output();
    Ok(())
}

fn remove_worktree(project: &Project, dir: &PathBuf) {
    if project.branch.is_none() {
        return;
    }
    if let Some(repo) = project.repo_path() {
        if dir.exists() {
            let _ = Command::new("git")
                .args([
                    "-C",
                    &repo.to_string_lossy(),
                    "worktree",
                    "remove",
                    &dir.to_string_lossy(),
                ])
                .output();
        }
    }
}
