use rusqlite::Connection;

/// Tag indices start at this value for new allocations.
pub const TAG_OFFSET: i32 = 10;
/// Agent ports are allocated starting from this value.
pub const PORT_BASE: u16 = 9100;
/// Maximum agent port number (inclusive).
pub const PORT_MAX: u16 = 9199;

/// A row from the `workspaces` table.
#[derive(Debug, Clone)]
pub struct WorkspaceRow {
    pub name: String,
    pub project: String,
    pub active: bool,
    pub tag_index: i32,
    pub dir: String,
    pub acp_port: Option<u16>,
    pub acp_url: Option<String>,
    pub acp_session_id: Option<String>,
    pub created_at: String,
}

/// A row from the `agents` table.
#[derive(Debug, Clone)]
pub struct AgentRow {
    pub id: String,
    pub workspace: String,
    pub template: String,
    pub name: String,
    pub status: String,
    pub port: u16,
    pub host: Option<String>,
    pub pid: Option<u32>,
    pub started_at: String,
    pub token_id: Option<String>,
    pub session_id: Option<String>,
    pub spawned_by: Option<String>,
}

/// A row from the `agent_tasks` table.
#[derive(Debug, Clone)]
pub struct TaskRow {
    pub task_id: String,
    pub agent_id: String,
    pub context_id: Option<String>,
    pub status: String,
    pub created_at: String,
}

/// SQLite-backed store for all ARP-related state: workspaces, agents, and tasks.
pub struct ArpStore {
    #[allow(dead_code)]
    conn: Connection,
}

impl ArpStore {
    /// Open (or create) a persistent SQLite database at the given path.
    pub fn open(_path: &str) -> Result<Self, String> {
        todo!()
    }

    /// Open an in-memory SQLite database (for tests).
    pub fn open_memory() -> Result<Self, String> {
        todo!()
    }

    // ── Workspace operations ──────────────────────────────────────────

    /// Insert a new workspace row.
    pub fn create_workspace(&self, _name: &str, _project: &str, _dir: &str) -> Result<(), String> {
        todo!()
    }

    /// Retrieve a workspace by primary key.
    pub fn get_workspace(&self, _name: &str) -> Result<Option<WorkspaceRow>, String> {
        todo!()
    }

    /// List workspaces, optionally filtering by project and/or active-only.
    pub fn list_workspaces(
        &self,
        _project: Option<&str>,
        _active_only: bool,
    ) -> Result<Vec<WorkspaceRow>, String> {
        todo!()
    }

    /// Mark a workspace as active, setting its tag_index, dir, and optional acp_port.
    pub fn activate_workspace(
        &self,
        _name: &str,
        _tag_index: i32,
        _dir: &str,
        _acp_port: Option<u16>,
    ) -> Result<(), String> {
        todo!()
    }

    /// Mark a workspace as inactive, clearing runtime fields.
    pub fn deactivate_workspace(&self, _name: &str) -> Result<(), String> {
        todo!()
    }

    /// Delete a workspace and cascade-remove its agents (and their tasks).
    pub fn destroy_workspace(&self, _name: &str) -> Result<(), String> {
        todo!()
    }

    /// Allocate the next unused tag index (starting at TAG_OFFSET), skipping
    /// indices already used by active workspaces.
    pub fn allocate_tag_index(&self) -> Result<i32, String> {
        todo!()
    }

    /// Allocate the next unused agent port (PORT_BASE..=PORT_MAX), skipping
    /// ports already claimed by agents or workspace acp_port values.
    pub fn allocate_agent_port(&self) -> Result<u16, String> {
        todo!()
    }

    // ── Agent operations ──────────────────────────────────────────────

    /// Insert a new agent row.
    pub fn add_agent(&self, _agent: AgentRow) -> Result<(), String> {
        todo!()
    }

    /// Retrieve an agent by its primary key (id).
    pub fn get_agent(&self, _agent_id: &str) -> Result<Option<AgentRow>, String> {
        todo!()
    }

    /// List agents with optional filters on workspace, status, and template.
    pub fn list_agents(
        &self,
        _workspace: Option<&str>,
        _status: Option<&str>,
        _template: Option<&str>,
    ) -> Result<Vec<AgentRow>, String> {
        todo!()
    }

    /// Update an agent's status field.
    pub fn update_agent_status(&self, _agent_id: &str, _status: &str) -> Result<(), String> {
        todo!()
    }

    /// Update an agent's pid field.
    pub fn update_agent_pid(&self, _agent_id: &str, _pid: u32) -> Result<(), String> {
        todo!()
    }

    /// Delete an agent row (and cascade-remove its tasks).
    pub fn remove_agent(&self, _agent_id: &str) -> Result<(), String> {
        todo!()
    }

    /// Flexible agent resolution following the ARP spec order:
    ///
    /// 1. By `agent_id` — exact match (skip stopped/stopping).
    /// 2. If identifier contains `/`, split into `workspace/name` and look up
    ///    in that workspace (skip stopped/stopping, prefer ready over busy).
    /// 3. By `name` across all workspaces (skip stopped/stopping, prefer ready
    ///    over busy).
    pub fn resolve_agent(&self, _identifier: &str) -> Result<Option<AgentRow>, String> {
        todo!()
    }

    // ── Task tracking ─────────────────────────────────────────────────

    /// Record a new task for an agent (status defaults to "working").
    pub fn track_task(
        &self,
        _agent_id: &str,
        _task_id: &str,
        _context_id: Option<&str>,
    ) -> Result<(), String> {
        todo!()
    }

    /// Mark a task as completed with the given status string.
    pub fn complete_task(
        &self,
        _agent_id: &str,
        _task_id: &str,
        _status: &str,
    ) -> Result<(), String> {
        todo!()
    }

    /// Return all tasks for an agent that are still in "working" status.
    pub fn active_tasks(&self, _agent_id: &str) -> Result<Vec<TaskRow>, String> {
        todo!()
    }

    /// Remove all tasks for an agent.
    pub fn clear_agent_tasks(&self, _agent_id: &str) -> Result<(), String> {
        todo!()
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: open an in-memory store for each test.
    fn store() -> ArpStore {
        ArpStore::open_memory().expect("open_memory should succeed")
    }

    /// Helper: build an `AgentRow` with sensible defaults.
    fn make_agent(id: &str, name: &str, workspace: &str, port: u16) -> AgentRow {
        AgentRow {
            id: id.into(),
            workspace: workspace.into(),
            template: "crush".into(),
            name: name.into(),
            status: "ready".into(),
            port,
            host: None,
            pid: Some(1234),
            started_at: "2026-06-01T12:00:00Z".into(),
            token_id: None,
            session_id: None,
            spawned_by: None,
        }
    }

    /// Helper: build an `AgentRow` with a specific status.
    fn make_agent_with_status(
        id: &str,
        name: &str,
        workspace: &str,
        port: u16,
        status: &str,
    ) -> AgentRow {
        AgentRow {
            status: status.into(),
            ..make_agent(id, name, workspace, port)
        }
    }

    // ── Workspace CRUD ────────────────────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn create_and_get_workspace() {
        let s = store();
        s.create_workspace("feat-1", "myproject", "/tmp/feat-1")
            .unwrap();
        let ws = s.get_workspace("feat-1").unwrap().expect("should exist");
        assert_eq!(ws.name, "feat-1");
        assert_eq!(ws.project, "myproject");
        assert_eq!(ws.dir, "/tmp/feat-1");
        assert!(ws.active); // newly created workspaces default to active=1
        assert_eq!(ws.tag_index, 0);
        assert!(ws.acp_port.is_none());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn get_workspace_nonexistent() {
        let s = store();
        let ws = s.get_workspace("nope").unwrap();
        assert!(ws.is_none());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn create_workspace_duplicate_errors() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        let result = s.create_workspace("ws1", "proj", "/tmp");
        assert!(result.is_err(), "duplicate workspace name should error");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn activate_workspace_sets_fields() {
        let s = store();
        s.create_workspace("ws1", "proj", "").unwrap();
        s.activate_workspace("ws1", 10, "/tmp/ws1", Some(9100))
            .unwrap();
        let ws = s.get_workspace("ws1").unwrap().unwrap();
        assert!(ws.active);
        assert_eq!(ws.tag_index, 10);
        assert_eq!(ws.dir, "/tmp/ws1");
        assert_eq!(ws.acp_port, Some(9100));
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn deactivate_workspace_clears_fields() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        s.activate_workspace("ws1", 10, "/tmp/ws1", Some(9100))
            .unwrap();
        s.deactivate_workspace("ws1").unwrap();
        let ws = s.get_workspace("ws1").unwrap().unwrap();
        assert!(!ws.active);
        assert_eq!(ws.tag_index, 0);
        assert!(ws.dir.is_empty());
        assert!(ws.acp_port.is_none());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn deactivate_preserves_project() {
        let s = store();
        s.create_workspace("ws1", "myproject", "/tmp").unwrap();
        s.activate_workspace("ws1", 10, "/tmp/ws1", None).unwrap();
        s.deactivate_workspace("ws1").unwrap();
        let ws = s.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(ws.project, "myproject");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn destroy_workspace_removes_row() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        s.destroy_workspace("ws1").unwrap();
        assert!(s.get_workspace("ws1").unwrap().is_none());
    }

    // ── Workspace listing / filtering ─────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_all_workspaces() {
        let s = store();
        s.create_workspace("alpha", "proj-a", "/tmp").unwrap();
        s.create_workspace("beta", "proj-b", "/tmp").unwrap();
        s.create_workspace("gamma", "proj-a", "/tmp").unwrap();
        let all = s.list_workspaces(None, false).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_workspaces_by_project() {
        let s = store();
        s.create_workspace("alpha", "proj-a", "/tmp").unwrap();
        s.create_workspace("beta", "proj-b", "/tmp").unwrap();
        s.create_workspace("gamma", "proj-a", "/tmp").unwrap();
        let filtered = s.list_workspaces(Some("proj-a"), false).unwrap();
        assert_eq!(filtered.len(), 2);
        let names: Vec<&str> = filtered.iter().map(|w| w.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"gamma"));
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_workspaces_active_only() {
        let s = store();
        s.create_workspace("active1", "proj", "/tmp").unwrap();
        s.activate_workspace("active1", 10, "/tmp/a", None).unwrap();
        s.create_workspace("inactive1", "proj", "/tmp").unwrap();
        s.deactivate_workspace("inactive1").unwrap();
        let active = s.list_workspaces(None, true).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "active1");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_workspaces_project_and_active() {
        let s = store();
        s.create_workspace("a", "proj-x", "/tmp").unwrap();
        s.activate_workspace("a", 10, "/d", None).unwrap();
        s.create_workspace("b", "proj-x", "/tmp").unwrap();
        s.deactivate_workspace("b").unwrap();
        s.create_workspace("c", "proj-y", "/tmp").unwrap();
        s.activate_workspace("c", 11, "/d", None).unwrap();
        let result = s.list_workspaces(Some("proj-x"), true).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "a");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_workspaces_empty() {
        let s = store();
        let ws = s.list_workspaces(None, false).unwrap();
        assert!(ws.is_empty());
    }

    // ── Tag index allocation ──────────────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn allocate_tag_index_starts_at_offset() {
        let s = store();
        let idx = s.allocate_tag_index().unwrap();
        assert_eq!(idx, TAG_OFFSET);
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn allocate_tag_index_skips_used() {
        let s = store();
        s.create_workspace("a", "p", "/tmp").unwrap();
        s.activate_workspace("a", TAG_OFFSET, "/d", None).unwrap();
        s.create_workspace("b", "p", "/tmp").unwrap();
        s.activate_workspace("b", TAG_OFFSET + 1, "/d", None)
            .unwrap();
        let idx = s.allocate_tag_index().unwrap();
        assert_eq!(idx, TAG_OFFSET + 2);
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn allocate_tag_index_ignores_inactive() {
        let s = store();
        s.create_workspace("a", "p", "/tmp").unwrap();
        s.activate_workspace("a", TAG_OFFSET, "/d", None).unwrap();
        s.deactivate_workspace("a").unwrap();
        let idx = s.allocate_tag_index().unwrap();
        assert_eq!(idx, TAG_OFFSET);
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn allocate_tag_index_fills_gap() {
        let s = store();
        // Use TAG_OFFSET and TAG_OFFSET+2, leaving TAG_OFFSET+1 as a gap
        s.create_workspace("a", "p", "/tmp").unwrap();
        s.activate_workspace("a", TAG_OFFSET, "/d", None).unwrap();
        s.create_workspace("c", "p", "/tmp").unwrap();
        s.activate_workspace("c", TAG_OFFSET + 2, "/d", None)
            .unwrap();
        let idx = s.allocate_tag_index().unwrap();
        assert_eq!(idx, TAG_OFFSET + 1);
    }

    // ── Agent port allocation ─────────────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn allocate_agent_port_starts_at_base() {
        let s = store();
        let port = s.allocate_agent_port().unwrap();
        assert_eq!(port, PORT_BASE);
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn allocate_agent_port_skips_agent_ports() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.activate_workspace("ws1", 10, "/d", None).unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", PORT_BASE))
            .unwrap();
        let port = s.allocate_agent_port().unwrap();
        assert_eq!(port, PORT_BASE + 1);
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn allocate_agent_port_skips_acp_ports() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.activate_workspace("ws1", 10, "/d", Some(PORT_BASE))
            .unwrap();
        let port = s.allocate_agent_port().unwrap();
        assert_eq!(port, PORT_BASE + 1);
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn allocate_agent_port_skips_both() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.activate_workspace("ws1", 10, "/d", Some(PORT_BASE))
            .unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", PORT_BASE + 1))
            .unwrap();
        let port = s.allocate_agent_port().unwrap();
        assert_eq!(port, PORT_BASE + 2);
    }

    // ── Agent CRUD ────────────────────────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn add_and_get_agent() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        let agent = make_agent("agent-1", "coder", "ws1", 9100);
        s.add_agent(agent).unwrap();
        let found = s.get_agent("agent-1").unwrap().expect("should exist");
        assert_eq!(found.id, "agent-1");
        assert_eq!(found.name, "coder");
        assert_eq!(found.workspace, "ws1");
        assert_eq!(found.template, "crush");
        assert_eq!(found.status, "ready");
        assert_eq!(found.port, 9100);
        assert_eq!(found.pid, Some(1234));
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn get_agent_nonexistent() {
        let s = store();
        assert!(s.get_agent("nope").unwrap().is_none());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn add_agent_with_optional_fields() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        let agent = AgentRow {
            id: "a-full".into(),
            workspace: "ws1".into(),
            template: "reviewer".into(),
            name: "my-reviewer".into(),
            status: "starting".into(),
            port: 9105,
            host: Some("10.0.0.1".into()),
            pid: None,
            started_at: "2026-06-01T12:00:00Z".into(),
            token_id: Some("tok-abc".into()),
            session_id: Some("sess-xyz".into()),
            spawned_by: Some("user-1".into()),
        };
        s.add_agent(agent).unwrap();
        let found = s.get_agent("a-full").unwrap().unwrap();
        assert_eq!(found.host, Some("10.0.0.1".into()));
        assert_eq!(found.pid, None);
        assert_eq!(found.token_id, Some("tok-abc".into()));
        assert_eq!(found.session_id, Some("sess-xyz".into()));
        assert_eq!(found.spawned_by, Some("user-1".into()));
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn update_agent_status() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.update_agent_status("a1", "busy").unwrap();
        let agent = s.get_agent("a1").unwrap().unwrap();
        assert_eq!(agent.status, "busy");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn update_agent_pid() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.update_agent_pid("a1", 42).unwrap();
        let agent = s.get_agent("a1").unwrap().unwrap();
        assert_eq!(agent.pid, Some(42));
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn remove_agent() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.add_agent(make_agent("a2", "reviewer", "ws1", 9101))
            .unwrap();
        s.remove_agent("a1").unwrap();
        assert!(s.get_agent("a1").unwrap().is_none());
        assert!(s.get_agent("a2").unwrap().is_some());
    }

    // ── Agent listing / filtering ─────────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_all_agents() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.create_workspace("ws2", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.add_agent(make_agent("a2", "reviewer", "ws2", 9101))
            .unwrap();
        let all = s.list_agents(None, None, None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_agents_by_workspace() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.create_workspace("ws2", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.add_agent(make_agent("a2", "reviewer", "ws2", 9101))
            .unwrap();
        let agents = s.list_agents(Some("ws1"), None, None).unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "a1");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_agents_by_status() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent_with_status("a1", "coder", "ws1", 9100, "ready"))
            .unwrap();
        s.add_agent(make_agent_with_status("a2", "reviewer", "ws1", 9101, "busy"))
            .unwrap();
        s.add_agent(make_agent_with_status("a3", "helper", "ws1", 9102, "stopped"))
            .unwrap();
        let ready = s.list_agents(None, Some("ready"), None).unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "a1");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_agents_by_template() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(AgentRow {
            template: "reviewer".into(),
            ..make_agent("a1", "my-reviewer", "ws1", 9100)
        })
        .unwrap();
        s.add_agent(make_agent("a2", "coder", "ws1", 9101))
            .unwrap(); // template = "crush"
        let reviewers = s.list_agents(None, None, Some("reviewer")).unwrap();
        assert_eq!(reviewers.len(), 1);
        assert_eq!(reviewers[0].id, "a1");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_agents_combined_filters() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.create_workspace("ws2", "p", "/tmp").unwrap();
        s.add_agent(make_agent_with_status("a1", "coder", "ws1", 9100, "ready"))
            .unwrap();
        s.add_agent(make_agent_with_status("a2", "coder", "ws1", 9101, "busy"))
            .unwrap();
        s.add_agent(make_agent_with_status("a3", "coder", "ws2", 9102, "ready"))
            .unwrap();
        let result = s
            .list_agents(Some("ws1"), Some("ready"), Some("crush"))
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "a1");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn list_agents_empty() {
        let s = store();
        let agents = s.list_agents(None, None, None).unwrap();
        assert!(agents.is_empty());
    }

    // ── Agent resolution ──────────────────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_by_id() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("agent-abc123", "coder", "ws1", 9100))
            .unwrap();
        let found = s.resolve_agent("agent-abc123").unwrap().unwrap();
        assert_eq!(found.id, "agent-abc123");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_by_name() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("agent-abc123", "coder", "ws1", 9100))
            .unwrap();
        let found = s.resolve_agent("coder").unwrap().unwrap();
        assert_eq!(found.id, "agent-abc123");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_by_ws_slash_name() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.create_workspace("ws2", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.add_agent(make_agent("a2", "coder", "ws2", 9101))
            .unwrap();
        let found = s.resolve_agent("ws2/coder").unwrap().unwrap();
        assert_eq!(found.id, "a2");
        assert_eq!(found.workspace, "ws2");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_id_takes_priority_over_name() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        // Agent whose id happens to equal another agent's name
        s.add_agent(make_agent("coder", "some-other-name", "ws1", 9100))
            .unwrap();
        s.add_agent(make_agent("a2", "coder", "ws1", 9101))
            .unwrap();
        let found = s.resolve_agent("coder").unwrap().unwrap();
        assert_eq!(found.id, "coder");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_skips_stopped() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent_with_status(
            "a1", "coder", "ws1", 9100, "stopped",
        ))
        .unwrap();
        assert!(s.resolve_agent("a1").unwrap().is_none());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_skips_stopping() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent_with_status(
            "a1", "coder", "ws1", 9100, "stopping",
        ))
        .unwrap();
        assert!(s.resolve_agent("a1").unwrap().is_none());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_prefers_ready_over_busy() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.create_workspace("ws2", "p", "/tmp").unwrap();
        s.add_agent(make_agent_with_status(
            "busy-1", "coder", "ws1", 9100, "busy",
        ))
        .unwrap();
        s.add_agent(make_agent_with_status(
            "ready-1", "coder", "ws2", 9101, "ready",
        ))
        .unwrap();
        let found = s.resolve_agent("coder").unwrap().unwrap();
        assert_eq!(found.id, "ready-1");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_stopped_id_falls_through_to_name() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.create_workspace("ws2", "p", "/tmp").unwrap();
        // Agent with matching id is stopped
        s.add_agent(make_agent_with_status(
            "coder",
            "some-name",
            "ws1",
            9100,
            "stopped",
        ))
        .unwrap();
        // Different agent with matching name is ready
        s.add_agent(make_agent_with_status(
            "a2", "coder", "ws2", 9101, "ready",
        ))
        .unwrap();
        let found = s.resolve_agent("coder").unwrap().unwrap();
        assert_eq!(found.id, "a2");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_ws_name_takes_priority_over_bare_name() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.create_workspace("ws2", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.add_agent(make_agent("a2", "coder", "ws2", 9101))
            .unwrap();
        let found = s.resolve_agent("ws1/coder").unwrap().unwrap();
        assert_eq!(found.id, "a1");
        assert_eq!(found.workspace, "ws1");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_ws_name_not_found() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        assert!(s.resolve_agent("ws999/coder").unwrap().is_none());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_no_match() {
        let s = store();
        assert!(s.resolve_agent("nonexistent").unwrap().is_none());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn resolve_agent_ws_name_skips_stopped_prefers_ready() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent_with_status(
            "stopped-1",
            "coder",
            "ws1",
            9100,
            "stopped",
        ))
        .unwrap();
        s.add_agent(make_agent_with_status(
            "busy-1", "coder", "ws1", 9101, "busy",
        ))
        .unwrap();
        s.add_agent(make_agent_with_status(
            "ready-1", "coder", "ws1", 9102, "ready",
        ))
        .unwrap();
        let found = s.resolve_agent("ws1/coder").unwrap().unwrap();
        assert_eq!(found.id, "ready-1");
    }

    // ── Task tracking ─────────────────────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn track_and_list_active_tasks() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "task-1", None).unwrap();
        s.track_task("a1", "task-2", Some("ctx-abc")).unwrap();
        let tasks = s.active_tasks("a1").unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().all(|t| t.status == "working"));
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn track_task_with_context() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "task-1", Some("ctx-123")).unwrap();
        let tasks = s.active_tasks("a1").unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].context_id, Some("ctx-123".into()));
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn complete_task_removes_from_active() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "task-1", None).unwrap();
        s.track_task("a1", "task-2", None).unwrap();
        s.complete_task("a1", "task-1", "completed").unwrap();
        let active = s.active_tasks("a1").unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].task_id, "task-2");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn complete_task_sets_status() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "task-1", None).unwrap();
        s.complete_task("a1", "task-1", "failed").unwrap();
        // Active tasks should not contain completed/failed tasks
        let active = s.active_tasks("a1").unwrap();
        assert!(active.is_empty());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn active_tasks_empty_initially() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        let tasks = s.active_tasks("a1").unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn clear_agent_tasks() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "t1", None).unwrap();
        s.track_task("a1", "t2", None).unwrap();
        s.clear_agent_tasks("a1").unwrap();
        let tasks = s.active_tasks("a1").unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn clear_tasks_only_affects_target_agent() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.add_agent(make_agent("a2", "reviewer", "ws1", 9101))
            .unwrap();
        s.track_task("a1", "t1", None).unwrap();
        s.track_task("a2", "t2", None).unwrap();
        s.clear_agent_tasks("a1").unwrap();
        assert!(s.active_tasks("a1").unwrap().is_empty());
        assert_eq!(s.active_tasks("a2").unwrap().len(), 1);
    }

    // ── Task + agent status integration ───────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn active_tasks_only_returns_working() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "t1", None).unwrap();
        s.track_task("a1", "t2", None).unwrap();
        s.track_task("a1", "t3", None).unwrap();
        s.complete_task("a1", "t1", "completed").unwrap();
        s.complete_task("a1", "t2", "failed").unwrap();
        let active = s.active_tasks("a1").unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].task_id, "t3");
        assert_eq!(active[0].status, "working");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn task_rows_have_correct_agent_id() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "t1", None).unwrap();
        let tasks = s.active_tasks("a1").unwrap();
        assert_eq!(tasks[0].agent_id, "a1");
    }

    // ── Cascading deletes ─────────────────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn remove_agent_cascades_to_tasks() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "t1", None).unwrap();
        s.track_task("a1", "t2", None).unwrap();
        s.remove_agent("a1").unwrap();
        // Tasks should be gone too — active_tasks on a missing agent should be empty
        let tasks = s.active_tasks("a1").unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn destroy_workspace_cascades_to_agents() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.add_agent(make_agent("a2", "reviewer", "ws1", 9101))
            .unwrap();
        s.destroy_workspace("ws1").unwrap();
        assert!(s.get_agent("a1").unwrap().is_none());
        assert!(s.get_agent("a2").unwrap().is_none());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn destroy_workspace_cascades_to_tasks() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "t1", None).unwrap();
        s.destroy_workspace("ws1").unwrap();
        let tasks = s.active_tasks("a1").unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn destroy_workspace_does_not_affect_other_workspaces() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.create_workspace("ws2", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.add_agent(make_agent("a2", "reviewer", "ws2", 9101))
            .unwrap();
        s.track_task("a2", "t1", None).unwrap();
        s.destroy_workspace("ws1").unwrap();
        assert!(s.get_workspace("ws2").unwrap().is_some());
        assert!(s.get_agent("a2").unwrap().is_some());
        assert_eq!(s.active_tasks("a2").unwrap().len(), 1);
    }

    // ── Edge cases ────────────────────────────────────────────────────

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn open_file_backed_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let path_str = path.to_str().unwrap();
        let s = ArpStore::open(path_str).unwrap();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        drop(s);
        // Reopen and verify persistence
        let s2 = ArpStore::open(path_str).unwrap();
        let ws = s2.get_workspace("ws1").unwrap().expect("should persist");
        assert_eq!(ws.project, "proj");
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn workspace_created_at_is_populated() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        let ws = s.get_workspace("ws1").unwrap().unwrap();
        assert!(
            !ws.created_at.is_empty(),
            "created_at should be auto-populated"
        );
    }

    #[test]
    #[ignore = "awaiting ArpStore implementation"]
    fn task_created_at_is_populated() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        s.track_task("a1", "t1", None).unwrap();
        let tasks = s.active_tasks("a1").unwrap();
        assert!(
            !tasks[0].created_at.is_empty(),
            "task created_at should be auto-populated"
        );
    }
}
