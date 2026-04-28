# ARP Implementation Plan

Phase 1 of the Agent Registry Protocol — multi-agent model and A2A proxy.
Auth/tokens deferred to Phase 2.

## Changes

### 1. Dependencies (`Cargo.toml`)
- Add `a2a-rs-core` for A2A v1.0 types (AgentCard, Task, Message, Part, etc.)
- Add `a2a-rs-client` for proxying requests to agent processes
- Add `uuid` for agent instance ID generation
- Add `chrono` for ISO 8601 timestamps

### 2. Data Model (`src/state.rs`)
- Add `AgentInstanceState` struct:
  - `id: String` — unique instance ID (uuid)
  - `template: String` — template name used to spawn
  - `name: String` — instance name (may differ from template)
  - `status: String` — "starting" | "ready" | "busy" | "error" | "stopping" | "stopped"
  - `port: u16` — assigned port
  - `pid: Option<u32>` — OS process ID
  - `workspace: String` — parent workspace name
  - `started_at: String` — ISO 8601
- Add `agents: Vec<AgentInstanceState>` to `WorkspaceState`
- Backward compat: if `agents` is empty, treat existing `acp_port` as a single legacy agent
- New `allocate_agent_port()` that scans all agents across all workspaces

### 3. Agent Supervisor (`src/agent_supervisor.rs`)
New module replacing the single-agent-per-workspace model:
- `AgentSupervisor` struct with `HashMap<String, ManagedAgent>` keyed by agent_id
- Each `ManagedAgent` tracks: workspace, template, port, process, stop_signal, agent_card cache
- `spawn_agent(workspace, dir, template, name, port, command, env)` → agent_id
- `stop_agent(agent_id)` with SIGTERM→grace→SIGKILL
- `restart_agent(agent_id)`
- Health check loop: poll `GET /.well-known/agent-card.json` on each agent's port
- Status state machine: starting → ready → busy → error → stopping → stopped
- Fetch and cache `AgentCard` from the agent's direct port
- Generate synthetic `AgentCard` for legacy agents that don't serve one

Keep old `acp_supervisor.rs` module — it still handles legacy single-agent workspaces.
The new `agent_supervisor` is used for ARP-managed multi-agent workspaces.

### 4. A2A Proxy Endpoints (`src/a2a_proxy.rs`)
New module with axum routes mounted at `/a2a/`:

**Registry:**
- `GET /a2a/agents` — list all AgentCards for ready agents
- `GET /a2a/discover` — filtered discovery by capability, workspace, status

**Per-agent (proxied A2A v1.0):**
- `GET /a2a/agents/{agent_id}/.well-known/agent-card.json` — enriched AgentCard
- `POST /a2a/agents/{agent_id}/message:send` — proxy SendMessage
- `POST /a2a/agents/{agent_id}/message:stream` — proxy SendStreamingMessage (SSE passthrough)
- `GET /a2a/agents/{agent_id}/tasks/{task_id}` — proxy GetTask
- `POST /a2a/agents/{agent_id}/tasks/{task_id}:cancel` — proxy CancelTask

**Routing:**
- `POST /a2a/route/message:send` — route by AgentSkill.tags

### 5. AgentCard Enrichment
When serving through proxy, enrich `AgentCard.metadata` with:
```json
{
  "arp": {
    "agent_id": "...",
    "workspace": "...",
    "project": "...",
    "template": "...",
    "status": "ready",
    "direct_url": "http://localhost:9100",
    "started_at": "..."
  }
}
```
Set `supported_interfaces[0].url` to proxied URL.

### 6. Server Integration (`src/server.rs`)
- Mount a2a_proxy routes under the existing axum app at `/a2a/`
- Share the hyper client for proxying
- Existing `/api/` and `/acp/` routes untouched

### 7. Module Registration (`src/lib.rs`)
- Add `pub mod agent_supervisor;`
- Add `pub mod a2a_proxy;`

## Not In This Phase
- Auth/tokens (identity-and-scopes.md)
- MCP tool interface (tools-*.md)
- MCP resources and prompts (tools-discovery.md)
- Push notifications
- Agent-to-agent direct communication auth
