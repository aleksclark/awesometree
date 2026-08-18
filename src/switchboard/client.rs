use super::config::SwitchboardConfig;
use super::Catalog;
use crate::model::error::{ErrorCode, Result, SwitchboardError};
use crate::model::lifecycle::WorkSessionState;
use crate::model::project::{ProjectEnvelope, ProjectSummary};
use crate::model::work_profile::WorkProfile;
use crate::model::work_session::WorkSession;
use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, ClientInfo, Implementation, RawContent};
use rmcp::service::RunningService;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{json, Map, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

type ClientService = RunningService<RoleClient, ClientInfo>;

/// Production Switchboard MCP client.
pub struct SwitchboardClient {
    cfg: SwitchboardConfig,
    /// Lazily connected service; recreated on failure.
    inner: Mutex<Option<Arc<ClientService>>>,
}

impl SwitchboardClient {
    pub fn new(cfg: SwitchboardConfig) -> Self {
        Self {
            cfg,
            inner: Mutex::new(None),
        }
    }

    pub fn from_env() -> Self {
        Self::new(SwitchboardConfig::from_env())
    }

    pub fn endpoint(&self) -> &str {
        &self.cfg.endpoint
    }

    async fn connect(&self) -> Result<Arc<ClientService>> {
        let mut guard = self.inner.lock().await;
        if let Some(svc) = guard.as_ref() {
            return Ok(svc.clone());
        }
        let transport = StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(self.cfg.endpoint.clone()),
        );
        let client_info = ClientInfo::new(
            Default::default(),
            Implementation::new("awesometree", env!("CARGO_PKG_VERSION")),
        );
        let svc = client_info
            .serve(transport)
            .await
            .map_err(|e| SwitchboardError::unavailable("connect", e.to_string()))?;
        let arc = Arc::new(svc);
        *guard = Some(arc.clone());
        Ok(arc)
    }

    async fn invalidate(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(svc) = guard.take() {
            // Drop connection; ignore close errors.
            drop(svc);
        }
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let args_map: Option<Map<String, Value>> = match arguments {
            Value::Object(m) => Some(m),
            Value::Null => None,
            other => {
                let mut m = Map::new();
                m.insert("value".into(), other);
                Some(m)
            }
        };
        let mut params = CallToolRequestParams::new(name.to_string());
        if let Some(m) = args_map {
            params = params.with_arguments(m);
        }

        let mut last_err = None;
        for attempt in 0..2 {
            let svc = match self.connect().await {
                Ok(s) => s,
                Err(e) => {
                    last_err = Some(e);
                    break;
                }
            };
            match svc.call_tool(params.clone()).await {
                Ok(result) => {
                    if result.is_error.unwrap_or(false) {
                        if let Some(sc) = result.structured_content {
                            return Err(SwitchboardError::from_switchboard_body(&sc, name));
                        }
                        let msg = content_texts(&result.content);
                        return Err(SwitchboardError::new(
                            ErrorCode::InternalError,
                            if msg.is_empty() {
                                format!("{name} failed")
                            } else {
                                msg
                            },
                        )
                        .with_operation(name));
                    }
                    if let Some(sc) = result.structured_content {
                        return Ok(sc);
                    }
                    // Fall back to parsing text content as JSON.
                    for text in content_text_iter(&result.content) {
                        if let Ok(v) = serde_json::from_str::<Value>(&text) {
                            return Ok(v);
                        }
                    }
                    return Ok(Value::Null);
                }
                Err(e) => {
                    last_err = Some(SwitchboardError::unavailable(name, e.to_string()));
                    self.invalidate().await;
                    if attempt == 0 {
                        continue;
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| SwitchboardError::unavailable(name, "unknown")))
    }
}

#[async_trait]
impl Catalog for SwitchboardClient {
    async fn health(&self) -> Result<()> {
        // list tools is a cheap readiness probe
        let svc = self.connect().await?;
        svc.list_tools(Default::default())
            .await
            .map_err(|e| SwitchboardError::unavailable("health", e.to_string()))?;
        Ok(())
    }

    async fn list_projects(&self, query: Option<&str>) -> Result<Vec<ProjectSummary>> {
        let mut args = json!({});
        if let Some(q) = query {
            args["query"] = Value::String(q.into());
        }
        let v = self.call_tool("project_list", args).await?;
        let projects = v
            .get("projects")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        serde_json::from_value(projects).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("parse projects: {e}"))
                .with_operation("project_list")
        })
    }

    async fn get_project(&self, project_id: &str) -> Result<ProjectEnvelope> {
        let v = self
            .call_tool("project_get", json!({"projectId": project_id}))
            .await?;
        // Map tool output into ProjectEnvelope.
        let project_id = v
            .get("projectId")
            .and_then(|x| x.as_str())
            .unwrap_or(project_id)
            .to_string();
        let revision = v
            .get("revision")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let source_revision = v
            .get("sourceRevision")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let definition = v.get("definition").cloned().unwrap_or(json!({}));
        let summary = v
            .get("summary")
            .cloned()
            .and_then(|s| serde_json::from_value(s).ok());
        let uri = v.get("uri").and_then(|x| x.as_str()).map(|s| s.to_string());
        Ok(ProjectEnvelope {
            project_id,
            revision,
            source_revision,
            definition,
            summary,
            uri,
        })
    }

    async fn create_project(&self, definition: Value) -> Result<ProjectSummary> {
        let v = self
            .call_tool("project_create", json!({"definition": definition}))
            .await?;
        let project = v.get("project").cloned().unwrap_or(v);
        serde_json::from_value(project).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("parse create: {e}"))
                .with_operation("project_create")
        })
    }

    async fn update_project(
        &self,
        project_id: &str,
        expected_source_revision: &str,
        patch: Value,
    ) -> Result<ProjectSummary> {
        let v = self
            .call_tool(
                "project_update",
                json!({
                    "projectId": project_id,
                    "expectedSourceRevision": expected_source_revision,
                    "patch": patch,
                }),
            )
            .await?;
        let project = v.get("project").cloned().unwrap_or(v);
        serde_json::from_value(project).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("parse update: {e}"))
                .with_operation("project_update")
        })
    }

    async fn delete_project(
        &self,
        project_id: &str,
        expected_source_revision: &str,
    ) -> Result<()> {
        self.call_tool(
            "project_delete",
            json!({
                "projectId": project_id,
                "expectedSourceRevision": expected_source_revision,
            }),
        )
        .await?;
        Ok(())
    }

    async fn list_work_profiles(&self) -> Result<Vec<WorkProfile>> {
        let v = self.call_tool("project_work_profile_list", json!({})).await?;
        let list = v
            .get("work_profiles")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        serde_json::from_value(list).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("parse work_profiles: {e}"))
                .with_operation("project_work_profile_list")
        })
    }

    async fn get_work_profile(&self, id: &str) -> Result<WorkProfile> {
        let v = self
            .call_tool("project_work_profile_get", json!({"id": id}))
            .await?;
        serde_json::from_value(v).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("parse work_profile: {e}"))
                .with_operation("project_work_profile_get")
        })
    }

    async fn put_work_profile(&self, profile: WorkProfile) -> Result<WorkProfile> {
        let args = serde_json::to_value(&profile).map_err(|e| {
            SwitchboardError::new(ErrorCode::InvalidInput, e.to_string())
                .with_operation("project_work_profile_put")
        })?;
        let v = self.call_tool("project_work_profile_put", args).await?;
        serde_json::from_value(v).map_err(|e| {
            SwitchboardError::new(ErrorCode::InternalError, format!("parse put profile: {e}"))
                .with_operation("project_work_profile_put")
        })
    }

    async fn delete_work_profile(&self, id: &str) -> Result<()> {
        self.call_tool("project_work_profile_delete", json!({"id": id}))
            .await?;
        Ok(())
    }

    async fn list_work_sessions(
        &self,
        state: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Vec<WorkSession>> {
        let mut args = json!({});
        if let Some(s) = state {
            args["state"] = Value::String(s.into());
        }
        if let Some(p) = project_id {
            args["project_id"] = Value::String(p.into());
        }
        let v = self
            .call_tool("project_work_session_list", args)
            .await?;
        let list = v
            .get("work_sessions")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        // State may arrive as string from Go JSON.
        parse_work_sessions(list)
    }

    async fn get_work_session(&self, id: &str) -> Result<WorkSession> {
        let v = self
            .call_tool("project_work_session_get", json!({"id": id}))
            .await?;
        parse_work_session(v)
    }

    async fn create_work_session(&self, session: WorkSession) -> Result<WorkSession> {
        let mut args = json!({
            "version": session.version,
            "work_session_id": session.work_session_id,
            "state": session.state.as_str(),
        });
        if let Some(d) = &session.display_name {
            args["display_name"] = Value::String(d.clone());
        }
        if let Some(p) = &session.project_id {
            args["project_id"] = Value::String(p.clone());
        }
        if let Some(r) = &session.project_revision {
            args["project_revision"] = Value::String(r.clone());
        }
        if let Some(s) = &session.project_snapshot_id {
            args["project_snapshot_id"] = Value::String(s.clone());
        }
        if let Some(w) = &session.work_profile_id {
            args["work_profile_id"] = Value::String(w.clone());
        }
        if !session.agent_profile_ids.is_empty() {
            args["agent_profile_ids"] = json!(session.agent_profile_ids);
        }
        if let Some(pol) = &session.policy {
            args["policy"] = pol.clone();
        }
        let v = self
            .call_tool("project_work_session_create", args)
            .await?;
        parse_work_session(v)
    }

    async fn transition_work_session(
        &self,
        id: &str,
        state: WorkSessionState,
    ) -> Result<WorkSession> {
        let v = self
            .call_tool(
                "project_work_session_transition",
                json!({"id": id, "state": state.as_str()}),
            )
            .await?;
        parse_work_session(v)
    }

    async fn patch_work_session(
        &self,
        id: &str,
        display_name: Option<String>,
        policy: Option<Value>,
    ) -> Result<WorkSession> {
        let mut args = json!({"id": id});
        if let Some(d) = display_name {
            args["display_name"] = Value::String(d);
        }
        if let Some(p) = policy {
            args["policy"] = p;
        }
        let v = self
            .call_tool("project_work_session_patch", args)
            .await?;
        parse_work_session(v)
    }

    async fn delete_work_session(&self, id: &str) -> Result<()> {
        self.call_tool("project_work_session_delete", json!({"id": id}))
            .await?;
        Ok(())
    }
}

fn content_texts(content: &[rmcp::model::Content]) -> String {
    content_text_iter(content).join("\n")
}

fn content_text_iter(content: &[rmcp::model::Content]) -> Vec<String> {
    content
        .iter()
        .filter_map(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

fn parse_work_sessions(list: Value) -> Result<Vec<WorkSession>> {
    let arr = list.as_array().cloned().unwrap_or_default();
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        out.push(parse_work_session(item)?);
    }
    Ok(out)
}

fn parse_work_session(mut v: Value) -> Result<WorkSession> {
    // Normalize state string if needed (serde handles enum).
    if let Some(obj) = v.as_object_mut() {
        if let Some(Value::String(s)) = obj.get("state").cloned() {
            // already string — fine
            let _ = s;
        }
        // Go may serialize times as RFC3339 strings already.
    }
    serde_json::from_value(v).map_err(|e| {
        SwitchboardError::new(
            ErrorCode::InternalError,
            format!("parse work_session: {e}"),
        )
        .with_operation("parse_work_session")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::error::ErrorCode;

    #[test]
    fn maps_typed_error_body() {
        let body = json!({
            "error": {
                "code": "missing_default_profile",
                "message": "default missing",
                "entity_kind": "work_profile",
                "entity_id": "default"
            }
        });
        let e = SwitchboardError::from_switchboard_body(&body, "create");
        assert_eq!(e.code, ErrorCode::MissingDefaultProfile);
        assert_eq!(e.entity_id.as_deref(), Some("default"));
    }

    #[test]
    fn maps_revision_conflict() {
        let body = json!({
            "error": {
                "code": "revision_conflict",
                "message": "project revision changed",
                "projectId": "demo",
                "expectedSourceRevision": "sha256:aaa",
                "currentSourceRevision": "sha256:bbb"
            }
        });
        let e = SwitchboardError::from_switchboard_body(&body, "project_update");
        assert_eq!(e.code, ErrorCode::Conflict);
        assert_eq!(e.expected.as_deref(), Some("sha256:aaa"));
        assert_eq!(e.current.as_deref(), Some("sha256:bbb"));
    }
}
