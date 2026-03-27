use crate::log as dlog;
use crate::state;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Notify;

const RESTART_DELAY: Duration = Duration::from_secs(2);
const STOP_GRACE: Duration = Duration::from_secs(5);

struct ManagedProcess {
    stop_signal: Arc<Notify>,
}

pub struct Supervisor {
    procs: Arc<Mutex<HashMap<String, ManagedProcess>>>,
    rt: tokio::runtime::Handle,
}

impl Supervisor {
    pub fn new(rt: tokio::runtime::Handle) -> Self {
        Self {
            procs: Arc::new(Mutex::new(HashMap::new())),
            rt,
        }
    }

    pub fn start(&self, workspace: &str, dir: &str, port: u16, command: Option<&str>) {
        let ws = workspace.to_string();

        {
            let procs = self.procs.lock().unwrap();
            if procs.contains_key(&ws) {
                dlog::log(format!("ACP supervisor: {ws} already running"));
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
        let cmd_str = command.unwrap_or("crush serve").to_string();
        let port_str = port.to_string();

        self.rt.spawn(async move {
            dlog::log(format!("ACP supervisor: starting {ws} on port {port}"));

            loop {
                let child = spawn_acp(&cmd_str, &dir, &port_str).await;
                let mut child = match child {
                    Ok(c) => c,
                    Err(e) => {
                        dlog::log(format!("ACP supervisor: {ws} spawn failed: {e}"));
                        tokio::select! {
                            _ = stop_signal.notified() => {
                                dlog::log(format!("ACP supervisor: {ws} stopped (spawn failed)"));
                                break;
                            }
                            _ = tokio::time::sleep(RESTART_DELAY) => continue,
                        }
                    }
                };

                let pid = child.id().unwrap_or(0);
                dlog::log(format!("ACP supervisor: {ws} running (pid {pid}, port {port})"));

                update_state_acp(&ws, port);

                tokio::select! {
                    status = child.wait() => {
                        let code = status.map(|s| s.code()).unwrap_or(None);
                        dlog::log(format!(
                            "ACP supervisor: {ws} exited (code {:?}), restarting in {}s",
                            code, RESTART_DELAY.as_secs()
                        ));
                        tokio::select! {
                            _ = stop_signal.notified() => {
                                dlog::log(format!("ACP supervisor: {ws} stopped (no restart)"));
                                break;
                            }
                            _ = tokio::time::sleep(RESTART_DELAY) => continue,
                        }
                    }
                    _ = stop_signal.notified() => {
                        dlog::log(format!("ACP supervisor: stopping {ws} (pid {pid})"));
                        graceful_stop(&mut child).await;
                        dlog::log(format!("ACP supervisor: {ws} stopped"));
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
            dlog::log(format!("ACP supervisor: signaling stop for {name}"));
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

async fn spawn_acp(cmd: &str, dir: &str, port: &str) -> Result<Child, String> {
    let full_cmd = format!("{cmd} --cwd {dir}");
    Command::new("sh")
        .args(["-c", &full_cmd])
        .current_dir(dir)
        .env("PORT", port)
        .env("CRUSH_ACP_PORT", port)
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
            dlog::log("ACP supervisor: grace period expired, killing");
            let _ = child.kill().await;
        }
    }
}

fn update_state_acp(workspace: &str, port: u16) {
    let result = (|| {
        let mut st = state::load()?;
        if let Some(ws) = st.workspaces.get_mut(workspace) {
            ws.acp_port = Some(port);
            if ws.acp_url.is_none() {
                ws.acp_url = Some(format!("http://127.0.0.1:{port}"));
            }
            state::save(&st)?;
        }
        Ok::<(), String>(())
    })();
    if let Err(e) = result {
        dlog::log(format!("ACP supervisor: failed to update state for {workspace}: {e}"));
    }
}

static GLOBAL_SUPERVISOR: std::sync::OnceLock<Supervisor> = std::sync::OnceLock::new();

pub fn init(rt: tokio::runtime::Handle) {
    let _ = GLOBAL_SUPERVISOR.set(Supervisor::new(rt));
}

pub fn get() -> Option<&'static Supervisor> {
    GLOBAL_SUPERVISOR.get()
}

pub fn start_for_workspace(workspace: &str, dir: &str, port: u16, command: Option<&str>) {
    if let Some(sup) = get() {
        sup.start(workspace, dir, port, command);
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
    sync_workspaces();
}

pub fn sync_workspaces() {
    let sup = match get() {
        Some(s) => s,
        None => return,
    };

    let st = match state::load() {
        Ok(s) => s,
        Err(e) => {
            dlog::log(format!("ACP supervisor: load state failed: {e}"));
            return;
        }
    };

    let running = sup.running_workspaces();
    let running_set: std::collections::HashSet<&str> = running.iter().map(|s| s.as_str()).collect();

    for (name, ws) in &st.workspaces {
        if !ws.active || ws.dir.is_empty() {
            continue;
        }

        if running_set.contains(name.as_str()) {
            continue;
        }

        let project = match crate::interop::load(&ws.project) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let acp = match project.acp_config() {
            Some(acp) if acp.enabled => acp,
            _ => continue,
        };

        let port = match ws.acp_port {
            Some(p) => p,
            None => continue,
        };

        let cmd = acp.command.as_deref();
        sup.start(name, &ws.dir, port, cmd);
    }

    for name in &running {
        match st.workspaces.get(name) {
            Some(ws) if ws.active => {}
            _ => {
                dlog::log(format!("ACP supervisor: stopping orphan {name}"));
                sup.stop(name);
            }
        }
    }
}

pub fn start_sync_loop(interval: Duration) {
    if let Some(_) = get() {
        let interval = interval;
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
}
