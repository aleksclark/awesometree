use crate::log as dlog;
use crate::runtime_store;
use rand::Rng;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Notify;

const RESTART_DELAY: Duration = Duration::from_secs(2);
const STOP_GRACE: Duration = Duration::from_secs(5);
const BEZALEL_BIN: &str = "bezalel";

struct ManagedProcess {
    stop_signal: Arc<Notify>,
}

pub struct Supervisor {
    procs: Arc<Mutex<HashMap<String, ManagedProcess>>>,
    rt: tokio::runtime::Handle,
}

/// Generate a random bearer token for a bezalel instance.
pub fn generate_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Supervisor {
    pub fn new(rt: tokio::runtime::Handle) -> Self {
        Self {
            procs: Arc::new(Mutex::new(HashMap::new())),
            rt,
        }
    }

    pub fn start(&self, workspace: &str, dir: &str, port: u16, token: &str) {
        let ws = workspace.to_string();

        {
            let procs = self.procs.lock().unwrap();
            if procs.contains_key(&ws) {
                dlog::log(format!("Bezalel supervisor: {ws} already running"));
                return;
            }
        }

        let stop_signal = Arc::new(Notify::new());
        let managed = ManagedProcess {
            stop_signal: stop_signal.clone(),
        };

        {
            let mut procs = self.procs.lock().unwrap();
            procs.insert(ws.clone(), managed);
        }

        let procs = self.procs.clone();
        let dir = dir.to_string();
        let token = token.to_string();

        self.rt.spawn(async move {
            dlog::log(format!("Bezalel supervisor: starting {ws} on port {port}"));

            loop {
                let child = spawn_bezalel(&dir, port, &token).await;
                let mut child = match child {
                    Ok(c) => c,
                    Err(e) => {
                        dlog::log(format!("Bezalel supervisor: {ws} spawn failed: {e}"));
                        tokio::select! {
                            _ = stop_signal.notified() => {
                                dlog::log(format!("Bezalel supervisor: {ws} stopped (spawn failed)"));
                                break;
                            }
                            _ = tokio::time::sleep(RESTART_DELAY) => continue,
                        }
                    }
                };

                let pid = child.id().unwrap_or(0);
                dlog::log(format!("Bezalel supervisor: {ws} running (pid {pid}, port {port})"));

                tokio::select! {
                    status = child.wait() => {
                        let code = status.map(|s| s.code()).unwrap_or(None);
                        dlog::log(format!(
                            "Bezalel supervisor: {ws} exited (code {:?}), restarting in {}s",
                            code, RESTART_DELAY.as_secs()
                        ));
                        tokio::select! {
                            _ = stop_signal.notified() => {
                                dlog::log(format!("Bezalel supervisor: {ws} stopped (no restart)"));
                                break;
                            }
                            _ = tokio::time::sleep(RESTART_DELAY) => continue,
                        }
                    }
                    _ = stop_signal.notified() => {
                        dlog::log(format!("Bezalel supervisor: stopping {ws} (pid {pid})"));
                        graceful_stop(&mut child).await;
                        dlog::log(format!("Bezalel supervisor: {ws} stopped"));
                        break;
                    }
                }
            }

            procs.lock().unwrap().remove(&ws);
        });
    }

    pub fn stop(&self, workspace: &str) {
        let procs = self.procs.lock().unwrap();
        if let Some(managed) = procs.get(workspace) {
            managed.stop_signal.notify_one();
        }
    }

    pub fn stop_all(&self) {
        let procs = self.procs.lock().unwrap();
        for (name, managed) in procs.iter() {
            dlog::log(format!("Bezalel supervisor: signaling stop for {name}"));
            managed.stop_signal.notify_one();
        }
    }

    pub fn is_running(&self, workspace: &str) -> bool {
        self.procs.lock().unwrap().contains_key(workspace)
    }

    pub fn running_workspaces(&self) -> Vec<String> {
        self.procs.lock().unwrap().keys().cloned().collect()
    }
}

async fn spawn_bezalel(dir: &str, port: u16, token: &str) -> Result<Child, String> {
    Command::new(BEZALEL_BIN)
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--workdir",
            dir,
            "--auth-token",
            token,
        ])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("spawn: {e}"))
}

async fn graceful_stop(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
    }

    match tokio::time::timeout(STOP_GRACE, child.wait()).await {
        Ok(_) => {}
        Err(_) => {
            dlog::log("Bezalel supervisor: grace period expired, killing");
            let _ = child.kill().await;
        }
    }
}

static GLOBAL_SUPERVISOR: std::sync::OnceLock<Supervisor> = std::sync::OnceLock::new();

pub fn init(rt: tokio::runtime::Handle) {
    let _ = GLOBAL_SUPERVISOR.set(Supervisor::new(rt));
}

pub fn get() -> Option<&'static Supervisor> {
    GLOBAL_SUPERVISOR.get()
}

pub fn start_for_workspace(workspace: &str, dir: &str, port: u16, token: &str) {
    if let Some(sup) = get() {
        sup.start(workspace, dir, port, token);
    }
}

pub fn stop_for_workspace(workspace: &str) {
    if let Some(sup) = get() {
        sup.stop(workspace);
    }
}

pub fn stop_all() {
    if let Some(sup) = get() {
        sup.stop_all();
    }
}

pub fn start_active_workspaces() {
    let runtimes = match runtime_store::load_all() {
        Ok(r) => r,
        Err(e) => {
            crate::log::log(format!("bezalel: load runtime: {e}"));
            return;
        }
    };
    let running_set: std::collections::HashSet<String> = get()
        .map(|s| s.procs.lock().unwrap().keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .collect();
    for (name, rt) in runtimes {
        if !rt.headless {
            continue;
        }
        if running_set.contains(name.as_str()) {
            continue;
        }
        let Some(port) = rt.bezalel_port else { continue };
        let dir = rt.workspace.as_ref().map(|w| w.path.clone()).unwrap_or_default();
        let token = runtime_store::get_bezalel_token(&name).ok().flatten().unwrap_or_default();
        if token.is_empty() {
            continue;
        }
        start_for_workspace(&name, &dir, port, &token);
    }
}

/// Start bezalel for every active headless workspace that has a port + token,
/// and stop any running instance whose workspace is no longer active/headless.
pub fn sync_workspaces() {
    let sup = match get() {
        Some(s) => s,
        None => return,
    };

    let running = sup.running_workspaces();
    let running_set: std::collections::HashSet<&str> = running.iter().map(|s| s.as_str()).collect();

    // Restart headless WorkSessions that have runtime ports/tokens but no live process.
    if let Ok(runtimes) = runtime_store::load_all() {
        for (ws_id, rt) in runtimes {
            if !rt.headless {
                continue;
            }
            if running_set.contains(ws_id.as_str()) {
                continue;
            }
            let Some(port) = rt.bezalel_port else { continue; };
            let Ok(Some(token)) = runtime_store::get_bezalel_token(&ws_id) else { continue; };
            let dir = rt
                .workspace
                .as_ref()
                .map(|w| w.path.clone())
                .unwrap_or_default();
            sup.start(&ws_id, &dir, port, &token);
        }
    }

    for name in &running {
        match runtime_store::get(name) {
            Ok(Some(rt)) if rt.headless => {}
            _ => {
                stop_for_workspace(name);
            }
        }
    }
}

pub fn start_sync_loop(interval: Duration) {
    if get().is_some() {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                sync_workspaces();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_creation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sup = Supervisor::new(rt.handle().clone());
        assert!(sup.running_workspaces().is_empty());
        assert!(!sup.is_running("test"));
    }

    #[test]
    fn tokens_are_unique_and_hex() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
