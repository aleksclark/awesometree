#[allow(unused_imports)]
use rusqlite::{Connection, params};
use std::collections::HashSet;
use std::path::Path;

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
    conn: Connection,
}

impl ArpStore {
    fn init_schema(conn: &Connection) -> Result<(), String> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workspaces (
                name TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                active INTEGER NOT NULL DEFAULT 1,
                tag_index INTEGER NOT NULL DEFAULT 0,
                dir TEXT NOT NULL DEFAULT '',
                acp_port INTEGER,
                acp_url TEXT,
                acp_session_id TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                workspace TEXT NOT NULL REFERENCES workspaces(name) ON DELETE CASCADE,
                template TEXT NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'starting',
                port INTEGER NOT NULL,
                host TEXT,
                pid INTEGER,
                started_at TEXT NOT NULL,
                token_id TEXT,
                session_id TEXT,
                spawned_by TEXT
            );
            CREATE TABLE IF NOT EXISTS agent_tasks (
                task_id TEXT NOT NULL,
                agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                context_id TEXT,
                status TEXT NOT NULL DEFAULT 'working',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (task_id, agent_id)
            );",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Open (or create) a persistent SQLite database at the given path.
    pub fn open(path: &str) -> Result<Self, String> {
        let conn = Connection::open(Path::new(path)).map_err(|e| e.to_string())?;
        Self::init_schema(&conn)?;
        Ok(ArpStore { conn })
    }

    /// Open an in-memory SQLite database (for tests).
    pub fn open_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        Self::init_schema(&conn)?;
        Ok(ArpStore { conn })
    }

    // ── Workspace operations ──────────────────────────────────────────

    /// Insert a new workspace row.
    pub fn create_workspace(&self, name: &str, project: &str, dir: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO workspaces (name, project, active, tag_index, dir) VALUES (?1, ?2, 1, 0, ?3)",
                params![name, project, dir],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Retrieve a workspace by primary key.
    pub fn get_workspace(&self, name: &str) -> Result<Option<WorkspaceRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT name, project, active, tag_index, dir, acp_port, acp_url, acp_session_id, created_at \
                 FROM workspaces WHERE name = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![name], |row| {
                Ok(WorkspaceRow {
                    name: row.get(0)?,
                    project: row.get(1)?,
                    active: row.get::<_, i32>(2)? != 0,
                    tag_index: row.get(3)?,
                    dir: row.get(4)?,
                    acp_port: row.get::<_, Option<i32>>(5)?.map(|v| v as u16),
                    acp_url: row.get(6)?,
                    acp_session_id: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(ws)) => Ok(Some(ws)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    /// List workspaces, optionally filtering by project and/or active-only.
    pub fn list_workspaces(
        &self,
        project: Option<&str>,
        active_only: bool,
    ) -> Result<Vec<WorkspaceRow>, String> {
        let mut sql = String::from(
            "SELECT name, project, active, tag_index, dir, acp_port, acp_url, acp_session_id, created_at \
             FROM workspaces WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(p) = project {
            sql.push_str(" AND project = ?");
            param_values.push(Box::new(p.to_string()));
        }
        if active_only {
            sql.push_str(" AND active = 1");
        }
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(params_ref.as_slice(), |row| {
                Ok(WorkspaceRow {
                    name: row.get(0)?,
                    project: row.get(1)?,
                    active: row.get::<_, i32>(2)? != 0,
                    tag_index: row.get(3)?,
                    dir: row.get(4)?,
                    acp_port: row.get::<_, Option<i32>>(5)?.map(|v| v as u16),
                    acp_url: row.get(6)?,
                    acp_session_id: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Mark a workspace as active, setting its tag_index, dir, and optional acp_port.
    pub fn activate_workspace(
        &self,
        name: &str,
        tag_index: i32,
        dir: &str,
        acp_port: Option<u16>,
    ) -> Result<(), String> {
        let acp_port_val = acp_port.map(|p| p as i32);
        self.conn
            .execute(
                "UPDATE workspaces SET active = 1, tag_index = ?1, dir = ?2, acp_port = ?3 WHERE name = ?4",
                params![tag_index, dir, acp_port_val, name],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Mark a workspace as inactive, clearing runtime fields.
    pub fn deactivate_workspace(&self, name: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE workspaces SET active = 0, tag_index = 0, dir = '', acp_port = NULL, acp_url = NULL, acp_session_id = NULL WHERE name = ?1",
                params![name],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete a workspace and cascade-remove its agents (and their tasks).
    pub fn destroy_workspace(&self, name: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM workspaces WHERE name = ?1", params![name])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Allocate the next unused tag index (starting at TAG_OFFSET), skipping
    /// indices already used by active workspaces.
    pub fn allocate_tag_index(&self) -> Result<i32, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag_index FROM workspaces WHERE active = 1")
            .map_err(|e| e.to_string())?;
        let used: HashSet<i32> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        let mut idx = TAG_OFFSET;
        while used.contains(&idx) {
            idx += 1;
        }
        Ok(idx)
    }

    /// Allocate the next unused agent port (PORT_BASE..=PORT_MAX), skipping
    /// ports already claimed by agents or workspace acp_port values.
    pub fn allocate_agent_port(&self) -> Result<u16, String> {
        let mut used: HashSet<u16> = HashSet::new();

        // Collect acp_port from active workspaces
        let mut stmt = self
            .conn
            .prepare("SELECT acp_port FROM workspaces WHERE active = 1 AND acp_port IS NOT NULL")
            .map_err(|e| e.to_string())?;
        let acp_ports = stmt
            .query_map([], |row| row.get::<_, i32>(0))
            .map_err(|e| e.to_string())?;
        for port in acp_ports.flatten() {
            used.insert(port as u16);
        }

        // Collect port from all agents
        let mut stmt = self
            .conn
            .prepare("SELECT port FROM agents")
            .map_err(|e| e.to_string())?;
        let agent_ports = stmt
            .query_map([], |row| row.get::<_, i32>(0))
            .map_err(|e| e.to_string())?;
        for port in agent_ports.flatten() {
            used.insert(port as u16);
        }

        for port in PORT_BASE..=PORT_MAX {
            if !used.contains(&port) {
                return Ok(port);
            }
        }
        Err("no free ports available".to_string())
    }

    // ── Agent operations ──────────────────────────────────────────────

    /// Insert a new agent row.
    pub fn add_agent(&self, agent: AgentRow) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO agents (id, workspace, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    agent.id,
                    agent.workspace,
                    agent.template,
                    agent.name,
                    agent.status,
                    agent.port as i32,
                    agent.host,
                    agent.pid.map(|p| p as i64),
                    agent.started_at,
                    agent.token_id,
                    agent.session_id,
                    agent.spawned_by,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Retrieve an agent by its primary key (id).
    pub fn get_agent(&self, agent_id: &str) -> Result<Option<AgentRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, workspace, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by \
                 FROM agents WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![agent_id], |row| {
                Ok(AgentRow {
                    id: row.get(0)?,
                    workspace: row.get(1)?,
                    template: row.get(2)?,
                    name: row.get(3)?,
                    status: row.get(4)?,
                    port: row.get::<_, i32>(5)? as u16,
                    host: row.get(6)?,
                    pid: row.get::<_, Option<i64>>(7)?.map(|v| v as u32),
                    started_at: row.get(8)?,
                    token_id: row.get(9)?,
                    session_id: row.get(10)?,
                    spawned_by: row.get(11)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(agent)) => Ok(Some(agent)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    /// List agents with optional filters on workspace, status, and template.
    pub fn list_agents(
        &self,
        workspace: Option<&str>,
        status: Option<&str>,
        template: Option<&str>,
    ) -> Result<Vec<AgentRow>, String> {
        let mut sql = String::from(
            "SELECT id, workspace, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by \
             FROM agents WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(ws) = workspace {
            sql.push_str(" AND workspace = ?");
            param_values.push(Box::new(ws.to_string()));
        }
        if let Some(s) = status {
            sql.push_str(" AND status = ?");
            param_values.push(Box::new(s.to_string()));
        }
        if let Some(t) = template {
            sql.push_str(" AND template = ?");
            param_values.push(Box::new(t.to_string()));
        }
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(params_ref.as_slice(), |row| {
                Ok(AgentRow {
                    id: row.get(0)?,
                    workspace: row.get(1)?,
                    template: row.get(2)?,
                    name: row.get(3)?,
                    status: row.get(4)?,
                    port: row.get::<_, i32>(5)? as u16,
                    host: row.get(6)?,
                    pid: row.get::<_, Option<i64>>(7)?.map(|v| v as u32),
                    started_at: row.get(8)?,
                    token_id: row.get(9)?,
                    session_id: row.get(10)?,
                    spawned_by: row.get(11)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Update an agent's status field.
    pub fn update_agent_status(&self, agent_id: &str, status: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE agents SET status = ?1 WHERE id = ?2",
                params![status, agent_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Update an agent's pid field.
    pub fn update_agent_pid(&self, agent_id: &str, pid: u32) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE agents SET pid = ?1 WHERE id = ?2",
                params![pid as i64, agent_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete an agent row (and cascade-remove its tasks).
    pub fn remove_agent(&self, agent_id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM agents WHERE id = ?1", params![agent_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Flexible agent resolution following the ARP spec order:
    ///
    /// 1. By `agent_id` — exact match (skip stopped/stopping).
    /// 2. If identifier contains `/`, split into `workspace/name` and look up
    ///    in that workspace (skip stopped/stopping, prefer ready over busy).
    /// 3. By `name` across all workspaces (skip stopped/stopping, prefer ready
    ///    over busy).
    pub fn resolve_agent(&self, identifier: &str) -> Result<Option<AgentRow>, String> {
        // 1. Try by exact id, skip stopped/stopping
        if let Some(agent) = self.get_agent(identifier)? {
            if agent.status != "stopped" && agent.status != "stopping" {
                return Ok(Some(agent));
            }
        }

        // 2. If contains '/', split into workspace/name
        if identifier.contains('/') {
            let parts: Vec<&str> = identifier.splitn(2, '/').collect();
            let ws = parts[0];
            let name = parts[1];
            let agents = self.list_agents(Some(ws), None, None)?;
            let candidates: Vec<&AgentRow> = agents
                .iter()
                .filter(|a| a.name == name && a.status != "stopped" && a.status != "stopping")
                .collect();
            // Prefer ready over others
            if let Some(ready) = candidates.iter().find(|a| a.status == "ready") {
                return Ok(Some((*ready).clone()));
            }
            return Ok(candidates.first().map(|a| (*a).clone()));
        }

        // 3. By name across all agents
        let agents = self.list_agents(None, None, None)?;
        let candidates: Vec<&AgentRow> = agents
            .iter()
            .filter(|a| a.name == identifier && a.status != "stopped" && a.status != "stopping")
            .collect();
        // Prefer ready over others
        if let Some(ready) = candidates.iter().find(|a| a.status == "ready") {
            return Ok(Some((*ready).clone()));
        }
        Ok(candidates.first().map(|a| (*a).clone()))
    }

    // ── Task tracking ─────────────────────────────────────────────────

    /// Record a new task for an agent (status defaults to "working").
    pub fn track_task(
        &self,
        agent_id: &str,
        task_id: &str,
        context_id: Option<&str>,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO agent_tasks (task_id, agent_id, context_id, status) VALUES (?1, ?2, ?3, 'working')",
                params![task_id, agent_id, context_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Mark a task as completed with the given status string.
    pub fn complete_task(
        &self,
        agent_id: &str,
        task_id: &str,
        terminal_status: &str,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE agent_tasks SET status = ?1 WHERE agent_id = ?2 AND task_id = ?3",
                params![terminal_status, agent_id, task_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Return all tasks for an agent that are still in "working" status.
    pub fn active_tasks(&self, agent_id: &str) -> Result<Vec<TaskRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT task_id, agent_id, context_id, status, created_at \
                 FROM agent_tasks WHERE agent_id = ?1 AND status = 'working'",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![agent_id], |row| {
                Ok(TaskRow {
                    task_id: row.get(0)?,
                    agent_id: row.get(1)?,
                    context_id: row.get(2)?,
                    status: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Remove all tasks for an agent.
    pub fn clear_agent_tasks(&self, agent_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM agent_tasks WHERE agent_id = ?1",
                params![agent_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
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
    fn get_workspace_nonexistent() {
        let s = store();
        let ws = s.get_workspace("nope").unwrap();
        assert!(ws.is_none());
    }

    #[test]
    fn create_workspace_duplicate_errors() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        let result = s.create_workspace("ws1", "proj", "/tmp");
        assert!(result.is_err(), "duplicate workspace name should error");
    }

    #[test]
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
    fn deactivate_preserves_project() {
        let s = store();
        s.create_workspace("ws1", "myproject", "/tmp").unwrap();
        s.activate_workspace("ws1", 10, "/tmp/ws1", None).unwrap();
        s.deactivate_workspace("ws1").unwrap();
        let ws = s.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(ws.project, "myproject");
    }

    #[test]
    fn destroy_workspace_removes_row() {
        let s = store();
        s.create_workspace("ws1", "proj", "/tmp").unwrap();
        s.destroy_workspace("ws1").unwrap();
        assert!(s.get_workspace("ws1").unwrap().is_none());
    }

    // ── Workspace listing / filtering ─────────────────────────────────

    #[test]
    fn list_all_workspaces() {
        let s = store();
        s.create_workspace("alpha", "proj-a", "/tmp").unwrap();
        s.create_workspace("beta", "proj-b", "/tmp").unwrap();
        s.create_workspace("gamma", "proj-a", "/tmp").unwrap();
        let all = s.list_workspaces(None, false).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
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
    fn list_workspaces_empty() {
        let s = store();
        let ws = s.list_workspaces(None, false).unwrap();
        assert!(ws.is_empty());
    }

    // ── Tag index allocation ──────────────────────────────────────────

    #[test]
    fn allocate_tag_index_starts_at_offset() {
        let s = store();
        let idx = s.allocate_tag_index().unwrap();
        assert_eq!(idx, TAG_OFFSET);
    }

    #[test]
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
    fn allocate_tag_index_ignores_inactive() {
        let s = store();
        s.create_workspace("a", "p", "/tmp").unwrap();
        s.activate_workspace("a", TAG_OFFSET, "/d", None).unwrap();
        s.deactivate_workspace("a").unwrap();
        let idx = s.allocate_tag_index().unwrap();
        assert_eq!(idx, TAG_OFFSET);
    }

    #[test]
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
    fn allocate_agent_port_starts_at_base() {
        let s = store();
        let port = s.allocate_agent_port().unwrap();
        assert_eq!(port, PORT_BASE);
    }

    #[test]
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
    fn allocate_agent_port_skips_acp_ports() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.activate_workspace("ws1", 10, "/d", Some(PORT_BASE))
            .unwrap();
        let port = s.allocate_agent_port().unwrap();
        assert_eq!(port, PORT_BASE + 1);
    }

    #[test]
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
    fn get_agent_nonexistent() {
        let s = store();
        assert!(s.get_agent("nope").unwrap().is_none());
    }

    #[test]
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
    fn list_agents_empty() {
        let s = store();
        let agents = s.list_agents(None, None, None).unwrap();
        assert!(agents.is_empty());
    }

    // ── Agent resolution ──────────────────────────────────────────────

    #[test]
    fn resolve_agent_by_id() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("agent-abc123", "coder", "ws1", 9100))
            .unwrap();
        let found = s.resolve_agent("agent-abc123").unwrap().unwrap();
        assert_eq!(found.id, "agent-abc123");
    }

    #[test]
    fn resolve_agent_by_name() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("agent-abc123", "coder", "ws1", 9100))
            .unwrap();
        let found = s.resolve_agent("coder").unwrap().unwrap();
        assert_eq!(found.id, "agent-abc123");
    }

    #[test]
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
    fn resolve_agent_ws_name_not_found() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        assert!(s.resolve_agent("ws999/coder").unwrap().is_none());
    }

    #[test]
    fn resolve_agent_no_match() {
        let s = store();
        assert!(s.resolve_agent("nonexistent").unwrap().is_none());
    }

    #[test]
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
    fn active_tasks_empty_initially() {
        let s = store();
        s.create_workspace("ws1", "p", "/tmp").unwrap();
        s.add_agent(make_agent("a1", "coder", "ws1", 9100))
            .unwrap();
        let tasks = s.active_tasks("a1").unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
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
