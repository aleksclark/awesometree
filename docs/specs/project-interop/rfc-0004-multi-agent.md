# RFC-0004: Multi-Agent Coordination

**Status**: Draft / Future \
**Audience**: Orchestrator implementors, agent host implementors

> **Note**: This document describes features planned for a future version
> of the spec. The data structures defined here are included in version 1
> of the project definition schema as OPTIONAL fields to enable
> forward-compatible experimentation. Implementations MUST NOT depend on
> these features being stable.

## 1. Overview

When multiple agents collaborate on a single project — for example, one
agent designing an API while another implements it — they need:

- **Scoped capabilities**: different tools and context per agent role
- **Session awareness**: knowing which agents are active and what they
  are working on
- **Shared state**: a common scratchpad for task queues, decisions, and
  artifacts

This document specifies the `agents` field of a project definition and
the future shared-state and session protocols.

## 2. The `agents` Object

```json
{
  "agents": {
    "maxConcurrent": 3,
    "roles": {
      "architect": {
        "description": "High-level design and code review",
        "toolOverrides": {
          "<server-id>": {
            "allow": ["github_*", "linear_*"],
            "deny": ["*_delete_*", "*_merge_*"]
          }
        },
        "contextOverrides": {
          "files": ["architecture-decisions.md"]
        }
      },
      "implementer": {
        "description": "Feature implementation and testing",
        "toolOverrides": {
          "<server-id>": {
            "allow": ["github_*", "postgres_*"]
          }
        }
      }
    }
  }
}
```

### 2.1 Field Reference

| Field | Type | REQUIRED | Description |
|-------|------|----------|-------------|
| `maxConcurrent` | `integer` | OPTIONAL | Maximum number of simultaneous agent sessions. Advisory. |
| `roles` | `object` | OPTIONAL | Named role definitions. |

### 2.2 Role Fields

| Field | Type | REQUIRED | Description |
|-------|------|----------|-------------|
| `description` | `string` | RECOMMENDED | Human-readable description of the role's purpose. |
| `toolOverrides` | `object` | OPTIONAL | Per-server tool scoping overrides. Same schema as `tools` in RFC-0002. |
| `contextOverrides` | `object` | OPTIONAL | Context configuration overrides. Same schema as `context` in RFC-0003. |

## 3. Role Resolution

When an agent operates under a role, its effective tool scoping and
context configuration MUST be resolved as follows:

### 3.1 Tool Override Resolution

```
1. Start with the project-level tools (from the merged project definition)
2. For each server-id in the role's toolOverrides:
   a. IF the role defines allow → REPLACE the project-level allow for that server
   b. IF the role defines deny → APPEND to the project-level deny for that server
   c. IF the role defines defaults → MERGE with project-level defaults (role wins)
```

The rationale for REPLACE on allow but APPEND on deny:
- Roles typically **narrow** a project's tool set (allow is restrictive)
- Roles MUST NOT circumvent project-level denials (deny is additive)

### 3.2 Context Override Resolution

```
1. Start with the project-level context (from the merged project definition)
2. IF the role defines contextOverrides.files:
   THEN REPLACE the project-level files list
3. IF the role defines contextOverrides.repoIncludes:
   THEN REPLACE the project-level repoIncludes list
4. IF the role defines contextOverrides.maxBytes:
   THEN use the role's value
```

Roles MAY restrict an agent's context view to only the files relevant
to its purpose, reducing noise and context usage.

## 4. Sessions

> **Status**: Experimental. This section defines a data format for
> runtime session tracking. No implementation requirements are placed
> on conforming v1 implementations.

### 4.1 Session Object

A session represents a runtime binding of one agent to one project.

```json
{
  "id": "sess_01JK...",
  "agent": "claude-opus-4",
  "role": "implementer",
  "started": "2026-03-03T10:00:00Z",
  "status": "active",
  "workingOn": "Implement rate limiter in pkg/ratelimit",
  "heartbeat": "2026-03-03T10:05:00Z"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique session identifier. SHOULD be a ULID or UUID. |
| `agent` | `string` | Identifier of the agent (model name, host name, etc.). |
| `role` | `string` | Role name from the project definition, or empty. |
| `started` | `string` | ISO 8601 timestamp. |
| `status` | `string` | One of `"active"`, `"idle"`, `"completed"`, `"failed"`. |
| `workingOn` | `string` | Human-readable description of current task. |
| `heartbeat` | `string` | ISO 8601 timestamp of last activity. |

### 4.2 Session Store

Sessions SHOULD be stored at:

```
$XDG_CONFIG_HOME/project-interop/state/<project-name>/sessions/
```

Each session is a separate JSON file named `<session-id>.json`. This
enables concurrent read/write by multiple agent processes without
file-level locking (each agent writes only its own session file).

### 4.3 Session Lifecycle

```
[none] ──attach──▶ [active] ──idle──▶ [idle]
                    [active] ──complete──▶ [completed]
                    [active] ──fail──▶ [failed]
                    [idle] ──resume──▶ [active]
```

- Agent hosts SHOULD write a session file on attach
- Agent hosts SHOULD update `heartbeat` periodically (RECOMMENDED: every
  60 seconds)
- Agent hosts SHOULD delete the session file on clean detach
- Session files with a `heartbeat` older than 5 minutes MAY be
  considered stale and cleaned up by any implementation

### 4.4 Concurrency Control

If `maxConcurrent` is set and the number of active/idle sessions equals
or exceeds the limit, new `attach` operations SHOULD be rejected with an
error indicating the project is at capacity.

This is advisory — implementations MAY choose to warn rather than reject.

## 5. Shared State

> **Status**: Placeholder. The structures below sketch the direction for
> a future shared-state protocol. They are NOT part of the v1 spec.

### 5.1 State Namespaces

```json
{
  "sharedState": {
    "backend": "file",
    "namespaces": {
      "tasks": { "schema": "task-queue" },
      "decisions": { "schema": "append-log" },
      "artifacts": { "schema": "kv-store" }
    }
  }
}
```

| Backend | Description |
|---------|-------------|
| `file` | JSON files in `state/<project>/data/`. Suitable for low-concurrency. |
| `sqlite` | SQLite database in `state/<project>/`. Better concurrency. |

### 5.2 Task Queue Schema

A task queue enables agents to claim and complete work items:

```json
{
  "id": "task_01...",
  "title": "Implement /api/users endpoint",
  "assignee": "sess_01JK...",
  "status": "in_progress",
  "created": "2026-03-03T10:00:00Z",
  "updated": "2026-03-03T10:05:00Z"
}
```

### 5.3 Append Log Schema

A decision log provides an ordered, immutable record:

```json
{
  "id": "dec_01...",
  "author": "sess_01JK...",
  "timestamp": "2026-03-03T10:05:00Z",
  "content": "Chose token bucket over leaky bucket for rate limiting"
}
```

### 5.4 KV Store Schema

An artifact registry provides named key-value storage for build outputs,
test results, and other ephemeral data shared between agents.

## 6. MCP Exposure

Future implementations SHOULD expose session and state data as MCP
resources:

| URI | Description |
|-----|-------------|
| `project://<name>/agents` | List of active sessions. |
| `project://<name>/agents/<session-id>` | Single session detail. |
| `project://<name>/state/<namespace>` | Shared state namespace contents. |

And as MCP tools (via a project-aware tool proxy or standalone server):

| Tool | Description |
|------|-------------|
| `project_agent_attach` | Register a new session. |
| `project_agent_detach` | Remove a session. |
| `project_agent_heartbeat` | Update session heartbeat. |
| `project_task_claim` | Claim a task from the queue. |
| `project_task_complete` | Mark a task as done. |
| `project_decision_append` | Add a decision to the log. |
