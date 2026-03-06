use crate::interop::{self, AwesometreeExt, Project};
use crate::log as dlog;
use crate::state::{self, Store};
use crate::wm::{self, Adapter};
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

#[derive(Debug)]
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
            let tag = wm::tag_name(&project.name, ws_name);
            dlog::log(format!("Creating tag: {tag} (index: {tag_idx}, layout: {layout})"));
            self.wm.create_tag(&tag, tag_idx, layout)?;
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
                dlog::log(format!("Launching app: {expanded}"));
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
            let tag = wm::tag_name(&rw.project.name, ws_name);
            dlog::log(format!("Killing clients on tag: {tag}"));
            let _ = self.wm.kill_tag_clients(&tag);
            std::thread::sleep(std::time::Duration::from_millis(300));
            dlog::log(format!("Deleting tag: {tag}"));
            let _ = self.wm.delete_tag(&tag);
        }

        if !opts.keep_worktree {
            remove_worktree(&rw.project, &rw.dir)?;
        }

        self.state.set_inactive(ws_name);
        state::save(&self.state)
    }

    pub fn switch(&self, name: &str) -> Result<(), String> {
        let rw = self.resolve_workspace(name)?;
        let tag = wm::tag_name(&rw.project.name, name);
        dlog::log(format!("Switching to tag: {tag}"));
        self.wm.switch_tag(&tag)
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
        let ext = project.awesometree_ext();
        let base = match &ext.worktree_dir {
            Some(dir) => interop::expand_home(dir),
            None => interop::worktree_base().join(&project.name),
        };
        base.join(safe)
    } else {
        project
            .repo_path()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

pub fn ensure_worktree(ws_name: &str, project: &Project, dir: &PathBuf) -> Result<(), String> {
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

    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create worktree dir: {e}"))?;
    }

    let _ = Command::new("git")
        .args(["-C", &repo_str, "worktree", "prune"])
        .output();

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

fn remove_worktree(project: &Project, dir: &PathBuf) -> Result<(), String> {
    if project.branch.is_none() {
        return Ok(());
    }
    if let Some(repo) = project.repo_path() {
        if dir.exists() {
            let output = Command::new("git")
                .args([
                    "-C",
                    &repo.to_string_lossy(),
                    "worktree",
                    "remove",
                    &dir.to_string_lossy(),
                ])
                .output()
                .map_err(|e| format!("git worktree remove: {e}"))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                return Err(format!("git worktree remove failed: {stderr}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with_branch(name: &str, branch: &str) -> Project {
        Project::new(name, "/tmp/test-repo", branch)
    }

    fn project_no_branch(name: &str) -> Project {
        Project {
            name: name.into(),
            repo: Some("/tmp/test-repo".into()),
            version: "1".into(),
            ..Default::default()
        }
    }

    #[test]
    fn resolve_dir_with_branch() {
        let p = project_with_branch("myproj", "main");
        let dir = resolve_dir("feat-1", &p);
        assert!(dir.to_string_lossy().contains("myproj"));
        assert!(dir.to_string_lossy().ends_with("feat-1"));
    }

    #[test]
    fn resolve_dir_slash_replacement() {
        let p = project_with_branch("proj", "main");
        let dir = resolve_dir("user/feature", &p);
        assert!(dir.to_string_lossy().ends_with("user-feature"));
    }

    #[test]
    fn resolve_dir_no_branch_uses_repo() {
        let p = project_no_branch("proj");
        let dir = resolve_dir("ws", &p);
        assert_eq!(dir, PathBuf::from("/tmp/test-repo"));
    }

    #[test]
    fn resolve_dir_custom_worktree_dir() {
        let mut p = project_with_branch("proj", "main");
        let ext = interop::AwesometreeExt {
            worktree_dir: Some("/custom/wt".into()),
            ..Default::default()
        };
        p.set_awesometree_ext(&ext);
        let dir = resolve_dir("feat", &p);
        assert_eq!(dir, PathBuf::from("/custom/wt/feat"));
    }

    #[test]
    fn ensure_worktree_no_branch_noop() {
        let p = project_no_branch("proj");
        let dir = PathBuf::from("/tmp/nonexistent");
        assert!(ensure_worktree("ws", &p, &dir).is_ok());
    }

    #[test]
    fn ensure_worktree_dir_exists_noop() {
        let p = project_with_branch("proj", "main");
        let dir = PathBuf::from("/tmp");
        assert!(ensure_worktree("ws", &p, &dir).is_ok());
    }

    #[test]
    fn remove_worktree_no_branch_noop() {
        let p = project_no_branch("proj");
        let dir = PathBuf::from("/tmp/nonexistent");
        assert!(remove_worktree(&p, &dir).is_ok());
    }

    struct MockAdapter {
        tags_created: std::cell::RefCell<Vec<String>>,
        tags_deleted: std::cell::RefCell<Vec<String>>,
        tags_switched: std::cell::RefCell<Vec<String>>,
    }

    impl MockAdapter {
        fn new() -> Self {
            Self {
                tags_created: std::cell::RefCell::new(vec![]),
                tags_deleted: std::cell::RefCell::new(vec![]),
                tags_switched: std::cell::RefCell::new(vec![]),
            }
        }
    }

    impl wm::Adapter for MockAdapter {
        fn create_tag(&self, tag: &str, _index: i32, _layout: &str) -> Result<(), String> {
            self.tags_created.borrow_mut().push(tag.to_string());
            Ok(())
        }
        fn delete_tag(&self, tag: &str) -> Result<(), String> {
            self.tags_deleted.borrow_mut().push(tag.to_string());
            Ok(())
        }
        fn switch_tag(&self, tag: &str) -> Result<(), String> {
            self.tags_switched.borrow_mut().push(tag.to_string());
            Ok(())
        }
        fn kill_tag_clients(&self, _tag: &str) -> Result<(), String> {
            Ok(())
        }
        fn eval(&self, _lua: &str) -> Result<(), String> {
            Ok(())
        }
        fn get_current_tag_name(&self) -> Result<Option<String>, String> {
            Ok(None)
        }
        fn restore_previous_tag(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn manager_new() {
        let st = state::Store::default();
        let mgr = Manager::new(st, Box::new(MockAdapter::new()));
        assert!(mgr.state.all_names().is_empty());
    }

    #[test]
    fn manager_resolve_workspace_not_found() {
        let st = state::Store::default();
        let mgr = Manager::new(st, Box::new(MockAdapter::new()));
        let result = mgr.resolve_workspace("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("workspace not found"));
    }
}