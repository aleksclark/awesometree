---
title: "ARP — Discovery, Resources & Patterns"
version: 0.3.0
created: 2026-04-06
updated: 2026-04-28
status: draft
tags: [arp, mcp, a2a, discovery, agentcard, patterns]
---

# Discovery, Resources & Patterns

MCP tools for discovering agents, MCP resource subscriptions for monitoring, MCP prompts for common workflows, and multi-agent patterns showing how the tool groups compose.

## MCP Tools

### `agent/discover`

Discover agents across workspaces or on the network.

```json
{
  "name": "agent/discover",
  "description": "Discover available agents. Returns AgentCards — from managed workspaces (local) or by probing /.well-known/agent-card.json at given URLs (network).",
  "inputSchema": {
    "type": "object",
    "properties": {
      "scope": {
        "type": "string",
        "enum": ["local", "network"],
        "description": "Search scope: 'local' for managed agents only, 'network' probes URLs for AgentCards (default: local)"
      },
      "capability": { "type": "string", "description": "Filter by AgentSkill.tags (e.g., 'code', 'review', 'test')" },
      "urls": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Base URLs to probe for /.well-known/agent-card.json (for network scope)"
      }
    }
  }
}
```

**Returns:** Array of A2A `AgentCard` objects.

**Local scope:** Returns `AgentCard` for every managed agent with status `ready` or `busy`. Cards are enriched with `metadata.arp` (see [Agent Card Enrichment](overview.md#agent-card-enrichment)).

**Network scope:** For each URL in `urls`, issues `GET {url}/.well-known/agent-card.json` (A2A discovery per RFC 8615). Returns successfully fetched `AgentCard` objects. Non-managed agents will not have `metadata.arp`.

**Capability filtering:** When `capability` is set, only returns agents whose `AgentCard.skills[]` contains at least one `AgentSkill` with a matching `tags[]` entry.

## MCP Resources

The server exposes agent state as MCP resources for subscription-based monitoring. Hosts that support MCP resource subscriptions receive real-time notifications when these change.

### `agent://{agent_id}/status`

Current ARP lifecycle status of an agent instance.

```json
{
  "uri": "agent://{agent_id}/status",
  "name": "Agent Status",
  "description": "Current ARP lifecycle status of the agent instance",
  "mimeType": "application/json"
}
```

**Payload:**

```json
{
  "agent_id": "coder-abc123",
  "status": "ready",
  "port": 9100,
  "direct_url": "http://localhost:9100",
  "proxy_url": "http://localhost:9099/a2a/agents/coder-abc123",
  "workspace": "feat-auth",
  "template": "crush",
  "started_at": "2026-04-06T10:30:00Z"
}
```

**Subscription triggers:** Status changes (`starting` → `ready`, `ready` → `busy`, `busy` → `ready`, any → `error`, any → `stopped`).

### `agent://{agent_id}/card`

The agent's resolved A2A `AgentCard` (enriched with `metadata.arp`).

```json
{
  "uri": "agent://{agent_id}/card",
  "name": "A2A Agent Card",
  "description": "The agent's A2A AgentCard with capabilities, skills, and ARP lifecycle metadata",
  "mimeType": "application/json"
}
```

**Payload:** Full A2A `AgentCard` as shown in [Agent Card Enrichment](overview.md#agent-card-enrichment).

**Subscription triggers:** Agent restart (card may change if port changes), `AgentCard` update from the agent process.

### `workspace://{workspace_name}`

Full workspace state including all agents.

```json
{
  "uri": "workspace://{workspace_name}",
  "name": "Workspace State",
  "description": "Complete workspace state with all agent instances",
  "mimeType": "application/json"
}
```

**Payload:** Full `Workspace` object (see [Data Model](overview.md#workspace)).

**Subscription triggers:** Agent spawned, agent stopped, workspace status change.

## MCP Prompts

### `task/code-review`

Pre-built prompt for spawning a code review workflow.

```json
{
  "name": "task/code-review",
  "description": "Spawn a reviewer agent and send it a code review task via A2A SendMessage",
  "arguments": [
    { "name": "workspace", "description": "Workspace name", "required": true },
    { "name": "files", "description": "Files or directories to review", "required": false }
  ]
}
```

### `task/parallel-implementation`

Pre-built prompt for spawning multiple agents to work on subtasks in parallel.

```json
{
  "name": "task/parallel-implementation",
  "description": "Spawn multiple agents in a workspace, each assigned a subtask via A2A SendMessage",
  "arguments": [
    { "name": "workspace", "description": "Workspace name", "required": true },
    { "name": "subtask_count", "description": "Number of parallel agents (default: 2)", "required": false }
  ]
}
```

## Multi-Agent Workspace Patterns

These patterns show how the four tool groups compose to solve real workflows.

### Pattern 1: Coder + Reviewer

Two agents in one workspace — one writes code, the other reviews it.

```
workspace/create  name="feat-auth" project="myapp"
agent/spawn       workspace="feat-auth" template="crush" name="coder"
agent/spawn       workspace="feat-auth" template="crush" name="reviewer"
agent/message     agent_id="coder-xxx" message="Implement OAuth2 login flow"
# ... poll via agent/task_status until Task.status.state == TASK_STATE_COMPLETED ...
agent/message     agent_id="reviewer-xxx" message="Review the changes in this workspace"
```

### Pattern 2: Parallel Implementation

Multiple agents work on different parts of a codebase simultaneously.

```
workspace/create  name="refactor" project="myapp"
agent/spawn       workspace="refactor" template="crush" name="backend"
agent/spawn       workspace="refactor" template="crush" name="frontend"
agent/task        agent_id="backend-xxx" message="Refactor the API layer to use GraphQL"
agent/task        agent_id="frontend-xxx" message="Update React components for the new GraphQL API"
# Poll both via GetTask until Task.status.state is terminal (COMPLETED/FAILED)
agent/task_status agent_id="backend-xxx" task_id="..."
agent/task_status agent_id="frontend-xxx" task_id="..."
```

### Pattern 3: Supervisor + Workers

An MCP host (or agent) acts as supervisor, spawning specialist agents dynamically:

```
# Host evaluates a complex task, decides to parallelize
workspace/create  name="big-refactor" project="myapp"

# Spawn a planning agent first
agent/spawn       workspace="big-refactor" template="crush" name="planner"
agent/message     agent_id="planner-xxx" message="Analyze the codebase and break this into subtasks: ..."

# Based on planner's SendMessageResponse, spawn worker agents
agent/spawn       workspace="big-refactor" template="crush" name="worker-1"
agent/spawn       workspace="big-refactor" template="crush" name="worker-2"
agent/spawn       workspace="big-refactor" template="crush" name="worker-3"

# Assign subtasks — each returns a Task for tracking
agent/task        agent_id="worker-1-xxx" message="Subtask 1: ..."
agent/task        agent_id="worker-2-xxx" message="Subtask 2: ..."
agent/task        agent_id="worker-3-xxx" message="Subtask 3: ..."

# Monitor via GetTask until all reach TASK_STATE_COMPLETED
```

### Pattern 4: External A2A Client (Direct + Proxied)

An external A2A client discovers and uses agents without MCP — purely via A2A v1.0 HTTP+JSON endpoints:

```bash
# Discover agents via ARP registry
curl http://arp-server:9099/a2a/agents
# → Array of AgentCard objects (with metadata.arp.direct_url)

# Send via proxied A2A (SendMessage through ARP)
curl -X POST http://arp-server:9099/a2a/agents/coder-abc123/message:send \
  -H "Content-Type: application/json" \
  -d '{
    "message": {
      "role": "ROLE_USER",
      "parts": [{ "text_part": { "text": "Fix the auth bug" } }]
    }
  }'
# → SendMessageResponse: { "task": { "id": "...", "status": { "state": "TASK_STATE_WORKING" } } }

# Or bypass ARP and talk directly (using metadata.arp.direct_url from AgentCard)
curl -X POST http://localhost:9100/message:send \
  -H "Content-Type: application/json" \
  -d '{
    "message": {
      "role": "ROLE_USER",
      "parts": [{ "text_part": { "text": "Fix the auth bug" } }]
    }
  }'

# Poll task status via GetTask
curl http://localhost:9100/tasks/task-abc123
# → Task: { "id": "task-abc123", "status": { "state": "TASK_STATE_COMPLETED" }, "artifacts": [...] }
```
