//! Single application service coordinating Switchboard authority and local realization.

use crate::acp_supervisor;
use crate::bezalel_supervisor;
use crate::log as dlog;
use crate::model::error::{ErrorCode, Result, SwitchboardError};
use crate::model::lifecycle::WorkSessionState;
use crate::model::project::ProjectEnvelope;
use crate::model::resource::{ResourceBinding, WorkspaceResourceRef};
use crate::model::runtime::{RealizationStatus, WorkSessionRuntime};
use crate::model::work_session::{
    CreateWorkSessionRequest, CreateWorkSessionResponse, RealizationOptions, WorkSession,
    WorkSessionView, DEFAULT_WORK_PROFILE_ID,
};
use crate::model::WorkProfile;
use crate::paths;
use crate::runtime_store;
use crate::switchboard::Catalog;
use crate::wm::{self, Adapter};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

pub struct WorkSessionService {
    catalog: Arc<dyn Catalog>,
    wm: Option<Box<dyn Adapter>>,
}

impl WorkSessionService {
    pub fn new(catalog: Arc<dyn Catalog>, wm: Option<Box<dyn Adapter>>) -> Self {
        Self { catalog, wm }
    }

    pub fn catalog(&self) -> &Arc<dyn Catalog> {
        &self.catalog
    }

    // ── Projects (pass-through) ──────────────────────────────────────────

    pub async fn list_projects(
        &self,
        query: Option<&str>,
    ) -> Result<Vec<crate::model::ProjectSummary>> {
        self.catalog.list_projects(query).await
    }

    pub async fn get_project(&self, id: &str) -> Result<ProjectEnvelope> {
        self.catalog.get_project(id).await
    }

    pub async fn create_project(
        &self,
        definition: serde_json::Value,
    ) -> Result<crate::model::ProjectSummary> {
        self.catalog.create_project(definition).await
    }

    pub async fn update_project(
        &self,
        project_id: &str,
        expected_source_revision: &str,
        patch: serde_json::Value,
    ) -> Result<crate::model::ProjectSummary> {
        self.catalog
            .update_project(project_id, expected_source_revision, patch)
            .await
    }

    pub async fn delete_project(
        &self,
        project_id: &str,
        expected_source_revision: &str,
    ) -> Result<()> {
        self.catalog
            .delete_project(project_id, expected_source_revision)
            .await
    }

    // ── WorkProfiles ─────────────────────────────────────────────────────

    pub async fn list_work_profiles(&self) -> Result<Vec<WorkProfile>> {
        self.catalog.list_work_profiles().await
    }

    pub async fn get_work_profile(&self, id: &str) -> Result<WorkProfile> {
        self.catalog.get_work_profile(id).await
    }

    pub async fn resolve_default_work_profile(&self) -> Result<WorkProfile> {
        match self.catalog.get_work_profile(DEFAULT_WORK_PROFILE_ID).await {
            Ok(p) => Ok(p),
            Err(e) if e.code == ErrorCode::NotFound => Err(SwitchboardError::missing_default()),
            Err(e) => Err(e),
        }
    }

    // ── WorkSessions ─────────────────────────────────────────────────────

    pub async fn list_work_sessions(
        &self,
        state: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Vec<WorkSessionView>> {
        let sessions = self.catalog.list_work_sessions(state, project_id).await?;
        let runtimes = runtime_store::load_all().unwrap_or_default();
        Ok(sessions
            .into_iter()
            .map(|ws| {
                let runtime = runtimes.get(&ws.work_session_id).cloned();
                WorkSessionView {
                    work_session: ws,
                    runtime,
                }
            })
            .collect())
    }

    pub async fn get_work_session(&self, id: &str) -> Result<WorkSessionView> {
        let ws = self.catalog.get_work_session(id).await?;
        let runtime = runtime_store::get(id)?;
        Ok(WorkSessionView {
            work_session: ws,
            runtime,
        })
    }

    /// Create (or reconcile) a WorkSession and realize local Workspace resources.
    pub async fn create_work_session(
        &self,
        req: CreateWorkSessionRequest,
    ) -> Result<CreateWorkSessionResponse> {
        let profile_id = req.resolved_work_profile_id().to_string();

        // Resolve default profile explicitly so missing-default is typed.
        if req.work_profile_id.is_none() {
            self.resolve_default_work_profile().await?;
        } else {
            let p = self.catalog.get_work_profile(&profile_id).await.map_err(|e| {
                if e.code == ErrorCode::NotFound {
                    SwitchboardError::new(
                        ErrorCode::InvalidReference,
                        format!("work_profile_id {profile_id:?} not found"),
                    )
                    .with_entity("work_profile", &profile_id)
                } else {
                    e
                }
            })?;
            if !p.applies_to(&req.project_id) {
                return Err(SwitchboardError::new(
                    ErrorCode::InvalidReference,
                    format!("work profile {profile_id} is not associated with project {}", req.project_id),
                )
                .with_entity("work_profile", &profile_id));
            }
        }

        let project = self.catalog.get_project(&req.project_id).await?;

        let proposed = WorkSession {
            version: "1".into(),
            work_session_id: req.work_session_id.clone(),
            display_name: req
                .display_name
                .clone()
                .or_else(|| Some(req.work_session_id.clone())),
            project_id: Some(req.project_id.clone()),
            project_snapshot_id: None, // Switchboard fills from live revision
            project_revision: None,
            work_profile_id: Some(profile_id.clone()),
            agent_profile_ids: vec![],
            state: WorkSessionState::Proposed,
            policy: None,
            created_at: None,
            updated_at: None,
            closed_at: None,
        };

        let session = self.catalog.create_work_session(proposed).await?;

        // Realize local resources; compensate to aborted on failure.
        match self
            .realize(&session, &project, &req.realization)
            .await
        {
            Ok(runtime) => {
                // On transition failure leave proposed with runtime for retry.
                let open = self
                    .catalog
                    .transition_work_session(&session.work_session_id, WorkSessionState::Open)
                    .await?;
                Ok(CreateWorkSessionResponse {
                    work_profile_id: profile_id,
                    project_revision: open.project_revision.clone(),
                    project_snapshot_id: open.project_snapshot_id.clone(),
                    realization_status: runtime.realization_status,
                    runtime: Some(runtime),
                    work_session: open,
                    error: None,
                })
            }
            Err(realize_err) => {
                let _ = self.cleanup_partial(&session.work_session_id).await;
                let abort_err = self
                    .catalog
                    .transition_work_session(&session.work_session_id, WorkSessionState::Aborted)
                    .await
                    .err();
                let mut msg = format!("realization failed: {realize_err}");
                if let Some(ae) = abort_err {
                    msg.push_str(&format!("; compensation failed: {ae}"));
                }
                Err(SwitchboardError::new(ErrorCode::InternalError, msg)
                    .with_operation("create_work_session")
                    .with_entity("work_session", &req.work_session_id)
                    .with_cause(realize_err.to_string()))
            }
        }
    }

    pub async fn transition(
        &self,
        id: &str,
        to: WorkSessionState,
    ) -> Result<WorkSessionView> {
        let current = self.catalog.get_work_session(id).await?;
        if !current.state.can_transition_to(to) {
            return Err(SwitchboardError::new(
                ErrorCode::InvalidTransition,
                format!("cannot transition from {} to {to}", current.state),
            )
            .with_entity("work_session", id));
        }

        // Local side effects before/after authoritative transition.
        match (current.state, to) {
            (_, WorkSessionState::Paused) => {
                self.pause_local(id).await?;
            }
            (WorkSessionState::Paused, WorkSessionState::Open) => {
                self.resume_local(id).await?;
            }
            (_, WorkSessionState::Closed) | (_, WorkSessionState::Aborted) => {
                self.teardown_local(id, false).await?;
            }
            _ => {}
        }

        let ws = self.catalog.transition_work_session(id, to).await?;
        let runtime = runtime_store::get(id)?;
        Ok(WorkSessionView {
            work_session: ws,
            runtime,
        })
    }

    pub async fn destroy(&self, id: &str, keep_worktree: bool) -> Result<()> {
        // Prefer close then delete when open/paused.
        if let Ok(ws) = self.catalog.get_work_session(id).await {
            if !ws.state.is_terminal() {
                let _ = self
                    .transition(id, WorkSessionState::Closed)
                    .await;
            }
        }
        self.teardown_local(id, keep_worktree).await?;
        self.catalog.delete_work_session(id).await?;
        let _ = runtime_store::remove(id);
        let _ = runtime_store::clear_bezalel_token(id);
        Ok(())
    }

    /// Reconcile local runtime against Switchboard on daemon startup.
    pub async fn reconcile_on_startup(&self) -> Result<Vec<String>> {
        let mut notes = Vec::new();
        let sessions = self.catalog.list_work_sessions(None, None).await?;
        let mut local = runtime_store::load_all()?;
        let host = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_default();

        for ws in &sessions {
            if matches!(ws.state, WorkSessionState::Open | WorkSessionState::Paused) {
                match local.remove(&ws.work_session_id) {
                    Some(rt) => {
                        if let Some(ref w) = rt.workspace {
                            if !PathBuf::from(&w.path).exists() {
                                notes.push(format!(
                                    "work_session {}: missing worktree at {}",
                                    ws.work_session_id, w.path
                                ));
                            }
                        }
                    }
                    None => {
                        notes.push(format!(
                            "work_session {}: open in Switchboard but no local runtime on host {host}",
                            ws.work_session_id
                        ));
                    }
                }
            }
        }
        // Orphan local runtimes (no authoritative session).
        for (id, rt) in local {
            if rt.host_id == host || rt.host_id.is_empty() {
                notes.push(format!(
                    "orphan local runtime {id} (no Switchboard WorkSession); cleaning"
                ));
                let _ = self.teardown_local(&id, false).await;
                let _ = runtime_store::remove(&id);
            }
        }
        Ok(notes)
    }

    // ── Local realization ────────────────────────────────────────────────

    async fn realize(
        &self,
        session: &WorkSession,
        project: &ProjectEnvelope,
        opts: &RealizationOptions,
    ) -> Result<WorkSessionRuntime> {
        let ws_id = &session.work_session_id;
        // Idempotent: reuse existing worktree if present.
        if let Ok(Some(existing)) = runtime_store::get(ws_id) {
            if existing.realization_status == RealizationStatus::Ready {
                if let Some(ref w) = existing.workspace {
                    if PathBuf::from(&w.path).exists() {
                        return Ok(existing);
                    }
                }
            }
        }

        let dir = resolve_worktree_dir(ws_id, project);
        ensure_worktree(ws_id, project, &dir)?;

        let tag_idx = runtime_store::allocate_tag_index(ws_id)?;
        let ext = project.awesometree_ext();
        let layout = if ext.layout.is_empty() {
            "tile"
        } else {
            &ext.layout
        };
        let project_name = project.name();
        let tag = wm::tag_name(project_name, ws_id);

        if opts.create_tag && !opts.no_wm {
            if let Some(ref wm) = self.wm {
                dlog::log(format!("Creating tag: {tag} (index: {tag_idx}, layout: {layout})"));
                if let Err(e) = wm.create_tag(&tag, tag_idx, layout) {
                    dlog::log(format!("Warning: create tag failed: {e}"));
                } else {
                    let _ = wm.switch_tag(&tag);
                }
            }
        }

        let acp_port = runtime_store::allocate_acp_port(ws_id)?;
        if opts.launch_apps && !opts.headless {
            launch_apps(project, &dir, acp_port);
        }

        let acp_url = start_acp_if_configured(ws_id, &dir, acp_port, project);

        let mut bezalel_port = None;
        let mut bezalel_token_ref = None;
        if opts.headless {
            if let Some(port) = runtime_store::allocate_bezalel_port(ws_id)? {
                let token = bezalel_supervisor::generate_token();
                runtime_store::set_bezalel_token(ws_id, &token)?;
                bezalel_supervisor::start_for_workspace(
                    ws_id,
                    &dir.to_string_lossy(),
                    port,
                    &token,
                );
                bezalel_port = Some(port);
                bezalel_token_ref = Some(format!("bezalel:{ws_id}"));
            }
        }

        let resource_id = format!("workspace:{ws_id}");
        let workspace = WorkspaceResourceRef {
            workspace_id: format!("wt-{ws_id}"),
            resource_id: resource_id.clone(),
            environment_kind: "git-worktree".into(),
            path: dir.to_string_lossy().into_owned(),
        };
        let binding = ResourceBinding {
            work_session_id: ws_id.clone(),
            resource_id,
            locator: workspace.path.clone(),
            grant: None,
        };

        let runtime = WorkSessionRuntime {
            work_session_id: ws_id.clone(),
            host_id: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_default(),
            workspace: Some(workspace),
            resource_binding: Some(binding),
            tag_index: Some(tag_idx),
            tag_name: Some(tag),
            acp_port,
            acp_url,
            acp_session_id: None,
            headless: opts.headless,
            bezalel_port,
            bezalel_token_ref,
            process_ids: vec![],
            realization_status: RealizationStatus::Ready,
            last_error: None,
        };
        runtime_store::upsert(runtime.clone())?;
        Ok(runtime)
    }

    async fn cleanup_partial(&self, id: &str) -> Result<()> {
        self.teardown_local(id, false).await
    }

    async fn pause_local(&self, id: &str) -> Result<()> {
        acp_supervisor::stop_for_workspace(id);
        bezalel_supervisor::stop_for_workspace(id);
        let _ = runtime_store::modify(id, |rt| {
            rt.realization_status = RealizationStatus::Degraded;
        });
        Ok(())
    }

    async fn resume_local(&self, id: &str) -> Result<()> {
        // ACP/bezalel supervisors may be restarted by daemon sync; mark ready.
        let _ = runtime_store::modify(id, |rt| {
            rt.realization_status = RealizationStatus::Ready;
        });
        Ok(())
    }

    async fn teardown_local(&self, id: &str, keep_worktree: bool) -> Result<()> {
        acp_supervisor::stop_for_workspace(id);
        bezalel_supervisor::stop_for_workspace(id);

        if let Ok(Some(rt)) = runtime_store::get(id) {
            if let (Some(tag), Some(wm)) = (rt.tag_name.as_ref(), self.wm.as_ref()) {
                let _ = wm.kill_tag_clients(tag);
                std::thread::sleep(std::time::Duration::from_millis(200));
                let _ = wm.delete_tag(tag);
            }
            if !keep_worktree {
                if let Some(ws) = rt.workspace {
                    let path = PathBuf::from(&ws.path);
                    if path.exists() {
                        // Prefer git worktree remove when possible.
                        if let Some(parent_repo) = find_git_common_dir(&path) {
                            let _ = Command::new("git")
                                .args([
                                    "-C",
                                    &parent_repo.to_string_lossy(),
                                    "worktree",
                                    "remove",
                                    "--force",
                                    &ws.path,
                                ])
                                .output();
                        }
                        if path.exists() {
                            let _ = fs_remove_dir_all(&path);
                        }
                    }
                }
            }
            let _ = runtime_store::modify(id, |r| {
                r.realization_status = RealizationStatus::Cleaned;
                r.tag_index = None;
                r.tag_name = None;
                r.acp_port = None;
                r.acp_url = None;
                r.bezalel_port = None;
            });
        }
        let _ = runtime_store::clear_bezalel_token(id);
        Ok(())
    }
}

fn fs_remove_dir_all(path: &PathBuf) -> std::io::Result<()> {
    std::fs::remove_dir_all(path)
}

fn find_git_common_dir(worktree: &PathBuf) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["-C", &worktree.to_string_lossy(), "rev-parse", "--git-common-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let p = PathBuf::from(s);
    // common-dir is often .../repo/.git → repo is parent
    if p.ends_with(".git") {
        p.parent().map(|x| x.to_path_buf())
    } else {
        // worktree gitdir points at main .git
        p.parent()
            .and_then(|x| x.parent())
            .map(|x| x.to_path_buf())
    }
}

fn resolve_worktree_dir(ws_id: &str, project: &ProjectEnvelope) -> PathBuf {
    let safe = ws_id.replace('/', "-");
    let ext = project.awesometree_ext();
    if let Some(dir) = &ext.worktree_dir {
        return paths::expand_home(dir).join(&safe);
    }
    if let Some(repo) = project.primary_repo() {
        let repo_path = paths::expand_home(&repo);
        if let Some(parent) = repo_path.parent() {
            return parent
                .join("worktrees")
                .join(project.name())
                .join(safe);
        }
    }
    paths::home_dir()
        .join("worktrees")
        .join(project.name())
        .join(safe)
}

fn ensure_worktree(ws_id: &str, project: &ProjectEnvelope, dir: &PathBuf) -> Result<()> {
    let branch = match project.branch() {
        Some(b) => b,
        None => {
            // No branch → use repo path directly if present.
            return Ok(());
        }
    };
    if dir.exists() {
        return Ok(());
    }
    let repo = project
        .primary_repo()
        .ok_or_else(|| {
            SwitchboardError::new(ErrorCode::InvalidInput, "project has no repo path")
                .with_operation("ensure_worktree")
        })?;
    let repo_path = paths::expand_home(&repo);
    let repo_str = repo_path.to_string_lossy();
    if !repo_path.exists() {
        return Err(SwitchboardError::new(
            ErrorCode::InvalidReference,
            format!("repo not found: {repo_str}"),
        ));
    }
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("create worktree dir: {e}"))
        })?;
    }
    let _ = Command::new("git")
        .args(["-C", &repo_str, "worktree", "prune"])
        .output();
    let _ = Command::new("git")
        .args(["-C", &repo_str, "fetch", "origin", &branch])
        .output();

    let branch_exists = Command::new("git")
        .args(["-C", &repo_str, "rev-parse", "--verify", ws_id])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let dir_str = dir.to_string_lossy();
    let output = if branch_exists {
        Command::new("git")
            .args(["-C", &repo_str, "worktree", "add", &dir_str, ws_id])
            .output()
    } else {
        let tracking = format!("origin/{branch}");
        Command::new("git")
            .args([
                "-C",
                &repo_str,
                "worktree",
                "add",
                "-b",
                ws_id,
                &dir_str,
                &tracking,
            ])
            .output()
    };

    match output {
        Ok(o) if !o.status.success() => {
            return Err(SwitchboardError::new(
                ErrorCode::InternalError,
                format!(
                    "worktree create failed: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
            ));
        }
        Err(e) => {
            return Err(SwitchboardError::new(
                ErrorCode::InternalError,
                format!("worktree create: {e}"),
            ));
        }
        _ => {}
    }
    let _ = Command::new("git")
        .args(["-C", &repo_str, "branch", "--unset-upstream", ws_id])
        .output();
    Ok(())
}

fn start_acp_if_configured(
    ws_id: &str,
    dir: &PathBuf,
    acp_port: Option<u16>,
    project: &ProjectEnvelope,
) -> Option<String> {
    let acp = project.awesometree_ext().acp?;
    if !acp.enabled {
        return None;
    }
    let port = acp_port?;
    let dir_str = dir.to_string_lossy();
    let cmd = acp.command.as_deref();
    acp_supervisor::start_for_workspace(ws_id, &dir_str, port, cmd);
    let url = acp.url.as_deref().unwrap_or("http://127.0.0.1:{port}");
    Some(url.replace("{port}", &port.to_string()).replace(
        "{project}",
        project.name(),
    ).replace("{dir}", &dir_str))
}

fn launch_apps(project: &ProjectEnvelope, dir: &PathBuf, acp_port: Option<u16>) {
    let ext = project.awesometree_ext();
    let dir_str = dir.to_string_lossy();
    let apps = if ext.apps.is_empty() {
        vec!["zeditor -n {dir}".to_string()]
    } else {
        ext.apps.clone()
    };
    for app_cmd in &apps {
        let mut expanded = app_cmd
            .replace("{project}", project.name())
            .replace("{dir}", &dir_str);
        if let Some(p) = acp_port {
            expanded = expanded.replace("{port}", &p.to_string());
        }
        dlog::log(format!("Launching app: {expanded}"));
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
    use crate::model::error::ErrorCode;
    use crate::model::project::ProjectSummary;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeCatalog {
        profiles: Mutex<HashMap<String, WorkProfile>>,
        sessions: Mutex<HashMap<String, WorkSession>>,
        projects: Mutex<HashMap<String, ProjectEnvelope>>,
    }

    impl FakeCatalog {
        fn with_default() -> Self {
            let mut profiles = HashMap::new();
            profiles.insert(
                "default".into(),
                WorkProfile {
                    version: "1".into(),
                    work_profile_id: "default".into(),
                    display_name: Some("default".into()),
                    description: None,
                    project_ids: vec![],
                    intended_resources: vec![],
                    default_policy: None,
                },
            );
            Self {
                profiles: Mutex::new(profiles),
                sessions: Mutex::new(HashMap::new()),
                projects: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl Catalog for FakeCatalog {
        async fn health(&self) -> Result<()> {
            Ok(())
        }
        async fn list_projects(&self, _: Option<&str>) -> Result<Vec<ProjectSummary>> {
            Ok(vec![])
        }
        async fn get_project(&self, id: &str) -> Result<ProjectEnvelope> {
            self.projects
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or_else(|| {
                    SwitchboardError::new(ErrorCode::NotFound, "project not found")
                        .with_entity("project", id)
                })
        }
        async fn create_project(&self, _: serde_json::Value) -> Result<ProjectSummary> {
            unimplemented!()
        }
        async fn update_project(
            &self,
            _: &str,
            _: &str,
            _: serde_json::Value,
        ) -> Result<ProjectSummary> {
            unimplemented!()
        }
        async fn delete_project(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        async fn list_work_profiles(&self) -> Result<Vec<WorkProfile>> {
            Ok(self.profiles.lock().unwrap().values().cloned().collect())
        }
        async fn get_work_profile(&self, id: &str) -> Result<WorkProfile> {
            self.profiles.lock().unwrap().get(id).cloned().ok_or_else(|| {
                SwitchboardError::new(ErrorCode::NotFound, "not found").with_entity("work_profile", id)
            })
        }
        async fn put_work_profile(&self, p: WorkProfile) -> Result<WorkProfile> {
            self.profiles
                .lock()
                .unwrap()
                .insert(p.work_profile_id.clone(), p.clone());
            Ok(p)
        }
        async fn delete_work_profile(&self, id: &str) -> Result<()> {
            self.profiles.lock().unwrap().remove(id);
            Ok(())
        }
        async fn list_work_sessions(
            &self,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<Vec<WorkSession>> {
            Ok(self.sessions.lock().unwrap().values().cloned().collect())
        }
        async fn get_work_session(&self, id: &str) -> Result<WorkSession> {
            self.sessions.lock().unwrap().get(id).cloned().ok_or_else(|| {
                SwitchboardError::new(ErrorCode::NotFound, "not found").with_entity("work_session", id)
            })
        }
        async fn create_work_session(&self, mut s: WorkSession) -> Result<WorkSession> {
            s.project_revision = Some("sha256:".to_string() + &"ab".repeat(32));
            s.project_snapshot_id = Some(format!(
                "project://registry/projects/{}/revisions/{}",
                s.project_id.as_deref().unwrap_or(""),
                s.project_revision.as_deref().unwrap_or("")
            ));
            self.sessions
                .lock()
                .unwrap()
                .insert(s.work_session_id.clone(), s.clone());
            Ok(s)
        }
        async fn transition_work_session(
            &self,
            id: &str,
            state: WorkSessionState,
        ) -> Result<WorkSession> {
            let mut g = self.sessions.lock().unwrap();
            let s = g.get_mut(id).ok_or_else(|| {
                SwitchboardError::new(ErrorCode::NotFound, "not found").with_entity("work_session", id)
            })?;
            s.state = state;
            Ok(s.clone())
        }
        async fn patch_work_session(
            &self,
            id: &str,
            _: Option<String>,
            _: Option<serde_json::Value>,
        ) -> Result<WorkSession> {
            self.get_work_session(id).await
        }
        async fn delete_work_session(&self, id: &str) -> Result<()> {
            self.sessions.lock().unwrap().remove(id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn missing_default_fails_without_session() {
        let fake = FakeCatalog::with_default();
        fake.profiles.lock().unwrap().remove("default");
        let svc = WorkSessionService::new(Arc::new(fake), None);
        let err = svc
            .create_work_session(CreateWorkSessionRequest {
                work_session_id: "ws".into(),
                project_id: "p".into(),
                work_profile_id: None,
                display_name: None,
                realization: RealizationOptions {
                    no_wm: true,
                    ..Default::default()
                },
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::MissingDefaultProfile);
    }

    #[tokio::test]
    async fn resolves_exact_default_id() {
        let fake = FakeCatalog::with_default();
        // Confusable display-name profile must not win.
        fake.profiles.lock().unwrap().insert(
            "other".into(),
            WorkProfile {
                version: "1".into(),
                work_profile_id: "other".into(),
                display_name: Some("default".into()),
                description: None,
                project_ids: vec![],
                intended_resources: vec![],
                default_policy: None,
            },
        );
        let p = WorkSessionService::new(Arc::new(fake), None)
            .resolve_default_work_profile()
            .await
            .unwrap();
        assert_eq!(p.work_profile_id, "default");
    }
}
