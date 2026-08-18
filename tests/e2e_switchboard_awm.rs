//! End-to-end AWM cutover tests through production Switchboard MCP + real git.
//!
//! Starts a real Switchboard binary with an isolated temp config root and drives
//! WorkSessionService over the production MCP client. Skip only when the binary
//! cannot be executed; set SWITCHBOARD_REQUIRED=1 to fail hard instead.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use awesometree::switchboard::{Catalog, SwitchboardClient, SwitchboardConfig};
use std::sync::{Mutex, OnceLock};

fn e2e_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|p| p.into_inner())
}



fn switchboard_required() -> bool {
    matches!(
        std::env::var("SWITCHBOARD_REQUIRED").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

fn switchboard_bin() -> PathBuf {
    if let Ok(p) = std::env::var("SWITCHBOARD_BIN") {
        return PathBuf::from(p);
    }
    let candidate = PathBuf::from(
        "/home/aleks/work/projects/switchboard/worktrees/impl-project-catalog-mcp/dist/switchboard",
    );
    if candidate.exists() {
        return candidate;
    }
    // Local install fallback.
    if let Ok(p) = which_switchboard() {
        return p;
    }
    PathBuf::from("switchboard")
}

fn which_switchboard() -> Result<PathBuf, ()> {
    let out = Command::new("which").arg("switchboard").output().map_err(|_| ())?;
    if !out.status.success() {
        return Err(());
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        Err(())
    } else {
        Ok(PathBuf::from(s))
    }
}

fn bin_runnable(bin: &Path) -> bool {
    Command::new(bin)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn require_switchboard_bin() -> Option<PathBuf> {
    let bin = switchboard_bin();
    if bin_runnable(&bin) {
        return Some(bin);
    }
    if switchboard_required() {
        panic!(
            "SWITCHBOARD_REQUIRED=1 but switchboard is not runnable at {}",
            bin.display()
        );
    }
    eprintln!(
        "skip: switchboard binary not runnable at {} (set SWITCHBOARD_REQUIRED=1 to fail hard)",
        bin.display()
    );
    None
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct SwitchboardProc {
    child: Child,
    endpoint: String,
    /// Keep config root alive for process lifetime.
    _config_root: tempfile::TempDir,
}

impl Drop for SwitchboardProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_switchboard() -> Option<SwitchboardProc> {
    let bin = match require_switchboard_bin() {
        Some(b) => b,
        None => return None,
    };
    let config_root = tempfile::tempdir().expect("temp config");
    let xdg = config_root.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    let port = free_port();
    let endpoint = format!("http://127.0.0.1:{port}/mcp");

    let child = Command::new(&bin)
        .args(["-port", &port.to_string()])
        .env("HOME", config_root.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn switchboard from {}: {e}", bin.display()));

    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_millis(200),
        )
        .is_ok()
        {
            return Some(SwitchboardProc {
                child,
                endpoint,
                _config_root: config_root,
            });
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("switchboard did not become ready on port {port}");
}

fn init_git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let repo_s = repo.to_str().unwrap();
    assert!(Command::new("git")
        .args(["-C", repo_s, "init", "-b", "master"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["-C", repo_s, "config", "user.email", "test@example.com"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["-C", repo_s, "config", "user.name", "test"])
        .status()
        .unwrap()
        .success());
    std::fs::write(repo.join("README.md"), "e2e\n").unwrap();
    assert!(Command::new("git")
        .args(["-C", repo_s, "add", "."])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["-C", repo_s, "commit", "-m", "init"])
        .status()
        .unwrap()
        .success());
    repo
}

struct EnvGuard {
    keys: Vec<&'static str>,
}

impl EnvGuard {
    fn set(pairs: &[(&'static str, String)]) -> Self {
        let mut keys = Vec::new();
        for (k, v) in pairs {
            // SAFETY: single-threaded test process env mutation for isolation.
            unsafe { std::env::set_var(k, v); }
            keys.push(*k);
        }
        Self { keys }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for k in &self.keys {
            unsafe { std::env::remove_var(k); }
        }
    }
}

async fn wait_healthy(catalog: &awesometree::switchboard::SwitchboardClient) {
    let mut last = None;
    for _ in 0..60 {
        match catalog.health().await {
            Ok(()) => return,
            Err(e) => {
                last = Some(e.to_string());
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
    panic!(
        "switchboard MCP not healthy at {}: {}",
        catalog.endpoint(),
        last.unwrap_or_else(|| "unknown".into())
    );
}

#[tokio::test]
async fn e2e_default_profile_work_session_create() {
    let _guard = e2e_env_lock();
    let Some(sb) = start_switchboard() else { return; };

    let tmp = tempfile::tempdir().unwrap();
    let runtime_path = tmp.path().join("runtime.json");
    let secrets_path = tmp.path().join("secrets.json");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".config/awesometree")).unwrap();

    let _env = EnvGuard::set(&[
        (
            "AWESOMETREE_SWITCHBOARD_URL",
            sb.endpoint.clone(),
        ),
        (
            "AWESOMETREE_RUNTIME_PATH",
            runtime_path.to_string_lossy().into_owned(),
        ),
        (
            "AWESOMETREE_SECRETS_PATH",
            secrets_path.to_string_lossy().into_owned(),
        ),
        ("HOME", home.to_string_lossy().into_owned()),
    ]);

    let repo = init_git_repo(tmp.path());

    let catalog = std::sync::Arc::new(SwitchboardClient::new(
        SwitchboardConfig::with_endpoint(sb.endpoint.clone()),
    ));
    wait_healthy(&catalog).await;

    let svc = awesometree::work_session_service::WorkSessionService::new(catalog, None);

    let default = svc
        .resolve_default_work_profile()
        .await
        .expect("default WorkProfile must exist");
    assert_eq!(default.work_profile_id, "default");

    let def = awesometree::model::project::definition_for_create(
        "e2e-proj",
        Some("e2e"),
        Some(repo.to_str().unwrap()),
        Some("master"),
        None,
    );
    let project = svc
        .create_project(def)
        .await
        .expect("create project via Switchboard");
    assert_eq!(project.project_id, "e2e-proj");

    // Omit profile → exact ID "default".
    let resp = svc
        .create_work_session(awesometree::model::work_session::CreateWorkSessionRequest {
            work_session_id: "e2e-ws-1".into(),
            project_id: "e2e-proj".into(),
            work_profile_id: None,
            display_name: Some("E2E".into()),
            realization: awesometree::model::work_session::RealizationOptions {
                create_tag: false,
                launch_apps: false,
                headless: false,
                no_wm: true,
            },
        })
        .await
        .expect("create work session");

    assert_eq!(resp.work_profile_id, "default");
    assert_eq!(resp.work_session.work_session_id, "e2e-ws-1");
    assert!(
        matches!(
            resp.work_session.state,
            awesometree::model::lifecycle::WorkSessionState::Open
                | awesometree::model::lifecycle::WorkSessionState::Proposed
        ),
        "state={}",
        resp.work_session.state
    );
    assert!(
        resp.work_session
            .project_revision
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
            || resp
                .project_revision
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false),
        "must pin a project revision"
    );

    // Direct Switchboard read-back agrees.
    let got = svc.get_work_session("e2e-ws-1").await.expect("get");
    assert_eq!(got.work_session.work_profile_id.as_deref(), Some("default"));
    assert_eq!(got.work_session.project_id.as_deref(), Some("e2e-proj"));

    // Runtime is host-local and keyed by work_session_id.
    let rt = awesometree::runtime_store::get("e2e-ws-1")
        .expect("runtime load")
        .expect("runtime present");
    assert_eq!(rt.work_session_id, "e2e-ws-1");

    let serialized = serde_json::to_string(&rt).unwrap();
    assert!(
        !serialized.contains("acp_"),
        "runtime must not carry ACP fields: {serialized}"
    );
    assert!(
        !serialized.contains("\"token\""),
        "runtime must not embed raw tokens: {serialized}"
    );
    // Secrets file, if present, is separate from runtime authority.
    if secrets_path.exists() {
        let secrets = std::fs::read_to_string(&secrets_path).unwrap_or_default();
        assert!(
            !secrets.contains("e2e-ws-1") || secrets.contains("bezalel"),
            "unexpected secrets payload"
        );
    }
    // Runtime document itself is not Switchboard authority.
    let runtime_doc = std::fs::read_to_string(&runtime_path).unwrap_or_default();
    assert!(runtime_doc.contains("e2e-ws-1"));
    assert!(!runtime_doc.contains("\"workspaces\""));

    let _ = svc.destroy("e2e-ws-1", false).await;
}

#[tokio::test]
async fn e2e_missing_default_fails_closed() {
    let _guard = e2e_env_lock();
    let Some(sb) = start_switchboard() else { return; };

    let tmp = tempfile::tempdir().unwrap();
    let runtime_path = tmp.path().join("runtime.json");
    let secrets_path = tmp.path().join("secrets.json");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".config/awesometree")).unwrap();

    let _env = EnvGuard::set(&[
        ("AWESOMETREE_SWITCHBOARD_URL", sb.endpoint.clone()),
        (
            "AWESOMETREE_RUNTIME_PATH",
            runtime_path.to_string_lossy().into_owned(),
        ),
        (
            "AWESOMETREE_SECRETS_PATH",
            secrets_path.to_string_lossy().into_owned(),
        ),
        ("HOME", home.to_string_lossy().into_owned()),
    ]);

    let repo = init_git_repo(tmp.path());
    let catalog = std::sync::Arc::new(SwitchboardClient::new(
        SwitchboardConfig::with_endpoint(sb.endpoint.clone()),
    ));
    wait_healthy(&catalog).await;

    // Delete the seeded default WorkProfile so create-without-profile fails closed.
    catalog
        .delete_work_profile("default")
        .await
        .expect("delete seeded default profile");

    // Confirm it is gone via production client.
    let missing = catalog.get_work_profile("default").await;
    assert!(
        missing.is_err(),
        "default profile should be absent after delete"
    );

    let svc = awesometree::work_session_service::WorkSessionService::new(catalog.clone(), None);

    let def = awesometree::model::project::definition_for_create(
        "e2e-no-default",
        Some("e2e"),
        Some(repo.to_str().unwrap()),
        Some("master"),
        None,
    );
    svc.create_project(def)
        .await
        .expect("create project via Switchboard");

    let err = svc
        .create_work_session(awesometree::model::work_session::CreateWorkSessionRequest {
            work_session_id: "e2e-ws-missing-default".into(),
            project_id: "e2e-no-default".into(),
            work_profile_id: None,
            display_name: Some("should-fail".into()),
            realization: awesometree::model::work_session::RealizationOptions {
                create_tag: false,
                launch_apps: false,
                headless: false,
                no_wm: true,
            },
        })
        .await
        .expect_err("must not invent a local default WorkProfile");

    assert_eq!(
        err.code,
        awesometree::model::error::ErrorCode::MissingDefaultProfile,
        "expected missing_default_profile, got {err:?}"
    );

    // No local runtime authority row for the failed create.
    let rt = awesometree::runtime_store::get("e2e-ws-missing-default").expect("runtime load");
    assert!(rt.is_none(), "failed create must not leave runtime");

    // No WorkSession authority invented locally either.
    let listed = svc.list_work_sessions(None, Some("e2e-no-default")).await.unwrap();
    assert!(
        listed
            .iter()
            .all(|v| v.work_session.work_session_id != "e2e-ws-missing-default"),
        "failed create must not leave Switchboard WorkSession"
    );
}
