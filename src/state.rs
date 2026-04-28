use crate::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Store {
    #[serde(default)]
    pub workspaces: HashMap<String, WorkspaceState>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct WorkspaceState {
    pub project: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub tag_index: i32,
    #[serde(default)]
    pub dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<AgentInstanceState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Starting,
    Ready,
    Busy,
    Error,
    Stopping,
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

impl Default for AgentStatus {
    fn default() -> Self {
        AgentStatus::Stopped
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AgentInstanceState {
    pub id: String,
    pub template: String,
    pub name: String,
    pub workspace: String,
    pub status: AgentStatus,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default)]
    pub started_at: String,
}

impl AgentInstanceState {
    /// Returns the base URL for this agent's A2A endpoints.
    /// Uses `host` if set, otherwise falls back to 127.0.0.1:{port}.
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
pub const ACP_PORT_BASE: u16 = 9100;
pub const ACP_PORT_MAX: u16 = 9199;

fn state_dir() -> PathBuf {
    paths::home_dir().join(".config/awesometree")
}

fn state_path() -> PathBuf {
    state_dir().join("state.json")
}

pub fn load() -> Result<Store, String> {
    let path = state_path();
    if !path.exists() {
        return Ok(Store::default());
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("read state: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse state: {e}"))
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
    fs::create_dir_all(&dir).map_err(|e| format!("create state dir: {e}"))?;
    let data = serde_json::to_string_pretty(store).map_err(|e| format!("serialize state: {e}"))?;
    let path = state_path();
    let tmp = dir.join(format!(".state.json.{}.tmp", std::process::id()));
    fs::write(&tmp, &data).map_err(|e| format!("write tmp: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
    Ok(())
}

impl Store {
    pub fn workspace(&self, name: &str) -> Option<&WorkspaceState> {
        self.workspaces.get(name)
    }

    pub fn set_active(&mut self, name: &str, project: &str, tag_index: i32, dir: &str, acp_port: Option<u16>, acp_url: Option<String>) {
        let ws = self.workspaces.entry(name.to_string()).or_insert_with(|| {
            WorkspaceState {
                project: project.to_string(),
                ..Default::default()
            }
        });
        ws.project = project.to_string();
        ws.active = true;
        ws.tag_index = tag_index;
        ws.dir = dir.to_string();
        ws.acp_port = acp_port;
        ws.acp_url = acp_url;
    }

    pub fn set_inactive(&mut self, name: &str) {
        if let Some(ws) = self.workspaces.get_mut(name) {
            ws.active = false;
            ws.tag_index = 0;
            ws.dir.clear();
            ws.acp_port = None;
            ws.acp_url = None;
            ws.acp_session_id = None;
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.workspaces.remove(name);
    }

    pub fn active_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .workspaces
            .iter()
            .filter(|(_, ws)| ws.active)
            .map(|(name, _)| name.clone())
            .collect();
        names.sort();
        names
    }

    pub fn all_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.workspaces.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn workspaces_for_project(&self, project: &str) -> Vec<(String, &WorkspaceState)> {
        let mut result: Vec<_> = self
            .workspaces
            .iter()
            .filter(|(_, ws)| ws.project == project)
            .map(|(name, ws)| (name.clone(), ws))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    pub fn allocate_tag_index(&self, name: &str) -> i32 {
        if let Some(ws) = self.workspaces.get(name) {
            if ws.tag_index > 0 {
                return ws.tag_index;
            }
        }
        let used: std::collections::HashSet<i32> = self
            .workspaces
            .values()
            .filter(|ws| ws.active)
            .map(|ws| ws.tag_index)
            .collect();
        let mut i = TAG_OFFSET;
        while used.contains(&i) {
            i += 1;
        }
        i
    }

    pub fn allocate_acp_port(&self, name: &str) -> Option<u16> {
        if let Some(ws) = self.workspaces.get(name) {
            if let Some(port) = ws.acp_port {
                return Some(port);
            }
        }
        let used: std::collections::HashSet<u16> = self
            .workspaces
            .values()
            .filter(|ws| ws.active)
            .filter_map(|ws| ws.acp_port)
            .collect();
        let mut port = ACP_PORT_BASE;
        while used.contains(&port) && port <= ACP_PORT_MAX {
            port += 1;
        }
        if port > ACP_PORT_MAX {
            None
        } else {
            Some(port)
        }
    }

    pub fn workspace_by_acp_port(&self, port: u16) -> Option<(&str, &WorkspaceState)> {
        self.workspaces
            .iter()
            .find(|(_, ws)| ws.active && ws.acp_port == Some(port))
            .map(|(name, ws)| (name.as_str(), ws))
    }

    pub fn workspace_name_for_route(&self, route: &str) -> Option<(&str, &WorkspaceState)> {
        self.workspaces
            .iter()
            .find(|(name, ws)| ws.active && name.as_str() == route)
            .map(|(name, ws)| (name.as_str(), ws))
    }

    pub fn allocate_agent_port(&self) -> Option<u16> {
        let used: std::collections::HashSet<u16> = self
            .workspaces
            .values()
            .filter(|ws| ws.active)
            .flat_map(|ws| {
                let legacy = ws.acp_port.into_iter();
                let agent_ports = ws.agents.iter().map(|a| a.port);
                legacy.chain(agent_ports)
            })
            .collect();
        let mut port = ACP_PORT_BASE;
        while used.contains(&port) && port <= ACP_PORT_MAX {
            port += 1;
        }
        if port > ACP_PORT_MAX {
            None
        } else {
            Some(port)
        }
    }

    pub fn find_agent(&self, agent_id: &str) -> Option<(&str, &AgentInstanceState)> {
        for (ws_name, ws) in &self.workspaces {
            for agent in &ws.agents {
                if agent.id == agent_id {
                    return Some((ws_name.as_str(), agent));
                }
            }
        }
        None
    }

    pub fn find_agent_mut(&mut self, agent_id: &str) -> Option<&mut AgentInstanceState> {
        for ws in self.workspaces.values_mut() {
            for agent in &mut ws.agents {
                if agent.id == agent_id {
                    return Some(agent);
                }
            }
        }
        None
    }

    pub fn all_agents(&self) -> Vec<(&str, &AgentInstanceState)> {
        let mut result = Vec::new();
        for (ws_name, ws) in &self.workspaces {
            for agent in &ws.agents {
                result.push((ws_name.as_str(), agent));
            }
        }
        result.sort_by(|a, b| a.1.id.cmp(&b.1.id));
        result
    }

    pub fn add_agent(&mut self, ws_name: &str, agent: AgentInstanceState) {
        if let Some(ws) = self.workspaces.get_mut(ws_name) {
            ws.agents.push(agent);
        }
    }

    pub fn remove_agent(&mut self, agent_id: &str) {
        for ws in self.workspaces.values_mut() {
            ws.agents.retain(|a| a.id != agent_id);
        }
    }

    pub fn update_agent_status(&mut self, agent_id: &str, status: AgentStatus) {
        if let Some(agent) = self.find_agent_mut(agent_id) {
            agent.status = status;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> Store {
        Store::default()
    }

    #[test]
    fn empty_store() {
        let s = make_store();
        assert!(s.workspace("foo").is_none());
        assert!(s.active_names().is_empty());
        assert!(s.all_names().is_empty());
    }

    #[test]
    fn set_active_creates_workspace() {
        let mut s = make_store();
        s.set_active("feat-1", "myproject", 10, "/tmp/feat-1", None, None);
        let ws = s.workspace("feat-1").unwrap();
        assert_eq!(ws.project, "myproject");
        assert!(ws.active);
        assert_eq!(ws.tag_index, 10);
        assert_eq!(ws.dir, "/tmp/feat-1");
        assert!(ws.acp_port.is_none());
    }

    #[test]
    fn set_active_updates_existing() {
        let mut s = make_store();
        s.set_active("feat-1", "proj-a", 10, "/tmp/a", None, None);
        s.set_active("feat-1", "proj-b", 11, "/tmp/b", None, None);
        let ws = s.workspace("feat-1").unwrap();
        assert_eq!(ws.project, "proj-b");
        assert_eq!(ws.tag_index, 11);
    }

    #[test]
    fn set_inactive_clears_fields() {
        let mut s = make_store();
        s.set_active("feat-1", "proj", 10, "/tmp/feat-1", Some(9100), None);
        s.set_inactive("feat-1");
        let ws = s.workspace("feat-1").unwrap();
        assert!(!ws.active);
        assert_eq!(ws.tag_index, 0);
        assert!(ws.dir.is_empty());
        assert_eq!(ws.project, "proj");
        assert!(ws.acp_port.is_none());
    }

    #[test]
    fn set_inactive_nonexistent_noop() {
        let mut s = make_store();
        s.set_inactive("ghost");
        assert!(s.workspace("ghost").is_none());
    }

    #[test]
    fn remove_workspace() {
        let mut s = make_store();
        s.set_active("feat-1", "proj", 10, "/tmp", None, None);
        s.remove("feat-1");
        assert!(s.workspace("feat-1").is_none());
    }

    #[test]
    fn active_names_sorted() {
        let mut s = make_store();
        s.set_active("charlie", "p", 10, "/tmp", None, None);
        s.set_active("alice", "p", 11, "/tmp", None, None);
        s.set_active("bob", "p", 12, "/tmp", None, None);
        s.set_inactive("bob");
        assert_eq!(s.active_names(), vec!["alice", "charlie"]);
    }

    #[test]
    fn all_names_sorted() {
        let mut s = make_store();
        s.set_active("charlie", "p", 10, "/tmp", None, None);
        s.set_active("alice", "p", 11, "/tmp", None, None);
        s.set_inactive("alice");
        assert_eq!(s.all_names(), vec!["alice", "charlie"]);
    }

    #[test]
    fn workspaces_for_project_filters() {
        let mut s = make_store();
        s.set_active("feat-1", "proj-a", 10, "/tmp", None, None);
        s.set_active("feat-2", "proj-b", 11, "/tmp", None, None);
        s.set_active("feat-3", "proj-a", 12, "/tmp", None, None);
        let result = s.workspaces_for_project("proj-a");
        let names: Vec<_> = result.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["feat-1", "feat-3"]);
    }

    #[test]
    fn workspaces_for_project_empty() {
        let s = make_store();
        assert!(s.workspaces_for_project("nope").is_empty());
    }

    #[test]
    fn allocate_tag_index_returns_existing() {
        let mut s = make_store();
        s.set_active("feat-1", "p", 15, "/tmp", None, None);
        assert_eq!(s.allocate_tag_index("feat-1"), 15);
    }

    #[test]
    fn allocate_tag_index_starts_at_offset() {
        let s = make_store();
        assert_eq!(s.allocate_tag_index("new"), TAG_OFFSET);
    }

    #[test]
    fn allocate_tag_index_skips_used() {
        let mut s = make_store();
        s.set_active("a", "p", TAG_OFFSET, "/tmp", None, None);
        s.set_active("b", "p", TAG_OFFSET + 1, "/tmp", None, None);
        assert_eq!(s.allocate_tag_index("c"), TAG_OFFSET + 2);
    }

    #[test]
    fn allocate_tag_index_ignores_inactive() {
        let mut s = make_store();
        s.set_active("a", "p", TAG_OFFSET, "/tmp", None, None);
        s.set_inactive("a");
        assert_eq!(s.allocate_tag_index("b"), TAG_OFFSET);
    }

    #[test]
    fn allocate_tag_index_zero_gets_new() {
        let mut s = make_store();
        s.workspaces.insert("ws".into(), WorkspaceState {
            project: "p".into(),
            tag_index: 0,
            ..Default::default()
        });
        assert_eq!(s.allocate_tag_index("ws"), TAG_OFFSET);
    }

    #[test]
    fn serialization_roundtrip() {
        let mut s = make_store();
        s.set_active("feat-1", "proj", 10, "/tmp/feat-1", Some(9100), None);
        let json = serde_json::to_string(&s).unwrap();
        let s2: Store = serde_json::from_str(&json).unwrap();
        let ws = s2.workspace("feat-1").unwrap();
        assert_eq!(ws.project, "proj");
        assert!(ws.active);
        assert_eq!(ws.tag_index, 10);
        assert_eq!(ws.acp_port, Some(9100));
    }

    #[test]
    fn deserialize_missing_fields() {
        let json = r#"{"workspaces":{"ws1":{"project":"p"}}}"#;
        let s: Store = serde_json::from_str(json).unwrap();
        let ws = s.workspace("ws1").unwrap();
        assert_eq!(ws.project, "p");
        assert!(!ws.active);
        assert_eq!(ws.tag_index, 0);
        assert!(ws.dir.is_empty());
        assert!(ws.acp_port.is_none());
    }

    #[test]
    fn allocate_acp_port_starts_at_base() {
        let s = make_store();
        assert_eq!(s.allocate_acp_port("new"), Some(ACP_PORT_BASE));
    }

    #[test]
    fn allocate_acp_port_returns_existing() {
        let mut s = make_store();
        s.set_active("feat-1", "p", 10, "/tmp", Some(9105), None);
        assert_eq!(s.allocate_acp_port("feat-1"), Some(9105));
    }

    #[test]
    fn allocate_acp_port_skips_used() {
        let mut s = make_store();
        s.set_active("a", "p", 10, "/tmp", Some(ACP_PORT_BASE), None);
        s.set_active("b", "p", 11, "/tmp", Some(ACP_PORT_BASE + 1), None);
        assert_eq!(s.allocate_acp_port("c"), Some(ACP_PORT_BASE + 2));
    }

    #[test]
    fn allocate_acp_port_ignores_inactive() {
        let mut s = make_store();
        s.set_active("a", "p", 10, "/tmp", Some(ACP_PORT_BASE), None);
        s.set_inactive("a");
        assert_eq!(s.allocate_acp_port("b"), Some(ACP_PORT_BASE));
    }

    #[test]
    fn workspace_name_for_route_finds_active() {
        let mut s = make_store();
        s.set_active("my-feature", "proj", 10, "/tmp", Some(9100), None);
        let (name, ws) = s.workspace_name_for_route("my-feature").unwrap();
        assert_eq!(name, "my-feature");
        assert_eq!(ws.acp_port, Some(9100));
    }

    #[test]
    fn workspace_name_for_route_skips_inactive() {
        let mut s = make_store();
        s.set_active("feat", "proj", 10, "/tmp", Some(9100), None);
        s.set_inactive("feat");
        assert!(s.workspace_name_for_route("feat").is_none());
    }

    #[test]
    fn workspace_by_acp_port_finds_match() {
        let mut s = make_store();
        s.set_active("feat-1", "proj", 10, "/tmp", Some(9101), None);
        let (name, _) = s.workspace_by_acp_port(9101).unwrap();
        assert_eq!(name, "feat-1");
    }

    #[test]
    fn workspace_by_acp_port_no_match() {
        let s = make_store();
        assert!(s.workspace_by_acp_port(9999).is_none());
    }

    #[test]
    fn acp_port_serialization_roundtrip() {
        let mut s = make_store();
        s.set_active("feat", "proj", 10, "/tmp", Some(9105), None);
        let json = serde_json::to_string(&s).unwrap();
        let s2: Store = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.workspace("feat").unwrap().acp_port, Some(9105));
    }

    #[test]
    fn acp_port_none_not_serialized() {
        let mut s = make_store();
        s.set_active("feat", "proj", 10, "/tmp", None, None);
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("acp_port"));
    }

    fn make_agent(id: &str, name: &str, ws: &str, port: u16) -> AgentInstanceState {
        AgentInstanceState {
            id: id.into(),
            template: "crush".into(),
            name: name.into(),
            workspace: ws.into(),
            status: AgentStatus::Ready,
            port,
            pid: Some(1234),
            started_at: "2026-04-28T10:00:00Z".into(),
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
    fn agent_status_default() {
        let status = AgentStatus::default();
        assert_eq!(status, AgentStatus::Stopped);
    }

    #[test]
    fn add_and_find_agent() {
        let mut s = make_store();
        s.set_active("ws1", "proj", 10, "/tmp", None, None);
        let agent = make_agent("agent-1", "coder", "ws1", 9100);
        s.add_agent("ws1", agent);
        let (ws_name, found) = s.find_agent("agent-1").unwrap();
        assert_eq!(ws_name, "ws1");
        assert_eq!(found.name, "coder");
        assert_eq!(found.port, 9100);
    }

    #[test]
    fn find_agent_not_found() {
        let s = make_store();
        assert!(s.find_agent("nonexistent").is_none());
    }

    #[test]
    fn remove_agent() {
        let mut s = make_store();
        s.set_active("ws1", "proj", 10, "/tmp", None, None);
        s.add_agent("ws1", make_agent("agent-1", "coder", "ws1", 9100));
        s.add_agent("ws1", make_agent("agent-2", "reviewer", "ws1", 9101));
        s.remove_agent("agent-1");
        assert!(s.find_agent("agent-1").is_none());
        assert!(s.find_agent("agent-2").is_some());
    }

    #[test]
    fn update_agent_status() {
        let mut s = make_store();
        s.set_active("ws1", "proj", 10, "/tmp", None, None);
        s.add_agent("ws1", make_agent("agent-1", "coder", "ws1", 9100));
        s.update_agent_status("agent-1", AgentStatus::Busy);
        let (_, agent) = s.find_agent("agent-1").unwrap();
        assert_eq!(agent.status, AgentStatus::Busy);
    }

    #[test]
    fn all_agents_across_workspaces() {
        let mut s = make_store();
        s.set_active("ws1", "proj", 10, "/tmp", None, None);
        s.set_active("ws2", "proj", 11, "/tmp2", None, None);
        s.add_agent("ws1", make_agent("agent-a", "coder", "ws1", 9100));
        s.add_agent("ws2", make_agent("agent-b", "reviewer", "ws2", 9101));
        let all = s.all_agents();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn allocate_agent_port_avoids_agent_ports() {
        let mut s = make_store();
        s.set_active("ws1", "proj", 10, "/tmp", Some(ACP_PORT_BASE), None);
        s.add_agent("ws1", make_agent("a1", "coder", "ws1", ACP_PORT_BASE + 1));
        let port = s.allocate_agent_port().unwrap();
        assert_eq!(port, ACP_PORT_BASE + 2);
    }

    #[test]
    fn allocate_agent_port_empty_store() {
        let s = make_store();
        assert_eq!(s.allocate_agent_port(), Some(ACP_PORT_BASE));
    }

    #[test]
    fn agents_empty_by_default() {
        let mut s = make_store();
        s.set_active("ws1", "proj", 10, "/tmp", None, None);
        let ws = s.workspace("ws1").unwrap();
        assert!(ws.agents.is_empty());
    }

    #[test]
    fn agents_serialization_roundtrip() {
        let mut s = make_store();
        s.set_active("ws1", "proj", 10, "/tmp", None, None);
        s.add_agent("ws1", make_agent("agent-1", "coder", "ws1", 9100));
        let json = serde_json::to_string(&s).unwrap();
        let s2: Store = serde_json::from_str(&json).unwrap();
        let ws = s2.workspace("ws1").unwrap();
        assert_eq!(ws.agents.len(), 1);
        assert_eq!(ws.agents[0].id, "agent-1");
        assert_eq!(ws.agents[0].status, AgentStatus::Ready);
    }

    #[test]
    fn agents_empty_not_serialized() {
        let mut s = make_store();
        s.set_active("ws1", "proj", 10, "/tmp", None, None);
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("agents"));
    }

    #[test]
    fn backward_compat_deserialize_no_agents() {
        let json = r#"{"workspaces":{"ws1":{"project":"p","active":true,"tag_index":10,"dir":"/tmp","acp_port":9100}}}"#;
        let s: Store = serde_json::from_str(json).unwrap();
        let ws = s.workspace("ws1").unwrap();
        assert!(ws.agents.is_empty());
        assert_eq!(ws.acp_port, Some(9100));
    }
}
