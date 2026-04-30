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

    arp_proto::Project {
        name: proj.name.clone(),
        repo: proj.repo.clone().unwrap_or_default(),
        branch: proj.branch.clone().unwrap_or_default(),
        agents: Vec::new(),
        context,
    }
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
