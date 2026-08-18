//! Host-local runtime realization store keyed by `work_session_id`.
//!
//! Does NOT store Project, WorkProfile, or WorkSession definitions/lifecycle.
//! Uses OS-level file locking for cross-process safety.

use crate::model::error::{ErrorCode, Result, SwitchboardError};
use crate::model::runtime::{RealizationStatus, RuntimeSecrets, WorkSessionRuntime};
use crate::paths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// On-disk document version. Bump rejects old workspace-episode state.json.
const DOCUMENT_VERSION: u32 = 2;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
struct Document {
    version: u32,
    #[serde(default)]
    host_id: String,
    #[serde(default)]
    runtimes: HashMap<String, WorkSessionRuntime>,
    #[serde(default)]
    secrets: RuntimeSecrets,
}

static STORE_MUTEX: Mutex<()> = Mutex::new(());

fn default_dir() -> PathBuf {
    paths::home_dir().join(".config/awesometree")
}

fn runtime_path() -> PathBuf {
    if let Ok(p) = std::env::var("AWESOMETREE_RUNTIME_PATH") {
        return PathBuf::from(p);
    }
    default_dir().join("runtime.json")
}

fn secrets_path() -> PathBuf {
    if let Ok(p) = std::env::var("AWESOMETREE_SECRETS_PATH") {
        return PathBuf::from(p);
    }
    default_dir().join("runtime-secrets.json")
}

fn legacy_state_path() -> PathBuf {
    default_dir().join("state.json")
}

fn host_id() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown-host".into())
}

fn detect_legacy() -> Result<()> {
    let path = legacy_state_path();
    if !path.exists() {
        return Ok(());
    }
    // If legacy state.json exists and runtime.json does not, fail explicitly.
    if !runtime_path().exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if data.contains("\"workspaces\"") {
                return Err(SwitchboardError::new(
                    ErrorCode::UnsupportedOldState,
                    format!(
                        "unsupported old state at {}: recreate WorkSessions in Switchboard; old workspace-episode state is not migrated",
                        path.display()
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn open_locked(path: &Path) -> Result<(File, Document)> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("create runtime dir: {e}"))
        })?;
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("open runtime: {e}"))
        })?;
    file.lock_exclusive().map_err(|e| {
        SwitchboardError::new(ErrorCode::LockTimeout, format!("lock runtime: {e}"))
    })?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(|e| {
        SwitchboardError::new(ErrorCode::InternalError, format!("read runtime: {e}"))
    })?;
    let doc = if buf.trim().is_empty() {
        Document {
            version: DOCUMENT_VERSION,
            host_id: host_id(),
            ..Default::default()
        }
    } else {
        let d: Document = serde_json::from_str(&buf).map_err(|e| {
            SwitchboardError::new(
                ErrorCode::UnsupportedOldState,
                format!("parse runtime store: {e}; delete {} and recreate", path.display()),
            )
        })?;
        if d.version != DOCUMENT_VERSION {
            return Err(SwitchboardError::new(
                ErrorCode::UnsupportedOldState,
                format!(
                    "runtime store version {} unsupported (want {DOCUMENT_VERSION})",
                    d.version
                ),
            ));
        }
        d
    };
    Ok((file, doc))
}

fn write_locked(file: &mut File, doc: &Document) -> Result<()> {
    let data = serde_json::to_string_pretty(doc).map_err(|e| {
        SwitchboardError::new(ErrorCode::InternalError, format!("serialize runtime: {e}"))
    })?;
    file.set_len(0).map_err(|e| {
        SwitchboardError::new(ErrorCode::InternalError, format!("truncate runtime: {e}"))
    })?;
    file.seek(SeekFrom::Start(0)).map_err(|e| {
        SwitchboardError::new(ErrorCode::InternalError, format!("seek runtime: {e}"))
    })?;
    file.write_all(data.as_bytes()).map_err(|e| {
        SwitchboardError::new(ErrorCode::InternalError, format!("write runtime: {e}"))
    })?;
    file.sync_all().ok();
    Ok(())
}

/// Load all runtime records for this host.
pub fn load_all() -> Result<HashMap<String, WorkSessionRuntime>> {
    let _g = STORE_MUTEX.lock().unwrap();
    detect_legacy()?;
    let path = runtime_path();
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let (file, doc) = open_locked(&path)?;
    drop(file);
    Ok(doc.runtimes)
}

pub fn get(work_session_id: &str) -> Result<Option<WorkSessionRuntime>> {
    let all = load_all()?;
    Ok(all.get(work_session_id).cloned())
}

pub fn upsert(runtime: WorkSessionRuntime) -> Result<()> {
    let _g = STORE_MUTEX.lock().unwrap();
    detect_legacy()?;
    let path = runtime_path();
    let (mut file, mut doc) = open_locked(&path)?;
    if doc.host_id.is_empty() {
        doc.host_id = host_id();
    }
    let mut rt = runtime;
    if rt.host_id.is_empty() {
        rt.host_id = doc.host_id.clone();
    }
    doc.runtimes
        .insert(rt.work_session_id.clone(), rt);
    write_locked(&mut file, &doc)
}

pub fn remove(work_session_id: &str) -> Result<()> {
    let _g = STORE_MUTEX.lock().unwrap();
    let path = runtime_path();
    if !path.exists() {
        return Ok(());
    }
    let (mut file, mut doc) = open_locked(&path)?;
    doc.runtimes.remove(work_session_id);
    write_locked(&mut file, &doc)
}

pub fn modify<F>(work_session_id: &str, f: F) -> Result<WorkSessionRuntime>
where
    F: FnOnce(&mut WorkSessionRuntime),
{
    let _g = STORE_MUTEX.lock().unwrap();
    detect_legacy()?;
    let path = runtime_path();
    let (mut file, mut doc) = open_locked(&path)?;
    if doc.host_id.is_empty() {
        doc.host_id = host_id();
    }
    let mut rt = doc
        .runtimes
        .remove(work_session_id)
        .unwrap_or_else(|| WorkSessionRuntime {
            work_session_id: work_session_id.to_string(),
            host_id: doc.host_id.clone(),
            realization_status: RealizationStatus::Pending,
            ..Default::default()
        });
    f(&mut rt);
    let out = rt.clone();
    doc.runtimes.insert(work_session_id.to_string(), rt);
    write_locked(&mut file, &doc)?;
    Ok(out)
}

/// Store a bezalel token host-locally (never sent to Switchboard).
pub fn set_bezalel_token(work_session_id: &str, token: &str) -> Result<()> {
    let _g = STORE_MUTEX.lock().unwrap();
    let path = secrets_path();
    let (mut file, mut doc) = open_secrets(&path)?;
    doc.bezalel_tokens
        .insert(work_session_id.to_string(), token.to_string());
    write_secrets(&mut file, &doc)
}

pub fn get_bezalel_token(work_session_id: &str) -> Result<Option<String>> {
    let _g = STORE_MUTEX.lock().unwrap();
    let path = secrets_path();
    if !path.exists() {
        return Ok(None);
    }
    let (file, doc) = open_secrets(&path)?;
    drop(file);
    Ok(doc.bezalel_tokens.get(work_session_id).cloned())
}

pub fn clear_bezalel_token(work_session_id: &str) -> Result<()> {
    let _g = STORE_MUTEX.lock().unwrap();
    let path = secrets_path();
    if !path.exists() {
        return Ok(());
    }
    let (mut file, mut doc) = open_secrets(&path)?;
    doc.bezalel_tokens.remove(work_session_id);
    write_secrets(&mut file, &doc)
}

fn open_secrets(path: &Path) -> Result<(File, RuntimeSecrets)> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("create secrets dir: {e}"))
        })?;
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("open secrets: {e}"))
        })?;
    file.lock_exclusive().map_err(|e| {
        SwitchboardError::new(ErrorCode::LockTimeout, format!("lock secrets: {e}"))
    })?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(|e| {
        SwitchboardError::new(ErrorCode::InternalError, format!("read secrets: {e}"))
    })?;
    let doc = if buf.trim().is_empty() {
        RuntimeSecrets::default()
    } else {
        serde_json::from_str(&buf).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("parse secrets: {e}"))
        })?
    };
    Ok((file, doc))
}

fn write_secrets(file: &mut File, doc: &RuntimeSecrets) -> Result<()> {
    let data = serde_json::to_string_pretty(doc).map_err(|e| {
        SwitchboardError::new(ErrorCode::InternalError, format!("serialize secrets: {e}"))
    })?;
    file.set_len(0).ok();
    file.seek(SeekFrom::Start(0)).ok();
    file.write_all(data.as_bytes()).map_err(|e| {
        SwitchboardError::new(ErrorCode::InternalError, format!("write secrets: {e}"))
    })?;
    // Restrict permissions best-effort.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Allocate next free tag index starting at TAG_OFFSET.
pub const TAG_OFFSET: i32 = 10;
pub const ACP_PORT_BASE: u16 = 9100;
pub const ACP_PORT_MAX: u16 = 9199;
pub const BEZALEL_PORT_BASE: u16 = 9200;
pub const BEZALEL_PORT_MAX: u16 = 9299;

pub fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn allocate_tag_index(exclude: &str) -> Result<i32> {
    let all = load_all()?;
    let used: std::collections::HashSet<i32> = all
        .values()
        .filter(|r| r.work_session_id != exclude)
        .filter_map(|r| r.tag_index)
        .collect();
    let mut idx = TAG_OFFSET;
    while used.contains(&idx) {
        idx += 1;
    }
    Ok(idx)
}

pub fn allocate_acp_port(exclude: &str) -> Result<Option<u16>> {
    let all = load_all()?;
    let used: std::collections::HashSet<u16> = all
        .values()
        .filter(|r| r.work_session_id != exclude)
        .filter_map(|r| r.acp_port)
        .collect();
    for port in ACP_PORT_BASE..=ACP_PORT_MAX {
        if !used.contains(&port) && is_port_available(port) {
            return Ok(Some(port));
        }
    }
    Ok(None)
}

pub fn allocate_bezalel_port(exclude: &str) -> Result<Option<u16>> {
    let all = load_all()?;
    let used: std::collections::HashSet<u16> = all
        .values()
        .filter(|r| r.work_session_id != exclude)
        .filter_map(|r| r.bezalel_port)
        .collect();
    for port in BEZALEL_PORT_BASE..=BEZALEL_PORT_MAX {
        if !used.contains(&port) && is_port_available(port) {
            return Ok(Some(port));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::resource::WorkspaceResourceRef;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    struct IsolatedEnv {
        _dir: PathBuf,
    }

    impl IsolatedEnv {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "awm-rt-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            // SAFETY: held under test_lock(); restored on drop.
            unsafe {
                std::env::set_var("AWESOMETREE_RUNTIME_PATH", dir.join("runtime.json"));
                std::env::set_var("AWESOMETREE_SECRETS_PATH", dir.join("secrets.json"));
                std::env::set_var("HOME", &dir);
            }
            Self { _dir: dir }
        }
    }

    impl Drop for IsolatedEnv {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("AWESOMETREE_RUNTIME_PATH");
                std::env::remove_var("AWESOMETREE_SECRETS_PATH");
            }
        }
    }

    #[test]
    fn runtime_roundtrip_keyed_by_work_session_id() {
        let _lock = test_lock();
        let _env = IsolatedEnv::new();
        let rt = WorkSessionRuntime {
            work_session_id: "ws-a".into(),
            host_id: "testhost".into(),
            workspace: Some(WorkspaceResourceRef {
                workspace_id: "w1".into(),
                resource_id: "r1".into(),
                environment_kind: "git-worktree".into(),
                path: "/tmp/wt".into(),
            }),
            tag_index: Some(11),
            realization_status: RealizationStatus::Ready,
            ..Default::default()
        };
        upsert(rt.clone()).unwrap();
        let got = get("ws-a").unwrap().expect("runtime present");
        assert_eq!(got.work_session_id, "ws-a");
        assert_eq!(got.tag_index, Some(11));
        assert!(got.workspace.is_some());
        remove("ws-a").unwrap();
        assert!(get("ws-a").unwrap().is_none());
    }

    #[test]
    fn secrets_not_in_runtime_document() {
        let _lock = test_lock();
        let _env = IsolatedEnv::new();
        set_bezalel_token("ws-s", "secret-token-value").unwrap();
        let path = std::env::var("AWESOMETREE_RUNTIME_PATH").unwrap();
        let _ = upsert(WorkSessionRuntime {
            work_session_id: "ws-s".into(),
            bezalel_token_ref: Some("bezalel:ws-s".into()),
            ..Default::default()
        });
        let data = fs::read_to_string(path).unwrap();
        assert!(!data.contains("secret-token-value"));
        assert_eq!(
            get_bezalel_token("ws-s").unwrap().as_deref(),
            Some("secret-token-value")
        );
    }

    #[test]
    fn rejects_legacy_workspaces_state() {
        let _lock = test_lock();
        let _env = IsolatedEnv::new();
        let home = std::env::var("HOME").unwrap();
        let cfg = PathBuf::from(&home).join(".config/awesometree");
        fs::create_dir_all(&cfg).unwrap();
        fs::write(
            cfg.join("state.json"),
            r#"{"workspaces":{"old":{"project":"p","active":true}}}"#,
        )
        .unwrap();
        let rt = std::env::var("AWESOMETREE_RUNTIME_PATH").unwrap();
        let _ = fs::remove_file(&rt);
        let err = load_all().expect_err("legacy state must fail");
        assert_eq!(err.code, ErrorCode::UnsupportedOldState);
    }
}
