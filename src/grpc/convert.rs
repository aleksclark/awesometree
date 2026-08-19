//! Conversion helpers between internal Rust types and proto types.

use crate::grpc::arp_proto;
use crate::model::lifecycle::WorkSessionState as ModelState;
use crate::model::work_session::WorkSessionView;
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
pub fn agent_instance_to_proto(agent: &state::AgentInstanceState) -> arp_proto::AgentInstance {
    let started_at = chrono::DateTime::parse_from_rfc3339(&agent.started_at)
        .ok()
        .map(|dt| prost_types::Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        });

    arp_proto::AgentInstance {
        id: agent.id.clone(),
        template: agent.template.clone(),
        workspace: agent.work_session_id.clone(),
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

/// Alias used by older call sites.
pub fn agent_to_proto(agent: &state::AgentInstanceState) -> arp_proto::AgentInstance {
    agent_instance_to_proto(agent)
}

/// Convert a WorkSession id into the discovery stream Workspace payload.
///
/// Not the AWM material Workspace Resource; discovery events still use the
/// legacy Workspace message until those streams get a major proto rename.
pub fn work_session_to_discovery_payload(
    work_session_id: &str,
    project_id: &str,
    dir: &str,
    active: bool,
    agents: &[state::AgentInstanceState],
) -> arp_proto::Workspace {
    arp_proto::Workspace {
        name: work_session_id.to_string(),
        project: project_id.to_string(),
        dir: dir.to_string(),
        status: if active {
            arp_proto::WorkspaceStatus::Active as i32
        } else {
            arp_proto::WorkspaceStatus::Inactive as i32
        },
        agents: agents.iter().map(agent_to_proto).collect(),
        created_at: None,
        metadata: None,
    }
}

/// Legacy name — prefer [`work_session_to_discovery_payload`].
#[deprecated(note = "use work_session_to_discovery_payload")]
pub fn work_session_to_proto_workspace(
    work_session_id: &str,
    project_id: &str,
    dir: &str,
    active: bool,
    agents: &[state::AgentInstanceState],
) -> arp_proto::Workspace {
    work_session_to_discovery_payload(work_session_id, project_id, dir, active, agents)
}

pub fn work_session_state_to_proto(state: ModelState) -> i32 {
    match state {
        ModelState::Proposed => arp_proto::WorkSessionState::Proposed as i32,
        ModelState::Open => arp_proto::WorkSessionState::Open as i32,
        ModelState::Paused => arp_proto::WorkSessionState::Paused as i32,
        ModelState::Closed => arp_proto::WorkSessionState::Closed as i32,
        ModelState::Aborted => arp_proto::WorkSessionState::Aborted as i32,
    }
}

pub fn work_session_view_to_proto(v: &WorkSessionView) -> arp_proto::WorkSession {
    arp_proto::WorkSession {
        work_session_id: v.work_session.work_session_id.clone(),
        project_id: v.work_session.project_id.clone().unwrap_or_default(),
        work_profile_id: v.work_session.work_profile_id.clone().unwrap_or_default(),
        project_revision: v.work_session.project_revision.clone().unwrap_or_default(),
        project_snapshot_id: v
            .work_session
            .project_snapshot_id
            .clone()
            .unwrap_or_default(),
        state: work_session_state_to_proto(v.work_session.state),
        display_name: v.work_session.display_name.clone().unwrap_or_default(),
        workspace_path: v
            .runtime
            .as_ref()
            .and_then(|r| r.workspace.as_ref())
            .map(|w| w.path.clone())
            .unwrap_or_default(),
        realization_status: v
            .runtime
            .as_ref()
            .map(|r| r.realization_status.to_string())
            .unwrap_or_default(),
        created_at: None,
    }
}

pub fn json_to_prost_struct(v: &serde_json::Value) -> Option<prost_types::Struct> {
    let obj = v.as_object()?;
    let mut fields = std::collections::BTreeMap::new();
    for (k, val) in obj {
        fields.insert(k.clone(), json_to_prost_value(val));
    }
    Some(prost_types::Struct { fields })
}

fn json_to_prost_value(v: &serde_json::Value) -> prost_types::Value {
    use prost_types::value::Kind;
    let kind = match v {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => Kind::ListValue(prost_types::ListValue {
            values: arr.iter().map(json_to_prost_value).collect(),
        }),
        serde_json::Value::Object(_) => Kind::StructValue(json_to_prost_struct(v).unwrap_or_default()),
    };
    prost_types::Value { kind: Some(kind) }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::state::{AgentInstanceState, AgentStatus};

    fn sample_agent() -> AgentInstanceState {
        AgentInstanceState {
            id: "a1".into(),
            template: "crush".into(),
            name: "bot".into(),
            work_session_id: "ws1".into(),
            status: AgentStatus::Ready,
            port: 9100,
            host: None,
            pid: None,
            started_at: "t".into(),
            token_id: None,
            session_id: None,
            spawned_by: None,
        }
    }

    #[test]
    fn agent_status_roundtrip() {
        assert_eq!(
            agent_status_to_proto(&AgentStatus::Ready),
            arp_proto::AgentStatus::Ready as i32
        );
    }

    #[test]
    fn agent_to_proto_maps_work_session() {
        let p = agent_to_proto(&sample_agent());
        assert_eq!(p.workspace, "ws1");
        assert_eq!(p.id, "a1");
    }
}
