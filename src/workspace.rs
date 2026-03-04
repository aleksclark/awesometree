use crate::config::{self, Config, Workspace};
use crate::wm::Adapter;
use std::process::Command;

pub struct Manager {
    pub config: Config,
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

impl Manager {
    pub fn new(config: Config, wm: Box<dyn Adapter>) -> Self {
        Self { config, wm }
    }

    pub fn up(&mut self, ws: &Workspace, opts: &UpOptions) -> Result<(), String> {
        eprintln!("  Creating workspace: {}", ws.name);
        self.ensure_worktree(ws)?;

        let dir = ws.resolve_dir();
        let tag_idx = config::allocate_tag_index(&ws.name, &self.config);

        if opts.create_tag {
            self.wm
                .create_tag(&ws.name, tag_idx, ws.resolve_layout())?;
        }

        if opts.launch_apps {
            let dir_str = dir.to_string_lossy();
            let _ = Command::new("zeditor")
                .args(["-n", &dir_str])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            for gui_cmd in &ws.gui {
                let _ = Command::new("sh")
                    .args(["-c", gui_cmd])
                    .current_dir(&dir)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
        }

        let dir_str = dir.to_string_lossy().into_owned();
        self.config.set_workspace_active(&ws.name, true, tag_idx, &dir_str);
        config::save_config(&self.config)
    }

    pub fn down(&mut self, ws: &Workspace, opts: &DownOptions) -> Result<(), String> {
        eprintln!("  Removing workspace: {}", ws.name);

        if opts.manage_tag {
            let _ = self.wm.kill_tag_clients(&ws.name);
            let _ = self.wm.delete_tag(&ws.name);
        }

        if !opts.keep_worktree {
            self.remove_worktree(ws);
        }

        self.config.set_workspace_inactive(&ws.name);
        config::save_config(&self.config)
    }

    pub fn switch(&self, name: &str) -> Result<(), String> {
        self.wm.switch_tag(name)
    }

    pub fn is_dirty(&self, ws: &Workspace) -> Result<bool, String> {
        let dir = ws.resolve_dir();
        let output = Command::new("git")
            .args(["-C", &dir.to_string_lossy(), "status", "--porcelain"])
            .output()
            .map_err(|e| format!("git status: {e}"))?;
        Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }

    fn ensure_worktree(&self, ws: &Workspace) -> Result<(), String> {
        let branch = ws.resolve_branch();
        if branch.is_empty() {
            return Ok(());
        }
        let dir = ws.resolve_dir();
        if dir.exists() {
            return Ok(());
        }

        let repo = ws.resolve_repo();
        if !std::path::Path::new(&repo).exists() {
            return Err(format!("repo not found: {repo}"));
        }

        let base = config::worktree_base();
        std::fs::create_dir_all(&base).map_err(|e| format!("create worktree base: {e}"))?;

        let _ = Command::new("git")
            .args(["-C", &repo, "fetch", "origin", &branch])
            .output();

        let branch_exists = Command::new("git")
            .args(["-C", &repo, "rev-parse", "--verify", &ws.name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let dir_str = dir.to_string_lossy();
        let output = if branch_exists {
            Command::new("git")
                .args(["-C", &repo, "worktree", "add", &dir_str, &ws.name])
                .output()
        } else {
            let tracking = format!("origin/{branch}");
            Command::new("git")
                .args([
                    "-C", &repo, "worktree", "add", "-b", &ws.name, &dir_str, &tracking,
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
            .args(["-C", &repo, "branch", "--unset-upstream", &ws.name])
            .output();
        Ok(())
    }

    fn remove_worktree(&self, ws: &Workspace) {
        let branch = ws.resolve_branch();
        if branch.is_empty() {
            return;
        }
        let dir = ws.resolve_dir();
        let repo = ws.resolve_repo();
        if dir.exists() {
            let _ = Command::new("git")
                .args([
                    "-C",
                    &repo,
                    "worktree",
                    "remove",
                    &dir.to_string_lossy(),
                ])
                .output();
        }
    }
}
