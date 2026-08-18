//! Host-local agent instance records only.
//!
//! Does NOT store Project, WorkProfile, or WorkSession definitions/lifecycle.
//! Agent rows are keyed by work_session_id (material episode identity from Switchboard).

use crate::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// On-disk agents document. Rejects legacy workspace-episode state.json.
const DOCUMENT_VERSION: u32 = 2;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Store {
    #[serde(default)]
    pub version: u32,
    /// work_session_id → agent instances
    #[serde(default)]
    pub agents: HashMap<String, Vec<AgentInstanceState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Starting,
    Ready,
    Busy,
    Error,
    Stopping,
    #[default]
    Stopped,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Starting => write!(f, "starting"),
            AgentStatus::Ready => write!(f, "ready"),
            AgentStatus::Busy => write!(f, "busy"),
            AgentStatus::Error => write!(f, "error"),
            AgentStatus::Stopping => write!(f, "stopping"),
            AgentStatus::Stopped => write!(f, "stopped"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AgentInstanceState {
    pub id: String,
    pub template: String,
    pub name: String,
    /// Authoritative WorkSession id from Switchboard (not a Workspace Resource).
    #[serde(alias = "workspace")]
    pub work_session_id: String,
    pub status: AgentStatus,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default)]
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawned_by: Option<String>,
}

impl AgentInstanceState {
    /// Returns the base URL for this agent's A2A endpoints.
    pub fn base_url(&self) -> String {
        match &self.host {
            Some(h) => {
                if h.starts_with("http") {
                    h.clone()
                } else {
                    format!("http://{}:{}", h, self.port)
                }
            }
            None => format!("http://127.0.0.1:{}", self.port),
        }
    }
}

pub const TAG_OFFSET: i32 = 10;
pub const AGENT_PORT_BASE: u16 = 9100;
pub const AGENT_PORT_MAX: u16 = 9199;
pub const BEZALEL_PORT_BASE: u16 = 9200;
pub const BEZALEL_PORT_MAX: u16 = 9299;

/// Check if a port is actually available on the system by attempting to bind.
pub fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn state_dir() -> PathBuf {
    paths::home_dir().join(".config/awesometree")
}

fn agents_path() -> PathBuf {
    if let Ok(p) = std::env::var("AWESOMETREE_AGENTS_PATH") {
        return PathBuf::from(p);
    }
    state_dir().join("agents.json")
}

fn legacy_state_path() -> PathBuf {
    state_dir().join("state.json")
}

fn reject_legacy() -> Result<(), String> {
    let path = legacy_state_path();
    if path.exists() && !agents_path().exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if data.contains("\"workspaces\"") {
                return Err(format!(
                    "unsupported old state at {}: recreate WorkSessions in Switchboard; old workspace-episode state is not migrated",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

pub fn load() -> Result<Store, String> {
    reject_legacy()?;
    let path = agents_path();
    if !path.exists() {
        return Ok(Store {
            version: DOCUMENT_VERSION,
            ..Default::default()
        });
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("read agents: {e}"))?;
    let mut store: Store =
        serde_json::from_str(&data).map_err(|e| format!("parse agents: {e}"))?;
    if store.version == 0 {
        store.version = DOCUMENT_VERSION;
    }
    if store.version != DOCUMENT_VERSION {
        return Err(format!(
            "unsupported agents store version {} (want {DOCUMENT_VERSION})",
            store.version
        ));
    }
    Ok(store)
}

static SAVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn save(store: &Store) -> Result<(), String> {
    let _guard = SAVE_LOCK.lock().unwrap();
    save_inner(store)
}

pub fn modify<F>(f: F) -> Result<(), String>
where
    F: FnOnce(&mut Store),
{
    let _guard = SAVE_LOCK.lock().unwrap();
    let mut store = load()?;
    f(&mut store);
    save_inner(&store)
}

fn save_inner(store: &Store) -> Result<(), String> {
    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create agents dir: {e}"))?;
    let mut s = store.clone();
    s.version = DOCUMENT_VERSION;
    let data = serde_json::to_string_pretty(&s).map_err(|e| format!("serialize agents: {e}"))?;
    let path = agents_path();
    let tmp = dir.join(format!(".agents.json.{}.tmp", std::process::id()));
    fs::write(&tmp, &data).map_err(|e| format!("write tmp: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
    Ok(())
}

impl Store {
    /// True when any agent rows exist for this WorkSession id.
    pub fn has_work_session(&self, work_session_id: &str) -> bool {
        self.agents.contains_key(work_session_id)
    }

    
    /// Look up the agent bucket keyed by work_session_id.
    pub fn work_session_id(&self, id: &str) -> Option<&Vec<AgentInstanceState>> {
        self.agents.get(id)
    }

    pub fn agents_for(&self, work_session_id: &str) -> &[AgentInstanceState] {
        self.agents
            .get(work_session_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn find_agent(&self, agent_id: &str) -> Option<(&str, &AgentInstanceState)> {
        for (ws, agents) in &self.agents {
            if let Some(a) = agents.iter().find(|a| a.id == agent_id) {
                return Some((ws.as_str(), a));
            }
        }
        None
    }

    pub fn find_agent_by_name(&self, name: &str) -> Option<(&str, &AgentInstanceState)> {
        let mut best: Option<(&str, &AgentInstanceState)> = None;
        for (ws, agents) in &self.agents {
            for agent in agents {
                if agent.name != name {
                    continue;
                }
                if agent.status == AgentStatus::Stopped || agent.status == AgentStatus::Stopping {
                    continue;
                }
                match best {
                    None => best = Some((ws.as_str(), agent)),
                    Some((_, existing)) => {
                        if agent.status == AgentStatus::Ready
                            && existing.status != AgentStatus::Ready
                        {
                            best = Some((ws.as_str(), agent));
                        }
                    }
                }
            }
        }
        best
    }

    pub fn find_agent_in_session(
        &self,
        work_session_id: &str,
        name: &str,
    ) -> Option<(&str, &AgentInstanceState)> {
        let agents = self.agents.get(work_session_id)?;
        let mut best: Option<&AgentInstanceState> = None;
        for agent in agents {
            if agent.name != name {
                continue;
            }
            if agent.status == AgentStatus::Stopped || agent.status == AgentStatus::Stopping {
                continue;
            }
            match best {
                None => best = Some(agent),
                Some(existing) => {
                    if agent.status == AgentStatus::Ready
                        && existing.status != AgentStatus::Ready
                    {
                        best = Some(agent);
                    }
                }
            }
        }
        best.map(|a| (a.work_session_id.as_str(), a))
    }

    pub fn resolve_agent_flexible(&self, identifier: &str) -> Option<(&str, &AgentInstanceState)> {
        if let Some(found) = self.find_agent(identifier) {
            if found.1.status != AgentStatus::Stopped && found.1.status != AgentStatus::Stopping {
                return Some(found);
            }
        }
        self.find_agent_by_name(identifier)
    }

    pub fn find_agent_mut(&mut self, agent_id: &str) -> Option<&mut AgentInstanceState> {
        for agents in self.agents.values_mut() {
            if let Some(a) = agents.iter_mut().find(|a| a.id == agent_id) {
                return Some(a);
            }
        }
        None
    }

    pub fn all_agents(&self) -> Vec<(&str, &AgentInstanceState)> {
        let mut out = Vec::new();
        for (ws, agents) in &self.agents {
            for a in agents {
                out.push((ws.as_str(), a));
            }
        }
        out
    }

    pub fn add_agent(&mut self, work_session_id: &str, mut agent: AgentInstanceState) {
        agent.work_session_id = work_session_id.to_string();
        self.agents
            .entry(work_session_id.to_string())
            .or_default()
            .push(agent);
    }

    pub fn update_agent_status(&mut self, agent_id: &str, status: AgentStatus) {
        if let Some(a) = self.find_agent_mut(agent_id) {
            a.status = status;
        }
    }

    pub fn remove_agent(&mut self, agent_id: &str) -> bool {
        for agents in self.agents.values_mut() {
            if let Some(i) = agents.iter().position(|a| a.id == agent_id) {
                agents.remove(i);
                return true;
            }
        }
        false
    }

    pub fn remove_session_agents(&mut self, work_session_id: &str) {
        self.agents.remove(work_session_id);
    }

    pub fn allocate_agent_port(&self, exclude_agent_id: Option<&str>) -> Option<u16> {
        let used: std::collections::HashSet<u16> = self
            .all_agents()
            .into_iter()
            .filter(|(_, a)| exclude_agent_id != Some(a.id.as_str()))
            .filter(|(_, a)| a.status != AgentStatus::Stopped)
            .map(|(_, a)| a.port)
            .collect();
        (AGENT_PORT_BASE..=AGENT_PORT_MAX).find(|port| !used.contains(port) && is_port_available(*port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(id: &str, name: &str, ws: &str, port: u16) -> AgentInstanceState {
        AgentInstanceState {
            id: id.into(),
            template: "crush".into(),
            name: name.into(),
            work_session_id: ws.into(),
            status: AgentStatus::Ready,
            port,
            host: None,
            pid: None,
            started_at: "now".into(),
            token_id: None,
            session_id: None,
            spawned_by: None,
        }
    }

    #[test]
    fn agent_status_display() {
        assert_eq!(AgentStatus::Starting.to_string(), "starting");
        assert_eq!(AgentStatus::Ready.to_string(), "ready");
        assert_eq!(AgentStatus::Busy.to_string(), "busy");
        assert_eq!(AgentStatus::Error.to_string(), "error");
        assert_eq!(AgentStatus::Stopping.to_string(), "stopping");
        assert_eq!(AgentStatus::Stopped.to_string(), "stopped");
    }

    #[test]
    fn agent_status_default_is_stopped() {
        let status = AgentStatus::default();
        assert_eq!(status, AgentStatus::Stopped);
    }

    #[test]
    fn add_and_find_agent() {
        let mut s = Store::default();
        s.add_agent("ws1", make_agent("a1", "main", "ws1", 9100));
        let (ws, a) = s.find_agent("a1").unwrap();
        assert_eq!(ws, "ws1");
        assert_eq!(a.name, "main");
        assert_eq!(a.work_session_id, "ws1");
    }

    #[test]
    fn find_agent_by_name_prefers_ready() {
        let mut s = Store::default();
        let mut busy = make_agent("a1", "bot", "ws1", 9100);
        busy.status = AgentStatus::Busy;
        let ready = make_agent("a2", "bot", "ws2", 9101);
        s.add_agent("ws1", busy);
        s.add_agent("ws2", ready);
        let (ws, a) = s.find_agent_by_name("bot").unwrap();
        assert_eq!(ws, "ws2");
        assert_eq!(a.id, "a2");
    }

    #[test]
    fn base_url_defaults_to_localhost() {
        let a = make_agent("a", "n", "ws", 9123);
        assert_eq!(a.base_url(), "http://127.0.0.1:9123");
    }
}
