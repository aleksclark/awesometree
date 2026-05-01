use rusqlite::{Connection, params};
use std::collections::HashSet;
use std::sync::Mutex;

pub const TAG_OFFSET: i32 = 10;
pub const PORT_BASE: u16 = 9100;
pub const PORT_MAX: u16 = 9199;

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

#[derive(Debug, Clone)]
pub struct TaskRow {
    pub task_id: String,
    pub agent_id: String,
    pub context_id: Option<String>,
    pub status: String,
    pub created_at: String,
}

pub struct ArpStore {
    conn: Mutex<Connection>,
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS workspaces (
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
    .map_err(|e| e.to_string())
}

fn read_workspace(row: &rusqlite::Row) -> rusqlite::Result<WorkspaceRow> {
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
}

fn read_agent(row: &rusqlite::Row) -> rusqlite::Result<AgentRow> {
    Ok(AgentRow {
        id: row.get(0)?,
        workspace: row.get(1)?,
        template: row.get(2)?,
        name: row.get(3)?,
        status: row.get(4)?,
        port: row.get::<_, i32>(5)? as u16,
        host: row.get(6)?,
        pid: row.get::<_, Option<i32>>(7)?.map(|v| v as u32),
        started_at: row.get(8)?,
        token_id: row.get(9)?,
        session_id: row.get(10)?,
        spawned_by: row.get(11)?,
    })
}

fn read_task(row: &rusqlite::Row) -> rusqlite::Result<TaskRow> {
    Ok(TaskRow {
        task_id: row.get(0)?,
        agent_id: row.get(1)?,
        context_id: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
    })
}

impl ArpStore {
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }

    pub fn open(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        init_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn open_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        init_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn create_workspace(&self, name: &str, project: &str, dir: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute("INSERT INTO workspaces (name, project, dir, active) VALUES (?1, ?2, ?3, 1)", params![name, project, dir])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_workspace(&self, name: &str) -> Result<Option<WorkspaceRow>, String> {
        let c = self.conn();
        let mut stmt = c.prepare("SELECT name, project, active, tag_index, dir, acp_port, acp_url, acp_session_id, created_at FROM workspaces WHERE name = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![name], read_workspace).map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(ws)) => Ok(Some(ws)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    pub fn list_workspaces(&self, project: Option<&str>, active_only: bool) -> Result<Vec<WorkspaceRow>, String> {
        let mut sql = String::from("SELECT name, project, active, tag_index, dir, acp_port, acp_url, acp_session_id, created_at FROM workspaces WHERE 1=1");
        let mut pv: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(p) = project { sql.push_str(" AND project = ?"); pv.push(Box::new(p.to_string())); }
        if active_only { sql.push_str(" AND active = 1"); }
        sql.push_str(" ORDER BY name");
        let c = self.conn();
        let mut stmt = c.prepare(&sql).map_err(|e| e.to_string())?;
        let pr: Vec<&dyn rusqlite::types::ToSql> = pv.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(pr.as_slice(), read_workspace).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn activate_workspace(&self, name: &str, tag_index: i32, dir: &str, acp_port: Option<u16>) -> Result<(), String> {
        let c = self.conn();
        let port_val = acp_port.map(|p| p as i32);
        c.execute("UPDATE workspaces SET active = 1, tag_index = ?1, dir = ?2, acp_port = ?3 WHERE name = ?4", params![tag_index, dir, port_val, name])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn deactivate_workspace(&self, name: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute("UPDATE workspaces SET active = 0, tag_index = 0, dir = '', acp_port = NULL, acp_url = NULL, acp_session_id = NULL WHERE name = ?1", params![name])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn destroy_workspace(&self, name: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute("DELETE FROM workspaces WHERE name = ?1", params![name]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn allocate_tag_index(&self) -> Result<i32, String> {
        let c = self.conn();
        let mut stmt = c.prepare("SELECT tag_index FROM workspaces WHERE active = 1 AND tag_index >= ?1").map_err(|e| e.to_string())?;
        let used: HashSet<i32> = stmt.query_map(params![TAG_OFFSET], |row| row.get(0)).map_err(|e| e.to_string())?.filter_map(|r| r.ok()).collect();
        let mut idx = TAG_OFFSET;
        while used.contains(&idx) { idx += 1; }
        Ok(idx)
    }

    pub fn allocate_agent_port(&self) -> Result<u16, String> {
        let c = self.conn();
        let mut used: HashSet<u16> = HashSet::new();
        {
            let mut stmt = c.prepare("SELECT acp_port FROM workspaces WHERE active = 1 AND acp_port IS NOT NULL").map_err(|e| e.to_string())?;
            for row in stmt.query_map([], |row| row.get::<_, i32>(0)).map_err(|e| e.to_string())?.flatten() { used.insert(row as u16); }
        }
        {
            let mut stmt = c.prepare("SELECT port FROM agents").map_err(|e| e.to_string())?;
            for row in stmt.query_map([], |row| row.get::<_, i32>(0)).map_err(|e| e.to_string())?.flatten() { used.insert(row as u16); }
        }
        let mut port = PORT_BASE;
        while used.contains(&port) && port <= PORT_MAX { port += 1; }
        if port > PORT_MAX { Err("no available ports".into()) } else { Ok(port) }
    }

    pub fn add_agent(&self, agent: &AgentRow) -> Result<(), String> {
        let c = self.conn();
        c.execute(
            "INSERT INTO agents (id, workspace, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![agent.id, agent.workspace, agent.template, agent.name, agent.status, agent.port as i32, agent.host, agent.pid.map(|p| p as i32), agent.started_at, agent.token_id, agent.session_id, agent.spawned_by],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_agent(&self, agent_id: &str) -> Result<Option<AgentRow>, String> {
        let c = self.conn();
        let mut stmt = c.prepare("SELECT id, workspace, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by FROM agents WHERE id = ?1").map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![agent_id], read_agent).map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(a)) => Ok(Some(a)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    pub fn list_agents(&self, workspace: Option<&str>, status: Option<&str>, template: Option<&str>) -> Result<Vec<AgentRow>, String> {
        let mut sql = String::from("SELECT id, workspace, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by FROM agents WHERE 1=1");
        let mut pv: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(ws) = workspace { sql.push_str(" AND workspace = ?"); pv.push(Box::new(ws.to_string())); }
        if let Some(st) = status { sql.push_str(" AND status = ?"); pv.push(Box::new(st.to_string())); }
        if let Some(tmpl) = template { sql.push_str(" AND template = ?"); pv.push(Box::new(tmpl.to_string())); }
        sql.push_str(" ORDER BY id");
        let c = self.conn();
        let mut stmt = c.prepare(&sql).map_err(|e| e.to_string())?;
        let pr: Vec<&dyn rusqlite::types::ToSql> = pv.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(pr.as_slice(), read_agent).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn update_agent_status(&self, agent_id: &str, status: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute("UPDATE agents SET status = ?1 WHERE id = ?2", params![status, agent_id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_agent_pid(&self, agent_id: &str, pid: u32) -> Result<(), String> {
        let c = self.conn();
        c.execute("UPDATE agents SET pid = ?1 WHERE id = ?2", params![pid as i32, agent_id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remove_agent(&self, agent_id: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute("DELETE FROM agents WHERE id = ?1", params![agent_id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn resolve_agent(&self, identifier: &str) -> Result<Option<AgentRow>, String> {
        if let Some(agent) = self.get_agent(identifier)? {
            if agent.status != "stopped" && agent.status != "stopping" {
                return Ok(Some(agent));
            }
        }
        if let Some(slash_pos) = identifier.find('/') {
            let ws = &identifier[..slash_pos];
            let name = &identifier[slash_pos + 1..];
            if !ws.is_empty() && !name.is_empty() {
                if let Some(agent) = self.resolve_by_ws_name(ws, name)? {
                    return Ok(Some(agent));
                }
            }
        }
        self.resolve_by_name(identifier)
    }

    fn resolve_by_ws_name(&self, workspace: &str, name: &str) -> Result<Option<AgentRow>, String> {
        let c = self.conn();
        let mut stmt = c.prepare("SELECT id, workspace, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by FROM agents WHERE workspace = ?1 AND name = ?2 AND status NOT IN ('stopped', 'stopping') ORDER BY CASE WHEN status = 'ready' THEN 0 ELSE 1 END, id LIMIT 1").map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![workspace, name], read_agent).map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(a)) => Ok(Some(a)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    fn resolve_by_name(&self, name: &str) -> Result<Option<AgentRow>, String> {
        let c = self.conn();
        let mut stmt = c.prepare("SELECT id, workspace, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by FROM agents WHERE name = ?1 AND status NOT IN ('stopped', 'stopping') ORDER BY CASE WHEN status = 'ready' THEN 0 ELSE 1 END, id LIMIT 1").map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![name], read_agent).map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(a)) => Ok(Some(a)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    pub fn track_task(&self, agent_id: &str, task_id: &str, context_id: Option<&str>) -> Result<(), String> {
        let c = self.conn();
        c.execute("INSERT INTO agent_tasks (task_id, agent_id, context_id, status) VALUES (?1, ?2, ?3, 'working')", params![task_id, agent_id, context_id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn complete_task(&self, agent_id: &str, task_id: &str, terminal_status: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute("UPDATE agent_tasks SET status = ?1 WHERE agent_id = ?2 AND task_id = ?3", params![terminal_status, agent_id, task_id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn active_tasks(&self, agent_id: &str) -> Result<Vec<TaskRow>, String> {
        let c = self.conn();
        let mut stmt = c.prepare("SELECT task_id, agent_id, context_id, status, created_at FROM agent_tasks WHERE agent_id = ?1 AND status = 'working'").map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![agent_id], read_task).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn clear_agent_tasks(&self, agent_id: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute("DELETE FROM agent_tasks WHERE agent_id = ?1", params![agent_id]).map_err(|e| e.to_string())?;
        Ok(())
    }
}

static GLOBAL_ARP_STORE: std::sync::OnceLock<ArpStore> = std::sync::OnceLock::new();

impl ArpStore {
    pub fn init_global(path: &str) -> Result<(), String> {
        let store = ArpStore::open(path)?;
        GLOBAL_ARP_STORE
            .set(store)
            .map_err(|_| "ArpStore already initialized".to_string())
    }

    pub fn global() -> Option<&'static ArpStore> {
        GLOBAL_ARP_STORE.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(id: &str, ws: &str, name: &str, port: u16) -> AgentRow {
        AgentRow {
            id: id.into(),
            workspace: ws.into(),
            template: "crush".into(),
            name: name.into(),
            status: "ready".into(),
            port,
            host: None,
            pid: Some(1234),
            started_at: "2026-04-28T10:00:00Z".into(),
            token_id: None,
            session_id: None,
            spawned_by: None,
        }
    }

    fn make_agent_with_status(id: &str, ws: &str, name: &str, port: u16, status: &str) -> AgentRow {
        AgentRow {
            status: status.into(),
            ..make_agent(id, ws, name, port)
        }
    }

    // ---- Workspace CRUD ----

    #[test]
    fn create_and_get_workspace() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("feat-auth", "myapp", "/tmp/feat-auth").unwrap();
        let ws = store.get_workspace("feat-auth").unwrap().unwrap();
        assert_eq!(ws.name, "feat-auth");
        assert_eq!(ws.project, "myapp");
        assert_eq!(ws.dir, "/tmp/feat-auth");
        assert!(ws.active);
        assert!(!ws.created_at.is_empty());
    }

    #[test]
    fn get_workspace_not_found() {
        let store = ArpStore::open_memory().unwrap();
        assert!(store.get_workspace("nope").unwrap().is_none());
    }

    #[test]
    fn create_duplicate_workspace_errors() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        assert!(store.create_workspace("ws1", "proj", "/tmp").is_err());
    }

    #[test]
    fn activate_workspace() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.deactivate_workspace("ws1").unwrap();
        store.activate_workspace("ws1", 15, "/new/dir", Some(9105)).unwrap();
        let ws = store.get_workspace("ws1").unwrap().unwrap();
        assert!(ws.active);
        assert_eq!(ws.tag_index, 15);
        assert_eq!(ws.dir, "/new/dir");
        assert_eq!(ws.acp_port, Some(9105));
    }

    #[test]
    fn deactivate_workspace() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.deactivate_workspace("ws1").unwrap();
        let ws = store.get_workspace("ws1").unwrap().unwrap();
        assert!(!ws.active);
        assert_eq!(ws.tag_index, 0);
        assert!(ws.acp_port.is_none());
    }

    #[test]
    fn destroy_workspace() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.destroy_workspace("ws1").unwrap();
        assert!(store.get_workspace("ws1").unwrap().is_none());
    }

    // ---- Workspace filtering ----

    #[test]
    fn list_workspaces_all() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj-a", "/tmp/1").unwrap();
        store.create_workspace("ws2", "proj-b", "/tmp/2").unwrap();
        let all = store.list_workspaces(None, false).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn list_workspaces_by_project() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj-a", "/tmp/1").unwrap();
        store.create_workspace("ws2", "proj-b", "/tmp/2").unwrap();
        store.create_workspace("ws3", "proj-a", "/tmp/3").unwrap();
        let result = store.list_workspaces(Some("proj-a"), false).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|ws| ws.project == "proj-a"));
    }

    #[test]
    fn list_workspaces_active_only() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp/1").unwrap();
        store.create_workspace("ws2", "proj", "/tmp/2").unwrap();
        store.deactivate_workspace("ws2").unwrap();
        let active = store.list_workspaces(None, true).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "ws1");
    }

    // ---- Tag/port allocation ----

    #[test]
    fn allocate_tag_starts_at_offset() {
        let store = ArpStore::open_memory().unwrap();
        assert_eq!(store.allocate_tag_index().unwrap(), TAG_OFFSET);
    }

    #[test]
    fn allocate_tag_skips_used() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "p", "/tmp").unwrap();
        store.activate_workspace("ws1", TAG_OFFSET, "/tmp", None).unwrap();
        store.create_workspace("ws2", "p", "/tmp").unwrap();
        store.activate_workspace("ws2", TAG_OFFSET + 1, "/tmp", None).unwrap();
        assert_eq!(store.allocate_tag_index().unwrap(), TAG_OFFSET + 2);
    }

    #[test]
    fn allocate_tag_ignores_inactive() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "p", "/tmp").unwrap();
        store.activate_workspace("ws1", TAG_OFFSET, "/tmp", None).unwrap();
        store.deactivate_workspace("ws1").unwrap();
        assert_eq!(store.allocate_tag_index().unwrap(), TAG_OFFSET);
    }

    #[test]
    fn allocate_port_starts_at_base() {
        let store = ArpStore::open_memory().unwrap();
        assert_eq!(store.allocate_agent_port().unwrap(), PORT_BASE);
    }

    #[test]
    fn allocate_port_skips_agent_ports() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "p", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", PORT_BASE)).unwrap();
        assert_eq!(store.allocate_agent_port().unwrap(), PORT_BASE + 1);
    }

    #[test]
    fn allocate_port_skips_acp_ports() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "p", "/tmp").unwrap();
        store.activate_workspace("ws1", 10, "/tmp", Some(PORT_BASE)).unwrap();
        assert_eq!(store.allocate_agent_port().unwrap(), PORT_BASE + 1);
    }

    #[test]
    fn allocate_port_skips_both() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "p", "/tmp").unwrap();
        store.activate_workspace("ws1", 10, "/tmp", Some(PORT_BASE)).unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", PORT_BASE + 1)).unwrap();
        assert_eq!(store.allocate_agent_port().unwrap(), PORT_BASE + 2);
    }

    // ---- Agent CRUD ----

    #[test]
    fn add_and_get_agent() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        let agent = make_agent("agent-1", "ws1", "coder", 9100);
        store.add_agent(&agent).unwrap();
        let found = store.get_agent("agent-1").unwrap().unwrap();
        assert_eq!(found.id, "agent-1");
        assert_eq!(found.name, "coder");
        assert_eq!(found.workspace, "ws1");
        assert_eq!(found.port, 9100);
        assert_eq!(found.status, "ready");
        assert_eq!(found.pid, Some(1234));
    }

    #[test]
    fn get_agent_not_found() {
        let store = ArpStore::open_memory().unwrap();
        assert!(store.get_agent("nope").unwrap().is_none());
    }

    #[test]
    fn update_agent_status_works() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.update_agent_status("a1", "busy").unwrap();
        let found = store.get_agent("a1").unwrap().unwrap();
        assert_eq!(found.status, "busy");
    }

    #[test]
    fn update_agent_pid_works() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.update_agent_pid("a1", 5678).unwrap();
        let found = store.get_agent("a1").unwrap().unwrap();
        assert_eq!(found.pid, Some(5678));
    }

    #[test]
    fn remove_agent_works() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.add_agent(&make_agent("a2", "ws1", "reviewer", 9101)).unwrap();
        store.remove_agent("a1").unwrap();
        assert!(store.get_agent("a1").unwrap().is_none());
        assert!(store.get_agent("a2").unwrap().is_some());
    }

    #[test]
    fn agent_optional_fields() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        let agent = AgentRow {
            token_id: Some("tok-123".into()),
            session_id: Some("sess-456".into()),
            spawned_by: Some("tok-parent".into()),
            host: Some("10.0.0.5".into()),
            ..make_agent("a1", "ws1", "coder", 9100)
        };
        store.add_agent(&agent).unwrap();
        let found = store.get_agent("a1").unwrap().unwrap();
        assert_eq!(found.token_id, Some("tok-123".into()));
        assert_eq!(found.session_id, Some("sess-456".into()));
        assert_eq!(found.spawned_by, Some("tok-parent".into()));
        assert_eq!(found.host, Some("10.0.0.5".into()));
    }

    // ---- Agent filtering ----

    #[test]
    fn list_agents_by_workspace() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.create_workspace("ws2", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.add_agent(&make_agent("a2", "ws2", "reviewer", 9101)).unwrap();
        let result = store.list_agents(Some("ws1"), None, None).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "a1");
    }

    #[test]
    fn list_agents_by_status() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent_with_status("a1", "ws1", "coder", 9100, "ready")).unwrap();
        store.add_agent(&make_agent_with_status("a2", "ws1", "reviewer", 9101, "busy")).unwrap();
        let result = store.list_agents(None, Some("ready"), None).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "a1");
    }

    #[test]
    fn list_agents_by_template() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&AgentRow {
            template: "claude".into(),
            ..make_agent("a1", "ws1", "coder", 9100)
        }).unwrap();
        store.add_agent(&make_agent("a2", "ws1", "reviewer", 9101)).unwrap();
        let result = store.list_agents(None, None, Some("crush")).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "a2");
    }

    #[test]
    fn list_agents_combined_filters() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.create_workspace("ws2", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent_with_status("a1", "ws1", "coder", 9100, "ready")).unwrap();
        store.add_agent(&make_agent_with_status("a2", "ws1", "reviewer", 9101, "busy")).unwrap();
        store.add_agent(&make_agent_with_status("a3", "ws2", "coder", 9102, "ready")).unwrap();
        let result = store.list_agents(Some("ws1"), Some("ready"), None).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "a1");
    }

    // ---- Agent resolution ----

    #[test]
    fn resolve_by_id() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("agent-abc123", "ws1", "coder", 9100)).unwrap();
        let found = store.resolve_agent("agent-abc123").unwrap().unwrap();
        assert_eq!(found.id, "agent-abc123");
    }

    #[test]
    fn resolve_by_name() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("agent-abc123", "ws1", "coder", 9100)).unwrap();
        let found = store.resolve_agent("coder").unwrap().unwrap();
        assert_eq!(found.id, "agent-abc123");
    }

    #[test]
    fn resolve_by_ws_name() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.create_workspace("ws2", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.add_agent(&make_agent("a2", "ws2", "coder", 9101)).unwrap();
        let found = store.resolve_agent("ws2/coder").unwrap().unwrap();
        assert_eq!(found.id, "a2");
    }

    #[test]
    fn resolve_id_takes_priority() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("coder", "ws1", "other-name", 9100)).unwrap();
        store.add_agent(&make_agent("a2", "ws1", "coder", 9101)).unwrap();
        let found = store.resolve_agent("coder").unwrap().unwrap();
        assert_eq!(found.id, "coder");
    }

    #[test]
    fn resolve_skips_stopped() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent_with_status("a1", "ws1", "coder", 9100, "stopped")).unwrap();
        assert!(store.resolve_agent("a1").unwrap().is_none());
    }

    #[test]
    fn resolve_skips_stopping() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent_with_status("a1", "ws1", "coder", 9100, "stopping")).unwrap();
        assert!(store.resolve_agent("a1").unwrap().is_none());
    }

    #[test]
    fn resolve_prefers_ready_over_busy() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.create_workspace("ws2", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent_with_status("a1", "ws1", "coder", 9100, "busy")).unwrap();
        store.add_agent(&make_agent_with_status("a2", "ws2", "coder", 9101, "ready")).unwrap();
        let found = store.resolve_agent("coder").unwrap().unwrap();
        assert_eq!(found.id, "a2");
    }

    #[test]
    fn resolve_not_found() {
        let store = ArpStore::open_memory().unwrap();
        assert!(store.resolve_agent("nonexistent").unwrap().is_none());
    }

    // ---- Task tracking ----

    #[test]
    fn track_and_list_tasks() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.track_task("a1", "task-1", Some("ctx-1")).unwrap();
        store.track_task("a1", "task-2", None).unwrap();
        let tasks = store.active_tasks("a1").unwrap();
        assert_eq!(tasks.len(), 2);
        let t1 = tasks.iter().find(|t| t.task_id == "task-1").unwrap();
        assert_eq!(t1.context_id, Some("ctx-1".into()));
        assert_eq!(t1.status, "working");
    }

    #[test]
    fn complete_task_removes_from_active() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.track_task("a1", "task-1", None).unwrap();
        store.track_task("a1", "task-2", None).unwrap();
        store.complete_task("a1", "task-1", "completed").unwrap();
        let tasks = store.active_tasks("a1").unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, "task-2");
    }

    #[test]
    fn clear_agent_tasks_removes_all() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.track_task("a1", "task-1", None).unwrap();
        store.track_task("a1", "task-2", None).unwrap();
        store.clear_agent_tasks("a1").unwrap();
        assert!(store.active_tasks("a1").unwrap().is_empty());
    }

    #[test]
    fn tasks_isolated_between_agents() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.add_agent(&make_agent("a2", "ws1", "reviewer", 9101)).unwrap();
        store.track_task("a1", "task-1", None).unwrap();
        store.track_task("a2", "task-2", None).unwrap();
        assert_eq!(store.active_tasks("a1").unwrap().len(), 1);
        assert_eq!(store.active_tasks("a2").unwrap().len(), 1);
    }

    #[test]
    fn active_tasks_only_returns_working() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.track_task("a1", "task-1", None).unwrap();
        store.track_task("a1", "task-2", None).unwrap();
        store.complete_task("a1", "task-1", "completed").unwrap();
        store.complete_task("a1", "task-2", "failed").unwrap();
        assert!(store.active_tasks("a1").unwrap().is_empty());
    }

    // ---- Cascading deletes ----

    #[test]
    fn destroy_workspace_removes_agents() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.add_agent(&make_agent("a2", "ws1", "reviewer", 9101)).unwrap();
        store.destroy_workspace("ws1").unwrap();
        assert!(store.get_agent("a1").unwrap().is_none());
        assert!(store.get_agent("a2").unwrap().is_none());
    }

    #[test]
    fn remove_agent_removes_tasks() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.track_task("a1", "task-1", None).unwrap();
        store.remove_agent("a1").unwrap();
        assert!(store.active_tasks("a1").unwrap().is_empty());
    }

    #[test]
    fn destroy_workspace_cascades_to_tasks() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.track_task("a1", "task-1", None).unwrap();
        store.destroy_workspace("ws1").unwrap();
        assert!(store.active_tasks("a1").unwrap().is_empty());
    }

    #[test]
    fn destroy_workspace_does_not_affect_other_workspace() {
        let store = ArpStore::open_memory().unwrap();
        store.create_workspace("ws1", "proj", "/tmp").unwrap();
        store.create_workspace("ws2", "proj", "/tmp").unwrap();
        store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        store.add_agent(&make_agent("a2", "ws2", "reviewer", 9101)).unwrap();
        store.destroy_workspace("ws1").unwrap();
        assert!(store.get_agent("a1").unwrap().is_none());
        assert!(store.get_agent("a2").unwrap().is_some());
    }

    // ---- File persistence ----

    #[test]
    fn file_backed_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let path_str = path.to_str().unwrap();

        {
            let store = ArpStore::open(path_str).unwrap();
            store.create_workspace("ws1", "proj", "/tmp").unwrap();
            store.add_agent(&make_agent("a1", "ws1", "coder", 9100)).unwrap();
        }

        {
            let store = ArpStore::open(path_str).unwrap();
            assert!(store.get_workspace("ws1").unwrap().is_some());
            assert!(store.get_agent("a1").unwrap().is_some());
        }
    }
}
