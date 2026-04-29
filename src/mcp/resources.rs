use crate::state;
use rmcp::model::*;

pub fn list_resources() -> Result<ListResourcesResult, ErrorData> {
    let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;
    let mut resources = Vec::new();

    for (ws_name, ws) in &st.workspaces {
        resources.push(
            RawResource::new(
                format!("workspace://{ws_name}"),
                format!("Workspace: {ws_name}"),
            )
            .no_annotation(),
        );

        for agent in &ws.agents {
            resources.push(
                RawResource::new(
                    format!("agent://{}/status", agent.id),
                    format!("Agent Status: {}", agent.name),
                )
                .no_annotation(),
            );
            resources.push(
                RawResource::new(
                    format!("agent://{}/card", agent.id),
                    format!("Agent Card: {}", agent.name),
                )
                .no_annotation(),
            );
        }
    }

    Ok(ListResourcesResult::with_all_items(resources))
}

pub fn read_resource(uri: &str) -> Result<ReadResourceResult, ErrorData> {
    let st = state::load().map_err(|e| ErrorData::internal_error(e, None))?;

    if let Some(ws_name) = uri.strip_prefix("workspace://") {
        let ws = st.workspace(ws_name).ok_or_else(|| {
            ErrorData::resource_not_found(
                format!("workspace not found: {ws_name}"),
                None,
            )
        })?;
        let json = serde_json::json!({
            "name": ws_name,
            "project": ws.project,
            "dir": ws.dir,
            "status": if ws.active { "active" } else { "inactive" },
            "agents": ws.agents,
        });
        let text = serde_json::to_string_pretty(&json)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        return Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri)]));
    }

    if let Some(rest) = uri.strip_prefix("agent://") {
        if let Some(agent_id) = rest.strip_suffix("/status") {
            let (ws_name, agent) = st.find_agent(agent_id).ok_or_else(|| {
                ErrorData::resource_not_found(format!("agent not found: {agent_id}"), None)
            })?;
            let ws = st.workspace(ws_name).unwrap();
            let json = serde_json::json!({
                "agent_id": agent.id,
                "status": agent.status.to_string(),
                "port": agent.port,
                "direct_url": agent.base_url(),
                "proxy_url": format!("http://localhost:9099/a2a/agents/{}", agent.id),
                "workspace": ws_name,
                "project": ws.project,
                "template": agent.template,
                "started_at": agent.started_at,
            });
            let text = serde_json::to_string_pretty(&json)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri)]));
        }

        if let Some(agent_id) = rest.strip_suffix("/card") {
            let (ws_name, agent) = st.find_agent(agent_id).ok_or_else(|| {
                ErrorData::resource_not_found(format!("agent not found: {agent_id}"), None)
            })?;
            let ws = st.workspace(ws_name).unwrap();
            let card = crate::a2a_proxy::enriched_agent_card(agent, &ws.project);
            let json = serde_json::to_value(&card)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
            let text = serde_json::to_string_pretty(&json)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri)]));
        }
    }

    Err(ErrorData::resource_not_found(format!("unknown resource: {uri}"), None))
}

pub fn list_resource_templates() -> Result<ListResourceTemplatesResult, ErrorData> {
    Ok(ListResourceTemplatesResult::with_all_items(vec![
        RawResourceTemplate::new("agent://{agent_id}/status", "Agent Status")
            .with_description("Current ARP lifecycle status of an agent instance")
            .with_mime_type("application/json")
            .no_annotation(),
        RawResourceTemplate::new("agent://{agent_id}/card", "A2A Agent Card")
            .with_description("The agent's A2A AgentCard with ARP lifecycle metadata")
            .with_mime_type("application/json")
            .no_annotation(),
        RawResourceTemplate::new("workspace://{workspace_name}", "Workspace State")
            .with_description("Complete workspace state with all agent instances")
            .with_mime_type("application/json")
            .no_annotation(),
    ]))
}
