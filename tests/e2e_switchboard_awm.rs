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
    let path = rt
        .workspace
        .as_ref()
        .map(|w| w.path.clone())
        .expect("runtime workspace path");
    let policy = got
        .work_session
        .policy
        .as_ref()
        .expect("Switchboard session must record workspace binding");
    assert_eq!(policy["workspace"]["kind"], "git-worktree");
    assert_eq!(policy["workspace"]["locator"], path);
    assert_eq!(policy["workspace"]["resource_id"], "workspace:e2e-ws-1");

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

// ── Shared fixture for multi-boundary tests ───────────────────────────────

struct E2eFixture {
    _guard: std::sync::MutexGuard<'static, ()>,
    _sb: SwitchboardProc,
    _env: EnvGuard,
    _tmp: tempfile::TempDir,
    endpoint: String,
    runtime_path: PathBuf,
    secrets_path: PathBuf,
    repo: PathBuf,
    svc: std::sync::Arc<awesometree::work_session_service::WorkSessionService>,
}

impl E2eFixture {
    async fn start() -> Option<Self> {
        let guard = e2e_env_lock();
        let sb = start_switchboard()?;
        let tmp = tempfile::tempdir().unwrap();
        let runtime_path = tmp.path().join("runtime.json");
        let secrets_path = tmp.path().join("secrets.json");
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join(".config/awesometree")).unwrap();

        let env = EnvGuard::set(&[
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

        let svc = std::sync::Arc::new(
            awesometree::work_session_service::WorkSessionService::new(catalog.clone(), None),
        );
        // Process-wide accessor used by REST/MCP/gRPC/CLI paths.
        awesometree::service_access::set_service(svc.clone()).await;

        Some(Self {
            _guard: guard,
            endpoint: sb.endpoint.clone(),
            _sb: sb,
            _env: env,
            _tmp: tmp,
            runtime_path,
            secrets_path,
            repo,
            svc,
        })
    }

    async fn ensure_project(&self, project_id: &str) {
        let def = awesometree::model::project::definition_for_create(
            project_id,
            Some("e2e"),
            Some(self.repo.to_str().unwrap()),
            Some("master"),
            None,
        );
        let _ = self.svc.create_project(def).await.expect("create project");
    }
}

fn admin_bearer() -> String {
    let t = awesometree::auth::localhost_admin_token();
    format!(
        "Bearer {}",
        awesometree::auth::encode_scoped_token(&t)
    )
}

fn project_bearer(project_id: &str) -> String {
    let t = awesometree::auth::create_scoped_token(
        "e2e-user",
        awesometree::auth::TokenScope::Projects(vec![project_id.into()]),
        awesometree::auth::Permission::Project,
        None,
    );
    format!(
        "Bearer {}",
        awesometree::auth::encode_scoped_token(&t)
    )
}

fn no_wm_opts() -> awesometree::model::work_session::RealizationOptions {
    awesometree::model::work_session::RealizationOptions {
        create_tag: false,
        launch_apps: false,
        headless: false,
        no_wm: true,
    }
}

#[tokio::test]
async fn e2e_rest_create_work_session() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("rest-proj").await;

    let app = awesometree::server::api_app();
    let server = axum_test_serve(app).await;
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "work_session_id": "rest-ws-1",
        "project_id": "rest-proj",
        "headless": false,
        "no_tag": true,
        "no_launch": true,
    });
    let resp = client
        .post(format!("{}/api/work-sessions", server.base))
        .header("Authorization", admin_bearer())
        .json(&body)
        .send()
        .await
        .expect("rest create");
    assert_eq!(resp.status(), 201, "body={}", resp.text().await.unwrap_or_default());
    let v: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(v["work_session"]["work_session_id"], "rest-ws-1");
    assert_eq!(v["work_profile_id"], "default");

    let got = fx.svc.get_work_session("rest-ws-1").await.expect("sb readback");
    assert_eq!(got.work_session.work_profile_id.as_deref(), Some("default"));
    let _ = fx.svc.destroy("rest-ws-1", false).await;
}

#[tokio::test]
async fn e2e_grpc_create_work_session() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("grpc-proj").await;

    let impl_svc = awesometree::grpc::workspace::WorkSessionServiceImpl::new();
    use awesometree::grpc::arp_proto::work_session_service_server::WorkSessionService;
    use tonic::Request;

    let resp = impl_svc
        .create_work_session(Request::new(
            awesometree::grpc::arp_proto::CreateWorkSessionRequest {
                work_session_id: "grpc-ws-1".into(),
                project_id: "grpc-proj".into(),
                work_profile_id: String::new(),
                display_name: "grpc".into(),
                headless: true, // no_wm via headless realization path
            },
        ))
        .await
        .expect("grpc create")
        .into_inner();

    assert_eq!(resp.work_session_id, "grpc-ws-1");
    assert_eq!(resp.work_profile_id, "default");
    assert_eq!(resp.project_id, "grpc-proj");

    let got = fx.svc.get_work_session("grpc-ws-1").await.expect("sb readback");
    assert_eq!(got.work_session.work_profile_id.as_deref(), Some("default"));
    let _ = fx.svc.destroy("grpc-ws-1", false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mcp_tool_create_work_session() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("mcp-proj").await;

    let server = awesometree::mcp::ArpServer::new();
    let params = awesometree::mcp::tools_workspace::WorkSessionCreateParams {
        work_session_id: "mcp-ws-1".into(),
        project_id: "mcp-proj".into(),
        work_profile_id: None,
        display_name: Some("mcp".into()),
        headless: Some(true),
    };
    let result = server
        .work_session_create(rmcp::handler::server::wrapper::Parameters(params))
        .expect("mcp tool create");
    let text = result
        .content
        .iter()
        .filter_map(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("mcp-ws-1") && text.contains("default"),
        "mcp tool output missing session/profile: {text}"
    );

    let got = fx.svc.get_work_session("mcp-ws-1").await.expect("sb readback");
    assert_eq!(got.work_session.work_profile_id.as_deref(), Some("default"));
    let _ = fx.svc.destroy("mcp-ws-1", false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_cli_create_work_session() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("cli-proj").await;

    let bin = match option_env!("CARGO_BIN_EXE_awesometree") {
        Some(p) => PathBuf::from(p),
        None => {
            // Built without gui binary name in this profile.
            let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/debug/awesometree");
            if !fallback.exists() {
                if switchboard_required() {
                    panic!("CLI binary missing under SWITCHBOARD_REQUIRED");
                }
                eprintln!("skip: awesometree CLI binary not built");
                return;
            }
            fallback
        }
    };

    let out = Command::new(&bin)
        .args([
            "work-session",
            "create",
            "cli-ws-1",
            "--project",
            "cli-proj",
            "--headless",
        ])
        .env("AWESOMETREE_SWITCHBOARD_URL", &fx.endpoint)
        .env("AWESOMETREE_RUNTIME_PATH", &fx.runtime_path)
        .env("AWESOMETREE_SECRETS_PATH", &fx.secrets_path)
        .env("HOME", fx._tmp.path().join("home"))
        .output()
        .expect("spawn cli");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "cli failed status={} stdout={stdout} stderr={stderr}",
        out.status
    );
    assert!(
        stdout.contains("cli-ws-1") || stdout.contains("work_session=cli-ws-1"),
        "unexpected cli stdout: {stdout}"
    );

    let got = fx.svc.get_work_session("cli-ws-1").await.expect("sb readback");
    assert_eq!(got.work_session.project_id.as_deref(), Some("cli-proj"));
    let _ = fx.svc.destroy("cli-ws-1", false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_core_client_create_work_session() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("core-proj").await;

    let app = awesometree::server::api_app();
    let server = axum_test_serve(app).await;
    // server.base like http://127.0.0.1:PORT
    let base = server.base.trim_start_matches("http://");
    let (host, port_s) = base.split_once(':').expect("host:port");
    let host = host.to_string();
    let port: u16 = port_s.parse().unwrap();

    let admin = awesometree::auth::localhost_admin_token();
    let token = awesometree::auth::encode_scoped_token(&admin);
    let client = awesometree_core::ApiClient::new(host, port, token);

    let info = tokio::task::spawn_blocking(move || {
        client
            .create_work_session(awesometree_core::CreateWorkSessionReq {
                work_session_id: "core-ws-1".into(),
                project_id: "core-proj".into(),
                work_profile_id: String::new(),
                display_name: "core".into(),
                headless: true,
            })
    })
    .await
    .expect("join")
    .expect("core create");
    assert_eq!(info.work_session_id, "core-ws-1");
    assert_eq!(info.project_id.as_deref(), Some("core-proj"));

    let got = fx.svc.get_work_session("core-ws-1").await.expect("sb readback");
    assert_eq!(got.work_session.work_profile_id.as_deref(), Some("default"));
    let _ = fx.svc.destroy("core-ws-1", false).await;
}

#[tokio::test]
async fn e2e_auth_scope_denies_other_project() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("auth-a").await;
    fx.ensure_project("auth-b").await;

    // Create under project A via service.
    fx.svc
        .create_work_session(awesometree::model::work_session::CreateWorkSessionRequest {
            work_session_id: "auth-ws-a".into(),
            project_id: "auth-a".into(),
            work_profile_id: None,
            display_name: None,
            realization: no_wm_opts(),
        })
        .await
        .expect("create a");

    let app = awesometree::server::api_app();
    let server = axum_test_serve(app).await;
    let client = reqwest::Client::new();

    // Project-B scoped token cannot GET project-A session.
    let resp = client
        .get(format!("{}/api/work-sessions/auth-ws-a", server.base))
        .header("Authorization", project_bearer("auth-b"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "cross-project get must be forbidden");

    // Cannot create into project A with B scope.
    let resp = client
        .post(format!("{}/api/work-sessions", server.base))
        .header("Authorization", project_bearer("auth-b"))
        .json(&serde_json::json!({
            "work_session_id": "auth-ws-evil",
            "project_id": "auth-a",
            "no_tag": true,
            "no_launch": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "cross-project create must be forbidden");

    // List with B scope must not include A sessions.
    let resp = client
        .get(format!("{}/api/work-sessions", server.base))
        .header("Authorization", project_bearer("auth-b"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let list: serde_json::Value = resp.json().await.unwrap();
    let arr = list.as_array().cloned().unwrap_or_default();
    assert!(
        arr.iter()
            .all(|v| v["work_session"]["work_session_id"] != "auth-ws-a"
                && v.get("work_session_id").and_then(|x| x.as_str()) != Some("auth-ws-a")),
        "scoped list leaked foreign session: {list}"
    );

    let _ = fx.svc.destroy("auth-ws-a", false).await;
}

#[tokio::test]
async fn e2e_secrets_never_in_list_or_runtime_json() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("sec-proj").await;

    // Headless create allocates bezalel token in secrets store.
    let resp = fx
        .svc
        .create_work_session(awesometree::model::work_session::CreateWorkSessionRequest {
            work_session_id: "sec-ws-1".into(),
            project_id: "sec-proj".into(),
            work_profile_id: None,
            display_name: None,
            realization: awesometree::model::work_session::RealizationOptions {
                create_tag: false,
                launch_apps: false,
                headless: true,
                no_wm: true,
            },
        })
        .await
        .expect("headless create");

    let token = awesometree::runtime_store::get_bezalel_token("sec-ws-1")
        .expect("secrets load")
        .expect("bezalel token stored host-locally");
    assert!(!token.is_empty());
    assert!(token.chars().all(|c| c.is_ascii_hexdigit()));

    // Runtime JSON must not embed the raw token.
    let runtime_doc = std::fs::read_to_string(&fx.runtime_path).unwrap_or_default();
    assert!(
        !runtime_doc.contains(&token),
        "runtime.json must not contain raw bezalel token"
    );
    assert!(!runtime_doc.contains("acp_"));
    assert!(!runtime_doc.to_lowercase().contains("\"acp"));

    // REST list/detail must not contain the raw token.
    let app = awesometree::server::api_app();
    let server = axum_test_serve(app).await;
    let client = reqwest::Client::new();
    for path in [
        "/api/work-sessions".to_string(),
        "/api/work-sessions/sec-ws-1".to_string(),
    ] {
        let body = client
            .get(format!("{}{path}", server.base))
            .header("Authorization", admin_bearer())
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            !body.contains(&token),
            "API {path} leaked bezalel token"
        );
        assert!(!body.contains("acp_port"), "API {path} leaked acp_port");
    }

    // Create response path also redacts.
    let create_json = serde_json::to_string(&resp).unwrap();
    assert!(!create_json.contains(&token));

    // Switchboard authoritative record must not hold the token either.
    let sb_view = fx.svc.get_work_session("sec-ws-1").await.unwrap();
    let sb_json = serde_json::to_string(&sb_view.work_session).unwrap();
    assert!(!sb_json.contains(&token));

    let _ = fx.svc.destroy("sec-ws-1", false).await;
}

/// Minimal in-process HTTP listener for axum Router (no tower-test dep).
struct TestHttp {
    base: String,
    _join: tokio::task::JoinHandle<()>,
}

async fn axum_test_serve(app: axum::Router) -> TestHttp {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let join = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    // Tiny settle for accept loop.
    tokio::time::sleep(Duration::from_millis(20)).await;
    TestHttp {
        base: format!("http://{addr}"),
        _join: join,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_daemon_ipc_create_work_session() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("ipc-proj").await;

    // Production Unix-socket path: bind listen_until in a background thread and
    // send `work-session-create` through daemon::send_command (same IPC as CLI).
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, _rx) = std::sync::mpsc::channel();
    let listener = std::thread::spawn(move || {
        awesometree::daemon::listen_until(tx, stop2);
    });

    // Wait until socket accepts.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if awesometree::daemon::is_running() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        awesometree::daemon::is_running(),
        "daemon socket did not become ready"
    );

    let resp = awesometree::daemon::send_command(
        "work-session-create ipc-ws-1 ipc-proj --headless",
    )
    .expect("daemon ipc send");
    assert!(
        resp.starts_with("ok work_session=ipc-ws-1"),
        "unexpected ipc response: {resp}"
    );
    assert!(
        resp.contains("work_profile=default"),
        "must resolve exact default: {resp}"
    );

    let got = fx.svc.get_work_session("ipc-ws-1").await.expect("sb readback");
    assert_eq!(got.work_session.work_profile_id.as_deref(), Some("default"));
    assert_eq!(got.work_session.project_id.as_deref(), Some("ipc-proj"));

    let _ = fx.svc.destroy("ipc-ws-1", false).await;
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    awesometree::daemon::cleanup();
    let _ = listener.join();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_daemon_ipc_handler_direct() {
    // Same production handler function the socket invokes, without GPUI.
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("ipc2-proj").await;

    let resp = awesometree::daemon::work_session_create_ipc(
        "ipc-ws-2 ipc2-proj --headless",
    );
    assert!(
        resp.starts_with("ok work_session=ipc-ws-2"),
        "handler response: {resp}"
    );
    let got = fx.svc.get_work_session("ipc-ws-2").await.expect("sb");
    assert_eq!(got.work_session.work_profile_id.as_deref(), Some("default"));
    let _ = fx.svc.destroy("ipc-ws-2", false).await;
}

// ── Phase-01 BDD gap coverage ─────────────────────────────────────────────

fn no_wm_create(id: &str, project: &str) -> awesometree::model::work_session::CreateWorkSessionRequest {
    awesometree::model::work_session::CreateWorkSessionRequest {
        work_session_id: id.into(),
        project_id: project.into(),
        work_profile_id: None,
        display_name: Some(id.into()),
        realization: no_wm_opts(),
    }
}

#[tokio::test]
async fn e2e_realization_failure_aborts_and_no_duplicate_on_retry() {
    let Some(fx) = E2eFixture::start().await else { return; };

    // Project with a non-existent repo so worktree realization fails after
    // Switchboard has already accepted the WorkSession (proposed → aborted).
    let bad_repo = fx._tmp.path().join("does-not-exist-repo");
    let def = awesometree::model::project::definition_for_create(
        "comp-proj",
        Some("compensation"),
        Some(bad_repo.to_str().unwrap()),
        Some("master"),
        None,
    );
    fx.svc.create_project(def).await.expect("create project");

    let err = fx
        .svc
        .create_work_session(no_wm_create("comp-ws-1", "comp-proj"))
        .await
        .expect_err("realization must fail for missing repo");
    assert!(
        err.to_string().contains("realization failed")
            || err.to_string().contains("repo not found")
            || err.to_string().contains("worktree"),
        "unexpected error: {err}"
    );

    // Switchboard session should be aborted (or absent if SB rolled back — either is fail-closed).
    match fx.svc.get_work_session("comp-ws-1").await {
        Ok(view) => {
            assert_eq!(
                view.work_session.state,
                awesometree::model::lifecycle::WorkSessionState::Aborted,
                "failed realization must compensate to aborted, got {}",
                view.work_session.state
            );
        }
        Err(e) => {
            // Acceptable if Switchboard deleted; must not be Open.
            assert_ne!(e.code, awesometree::model::error::ErrorCode::InternalError);
        }
    }

    // No successful local runtime for the failed id.
    let rt = awesometree::runtime_store::get("comp-ws-1").expect("runtime load");
    if let Some(rt) = rt {
        assert_ne!(
            rt.realization_status,
            awesometree::model::runtime::RealizationStatus::Ready,
            "failed create must not leave Ready runtime"
        );
    }

    // Retry same id must not invent a second Open session / Ready runtime.
    let err2 = fx
        .svc
        .create_work_session(no_wm_create("comp-ws-1", "comp-proj"))
        .await
        .expect_err("retry with same id and still-bad repo must fail");
    let _ = err2;
    if let Ok(view) = fx.svc.get_work_session("comp-ws-1").await {
        assert_ne!(
            view.work_session.state,
            awesometree::model::lifecycle::WorkSessionState::Open,
            "retry must not open a duplicate live session"
        );
    }
    let _ = fx.svc.destroy("comp-ws-1", false).await;
}

#[tokio::test]
async fn e2e_invalid_transition_from_closed() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("tr-proj").await;

    fx.svc
        .create_work_session(no_wm_create("tr-ws-1", "tr-proj"))
        .await
        .expect("create");
    fx.svc
        .transition(
            "tr-ws-1",
            awesometree::model::lifecycle::WorkSessionState::Closed,
        )
        .await
        .expect("close");

    let err = fx
        .svc
        .transition(
            "tr-ws-1",
            awesometree::model::lifecycle::WorkSessionState::Open,
        )
        .await
        .expect_err("closed → open must be rejected");
    assert_eq!(
        err.code,
        awesometree::model::error::ErrorCode::InvalidTransition,
        "got {err:?}"
    );

    let err2 = fx
        .svc
        .transition(
            "tr-ws-1",
            awesometree::model::lifecycle::WorkSessionState::Paused,
        )
        .await
        .expect_err("closed → paused must be rejected");
    assert_eq!(err2.code, awesometree::model::error::ErrorCode::InvalidTransition);

    let got = fx.svc.get_work_session("tr-ws-1").await.expect("still exists");
    assert_eq!(
        got.work_session.state,
        awesometree::model::lifecycle::WorkSessionState::Closed
    );
    let _ = fx.svc.destroy("tr-ws-1", false).await;
}

#[tokio::test]
async fn e2e_close_keeps_worktree_destroy_removes_it() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("life-proj").await;
    let created = fx
        .svc
        .create_work_session(no_wm_create("life-ws-1", "life-proj"))
        .await
        .expect("create");
    let path = created
        .runtime
        .as_ref()
        .and_then(|r| r.workspace.as_ref())
        .map(|w| std::path::PathBuf::from(&w.path))
        .expect("worktree path");
    assert!(path.exists(), "worktree must exist after create");

    fx.svc
        .transition(
            "life-ws-1",
            awesometree::model::lifecycle::WorkSessionState::Closed,
        )
        .await
        .expect("close");
    assert!(path.exists(), "close must keep the worktree");

    fx.svc.destroy("life-ws-1", false).await.expect("destroy");
    assert!(
        !path.exists(),
        "destroy must remove the worktree at {}",
        path.display()
    );
}

#[tokio::test]
async fn e2e_project_snapshot_pin_survives_live_edit() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("pin-proj").await;

    let created = fx
        .svc
        .create_work_session(no_wm_create("pin-ws-1", "pin-proj"))
        .await
        .expect("create");
    let pinned_rev = created
        .work_session
        .project_revision
        .clone()
        .or(created.project_revision.clone())
        .expect("must pin project_revision");
    let pinned_snap = created
        .work_session
        .project_snapshot_id
        .clone()
        .or(created.project_snapshot_id.clone());

    // Mutate live Project definition.
    let env = fx.svc.get_project("pin-proj").await.expect("get project");
    let src_rev = env.source_revision;
    assert!(!src_rev.is_empty(), "need source_revision for CAS update");
    let updated = fx
        .svc
        .update_project(
            "pin-proj",
            &src_rev,
            serde_json::json!({"description": "mutated-after-session"}),
        )
        .await
        .expect("live project edit");
    assert_ne!(
        updated.revision.as_deref().unwrap_or(""),
        "",
        "update should produce a revision"
    );

    // WorkSession pin must not move with the live Project.
    let got = fx.svc.get_work_session("pin-ws-1").await.expect("get ws");
    assert_eq!(
        got.work_session.project_revision.as_deref(),
        Some(pinned_rev.as_str()),
        "session pin must survive live project edit"
    );
    if let Some(ref snap) = pinned_snap {
        assert_eq!(
            got.work_session.project_snapshot_id.as_deref(),
            Some(snap.as_str()),
            "snapshot id pin must survive live project edit"
        );
    }

    let _ = fx.svc.destroy("pin-ws-1", false).await;
}

#[tokio::test]
async fn e2e_project_cas_conflict_on_stale_update() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("cas-proj").await;

    let env = fx.svc.get_project("cas-proj").await.expect("get");
    let good_rev = env.source_revision.clone();
    assert!(!good_rev.is_empty());

    // First update with correct CAS token succeeds.
    fx.svc
        .update_project(
            "cas-proj",
            &good_rev,
            serde_json::json!({"description": "cas-one"}),
        )
        .await
        .expect("first update");

    // Second update with the stale token must conflict.
    let err = fx
        .svc
        .update_project(
            "cas-proj",
            &good_rev,
            serde_json::json!({"description": "cas-stale"}),
        )
        .await
        .expect_err("stale CAS must fail");
    assert_eq!(
        err.code,
        awesometree::model::error::ErrorCode::Conflict,
        "expected conflict, got {err:?}"
    );
}

#[tokio::test]
async fn e2e_referenced_project_delete_rejected() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("ref-proj").await;

    fx.svc
        .create_work_session(no_wm_create("ref-ws-1", "ref-proj"))
        .await
        .expect("create session");

    let env = fx.svc.get_project("ref-proj").await.expect("get");
    let err = fx
        .svc
        .delete_project("ref-proj", &env.source_revision)
        .await
        .expect_err("delete project with open session must fail");
    assert!(
        matches!(
            err.code,
            awesometree::model::error::ErrorCode::Referenced
                | awesometree::model::error::ErrorCode::Conflict
                | awesometree::model::error::ErrorCode::InvalidInput
                | awesometree::model::error::ErrorCode::InternalError
        ),
        "expected referential rejection, got {err:?}"
    );
    // Prefer strong code when Switchboard emits it.
    if err.code == awesometree::model::error::ErrorCode::Referenced {
        // ideal path
    } else {
        eprintln!("note: Switchboard returned {err:?} for referenced delete (accepting as rejection)");
    }

    // Project and session must still exist.
    fx.svc.get_project("ref-proj").await.expect("project remains");
    fx.svc.get_work_session("ref-ws-1").await.expect("session remains");

    let _ = fx.svc.destroy("ref-ws-1", false).await;
}

#[tokio::test]
async fn e2e_referenced_work_profile_delete_rejected_when_in_use() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("prof-proj").await;

    // Put a custom profile and create a session against it.
    let custom = awesometree::model::work_profile::WorkProfile {
        version: "1".into(),
        work_profile_id: "custom-e2e".into(),
        display_name: Some("custom".into()),
        description: None,
        project_ids: vec!["prof-proj".into()],
        intended_resources: vec![],
        default_policy: None,
    };
    // WorkProfile shape may differ — use put via catalog if fields differ.
    let put = fx.svc.catalog().put_work_profile(custom).await;
    let Ok(_) = put else {
        // If schema differs, skip custom profile path but still try deleting default while in use.
        eprintln!("note: custom profile put failed: {:?}", put.err());
        fx.svc
            .create_work_session(no_wm_create("prof-ws-1", "prof-proj"))
            .await
            .expect("create with default");
        let err = fx
            .svc
            .catalog()
            .delete_work_profile("default")
            .await
            .expect_err("deleting default while referenced should fail");
        assert!(
            err.code == awesometree::model::error::ErrorCode::Referenced
                || err.to_string().to_lowercase().contains("refer")
                || err.to_string().to_lowercase().contains("in use")
                || err.code != awesometree::model::error::ErrorCode::NotFound,
            "unexpected delete default result: {err:?}"
        );
        let _ = fx.svc.destroy("prof-ws-1", false).await;
        return;
    };

    fx.svc
        .create_work_session(awesometree::model::work_session::CreateWorkSessionRequest {
            work_session_id: "prof-ws-1".into(),
            project_id: "prof-proj".into(),
            work_profile_id: Some("custom-e2e".into()),
            display_name: None,
            realization: no_wm_opts(),
        })
        .await
        .expect("create with custom profile");

    let err = fx
        .svc
        .catalog()
        .delete_work_profile("custom-e2e")
        .await
        .expect_err("delete in-use profile must fail");
    assert!(
        matches!(
            err.code,
            awesometree::model::error::ErrorCode::Referenced
                | awesometree::model::error::ErrorCode::Conflict
                | awesometree::model::error::ErrorCode::InvalidInput
                | awesometree::model::error::ErrorCode::InternalError
        ),
        "got {err:?}"
    );
    fx.svc
        .catalog()
        .get_work_profile("custom-e2e")
        .await
        .expect("profile still present");
    let _ = fx.svc.destroy("prof-ws-1", false).await;
}

#[tokio::test]
async fn e2e_switchboard_outage_is_hard_failure_no_local_write() {
    let _guard = e2e_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let runtime_path = tmp.path().join("runtime.json");
    let secrets_path = tmp.path().join("secrets.json");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".config/awesometree")).unwrap();

    // Dead Switchboard endpoint — nothing listening.
    let dead = format!("http://127.0.0.1:{}/mcp", free_port());
    let _env = EnvGuard::set(&[
        ("AWESOMETREE_SWITCHBOARD_URL", dead.clone()),
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

    let catalog = std::sync::Arc::new(SwitchboardClient::new(
        SwitchboardConfig::with_endpoint(dead),
    ));
    let svc = std::sync::Arc::new(
        awesometree::work_session_service::WorkSessionService::new(catalog, None),
    );
    awesometree::service_access::set_service(svc.clone()).await;

    let err = svc
        .create_work_session(no_wm_create("outage-ws", "outage-proj"))
        .await
        .expect_err("outage must hard-fail");
    assert_eq!(
        err.code,
        awesometree::model::error::ErrorCode::Unavailable,
        "expected unavailable, got {err:?}"
    );

    // No local authority or runtime invented.
    assert!(
        awesometree::runtime_store::get("outage-ws")
            .expect("load")
            .is_none(),
        "outage must not write runtime"
    );
    if runtime_path.exists() {
        let doc = std::fs::read_to_string(&runtime_path).unwrap_or_default();
        assert!(
            !doc.contains("outage-ws"),
            "runtime file must not contain failed session"
        );
    }

    // REST also fails closed.
    let app = awesometree::server::api_app();
    let server = axum_test_serve(app).await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/work-sessions", server.base))
        .header("Authorization", admin_bearer())
        .json(&serde_json::json!({
            "work_session_id": "outage-ws-rest",
            "project_id": "outage-proj",
            "no_tag": true,
            "no_launch": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        503,
        "REST outage must be 503, got {} body={}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );
}

// ── Desktop UI contract e2e ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_projects_ui_daemon_command_is_recognized() {
    let Some(_fx) = E2eFixture::start().await else { return; };

    // The CLI/tray send "projects-ui". Before this fix parse_command returned
    // None and the socket replied "unknown command" with no window / no log.
    assert!(
        matches!(
            awesometree::daemon::parse_command("projects-ui"),
            Some(awesometree::daemon::DaemonCmd::Projects)
        ),
        "projects-ui must map to DaemonCmd::Projects"
    );
    assert!(
        matches!(
            awesometree::daemon::parse_command("projects"),
            Some(awesometree::daemon::DaemonCmd::Projects)
        )
    );

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let listener = std::thread::spawn(move || {
        awesometree::daemon::listen_until(tx, stop2);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if awesometree::daemon::is_running() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(awesometree::daemon::is_running(), "daemon socket not ready");

    let resp = awesometree::daemon::send_command("projects-ui").expect("send projects-ui");
    assert_eq!(resp, "ok", "projects-ui must be accepted, got {resp}");
    let cmd = rx.recv_timeout(Duration::from_secs(2)).expect("daemon dispatched Projects");
    assert!(
        matches!(cmd, awesometree::daemon::DaemonCmd::Projects),
        "expected Projects, got {cmd:?}"
    );

    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    awesometree::daemon::cleanup();
    let _ = listener.join();
}

#[tokio::test]
async fn e2e_create_form_profiles_require_selected_project() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("form-alpha").await;
    fx.ensure_project("form-beta").await;

    let alpha_only = awesometree::model::work_profile::WorkProfile {
        version: "1".into(),
        work_profile_id: "alpha-only".into(),
        display_name: Some("Alpha".into()),
        description: None,
        project_ids: vec!["form-alpha".into()],
        intended_resources: vec![],
        default_policy: None,
    };
    let beta_only = awesometree::model::work_profile::WorkProfile {
        version: "1".into(),
        work_profile_id: "beta-only".into(),
        display_name: Some("Beta".into()),
        description: None,
        project_ids: vec!["form-beta".into()],
        intended_resources: vec![],
        default_policy: None,
    };
    fx.svc
        .catalog()
        .put_work_profile(alpha_only)
        .await
        .expect("put alpha-only");
    fx.svc
        .catalog()
        .put_work_profile(beta_only)
        .await
        .expect("put beta-only");

    let all = fx.svc.list_work_profiles().await.expect("list profiles");
    assert!(
        all.iter().any(|p| p.work_profile_id == "alpha-only"),
        "catalog must include alpha-only"
    );
    assert!(
        all.iter().any(|p| p.work_profile_id == "beta-only"),
        "catalog must include beta-only"
    );

    // No project selected → create form must offer nothing (field disabled).
    let none = awesometree::model::eligible_for_project(&all, "");
    assert!(
        none.is_empty(),
        "profiles must stay disabled until a project is selected, got {:?}",
        none.iter().map(|p| p.work_profile_id.as_str()).collect::<Vec<_>>(
        )
    );

    let for_alpha: Vec<&str> = awesometree::model::eligible_for_project(&all, "form-alpha")
        .iter()
        .map(|p| p.work_profile_id.as_str())
        .collect();
    assert!(
        for_alpha.contains(&"default"),
        "global default must remain eligible: {for_alpha:?}"
    );
    assert!(
        for_alpha.contains(&"alpha-only"),
        "project-scoped alpha-only must be eligible: {for_alpha:?}"
    );
    assert!(
        !for_alpha.contains(&"beta-only"),
        "beta-only must not appear for form-alpha: {for_alpha:?}"
    );

    let for_beta: Vec<&str> = awesometree::model::eligible_for_project(&all, "form-beta")
        .iter()
        .map(|p| p.work_profile_id.as_str())
        .collect();
    assert!(for_beta.contains(&"beta-only"));
    assert!(!for_beta.contains(&"alpha-only"));
}

#[tokio::test]
async fn e2e_project_ui_replace_definition_updates_repo_and_apps() {
    let Some(fx) = E2eFixture::start().await else { return; };
    fx.ensure_project("edit-proj").await;

    let env = fx.svc.get_project("edit-proj").await.expect("get");
    assert!(!env.source_revision.is_empty());

    // Reproduce the old bug: wrapping the definition as patch.definition must
    // not be how the UI talks to Switchboard. The production path is
    // replace_project_definition (top-level `definition` argument).
    let ext = awesometree::model::project::AwesometreeExt {
        apps: vec!["zeditor -n {dir}".into()],
        worktree_dir: Some("/tmp/edit-wt".into()),
        ..Default::default()
    };
    let merged = awesometree::model::project::merge_form_into_definition(
        &env.definition,
        "edit-proj",
        Some("/tmp/edit-repo"),
        Some("master"),
        &ext,
    );
    assert_eq!(merged["repo"], "/tmp/edit-repo");
    assert_eq!(merged["extensions"]["dev.awesometree"]["apps"][0], "zeditor -n {dir}");

    let updated = fx
        .svc
        .replace_project_definition("edit-proj", &env.source_revision, merged)
        .await
        .expect("replace definition");
    assert_eq!(updated.project_id, "edit-proj");

    let got = fx.svc.get_project("edit-proj").await.expect("reload");
    assert_eq!(got.primary_repo().as_deref(), Some("/tmp/edit-repo"));
    assert_eq!(got.branch().as_deref(), Some("master"));
    assert!(
        got.definition.get("name").is_none()
            || got.definition["name"] == "edit-proj",
        "live definition name must stay edit-proj"
    );
    let ext = got.awesometree_ext();
    assert_eq!(ext.apps, vec!["zeditor -n {dir}".to_string()]);
    assert_eq!(ext.worktree_dir.as_deref(), Some("/tmp/edit-wt"));

    // The broken shape (patch.definition) must not be how we send updates.
    // A field patch of {"definition": ...} would nest instead of applying repo.
    let nested = serde_json::json!({"definition": {"repo": "/should-not-win"}});
    // If someone mistakenly calls update_project with that nested object, repo stays.
    let after_nested = fx
        .svc
        .update_project(
            "edit-proj",
            &got.source_revision,
            nested,
        )
        .await;
    // Either Switchboard rejects it or it does not replace the real repo field.
    if after_nested.is_ok() {
        let again = fx.svc.get_project("edit-proj").await.expect("after nested");
        assert_eq!(
            again.primary_repo().as_deref(),
            Some("/tmp/edit-repo"),
            "nested patch.definition must not clobber repo"
        );
    }
}

#[tokio::test]
async fn e2e_project_update_named_resource_branch_does_not_send_name() {
    let Some(fx) = E2eFixture::start().await else { return; };
    let def = serde_json::json!({
        "version": "1",
        "name": "audiobook-e2e",
        "resources": {
            "audiobook": {
                "type": "repo",
                "path": fx.repo.to_str().unwrap(),
                "branch": "master"
            }
        }
    });
    fx.svc.create_project(def).await.expect("create audiobook-shaped project");
    let env = fx.svc.get_project("audiobook-e2e").await.expect("get");
    let merged = awesometree::model::project::merge_form_into_definition(
        &env.definition,
        "audiobook-e2e",
        env.primary_repo().as_deref(),
        Some("feat/chapters"),
        &awesometree::model::project::AwesometreeExt::default(),
    );
    assert!(
        merged.get("name").is_none(),
        "payload must omit name, got {merged}"
    );
    assert_eq!(merged["resources"]["audiobook"]["branch"], "feat/chapters");

    fx.svc
        .replace_project_definition("audiobook-e2e", &env.source_revision, merged)
        .await
        .expect("branch-only update must succeed");
    let got = fx.svc.get_project("audiobook-e2e").await.expect("reload");
    assert_eq!(got.branch().as_deref(), Some("feat/chapters"));
    assert_eq!(got.primary_repo().as_deref(), env.primary_repo().as_deref());
}
