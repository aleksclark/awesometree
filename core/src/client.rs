use crate::models::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[derive(uniffi::Object)]
pub struct ApiClient {
    host: String,
    port: u16,
    token: String,
}

#[uniffi::export]
impl ApiClient {
    #[uniffi::constructor]
    pub fn new(host: String, port: u16, token: String) -> Self {
        Self { host, port, token }
    }

    #[uniffi::constructor]
    pub fn from_connection(conn: ServerConnection) -> Self {
        Self {
            host: conn.host,
            port: conn.port,
            token: conn.token,
        }
    }

    pub fn list_work_sessions(&self) -> Result<Vec<WorkSessionInfo>, ApiError> {
        let body = self.get("/api/work-sessions")?;
        parse_work_session_list(&body)
    }

    pub fn get_work_session(&self, id: String) -> Result<WorkSessionInfo, ApiError> {
        let body = self.get(&format!("/api/work-sessions/{id}"))?;
        parse_work_session_view(&body)
    }

    pub fn create_work_session(&self, req: CreateWorkSessionReq) -> Result<WorkSessionInfo, ApiError> {
        let mut payload = json!({
            "work_session_id": req.work_session_id,
            "project_id": req.project_id,
            "headless": req.headless,
        });
        if !req.work_profile_id.is_empty() {
            payload["work_profile_id"] = json!(req.work_profile_id);
        }
        if !req.display_name.is_empty() {
            payload["display_name"] = json!(req.display_name);
        }
        let body = self.post("/api/work-sessions", &payload.to_string())?;
        // Response is CreateWorkSessionResponse
        let v: Value = serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })?;
        let runtime = v.get("runtime").cloned();
        let ws = v.get("work_session").cloned().unwrap_or(v);
        parse_work_session_value(&ws, runtime.as_ref())
    }

    pub fn delete_work_session(&self, id: String) -> Result<(), ApiError> {
        self.delete(&format!("/api/work-sessions/{id}"))?;
        Ok(())
    }

    pub fn transition_work_session(&self, id: String, state: String) -> Result<WorkSessionInfo, ApiError> {
        let payload = json!({"state": state}).to_string();
        let body = self.post(&format!("/api/work-sessions/{id}/transition"), &payload)?;
        parse_work_session_view(&body)
    }

    pub fn list_work_profiles(&self) -> Result<Vec<WorkProfileInfo>, ApiError> {
        let body = self.get("/api/work-profiles")?;
        let list: Vec<Value> = serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })?;
        Ok(list
            .into_iter()
            .map(|v| WorkProfileInfo {
                work_profile_id: v
                    .get("work_profile_id")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .into(),
                display_name: v
                    .get("display_name")
                    .and_then(|x| x.as_str())
                    .map(|s| s.into()),
                description: v
                    .get("description")
                    .and_then(|x| x.as_str())
                    .map(|s| s.into()),
                project_ids: v
                    .get("project_ids")
                    .and_then(|x| x.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect())
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectInfo>, ApiError> {
        let body = self.get("/api/projects")?;
        let list: Vec<Value> = serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })?;
        Ok(list
            .into_iter()
            .map(|v| ProjectInfo {
                project_id: v
                    .get("projectId")
                    .or_else(|| v.get("project_id"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .into(),
                title: v
                    .get("title")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .into(),
                description: v
                    .get("description")
                    .and_then(|x| x.as_str())
                    .map(|s| s.into()),
                revision: v
                    .get("revision")
                    .and_then(|x| x.as_str())
                    .map(|s| s.into()),
                source_revision: v
                    .get("sourceRevision")
                    .or_else(|| v.get("source_revision"))
                    .and_then(|x| x.as_str())
                    .map(|s| s.into()),
            })
            .collect())
    }

    pub fn get_project(&self, id: String) -> Result<ProjectDetail, ApiError> {
        let body = self.get(&format!("/api/projects/{id}"))?;
        let v: Value = serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })?;
        Ok(ProjectDetail {
            project_id: v
                .get("projectId")
                .or_else(|| v.get("project_id"))
                .and_then(|x| x.as_str())
                .unwrap_or(&id)
                .into(),
            revision: v
                .get("revision")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .into(),
            source_revision: v
                .get("sourceRevision")
                .or_else(|| v.get("source_revision"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .into(),
            definition_json: v
                .get("definition")
                .cloned()
                .unwrap_or(json!({}))
                .to_string(),
        })
    }

    pub fn create_project(&self, req: CreateProjectReq) -> Result<ProjectInfo, ApiError> {
        let payload = json!({
            "project_id": req.project_id,
            "description": if req.description.is_empty() { Value::Null } else { json!(req.description) },
            "repo": if req.repo.is_empty() { Value::Null } else { json!(req.repo) },
            "branch": if req.branch.is_empty() { Value::Null } else { json!(req.branch) },
        })
        .to_string();
        let body = self.post("/api/projects", &payload)?;
        let v: Value = serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })?;
        Ok(ProjectInfo {
            project_id: v
                .get("projectId")
                .or_else(|| v.get("project_id"))
                .and_then(|x| x.as_str())
                .unwrap_or(&req.project_id)
                .into(),
            title: v
                .get("title")
                .and_then(|x| x.as_str())
                .unwrap_or(&req.project_id)
                .into(),
            description: v
                .get("description")
                .and_then(|x| x.as_str())
                .map(|s| s.into()),
            revision: v
                .get("revision")
                .and_then(|x| x.as_str())
                .map(|s| s.into()),
            source_revision: v
                .get("sourceRevision")
                .and_then(|x| x.as_str())
                .map(|s| s.into()),
        })
    }

    pub fn delete_project(&self, id: String, expected_source_revision: String) -> Result<(), ApiError> {
        self.delete(&format!(
            "/api/projects/{id}?expected_source_revision={expected_source_revision}"
        ))?;
        Ok(())
    }
}

fn parse_work_session_list(body: &str) -> Result<Vec<WorkSessionInfo>, ApiError> {
    let list: Vec<Value> = serde_json::from_str(body).map_err(|e| ApiError::Parse {
        message: e.to_string(),
    })?;
    list.into_iter()
        .map(|v| {
            let ws = v.get("work_session").cloned().unwrap_or(v.clone());
            parse_work_session_value(&ws, v.get("runtime"))
        })
        .collect()
}

fn parse_work_session_view(body: &str) -> Result<WorkSessionInfo, ApiError> {
    let v: Value = serde_json::from_str(body).map_err(|e| ApiError::Parse {
        message: e.to_string(),
    })?;
    let ws = v.get("work_session").cloned().unwrap_or(v.clone());
    parse_work_session_value(&ws, v.get("runtime"))
}

fn parse_work_session_value(ws: &Value, runtime: Option<&Value>) -> Result<WorkSessionInfo, ApiError> {
    let state = match ws.get("state") {
        Some(Value::String(s)) => s.clone(),
        other => other
            .map(|v| v.to_string().trim_matches('"').to_string())
            .unwrap_or_default(),
    };
    Ok(WorkSessionInfo {
        work_session_id: ws
            .get("work_session_id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .into(),
        project_id: ws
            .get("project_id")
            .and_then(|x| x.as_str())
            .map(|s| s.into()),
        work_profile_id: ws
            .get("work_profile_id")
            .and_then(|x| x.as_str())
            .map(|s| s.into()),
        state,
        project_revision: ws
            .get("project_revision")
            .and_then(|x| x.as_str())
            .map(|s| s.into()),
        project_snapshot_id: ws
            .get("project_snapshot_id")
            .and_then(|x| x.as_str())
            .map(|s| s.into()),
        display_name: ws
            .get("display_name")
            .and_then(|x| x.as_str())
            .map(|s| s.into()),
        dir: runtime
            .and_then(|r| r.get("workspace"))
            .and_then(|w| w.get("path"))
            .and_then(|p| p.as_str())
            .map(|s| s.into()),
        realization_status: runtime
            .and_then(|r| r.get("realization_status"))
            .and_then(|s| s.as_str())
            .map(|s| s.into()),
        headless: runtime
            .and_then(|r| r.get("headless"))
            .and_then(|h| h.as_bool())
            .unwrap_or(false),
    })
}

impl ApiClient {
    fn connect(&self) -> Result<TcpStream, ApiError> {
        let addr = format!("{}:{}", self.host, self.port);
        let stream = TcpStream::connect_timeout(
            &addr.parse().map_err(|e| ApiError::Network {
                message: format!("invalid address: {e}"),
            })?,
            Duration::from_secs(10),
        )
        .map_err(|e| ApiError::Network {
            message: e.to_string(),
        })?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .ok();
        stream
            .set_write_timeout(Some(Duration::from_secs(30)))
            .ok();
        Ok(stream)
    }

    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, ApiError> {
        let mut stream = self.connect()?;
        let body_len = body.map(|b| b.len()).unwrap_or(0);
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n{}",
            self.host,
            self.port,
            self.token,
            body.unwrap_or("")
        );
        stream
            .write_all(req.as_bytes())
            .map_err(|e| ApiError::Network {
                message: e.to_string(),
            })?;
        stream.flush().ok();

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader
            .read_line(&mut status_line)
            .map_err(|e| ApiError::Network {
                message: e.to_string(),
            })?;
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        if status == 401 {
            return Err(ApiError::AuthFailed);
        }

        let mut content_length = None;
        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| ApiError::Network {
                    message: e.to_string(),
                })?;
            if line == "\r\n" || line == "\n" || line.is_empty() {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("content-length:") {
                content_length = v.trim().parse().ok();
            }
        }

        let mut body_buf = Vec::new();
        if let Some(len) = content_length {
            body_buf.resize(len, 0);
            reader
                .read_exact(&mut body_buf)
                .map_err(|e| ApiError::Network {
                    message: e.to_string(),
                })?;
        } else {
            reader
                .read_to_end(&mut body_buf)
                .map_err(|e| ApiError::Network {
                    message: e.to_string(),
                })?;
        }
        let body = String::from_utf8_lossy(&body_buf).into_owned();
        if !(200..300).contains(&status) {
            return Err(ApiError::Server {
                status,
                message: body,
            });
        }
        Ok(body)
    }

    fn get(&self, path: &str) -> Result<String, ApiError> {
        self.request("GET", path, None)
    }
    fn post(&self, path: &str, body: &str) -> Result<String, ApiError> {
        self.request("POST", path, Some(body))
    }
    fn delete(&self, path: &str) -> Result<String, ApiError> {
        self.request("DELETE", path, None)
    }
}
