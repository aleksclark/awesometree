//! Conversion helpers between internal Rust types and proto types.

use crate::grpc::arp_proto;
use crate::state;

/// Convert internal `AgentStatus` to proto `AgentStatus` enum i32.
pub fn agent_status_to_proto(status: &state::AgentStatus) -> i32 {
    match status {
        state::AgentStatus::Starting => arp_proto::AgentStatus::Starting as i32,
        state::AgentStatus::Ready => arp_proto::AgentStatus::Ready as i32,
        state::AgentStatus::Busy => arp_proto::AgentStatus::Busy as i32,
        state::AgentStatus::Error => arp_proto::AgentStatus::Error as i32,
        state::AgentStatus::Stopping => arp_proto::AgentStatus::Stopping as i32,
        state::AgentStatus::Stopped => arp_proto::AgentStatus::Stopped as i32,
    }
}

/// Convert an internal `AgentInstanceState` to a proto `AgentInstance`.
pub fn agent_instance_to_proto(
    agent: &state::AgentInstanceState,
) -> arp_proto::AgentInstance {
    let started_at = chrono::DateTime::parse_from_rfc3339(&agent.started_at)
        .ok()
        .map(|dt| prost_types::Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        });

    arp_proto::AgentInstance {
        id: agent.id.clone(),
        template: agent.template.clone(),
        workspace: agent.workspace.clone(),
        status: agent_status_to_proto(&agent.status),
        port: agent.port as i32,
        direct_url: agent.base_url(),
        proxy_url: String::new(),
        pid: agent.pid.map(|p| p as i32).unwrap_or(0),
        context_id: String::new(),
        a2a_agent_card: None,
        token_id: agent.token_id.clone().unwrap_or_default(),
        session_id: agent.session_id.clone().unwrap_or_default(),
        spawned_by: agent.spawned_by.clone().unwrap_or_default(),
        started_at,
        metadata: None,
    }
}

/// Convert an internal `WorkspaceState` to a proto `Workspace`.
pub fn workspace_to_proto(
    name: &str,
    ws: &state::WorkspaceState,
) -> arp_proto::Workspace {
    let status = if ws.active {
        arp_proto::WorkspaceStatus::Active as i32
    } else {
        arp_proto::WorkspaceStatus::Inactive as i32
    };

    let agents: Vec<arp_proto::AgentInstance> = ws
        .agents
        .iter()
        .map(agent_instance_to_proto)
        .collect();

    arp_proto::Workspace {
        name: name.to_string(),
        project: ws.project.clone(),
        dir: ws.dir.clone(),
        status,
        agents,
        created_at: None,
        metadata: None,
    }
}

/// Convert an internal `interop::Project` to a proto `Project`.
pub fn interop_project_to_proto(
    proj: &crate::interop::Project,
) -> arp_proto::Project {
    let context = proj.context.as_ref().map(|ctx| arp_proto::ProjectContext {
        files: ctx.files.clone().unwrap_or_default(),
        repo_includes: ctx.repo_includes.clone().unwrap_or_default(),
        max_bytes: ctx.max_bytes.map(|b| b as i64).unwrap_or(0),
    });

    let agents = interop_agents_to_proto(proj);

    arp_proto::Project {
        name: proj.name.clone(),
        repo: proj.repo.clone().unwrap_or_default(),
        branch: proj.branch.clone().unwrap_or_default(),
        agents,
        context,
    }
}

/// Convert interop Project agents JSON to proto AgentTemplate vec.
pub fn interop_agents_to_proto(
    proj: &crate::interop::Project,
) -> Vec<arp_proto::AgentTemplate> {
    let templates = proj.agent_templates();
    templates
        .into_iter()
        .map(|(name, cfg)| {
            let health_check = cfg.health_check.map(|hc| arp_proto::HealthCheckConfig {
                path: hc.path.unwrap_or_default(),
                interval_ms: hc.interval_ms.unwrap_or(0),
                timeout_ms: hc.timeout_ms.unwrap_or(0),
                retries: hc.retries.unwrap_or(0),
            });
            arp_proto::AgentTemplate {
                name,
                command: cfg.command.unwrap_or_default(),
                port_env: cfg.port_env.unwrap_or_default(),
                health_check,
                env: cfg.env,
                capabilities: cfg.capabilities,
                a2a_card_config: None,
            }
        })
        .collect()
}

/// Convert proto AgentTemplate list to JSON Value for interop Project.agents.
pub fn proto_agents_to_json(
    agents: &[arp_proto::AgentTemplate],
) -> Option<serde_json::Value> {
    if agents.is_empty() {
        return None;
    }
    let mut map = serde_json::Map::new();
    for tmpl in agents {
        let mut cfg = serde_json::Map::new();
        if !tmpl.command.is_empty() {
            cfg.insert("command".into(), serde_json::Value::String(tmpl.command.clone()));
        }
        if !tmpl.port_env.is_empty() {
            cfg.insert("portEnv".into(), serde_json::Value::String(tmpl.port_env.clone()));
        }
        if let Some(hc) = &tmpl.health_check {
            let mut hc_map = serde_json::Map::new();
            if !hc.path.is_empty() {
                hc_map.insert("path".into(), serde_json::Value::String(hc.path.clone()));
            }
            if hc.interval_ms != 0 {
                hc_map.insert("intervalMs".into(), serde_json::json!(hc.interval_ms));
            }
            if hc.timeout_ms != 0 {
                hc_map.insert("timeoutMs".into(), serde_json::json!(hc.timeout_ms));
            }
            if hc.retries != 0 {
                hc_map.insert("retries".into(), serde_json::json!(hc.retries));
            }
            if !hc_map.is_empty() {
                cfg.insert("healthCheck".into(), serde_json::Value::Object(hc_map));
            }
        }
        if !tmpl.env.is_empty() {
            let env_obj: serde_json::Map<String, serde_json::Value> = tmpl
                .env
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            cfg.insert("env".into(), serde_json::Value::Object(env_obj));
        }
        if !tmpl.capabilities.is_empty() {
            cfg.insert(
                "capabilities".into(),
                serde_json::Value::Array(
                    tmpl.capabilities.iter().map(|c| serde_json::Value::String(c.clone())).collect(),
                ),
            );
        }
        map.insert(tmpl.name.clone(), serde_json::Value::Object(cfg));
    }
    Some(serde_json::Value::Object(map))
}

/// Convert a `serde_json::Value` to a `prost_types::Struct`.
pub fn json_to_prost_struct(val: &serde_json::Value) -> Option<prost_types::Struct> {
    if let serde_json::Value::Object(map) = val {
        let fields = map
            .iter()
            .filter_map(|(k, v)| {
                json_to_prost_value(v).map(|pv| (k.clone(), pv))
            })
            .collect();
        Some(prost_types::Struct { fields })
    } else {
        None
    }
}

/// Convert a `serde_json::Value` to a `prost_types::Value`.
fn json_to_prost_value(val: &serde_json::Value) -> Option<prost_types::Value> {
    use prost_types::value::Kind;
    let kind = match val {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => {
            let values = arr.iter().filter_map(json_to_prost_value).collect();
            Kind::ListValue(prost_types::ListValue { values })
        }
        serde_json::Value::Object(map) => {
            let fields = map
                .iter()
                .filter_map(|(k, v)| {
                    json_to_prost_value(v).map(|pv| (k.clone(), pv))
                })
                .collect();
            Kind::StructValue(prost_types::Struct { fields })
        }
    };
    Some(prost_types::Value { kind: Some(kind) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AgentInstanceState, AgentStatus};

    #[test]
    fn agent_status_to_proto_maps_correctly() {
        assert_eq!(agent_status_to_proto(&AgentStatus::Starting), arp_proto::AgentStatus::Starting as i32);
        assert_eq!(agent_status_to_proto(&AgentStatus::Ready), arp_proto::AgentStatus::Ready as i32);
        assert_eq!(agent_status_to_proto(&AgentStatus::Busy), arp_proto::AgentStatus::Busy as i32);
        assert_eq!(agent_status_to_proto(&AgentStatus::Error), arp_proto::AgentStatus::Error as i32);
        assert_eq!(agent_status_to_proto(&AgentStatus::Stopping), arp_proto::AgentStatus::Stopping as i32);
        assert_eq!(agent_status_to_proto(&AgentStatus::Stopped), arp_proto::AgentStatus::Stopped as i32);
    }

    #[test]
    fn agent_instance_to_proto_basic() {
        let agent = AgentInstanceState {
            id: "test-abc123".into(),
            template: "crush".into(),
            name: "coder".into(),
            workspace: "feat-auth".into(),
            status: AgentStatus::Ready,
            port: 9100,
            host: None,
            pid: Some(1234),
            started_at: "2026-04-28T10:00:00Z".into(),
            ..Default::default()
        };
        let proto = agent_instance_to_proto(&agent);
        assert_eq!(proto.id, "test-abc123");
        assert_eq!(proto.template, "crush");
        assert_eq!(proto.workspace, "feat-auth");
        assert_eq!(proto.port, 9100);
        assert_eq!(proto.pid, 1234);
        assert_eq!(proto.status, arp_proto::AgentStatus::Ready as i32);
        assert!(proto.direct_url.contains("9100"), "direct_url should contain port: {}", proto.direct_url);
    }

    #[test]
    fn agent_instance_to_proto_with_host() {
        let agent = AgentInstanceState {
            id: "test-xyz".into(),
            template: "echo".into(),
            name: "echo-agent".into(),
            workspace: "ws".into(),
            status: AgentStatus::Busy,
            port: 9200,
            host: Some("custom-host".into()),
            pid: None,
            started_at: "2026-04-28T10:00:00Z".into(),
            ..Default::default()
        };
        let proto = agent_instance_to_proto(&agent);
        assert!(proto.direct_url.contains("custom-host"), "direct_url should use host: {}", proto.direct_url);
        assert_eq!(proto.pid, 0, "pid should default to 0 when None");
        assert_eq!(proto.status, arp_proto::AgentStatus::Busy as i32);
    }

    #[test]
    fn agent_instance_to_proto_started_at_parses() {
        let agent = AgentInstanceState {
            id: "test-ts".into(),
            started_at: "2026-04-28T10:00:00Z".into(),
            ..Default::default()
        };
        let proto = agent_instance_to_proto(&agent);
        assert!(proto.started_at.is_some(), "valid RFC3339 should produce a timestamp");
        let ts = proto.started_at.unwrap();
        assert!(ts.seconds > 0, "timestamp seconds should be positive");
    }

    #[test]
    fn agent_instance_to_proto_bad_started_at() {
        let agent = AgentInstanceState {
            id: "test-bad-ts".into(),
            started_at: "not-a-date".into(),
            ..Default::default()
        };
        let proto = agent_instance_to_proto(&agent);
        assert!(proto.started_at.is_none(), "invalid date should produce None timestamp");
    }

    #[test]
    fn workspace_to_proto_basic() {
        use crate::state::WorkspaceState;
        let ws = WorkspaceState {
            project: "myproject".into(),
            active: true,
            tag_index: 5,
            dir: "/home/user/project".into(),
            agents: vec![
                AgentInstanceState {
                    id: "agent-1".into(),
                    template: "crush".into(),
                    name: "coder".into(),
                    workspace: "ws-1".into(),
                    status: AgentStatus::Ready,
                    port: 9100,
                    started_at: "2026-04-28T10:00:00Z".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let proto = workspace_to_proto("ws-1", &ws);
        assert_eq!(proto.name, "ws-1");
        assert_eq!(proto.project, "myproject");
        assert_eq!(proto.dir, "/home/user/project");
        assert_eq!(proto.status, arp_proto::WorkspaceStatus::Active as i32);
        assert_eq!(proto.agents.len(), 1);
        assert_eq!(proto.agents[0].id, "agent-1");
    }

    #[test]
    fn workspace_to_proto_inactive() {
        use crate::state::WorkspaceState;
        let ws = WorkspaceState {
            project: "proj".into(),
            active: false,
            agents: vec![],
            ..Default::default()
        };
        let proto = workspace_to_proto("ws-inactive", &ws);
        assert_eq!(proto.status, arp_proto::WorkspaceStatus::Inactive as i32);
        assert!(proto.agents.is_empty());
    }

    #[test]
    fn json_to_prost_struct_handles_nested_objects() {
        let json = serde_json::json!({
            "name": "test-agent",
            "status": "ready",
            "metadata": {
                "arp": {
                    "agent_id": "abc123"
                }
            }
        });
        let result = json_to_prost_struct(&json);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.fields.contains_key("name"));
        assert!(s.fields.contains_key("metadata"));
    }

    #[test]
    fn json_to_prost_struct_null_returns_none() {
        let json = serde_json::Value::Null;
        assert!(json_to_prost_struct(&json).is_none());
    }

    #[test]
    fn json_to_prost_struct_array_returns_none() {
        let json = serde_json::json!([1, 2, 3]);
        assert!(json_to_prost_struct(&json).is_none());
    }

    #[test]
    fn json_to_prost_struct_string_returns_none() {
        let json = serde_json::json!("just a string");
        assert!(json_to_prost_struct(&json).is_none());
    }

    #[test]
    fn json_to_prost_struct_empty_object() {
        let json = serde_json::json!({});
        let result = json_to_prost_struct(&json);
        assert!(result.is_some());
        assert!(result.unwrap().fields.is_empty());
    }

    #[test]
    fn json_to_prost_value_covers_all_types() {
        // Null
        let v = json_to_prost_value(&serde_json::Value::Null);
        assert!(v.is_some());

        // Bool
        let v = json_to_prost_value(&serde_json::json!(true));
        assert!(v.is_some());

        // Number
        let v = json_to_prost_value(&serde_json::json!(42.5));
        assert!(v.is_some());

        // String
        let v = json_to_prost_value(&serde_json::json!("hello"));
        assert!(v.is_some());

        // Array
        let v = json_to_prost_value(&serde_json::json!([1, "two", null]));
        assert!(v.is_some());

        // Object
        let v = json_to_prost_value(&serde_json::json!({"key": "value"}));
        assert!(v.is_some());
    }

    #[test]
    fn interop_project_to_proto_basic() {
        let proj = crate::interop::Project {
            name: "myproject".into(),
            version: "1.0".into(),
            repo: Some("https://github.com/example/repo".into()),
            branch: Some("main".into()),
            context: None,
            ..Default::default()
        };
        let proto = interop_project_to_proto(&proj);
        assert_eq!(proto.name, "myproject");
        assert_eq!(proto.repo, "https://github.com/example/repo");
        assert_eq!(proto.branch, "main");
        assert!(proto.context.is_none());
    }

    #[test]
    fn interop_project_to_proto_with_context() {
        let proj = crate::interop::Project {
            name: "proj".into(),
            version: "1.0".into(),
            context: Some(crate::interop::ContextConfig {
                files: Some(vec!["README.md".into(), "src/main.rs".into()]),
                repo_includes: Some(vec!["*.rs".into()]),
                max_bytes: Some(1024),
            }),
            ..Default::default()
        };
        let proto = interop_project_to_proto(&proj);
        assert_eq!(proto.repo, "");
        assert_eq!(proto.branch, "");
        let ctx = proto.context.unwrap();
        assert_eq!(ctx.files.len(), 2);
        assert_eq!(ctx.repo_includes, vec!["*.rs"]);
        assert_eq!(ctx.max_bytes, 1024);
    }
}
