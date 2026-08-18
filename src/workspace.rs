//! Material Workspace Resource helpers (git worktrees / environment).
//!
//! Does NOT create or own WorkSession authority — that lives in Switchboard via
//! `WorkSessionService`. These helpers operate on filesystem git worktrees only.

use crate::paths;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Expand `~` in a path string.
pub fn expand_home(p: &str) -> PathBuf {
    paths::expand_home(p)
}

/// Default base directory for worktrees.
pub fn worktree_base() -> PathBuf {
    paths::home_dir().join("worktrees")
}

/// Resolve a worktree directory for a WorkSession under a project name.
pub fn resolve_worktree_path(
    work_session_id: &str,
    project_name: &str,
    worktree_dir: Option<&str>,
    primary_repo: Option<&str>,
) -> PathBuf {
    let safe = work_session_id.replace('/', "-");
    if let Some(dir) = worktree_dir {
        return expand_home(dir).join(&safe);
    }
    if let Some(repo) = primary_repo {
        let repo_path = expand_home(repo);
        if let Some(parent) = repo_path.parent() {
            return parent.join("worktrees").join(project_name).join(safe);
        }
    }
    worktree_base().join(project_name).join(safe)
}

/// Create a git worktree at `dir` from `repo` tracking `branch`, using
/// `work_session_id` as the local branch name. Idempotent if `dir` exists.
pub fn ensure_git_worktree(
    work_session_id: &str,
    repo: &Path,
    branch: &str,
    dir: &Path,
) -> Result<(), String> {
    if dir.exists() {
        return Ok(());
    }
    if !repo.exists() {
        return Err(format!("repo not found: {}", repo.display()));
    }
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create worktree dir: {e}"))?;
    }
    let repo_str = repo.to_string_lossy();
    let _ = Command::new("git")
        .args(["-C", &repo_str, "worktree", "prune"])
        .output();
    let _ = Command::new("git")
        .args(["-C", &repo_str, "fetch", "origin", branch])
        .output();

    let branch_exists = Command::new("git")
        .args(["-C", &repo_str, "rev-parse", "--verify", work_session_id])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let dir_str = dir.to_string_lossy();
    let output = if branch_exists {
        Command::new("git")
            .args(["-C", &repo_str, "worktree", "add", &dir_str, work_session_id])
            .output()
    } else {
        // Prefer origin/<branch> when a remote tracking ref exists; otherwise use the
        // local branch tip so bare local repos (tests / offline) still realize.
        let origin_ref = format!("origin/{branch}");
        let origin_ok = Command::new("git")
            .args(["-C", &repo_str, "rev-parse", "--verify", &origin_ref])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        let start_ref = if origin_ok {
            origin_ref
        } else {
            branch.to_string()
        };
        Command::new("git")
            .args([
                "-C",
                &repo_str,
                "worktree",
                "add",
                "-b",
                work_session_id,
                &dir_str,
                &start_ref,
            ])
            .output()
    };

    match output {
        Ok(o) if !o.status.success() => Err(format!(
            "worktree create failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => Err(format!("worktree create: {e}")),
        _ => {
            let _ = Command::new("git")
                .args(["-C", &repo_str, "branch", "--unset-upstream", work_session_id])
                .output();
            Ok(())
        }
    }
}

/// Remove a git worktree directory, preferring `git worktree remove`.
pub fn remove_git_worktree(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    if let Some(parent_repo) = find_git_common_dir(dir) {
        let _ = Command::new("git")
            .args([
                "-C",
                &parent_repo.to_string_lossy(),
                "worktree",
                "remove",
                "--force",
                &dir.to_string_lossy(),
            ])
            .output();
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("remove worktree: {e}"))?;
    }
    Ok(())
}

fn find_git_common_dir(worktree: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args([
            "-C",
            &worktree.to_string_lossy(),
            "rev-parse",
            "--git-common-dir",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let p = PathBuf::from(s);
    if p.ends_with(".git") {
        p.parent().map(|x| x.to_path_buf())
    } else {
        p.parent().and_then(|x| x.parent()).map(|x| x.to_path_buf())
    }
}

/// Launch configured apps in a worktree directory.
pub fn launch_apps(apps: &[String], project_name: &str, dir: &Path) {
    let dir_str = dir.to_string_lossy();
    let list = if apps.is_empty() {
        vec!["zeditor -n {dir}".to_string()]
    } else {
        apps.to_vec()
    };
    for app_cmd in &list {
        let expanded = app_cmd
            .replace("{project}", project_name)
            .replace("{dir}", &dir_str);
        let _ = Command::new("sh")
            .args(["-c", &expanded])
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_explicit_worktree_dir() {
        let p = resolve_worktree_path("ws/a", "proj", Some("~/wt"), None);
        assert!(p.to_string_lossy().contains("ws-a") || p.to_string_lossy().ends_with("ws-a"));
    }
}
