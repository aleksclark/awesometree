//! SQLite agent/task runtime tables keyed by work_session_id.
//!
//! Does NOT store Project, WorkProfile, or WorkSession authority.

use rusqlite::{params, Connection};
use std::sync::Mutex;

pub const PORT_BASE: u16 = 9100;
pub const PORT_MAX: u16 = 9199;

#[derive(Debug, Clone)]
pub struct AgentRow {
    pub id: String,
    pub work_session_id: String,
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

        CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            work_session_id TEXT NOT NULL,
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

fn read_agent(row: &rusqlite::Row) -> rusqlite::Result<AgentRow> {
    Ok(AgentRow {
        id: row.get(0)?,
        work_session_id: row.get(1)?,
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
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn add_agent(&self, agent: &AgentRow) -> Result<(), String> {
        let c = self.conn();
        c.execute(
            "INSERT INTO agents (id, work_session_id, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                agent.id,
                agent.work_session_id,
                agent.template,
                agent.name,
                agent.status,
                agent.port as i32,
                agent.host,
                agent.pid.map(|p| p as i32),
                agent.started_at,
                agent.token_id,
                agent.session_id,
                agent.spawned_by
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_agent(&self, agent_id: &str) -> Result<Option<AgentRow>, String> {
        let c = self.conn();
        let mut stmt = c
            .prepare("SELECT id, work_session_id, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by FROM agents WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![agent_id], read_agent)
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn list_agents(
        &self,
        work_session_id: Option<&str>,
        status: Option<&str>,
        template: Option<&str>,
    ) -> Result<Vec<AgentRow>, String> {
        let c = self.conn();
        let mut sql = String::from(
            "SELECT id, work_session_id, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by FROM agents WHERE 1=1",
        );
        let mut pv: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(ws) = work_session_id {
            sql.push_str(" AND work_session_id = ?");
            pv.push(Box::new(ws.to_string()));
        }
        if let Some(st) = status {
            sql.push_str(" AND status = ?");
            pv.push(Box::new(st.to_string()));
        }
        if let Some(tp) = template {
            sql.push_str(" AND template = ?");
            pv.push(Box::new(tp.to_string()));
        }
        let mut stmt = c.prepare(&sql).map_err(|e| e.to_string())?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> = pv.iter().map(|b| b.as_ref()).collect();
        let rows = stmt
            .query_map(params_ref.as_slice(), read_agent)
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn update_agent_status(&self, agent_id: &str, status: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute(
            "UPDATE agents SET status = ?1 WHERE id = ?2",
            params![status, agent_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remove_agent(&self, agent_id: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute("DELETE FROM agents WHERE id = ?1", params![agent_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn resolve_agent(&self, identifier: &str) -> Result<Option<AgentRow>, String> {
        if let Some(a) = self.get_agent(identifier)? {
            if a.status != "stopped" && a.status != "stopping" {
                return Ok(Some(a));
            }
        }
        self.resolve_by_name(identifier)
    }

    fn resolve_by_name(&self, name: &str) -> Result<Option<AgentRow>, String> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, work_session_id, template, name, status, port, host, pid, started_at, token_id, session_id, spawned_by
             FROM agents WHERE name = ?1 AND status NOT IN ('stopped', 'stopping')
             ORDER BY CASE WHEN status = 'ready' THEN 0 ELSE 1 END, id LIMIT 1",
        )
        .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![name], read_agent)
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn track_task(
        &self,
        agent_id: &str,
        task_id: &str,
        context_id: Option<&str>,
    ) -> Result<(), String> {
        let c = self.conn();
        c.execute(
            "INSERT OR REPLACE INTO agent_tasks (task_id, agent_id, context_id, status) VALUES (?1, ?2, ?3, 'working')",
            params![task_id, agent_id, context_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn complete_task(
        &self,
        agent_id: &str,
        task_id: &str,
        terminal_status: &str,
    ) -> Result<(), String> {
        let c = self.conn();
        c.execute(
            "UPDATE agent_tasks SET status = ?1 WHERE agent_id = ?2 AND task_id = ?3",
            params![terminal_status, agent_id, task_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear_agent_tasks(&self, agent_id: &str) -> Result<(), String> {
        let c = self.conn();
        c.execute("DELETE FROM agent_tasks WHERE agent_id = ?1", params![agent_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_tasks(&self, agent_id: &str) -> Result<Vec<TaskRow>, String> {
        let c = self.conn();
        let mut stmt = c
            .prepare(
                "SELECT task_id, agent_id, context_id, status, created_at FROM agent_tasks WHERE agent_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![agent_id], read_task)
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn allocate_agent_port_with<F>(&self, is_free: F) -> Result<Option<u16>, String>
    where
        F: Fn(u16) -> bool,
    {
        let c = self.conn();
        let mut stmt = c
            .prepare("SELECT port FROM agents WHERE status NOT IN ('stopped')")
            .map_err(|e| e.to_string())?;
        let used: std::collections::HashSet<u16> = stmt
            .query_map([], |row| row.get::<_, i32>(0).map(|p| p as u16))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        for port in PORT_BASE..=PORT_MAX {
            if !used.contains(&port) && is_free(port) {
                return Ok(Some(port));
            }
        }
        Ok(None)
    }

    pub fn allocate_agent_port(&self) -> Result<Option<u16>, String> {
        self.allocate_agent_port_with(crate::state::is_port_available)
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
            work_session_id: ws.into(),
            template: "crush".into(),
            name: name.into(),
            status: "ready".into(),
            port,
            host: None,
            pid: None,
            started_at: "now".into(),
            token_id: None,
            session_id: None,
            spawned_by: None,
        }
    }

    fn make_agent_with_status(id: &str, ws: &str, name: &str, port: u16, status: &str) -> AgentRow {
        let mut a = make_agent(id, ws, name, port);
        a.status = status.into();
        a
    }

    #[test]
    fn add_and_get_agent() {
        let store = ArpStore::open_memory().unwrap();
        store.add_agent(&make_agent("a1", "ws1", "bot", 9100)).unwrap();
        let got = store.get_agent("a1").unwrap().unwrap();
        assert_eq!(got.work_session_id, "ws1");
        assert_eq!(got.name, "bot");
    }

    #[test]
    fn list_agents_filters() {
        let store = ArpStore::open_memory().unwrap();
        store.add_agent(&make_agent("a1", "ws1", "bot", 9100)).unwrap();
        store.add_agent(&make_agent("a2", "ws2", "bot", 9101)).unwrap();
        assert_eq!(store.list_agents(Some("ws1"), None, None).unwrap().len(), 1);
        assert_eq!(store.list_agents(None, Some("ready"), None).unwrap().len(), 2);
    }

    #[test]
    fn resolve_prefers_ready_over_busy() {
        let store = ArpStore::open_memory().unwrap();
        store
            .add_agent(&make_agent_with_status("a1", "ws1", "bot", 9100, "busy"))
            .unwrap();
        store
            .add_agent(&make_agent_with_status("a2", "ws2", "bot", 9101, "ready"))
            .unwrap();
        let found = store.resolve_agent("bot").unwrap().unwrap();
        assert_eq!(found.id, "a2");
    }

    #[test]
    fn resolve_skips_stopped() {
        let store = ArpStore::open_memory().unwrap();
        store
            .add_agent(&make_agent_with_status("a1", "ws1", "bot", 9100, "stopped"))
            .unwrap();
        assert!(store.resolve_agent("bot").unwrap().is_none());
    }

    #[test]
    fn resolve_skips_stopping() {
        let store = ArpStore::open_memory().unwrap();
        store
            .add_agent(&make_agent_with_status("a1", "ws1", "bot", 9100, "stopping"))
            .unwrap();
        assert!(store.resolve_agent("bot").unwrap().is_none());
    }

    #[test]
    fn track_and_list_tasks() {
        let store = ArpStore::open_memory().unwrap();
        store.add_agent(&make_agent("a1", "ws1", "bot", 9100)).unwrap();
        store.track_task("a1", "t1", Some("ctx")).unwrap();
        let tasks = store.list_tasks("a1").unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, "t1");
    }

    #[test]
    fn allocate_port_skips_used() {
        let store = ArpStore::open_memory().unwrap();
        store.add_agent(&make_agent("a1", "ws1", "bot", PORT_BASE)).unwrap();
        let port = store
            .allocate_agent_port_with(|_| true)
            .unwrap()
            .unwrap();
        assert_eq!(port, PORT_BASE + 1);
    }
}
