---
title: "ARP — Agent Lifecycle Tools"
version: 0.3.0
created: 2026-04-06
updated: 2026-04-28
status: draft
tags: [arp, mcp, a2a, agent-lifecycle, sendmessage, gettask]
---

# Agent Lifecycle Tools

MCP tools for spawning, monitoring, messaging, and stopping A2A agents within workspaces. These tools form the core of ARP — they manage agent processes and bridge between MCP tool calls and A2A v1.0 RPCs.

Agents are the bottom of the hierarchy: **Project → Workspace → Agent**. Each agent is an independent A2A-speaking process with its own port, `AgentCard`, and `context_id` space. Multiple agents can share one workspace directory.

## `agent/spawn`

Spawn a new agent instance in a workspace.

```json
{
  "name": "agent/spawn",
  "description": "Spawn a new A2A agent in an existing workspace. Each agent gets its own port, context_id space, and AgentCard. Multiple agents can coexist in one workspace.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "workspace": { "type": "string", "description": "Workspace to spawn agent in" },
      "template": { "type": "string", "description": "Agent template name (e.g., 'crush', 'claude-code', 'reviewer')" },
      "name": { "type": "string", "description": "Instance name override (default: template name). Required when spawning multiple agents of the same template." },
      "env": {
        "type": "object",
        "description": "Additional environment variables for this instance",
        "additionalProperties": { "type": "string" }
      },
      "prompt": { "type": "string", "description": "Initial task to send via A2A SendMessage (as a TextPart) after agent reaches ready status" },
      "scope": {
        "description": "Narrow the spawned agent's project scope (must be subset of caller's scope). Omit to inherit caller's full scope. See identity-and-scopes.md.",
        "oneOf": [
          { "type": "string", "enum": ["*"] },
          { "type": "array", "items": { "type": "string" } }
        ]
      },
      "permission": { "type": "string", "enum": ["session", "project"], "description": "Permission level for spawned agent (must be ≤ caller's permission). Omit to inherit. See identity-and-scopes.md." }
    },
    "required": ["workspace", "template"]
  }
}
```

**Returns:** `AgentInstance` object with both `direct_url` and `proxy_url`.

**Behavior:**
1. Validates caller's token — checks scope includes the workspace's project, and permission allows spawning
2. Allocates a port from the configured range (9100–9199)
3. Issues a child token to the new agent (scope ≤ caller's scope, permission ≤ caller's permission)
4. Starts the agent process with the template's `command`, setting `port_env` to the allocated port and `ARP_TOKEN` to the child token
5. Polls the `health_check.path` endpoint until it responds (or retries exhausted → status `error`)
6. On health check pass, status transitions to `ready`
7. If `prompt` is provided, sends a `SendMessageRequest` containing a `Message` with `role: ROLE_USER` and a single `TextPart`

**Example:**

```
agent/spawn  workspace="feat-auth" template="crush" name="coder"
agent/spawn  workspace="feat-auth" template="crush" name="reviewer"
agent/spawn  workspace="feat-auth" template="crush" name="coder" prompt="Implement OAuth2 login"
```

## `agent/list`

List all agent instances, optionally filtered.

```json
{
  "name": "agent/list",
  "description": "List agent instances across all workspaces or filtered by workspace/status.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "workspace": { "type": "string", "description": "Filter by workspace name" },
      "status": { "type": "string", "enum": ["starting", "ready", "busy", "error", "stopping", "stopped"], "description": "Filter by ARP agent status" },
      "template": { "type": "string", "description": "Filter by template name" }
    }
  }
}
```

**Returns:** Array of `AgentInstance` objects (see [Data Model](overview.md#agentinstance)).

## `agent/status`

Get detailed status of a specific agent instance.

```json
{
  "name": "agent/status",
  "description": "Get full status of an agent instance including health, resolved AgentCard, both access URLs, and resource usage.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "agent_id": { "type": "string", "description": "Agent instance ID" }
    },
    "required": ["agent_id"]
  }
}
```

**Returns:** `AgentInstance` with full resolved `AgentCard` (enriched with `metadata.arp`) and health check details.

## `agent/message`

Send an A2A `SendMessage` to an agent, proxied through ARP.

```json
{
  "name": "agent/message",
  "description": "Send an A2A SendMessage to an agent (proxied through ARP). Constructs a SendMessageRequest with a ROLE_USER Message containing a TextPart. For long-running tasks, use agent/task instead.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "agent_id": { "type": "string", "description": "Agent instance ID" },
      "message": { "type": "string", "description": "Text content (becomes a TextPart in the A2A Message)" },
      "context_id": { "type": "string", "description": "A2A Message.context_id for multi-turn conversations (omit to start new)" },
      "blocking": { "type": "boolean", "description": "Wait for SendMessageResponse (default: true). If false, returns immediately after sending." }
    },
    "required": ["agent_id", "message"]
  }
}
```

**Returns:** The A2A `SendMessageResponse` — either a `Message` (direct reply) or a `Task` (with `id`, `status`, and optionally `artifacts`).

**A2A mapping:** This tool constructs and sends:

```json
{
  "message": {
    "role": "ROLE_USER",
    "parts": [{ "text_part": { "text": "<user's message text>" } }],
    "context_id": "<context_id if provided>"
  }
}
```

via `POST /message:send` on the agent's direct A2A endpoint.

## `agent/task`

Send a task to an agent via A2A `SendMessage` and return the resulting `Task` for async tracking.

```json
{
  "name": "agent/task",
  "description": "Send a message to an agent via A2A SendMessage and return the Task for async tracking. Use agent/task_status to poll via A2A GetTask. Best for long-running operations.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "agent_id": { "type": "string", "description": "Agent instance ID" },
      "message": { "type": "string", "description": "Task description (becomes a TextPart)" },
      "context_id": { "type": "string", "description": "A2A Message.context_id for continuing a conversation" }
    },
    "required": ["agent_id", "message"]
  }
}
```

**Returns:** A2A `Task` object with `id`, `context_id`, and `status` (typically `TASK_STATE_SUBMITTED` or `TASK_STATE_WORKING`).

**Difference from `agent/message`:** `agent/task` always returns a `Task` for tracking (never a bare `Message`). If the agent's `SendMessageResponse` contains a `Message` instead of a `Task`, ARP wraps it in a synthetic completed `Task`.

## `agent/task_status`

Check the status of a running A2A task via `GetTask`.

```json
{
  "name": "agent/task_status",
  "description": "Get the current status of an A2A Task via GetTask. Returns TaskState, any Artifacts produced, and recent Message history.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "agent_id": { "type": "string", "description": "Agent instance ID" },
      "task_id": { "type": "string", "description": "A2A Task.id from agent/task" },
      "history_length": { "type": "integer", "description": "GetTaskRequest.history_length — max recent messages to include (default: 10)" }
    },
    "required": ["agent_id", "task_id"]
  }
}
```

**Returns:** A2A `Task` with:
- `status.state` — a `TaskState` enum value (`TASK_STATE_WORKING`, `TASK_STATE_COMPLETED`, `TASK_STATE_FAILED`, etc.)
- `artifacts[]` — output `Artifact` objects, each containing `Part` items
- `history[]` — recent `Message` objects (capped by `history_length`)

**A2A mapping:** Calls `GET /tasks/{task_id}` on the agent's direct A2A endpoint with `history_length` parameter.

**Terminal states:** When `status.state` is one of `TASK_STATE_COMPLETED`, `TASK_STATE_FAILED`, `TASK_STATE_CANCELED`, or `TASK_STATE_REJECTED`, the task is done. No further updates will occur.

## `agent/stop`

Gracefully stop an agent instance.

```json
{
  "name": "agent/stop",
  "description": "Gracefully stop an agent. Sends SIGTERM, waits for grace period, then SIGKILL. Running A2A Tasks in WORKING state are canceled via CancelTask before shutdown.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "agent_id": { "type": "string", "description": "Agent instance ID" },
      "grace_period_ms": { "type": "integer", "description": "Grace period in milliseconds before force kill (default: 5000)" }
    },
    "required": ["agent_id"]
  }
}
```

**Behavior:**
1. Transitions agent status to `stopping`
2. For each task in `TASK_STATE_WORKING`, sends `CancelTask` (`POST /tasks/{id}:cancel`)
3. Sends SIGTERM to the agent process
4. Waits `grace_period_ms` for process exit
5. If still running, sends SIGKILL
6. Transitions agent status to `stopped`
7. Frees the allocated port

## `agent/restart`

Restart an agent instance (stop + spawn with same configuration).

```json
{
  "name": "agent/restart",
  "description": "Restart an agent instance. Preserves the same template and configuration. A new port may be assigned. A2A context_id state is lost.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "agent_id": { "type": "string", "description": "Agent instance ID" }
    },
    "required": ["agent_id"]
  }
}
```

**Behavior:**
1. Calls `agent/stop` on the instance
2. Re-spawns with the same template, name, workspace, and env
3. A new port may be allocated (the `proxy_url` remains stable; `direct_url` may change)
4. Previous A2A `context_id` sessions are lost — the agent starts fresh
