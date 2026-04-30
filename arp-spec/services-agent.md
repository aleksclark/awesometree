---
title: "ARP — Agent Lifecycle Service"
version: 0.4.0
created: 2026-04-06
updated: 2026-05-01
status: draft
tags: [arp, grpc, protobuf, a2a, agent-lifecycle, sendmessage, gettask]
---

# Agent Lifecycle Service

gRPC service for spawning, monitoring, messaging, and stopping A2A agents within workspaces. These RPCs form the core of ARP — they manage agent processes and bridge between gRPC control-plane calls and A2A v1.0 RPCs.

Agents are the bottom of the hierarchy: **Project → Workspace → Agent**. Each agent is an independent A2A-speaking process with its own port, `AgentCard`, and `context_id` space. Multiple agents can share one workspace directory.

## Service Definition

```protobuf
syntax = "proto3";

package arp.v1;

import "google/api/annotations.proto";
import "google/api/field_behavior.proto";
import "google/protobuf/struct.proto";
import "lf/a2a/v1/a2a.proto";

// AgentService manages the lifecycle of A2A agent instances within workspaces.
service AgentService {
  // SpawnAgent spawns a new A2A agent instance in a workspace.
  rpc SpawnAgent(SpawnAgentRequest) returns (AgentInstance) {
    option (google.api.http) = {
      post: "/v1/agents"
      body: "*"
    };
  }

  // ListAgents lists all agent instances, optionally filtered.
  rpc ListAgents(ListAgentsRequest) returns (ListAgentsResponse) {
    option (google.api.http) = {
      get: "/v1/agents"
    };
  }

  // GetAgentStatus returns detailed status of a specific agent instance.
  rpc GetAgentStatus(GetAgentStatusRequest) returns (AgentInstance) {
    option (google.api.http) = {
      get: "/v1/agents/{agent_id}"
    };
  }

  // SendAgentMessage sends an A2A SendMessage to an agent, proxied through ARP.
  rpc SendAgentMessage(SendAgentMessageRequest) returns (SendAgentMessageResponse) {
    option (google.api.http) = {
      post: "/v1/agents/{agent_id}/messages"
      body: "*"
    };
  }

  // CreateAgentTask sends a task to an agent via A2A SendMessage
  // and returns the resulting Task for async tracking.
  rpc CreateAgentTask(CreateAgentTaskRequest) returns (lf.a2a.v1.Task) {
    option (google.api.http) = {
      post: "/v1/agents/{agent_id}/tasks"
      body: "*"
    };
  }

  // GetAgentTaskStatus checks the status of a running A2A task via GetTask.
  rpc GetAgentTaskStatus(GetAgentTaskStatusRequest) returns (lf.a2a.v1.Task) {
    option (google.api.http) = {
      get: "/v1/agents/{agent_id}/tasks/{task_id}"
    };
  }

  // StopAgent gracefully stops an agent instance.
  rpc StopAgent(StopAgentRequest) returns (AgentInstance) {
    option (google.api.http) = {
      post: "/v1/agents/{agent_id}:stop"
      body: "*"
    };
  }

  // RestartAgent restarts an agent instance (stop + spawn with same configuration).
  rpc RestartAgent(RestartAgentRequest) returns (AgentInstance) {
    option (google.api.http) = {
      post: "/v1/agents/{agent_id}:restart"
      body: "*"
    };
  }
}
```

## Messages

### SpawnAgent

```protobuf
message SpawnAgentRequest {
  // Workspace to spawn agent in.
  string workspace = 1 [(google.api.field_behavior) = REQUIRED];

  // Agent template name (e.g., "crush", "claude-code", "reviewer").
  string template = 2 [(google.api.field_behavior) = REQUIRED];

  // Instance name override (default: template name).
  // Required when spawning multiple agents of the same template.
  string name = 3;

  // Additional environment variables for this instance.
  map<string, string> env = 4;

  // Initial task to send via A2A SendMessage (as a TextPart)
  // after agent reaches ready status.
  string prompt = 5;

  // Narrow the spawned agent's project scope (must be subset of caller's scope).
  // Omit to inherit caller's full scope. See identity-and-scopes.md.
  Scope scope = 6;

  // Permission level for spawned agent (must be ≤ caller's permission).
  // Omit to inherit caller's permission. See identity-and-scopes.md.
  Permission permission = 7;
}
```

**Returns:** `AgentInstance` message with both `direct_url` and `proxy_url`.

**Behavior:**
1. Validates caller's token — checks scope includes the workspace's project, and permission allows spawning
2. Allocates a port from the configured range (9100–9199)
3. Issues a child token to the new agent (scope ≤ caller's scope, permission ≤ caller's permission)
4. Starts the agent process with the template's `command`, setting `port_env` to the allocated port and `ARP_TOKEN` to the child token
5. Polls the `health_check.path` endpoint until it responds (or retries exhausted → status `AGENT_STATUS_ERROR`)
6. On health check pass, status transitions to `AGENT_STATUS_READY`
7. If `prompt` is provided, sends a `SendMessageRequest` containing a `Message` with `role: ROLE_USER` and a single `TextPart`

**Example:**

```bash
grpcurl -d '{
  "workspace": "feat-auth",
  "template": "crush",
  "name": "coder"
}' localhost:9099 arp.v1.AgentService/SpawnAgent

grpcurl -d '{
  "workspace": "feat-auth",
  "template": "crush",
  "name": "coder",
  "prompt": "Implement OAuth2 login"
}' localhost:9099 arp.v1.AgentService/SpawnAgent
```

### ListAgents

```protobuf
message ListAgentsRequest {
  // Filter by workspace name.
  string workspace = 1;

  // Filter by ARP agent status.
  AgentStatus status = 2;

  // Filter by template name.
  string template = 3;
}

message ListAgentsResponse {
  repeated AgentInstance agents = 1;
}
```

**Returns:** Array of `AgentInstance` messages (see [Data Model](overview.md#agentinstance)).

### GetAgentStatus

```protobuf
message GetAgentStatusRequest {
  // Agent instance ID.
  string agent_id = 1 [(google.api.field_behavior) = REQUIRED];
}
```

**Returns:** `AgentInstance` with full resolved `AgentCard` (enriched with `metadata.arp`) and health check details.

### SendAgentMessage

Send an A2A `SendMessage` to an agent, proxied through ARP.

```protobuf
message SendAgentMessageRequest {
  // Agent instance ID.
  string agent_id = 1 [(google.api.field_behavior) = REQUIRED];

  // Text content (becomes a TextPart in the A2A Message).
  string message = 2 [(google.api.field_behavior) = REQUIRED];

  // A2A Message.context_id for multi-turn conversations (omit to start new).
  string context_id = 3;

  // Wait for SendMessageResponse (default: true).
  // If false, returns immediately after sending.
  bool blocking = 4;
}

message SendAgentMessageResponse {
  // The A2A SendMessageResponse — either a task or a message.
  oneof result {
    lf.a2a.v1.Task task = 1;
    lf.a2a.v1.Message message = 2;
  }
}
```

**A2A mapping:** This RPC constructs and sends:

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

### CreateAgentTask

Send a task to an agent via A2A `SendMessage` and return the resulting `Task` for async tracking.

```protobuf
message CreateAgentTaskRequest {
  // Agent instance ID.
  string agent_id = 1 [(google.api.field_behavior) = REQUIRED];

  // Task description (becomes a TextPart).
  string message = 2 [(google.api.field_behavior) = REQUIRED];

  // A2A Message.context_id for continuing a conversation.
  string context_id = 3;
}
```

**Returns:** A2A `lf.a2a.v1.Task` message with `id`, `context_id`, and `status` (typically `TASK_STATE_SUBMITTED` or `TASK_STATE_WORKING`).

**Difference from `SendAgentMessage`:** `CreateAgentTask` always returns a `Task` for tracking (never a bare `Message`). If the agent's `SendMessageResponse` contains a `Message` instead of a `Task`, ARP wraps it in a synthetic completed `Task`.

### GetAgentTaskStatus

Check the status of a running A2A task via `GetTask`.

```protobuf
message GetAgentTaskStatusRequest {
  // Agent instance ID.
  string agent_id = 1 [(google.api.field_behavior) = REQUIRED];

  // A2A Task.id from CreateAgentTask.
  string task_id = 2 [(google.api.field_behavior) = REQUIRED];

  // GetTaskRequest.history_length — max recent messages to include (default: 10).
  int32 history_length = 3;
}
```

**Returns:** A2A `lf.a2a.v1.Task` with:
- `status.state` — a `TaskState` enum value (`TASK_STATE_WORKING`, `TASK_STATE_COMPLETED`, `TASK_STATE_FAILED`, etc.)
- `artifacts[]` — output `Artifact` messages, each containing `Part` items
- `history[]` — recent `Message` messages (capped by `history_length`)

**A2A mapping:** Calls `GET /tasks/{task_id}` on the agent's direct A2A endpoint with `history_length` parameter.

**Terminal states:** When `status.state` is one of `TASK_STATE_COMPLETED`, `TASK_STATE_FAILED`, `TASK_STATE_CANCELED`, or `TASK_STATE_REJECTED`, the task is done. No further updates will occur.

### StopAgent

Gracefully stop an agent instance.

```protobuf
message StopAgentRequest {
  // Agent instance ID.
  string agent_id = 1 [(google.api.field_behavior) = REQUIRED];

  // Grace period in milliseconds before force kill (default: 5000).
  int32 grace_period_ms = 2;
}
```

**Returns:** `AgentInstance` with final status.

**Behavior:**
1. Transitions agent status to `AGENT_STATUS_STOPPING`
2. For each task in `TASK_STATE_WORKING`, sends `CancelTask` (`POST /tasks/{id}:cancel`)
3. Sends SIGTERM to the agent process
4. Waits `grace_period_ms` for process exit
5. If still running, sends SIGKILL
6. Transitions agent status to `AGENT_STATUS_STOPPED`
7. Frees the allocated port

### RestartAgent

Restart an agent instance (stop + spawn with same configuration).

```protobuf
message RestartAgentRequest {
  // Agent instance ID.
  string agent_id = 1 [(google.api.field_behavior) = REQUIRED];
}
```

**Returns:** `AgentInstance` with new status after restart.

**Behavior:**
1. Calls `StopAgent` on the instance
2. Re-spawns with the same template, name, workspace, and env
3. A new port may be allocated (the `proxy_url` remains stable; `direct_url` may change)
4. Previous A2A `context_id` sessions are lost — the agent starts fresh

## gRPC Status Codes

| Condition | gRPC Status | Description |
|-----------|-------------|-------------|
| Agent not found | `NOT_FOUND` | Any RPC with unknown `agent_id` |
| Workspace not found | `NOT_FOUND` | SpawnAgent referencing unknown workspace |
| Task not found | `NOT_FOUND` | GetAgentTaskStatus with unknown `task_id` |
| Missing required field | `INVALID_ARGUMENT` | Required fields not provided |
| Agent not ready | `FAILED_PRECONDITION` | SendAgentMessage/CreateAgentTask to agent not in `READY` or `BUSY` |
| Permission denied | `PERMISSION_DENIED` | Token permission insufficient for operation |
| Scope violation | `PERMISSION_DENIED` | Agent's project not in caller's token scope |
| Session violation | `PERMISSION_DENIED` | `session`-scoped token targeting agent from different session |
