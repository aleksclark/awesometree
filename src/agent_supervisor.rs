use crate::log as dlog;
use crate::state::{self, AgentStatus};
use a2a_rs_core::AgentCard;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Notify;

/// Events broadcast by the supervisor for real-time Watch streaming.
/// Named `SupervisorEvent` to avoid conflict with the proto `AgentEvent`.
#[derive(Debug, Clone, PartialEq)]
pub enum SupervisorEvent {
    StatusChanged {
        agent_id: String,
        status: AgentStatus,
        workspace: String,
    },
    Stopped {
        agent_id: String,
        workspace: String,
    },
}

const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(3);
const HEALTH_CHECK_RETRIES: u32 = 3;
const _RESTART_DELAY: Duration = Duration::from_secs(2);
const STOP_GRACE: Duration = Duration::from_secs(5);

struct ManagedAgent {
    #[allow(dead_code)]
    agent_id: String,
    workspace: String,
    template: String,
    #[allow(dead_code)]
    name: String,
    port: u16,
    dir: String,
    command: String,
    env: HashMap<String, String>,
    stop_signal: Arc<Notify>,
    agent_card: Arc<Mutex<Option<AgentCard>>>,
    grace_override: Arc<Mutex<Option<Duration>>>,
}

pub struct AgentSupervisor {
    agents: Arc<Mutex<HashMap<String, Arc<ManagedAgent>>>>,
    rt: tokio::runtime::Handle,
    event_tx: tokio::sync::broadcast::Sender<SupervisorEvent>,
}

pub struct SpawnOptions {
    pub workspace: String,
    pub dir: String,
    pub template: String,
    pub name: String,
    pub port: u16,
    pub command: String,
    pub env: HashMap<String, String>,
}

pub struct SpawnResult {
    pub agent_id: String,
    pub port: u16,
}

impl AgentSupervisor {
    pub fn new(rt: tokio::runtime::Handle) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            rt,
            event_tx,
        }
    }

    /// Subscribe to real-time supervisor events.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SupervisorEvent> {
        self.event_tx.subscribe()
    }

    pub fn spawn(&self, opts: SpawnOptions) -> SpawnResult {
        let agent_id = format!(
            "{}-{}",
            opts.name,
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        let stop_signal = Arc::new(Notify::new());
        let agent_card = Arc::new(Mutex::new(None));
        let grace_override = Arc::new(Mutex::new(None));

        let managed = Arc::new(ManagedAgent {
            agent_id: agent_id.clone(),
            workspace: opts.workspace.clone(),
            template: opts.template.clone(),
            name: opts.name.clone(),
            port: opts.port,
            dir: opts.dir.clone(),
            command: opts.command.clone(),
            env: opts.env.clone(),
            stop_signal: stop_signal.clone(),
            agent_card: agent_card.clone(),
            grace_override: grace_override.clone(),
        });

        let port = opts.port;

        {
            let mut agents = self.agents.lock().unwrap();
            agents.insert(agent_id.clone(), managed.clone());
        }

        let agents_map = self.agents.clone();
        let aid = agent_id.clone();
        let event_tx = self.event_tx.clone();
        let workspace = opts.workspace.clone();

        update_agent_state(&aid, AgentStatus::Starting, Some(opts.port));
        let _ = event_tx.send(SupervisorEvent::StatusChanged {
            agent_id: aid.clone(),
            status: AgentStatus::Starting,
            workspace: workspace.clone(),
        });

        self.rt.spawn(async move {
            dlog::log(format!(
                "Agent supervisor: starting {} ({}) on port {} in workspace {}",
                aid, managed.template, managed.port, managed.workspace
            ));

            let child = spawn_agent_process(&managed).await;
            let mut child = match child {
                Ok(c) => c,
                Err(e) => {
                    dlog::log(format!("Agent supervisor: {} spawn failed: {e}", aid));
                    update_agent_state(&aid, AgentStatus::Error, None);
                    let _ = event_tx.send(SupervisorEvent::StatusChanged {
                        agent_id: aid.clone(),
                        status: AgentStatus::Error,
                        workspace: workspace.clone(),
                    });
                    agents_map.lock().unwrap().remove(&aid);
                    return;
                }
            };

            let pid = child.id().unwrap_or(0);
            dlog::log(format!(
                "Agent supervisor: {} running (pid {}, port {})",
                aid, pid, managed.port
            ));

            update_agent_pid(&aid, pid);

            let health_ok =
                wait_for_health(&aid, managed.port, &stop_signal).await;

            if !health_ok {
                dlog::log(format!("Agent supervisor: {} health check failed", aid));
                update_agent_state(&aid, AgentStatus::Error, None);
                let _ = event_tx.send(SupervisorEvent::StatusChanged {
                    agent_id: aid.clone(),
                    status: AgentStatus::Error,
                    workspace: workspace.clone(),
                });
                graceful_stop(&mut child, STOP_GRACE).await;
                agents_map.lock().unwrap().remove(&aid);
                return;
            }

            let card = fetch_agent_card(managed.port).await;
            if let Some(c) = card {
                *managed.agent_card.lock().unwrap() = Some(c);
            }

            update_agent_state(&aid, AgentStatus::Ready, None);
            let _ = event_tx.send(SupervisorEvent::StatusChanged {
                agent_id: aid.clone(),
                status: AgentStatus::Ready,
                workspace: workspace.clone(),
            });
            dlog::log(format!("Agent supervisor: {} is ready", aid));

            tokio::select! {
                status = child.wait() => {
                    let code = status.map(|s| s.code()).unwrap_or(None);
                    dlog::log(format!(
                        "Agent supervisor: {} exited (code {:?})",
                        aid, code
                    ));
                    update_agent_state(&aid, AgentStatus::Error, None);
                    let _ = event_tx.send(SupervisorEvent::StatusChanged {
                        agent_id: aid.clone(),
                        status: AgentStatus::Error,
                        workspace: workspace.clone(),
                    });
                }
                _ = stop_signal.notified() => {
                    dlog::log(format!("Agent supervisor: stopping {} (pid {})", aid, pid));
                    update_agent_state(&aid, AgentStatus::Stopping, None);
                    let _ = event_tx.send(SupervisorEvent::StatusChanged {
                        agent_id: aid.clone(),
                        status: AgentStatus::Stopping,
                        workspace: workspace.clone(),
                    });

                    cancel_agent_tasks(&aid, managed.port).await;

                    let grace = managed.grace_override.lock().unwrap().take()
                        .unwrap_or(STOP_GRACE);
                    graceful_stop(&mut child, grace).await;
                    update_agent_state(&aid, AgentStatus::Stopped, None);
                    let _ = event_tx.send(SupervisorEvent::Stopped {
                        agent_id: aid.clone(),
                        workspace: workspace.clone(),
                    });
                    dlog::log(format!("Agent supervisor: {} stopped", aid));
                }
            }

            agents_map.lock().unwrap().remove(&aid);
        });

        SpawnResult {
            agent_id,
            port,
        }
    }

    pub fn stop(&self, agent_id: &str) {
        self.stop_with_grace(agent_id, None);
    }

    pub fn stop_with_grace(&self, agent_id: &str, grace_period: Option<Duration>) {
        let agents = self.agents.lock().unwrap();
        if let Some(managed) = agents.get(agent_id) {
            if let Some(gp) = grace_period {
                *managed.grace_override.lock().unwrap() = Some(gp);
            }
            managed.stop_signal.notify_one();
        }
    }

    pub fn stop_all(&self) {
        let agents = self.agents.lock().unwrap();
        for (id, managed) in agents.iter() {
            dlog::log(format!("Agent supervisor: signaling stop for {id}"));
            managed.stop_signal.notify_one();
        }
    }

    pub fn stop_workspace_agents(&self, workspace: &str) {
        let agents = self.agents.lock().unwrap();
        for (id, managed) in agents.iter() {
            if managed.workspace == workspace {
                dlog::log(format!(
                    "Agent supervisor: signaling stop for {id} (workspace {workspace})"
                ));
                managed.stop_signal.notify_one();
            }
        }
    }

    pub fn is_running(&self, agent_id: &str) -> bool {
        self.agents.lock().unwrap().contains_key(agent_id)
    }

    pub fn running_agent_ids(&self) -> Vec<String> {
        self.agents.lock().unwrap().keys().cloned().collect()
    }

    pub fn agent_card(&self, agent_id: &str) -> Option<AgentCard> {
        let agents = self.agents.lock().unwrap();
        agents
            .get(agent_id)
            .and_then(|m| m.agent_card.lock().unwrap().clone())
    }

    pub fn agent_port(&self, agent_id: &str) -> Option<u16> {
        let agents = self.agents.lock().unwrap();
        agents.get(agent_id).map(|m| m.port)
    }

    pub fn agent_workspace(&self, agent_id: &str) -> Option<String> {
        let agents = self.agents.lock().unwrap();
        agents.get(agent_id).map(|m| m.workspace.clone())
    }
}

async fn spawn_agent_process(managed: &ManagedAgent) -> Result<Child, String> {
    let full_cmd = format!("{} --cwd {}", managed.command, managed.dir);
    let port_str = managed.port.to_string();

    let mut cmd = Command::new("sh");
    cmd.args(["-c", &full_cmd])
        .current_dir(&managed.dir)
        .env("PORT", &port_str)
        .env("A2A_PORT", &port_str)
        .env("CRUSH_ACP_PORT", &port_str)
        .env("CRUSH_A2A_PORT", &port_str)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    for (k, v) in &managed.env {
        cmd.env(k, v);
    }

    cmd.spawn().map_err(|e| format!("spawn: {e}"))
}

async fn wait_for_health(
    agent_id: &str,
    port: u16,
    stop_signal: &Notify,
) -> bool {
    let url = format!("http://127.0.0.1:{port}/.well-known/agent-card.json");
    let client = reqwest::Client::builder()
        .timeout(HEALTH_CHECK_TIMEOUT)
        .build()
        .unwrap_or_default();

    let mut attempts = 0u32;
    let max_attempts = HEALTH_CHECK_RETRIES * 10;

    loop {
        tokio::select! {
            _ = stop_signal.notified() => {
                return false;
            }
            _ = tokio::time::sleep(HEALTH_CHECK_INTERVAL / 5) => {}
        }

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                dlog::log(format!(
                    "Agent supervisor: {} health check passed (attempt {})",
                    agent_id,
                    attempts + 1
                ));
                return true;
            }
            Ok(resp) => {
                dlog::log(format!(
                    "Agent supervisor: {} health check returned {}",
                    agent_id,
                    resp.status()
                ));
            }
            Err(_) => {}
        }

        attempts += 1;
        if attempts >= max_attempts {
            dlog::log(format!(
                "Agent supervisor: {} health check exhausted ({} attempts)",
                agent_id, attempts
            ));
            return false;
        }
    }
}

async fn fetch_agent_card(port: u16) -> Option<AgentCard> {
    let url = format!("http://127.0.0.1:{port}/.well-known/agent-card.json");
    let client = reqwest::Client::builder()
        .timeout(HEALTH_CHECK_TIMEOUT)
        .build()
        .ok()?;

    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;
    serde_json::from_str(&text).ok()
}

async fn cancel_agent_tasks(agent_id: &str, port: u16) {
    let task_ids: Vec<String> = {
        match crate::arp_store::ArpStore::global() {
            Some(s) => s
                .active_tasks(agent_id)
                .unwrap_or_default()
                .into_iter()
                .map(|t| t.task_id)
                .collect(),
            None => return,
        }
    };

    if task_ids.is_empty() {
        return;
    }

    let base_url = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    for task_id in &task_ids {
        dlog::log(format!(
            "Agent supervisor: canceling task {} on agent {}",
            task_id, agent_id
        ));
        let cancel_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": uuid::Uuid::new_v4().to_string(),
            "method": "tasks/cancel",
            "params": { "id": task_id }
        });
        let _ = client
            .post(format!("{base_url}/"))
            .json(&cancel_body)
            .send()
            .await;
    }

    if let Some(s) = crate::arp_store::ArpStore::global() {
        let _ = s.clear_agent_tasks(agent_id);
    }
}

async fn graceful_stop(child: &mut Child, grace: Duration) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
    }

    match tokio::time::timeout(grace, child.wait()).await {
        Ok(_) => {}
        Err(_) => {
            dlog::log("Agent supervisor: grace period expired, killing");
            let _ = child.kill().await;
        }
    }
}

fn update_agent_state(agent_id: &str, status: AgentStatus, port: Option<u16>) {
    let aid = agent_id.to_string();
    let result = state::modify(|st| {
        if let Some(agent) = st.find_agent_mut(&aid) {
            agent.status = status.clone();
        }
        let _ = port;
    });
    if let Err(e) = result {
        dlog::log(format!(
            "Agent supervisor: failed to update state for {agent_id}: {e}"
        ));
    }
}

fn update_agent_pid(agent_id: &str, pid: u32) {
    let aid = agent_id.to_string();
    let result = state::modify(|st| {
        if let Some(agent) = st.find_agent_mut(&aid) {
            agent.pid = Some(pid);
        }
    });
    if let Err(e) = result {
        dlog::log(format!(
            "Agent supervisor: failed to update pid for {agent_id}: {e}"
        ));
    }
}

static GLOBAL_AGENT_SUPERVISOR: std::sync::OnceLock<AgentSupervisor> = std::sync::OnceLock::new();

pub fn init(rt: tokio::runtime::Handle) {
    let _ = GLOBAL_AGENT_SUPERVISOR.set(AgentSupervisor::new(rt));
}

pub fn get() -> Option<&'static AgentSupervisor> {
    GLOBAL_AGENT_SUPERVISOR.get()
}

pub fn spawn_agent(opts: SpawnOptions) -> Option<SpawnResult> {
    get().map(|sup| sup.spawn(opts))
}

pub fn stop_agent(agent_id: &str) {
    if let Some(sup) = get() {
        sup.stop(agent_id);
    }
}

pub fn stop_agent_with_grace(agent_id: &str, grace_period_ms: u32) {
    if let Some(sup) = get() {
        let grace = Duration::from_millis(grace_period_ms as u64);
        sup.stop_with_grace(agent_id, Some(grace));
    }
}

pub fn stop_workspace_agents(workspace: &str) {
    if let Some(sup) = get() {
        sup.stop_workspace_agents(workspace);
    }
}

pub fn stop_all_agents() {
    if let Some(sup) = get() {
        sup.stop_all();
    }
}

pub fn agent_card(agent_id: &str) -> Option<AgentCard> {
    get().and_then(|sup| sup.agent_card(agent_id))
}

pub fn agent_port(agent_id: &str) -> Option<u16> {
    get().and_then(|sup| sup.agent_port(agent_id))
}

/// Subscribe to real-time supervisor events from the global instance.
pub fn subscribe_events() -> Option<tokio::sync::broadcast::Receiver<SupervisorEvent>> {
    get().map(|sup| sup.subscribe())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_creation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sup = AgentSupervisor::new(rt.handle().clone());
        assert!(sup.running_agent_ids().is_empty());
        assert!(!sup.is_running("test"));
    }

    #[test]
    fn agent_card_none_when_not_running() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sup = AgentSupervisor::new(rt.handle().clone());
        assert!(sup.agent_card("nonexistent").is_none());
    }

    #[test]
    fn agent_port_none_when_not_running() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sup = AgentSupervisor::new(rt.handle().clone());
        assert!(sup.agent_port("nonexistent").is_none());
    }

    #[test]
    fn subscribe_returns_receiver() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sup = AgentSupervisor::new(rt.handle().clone());
        let _rx = sup.subscribe();
        // Should not panic; receiver is valid
    }

    #[test]
    fn broadcast_event_received() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sup = AgentSupervisor::new(rt.handle().clone());
        let mut rx = sup.subscribe();
        let _ = sup.event_tx.send(SupervisorEvent::StatusChanged {
            agent_id: "test-agent".into(),
            status: AgentStatus::Ready,
            workspace: "ws1".into(),
        });
        let event = rx.try_recv().unwrap();
        match event {
            SupervisorEvent::StatusChanged { agent_id, status, workspace } => {
                assert_eq!(agent_id, "test-agent");
                assert_eq!(status, AgentStatus::Ready);
                assert_eq!(workspace, "ws1");
            }
            _ => panic!("expected StatusChanged event"),
        }
    }

    #[test]
    fn broadcast_no_receivers_does_not_panic() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sup = AgentSupervisor::new(rt.handle().clone());
        // Sending with no receivers should not panic
        let _ = sup.event_tx.send(SupervisorEvent::Stopped {
            agent_id: "test-agent".into(),
            workspace: "ws1".into(),
        });
    }
}
