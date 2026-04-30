---
title: "ARP — Discovery, Routing & Patterns"
version: 0.4.0
created: 2026-04-06
updated: 2026-05-01
status: draft
tags: [arp, grpc, protobuf, a2a, discovery, agentcard, patterns]
---

# Discovery, Routing & Patterns

gRPC service for discovering agents, server-streaming RPCs for real-time monitoring, and multi-agent patterns showing how the service groups compose.

## Service Definition

```protobuf
syntax = "proto3";

package arp.v1;

import "google/api/annotations.proto";
import "google/api/field_behavior.proto";
import "lf/a2a/v1/a2a.proto";

// DiscoveryService provides agent discovery, routing, and real-time monitoring.
service DiscoveryService {
  // DiscoverAgents discovers available agents across workspaces or on the network.
  // Returns AgentCards from managed workspaces (local) or by probing
  // /.well-known/agent-card.json at given URLs (network).
  rpc DiscoverAgents(DiscoverAgentsRequest) returns (DiscoverAgentsResponse) {
    option (google.api.http) = {
      get: "/v1/discover"
    };
  }

  // WatchAgent streams real-time status updates for an agent instance.
  // The server sends an AgentEvent whenever the agent's status, port,
  // or AgentCard changes. Replaces the old MCP resource subscription model.
  rpc WatchAgent(WatchAgentRequest) returns (stream AgentEvent) {
    option (google.api.http) = {
      get: "/v1/agents/{agent_id}:watch"
    };
  }

  // WatchWorkspace streams real-time updates for a workspace.
  // The server sends a WorkspaceEvent whenever an agent is spawned,
  // stopped, or changes status within the workspace.
  rpc WatchWorkspace(WatchWorkspaceRequest) returns (stream WorkspaceEvent) {
    option (google.api.http) = {
      get: "/v1/workspaces/{workspace_name}:watch"
    };
  }
}
```

## Messages

### DiscoverAgents

```protobuf
enum DiscoveryScope {
  DISCOVERY_SCOPE_UNSPECIFIED = 0;
  DISCOVERY_SCOPE_LOCAL = 1;    // Managed agents only
  DISCOVERY_SCOPE_NETWORK = 2;  // Probe URLs for AgentCards
}

message DiscoverAgentsRequest {
  // Search scope: LOCAL for managed agents only,
  // NETWORK probes URLs for AgentCards (default: LOCAL).
  DiscoveryScope scope = 1;

  // Filter by AgentSkill.tags (e.g., "code", "review", "test").
  string capability = 2;

  // Base URLs to probe for /.well-known/agent-card.json (for NETWORK scope).
  repeated string urls = 3;
}

message DiscoverAgentsResponse {
  repeated lf.a2a.v1.AgentCard agent_cards = 1;
}
```

**Local scope:** Returns `AgentCard` for every managed agent with status `AGENT_STATUS_READY` or `AGENT_STATUS_BUSY`. Cards are enriched with `metadata.arp` (see [Agent Card Enrichment](overview.md#agent-card-enrichment)).

**Network scope:** For each URL in `urls`, issues `GET {url}/.well-known/agent-card.json` (A2A discovery per RFC 8615). Returns successfully fetched `AgentCard` messages. Non-managed agents will not have `metadata.arp`.

**Capability filtering:** When `capability` is set, only returns agents whose `AgentCard.skills[]` contains at least one `AgentSkill` with a matching `tags[]` entry.

### WatchAgent

Server-streaming RPC for real-time agent monitoring. Replaces the old subscription-based resource model.

```protobuf
message WatchAgentRequest {
  // Agent instance ID.
  string agent_id = 1 [(google.api.field_behavior) = REQUIRED];
}

message AgentEvent {
  // The event type.
  AgentEventType event_type = 1;

  // Current agent instance state.
  AgentInstance agent = 2;

  // The agent's resolved A2A AgentCard (included on CARD_UPDATED events).
  lf.a2a.v1.AgentCard agent_card = 3;
}

enum AgentEventType {
  AGENT_EVENT_TYPE_UNSPECIFIED = 0;
  AGENT_EVENT_TYPE_STATUS_CHANGED = 1;  // starting→ready, ready→busy, etc.
  AGENT_EVENT_TYPE_CARD_UPDATED = 2;    // AgentCard changed (restart, port change)
  AGENT_EVENT_TYPE_STOPPED = 3;         // Agent process terminated
}
```

**Stream behavior:**
- On connection, sends an initial `STATUS_CHANGED` event with current state
- Subsequent events sent on status changes (`AGENT_STATUS_STARTING` → `AGENT_STATUS_READY`, `AGENT_STATUS_READY` → `AGENT_STATUS_BUSY`, etc.)
- `CARD_UPDATED` sent when the agent restarts (port may change) or the AgentCard is updated
- `STOPPED` sent when the agent process terminates; stream closes after this event

### WatchWorkspace

Server-streaming RPC for real-time workspace monitoring.

```protobuf
message WatchWorkspaceRequest {
  // Workspace name.
  string workspace_name = 1 [(google.api.field_behavior) = REQUIRED];
}

message WorkspaceEvent {
  // The event type.
  WorkspaceEventType event_type = 1;

  // Full workspace state.
  Workspace workspace = 2;

  // The agent involved in this event (for AGENT_* events).
  AgentInstance agent = 3;
}

enum WorkspaceEventType {
  WORKSPACE_EVENT_TYPE_UNSPECIFIED = 0;
  WORKSPACE_EVENT_TYPE_AGENT_SPAWNED = 1;         // New agent spawned
  WORKSPACE_EVENT_TYPE_AGENT_STATUS_CHANGED = 2;  // Agent status changed
  WORKSPACE_EVENT_TYPE_AGENT_STOPPED = 3;          // Agent stopped
  WORKSPACE_EVENT_TYPE_WORKSPACE_DESTROYED = 4;    // Workspace destroyed; stream closes
}
```

**Stream behavior:**
- On connection, sends an initial `AGENT_SPAWNED` event for each existing agent
- Subsequent events sent as agents are spawned, change status, or stop
- `WORKSPACE_DESTROYED` sent when the workspace is destroyed; stream closes after this event

## HTTP Transcoding for A2A Registry Endpoints

The proxied A2A registry endpoints are exposed as HTTP transcoding bindings. These are **not** part of the gRPC `DiscoveryService` — they are standard A2A HTTP endpoints served by the ARP server's HTTP layer alongside the gRPC-Web transcoded endpoints:

```
Registry endpoints (ARP-specific):
  GET  /a2a/agents                          List all AgentCards for ready agents
  POST /a2a/route/message:send              Route SendMessage by skill/capability match
  GET  /a2a/discover                        Filtered discovery (by capability, workspace, status)
```

These endpoints return A2A-native JSON payloads (not gRPC-transcoded). See [Proxied Access](overview.md#proxied-access-via-arp-server) for full details.

## Multi-Agent Workspace Patterns

These patterns show how the gRPC services compose to solve real workflows. All examples use `grpcurl` for illustration; any gRPC client works.

### Pattern 1: Coder + Reviewer

Two agents in one workspace — one writes code, the other reviews it.

```bash
# Create workspace
grpcurl -d '{"name": "feat-auth", "project": "myapp"}' \
  localhost:9099 arp.v1.WorkspaceService/CreateWorkspace

# Spawn agents
grpcurl -d '{"workspace": "feat-auth", "template": "crush", "name": "coder"}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent

grpcurl -d '{"workspace": "feat-auth", "template": "crush", "name": "reviewer"}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent

# Send task to coder
grpcurl -d '{"agent_id": "coder-xxx", "message": "Implement OAuth2 login flow"}' \
  localhost:9099 arp.v1.AgentService/SendAgentMessage

# ... poll via GetAgentTaskStatus until Task.status.state == TASK_STATE_COMPLETED ...

# Send review task
grpcurl -d '{"agent_id": "reviewer-xxx", "message": "Review the changes in this workspace"}' \
  localhost:9099 arp.v1.AgentService/SendAgentMessage
```

### Pattern 2: Parallel Implementation

Multiple agents work on different parts of a codebase simultaneously.

```bash
# Create workspace
grpcurl -d '{"name": "refactor", "project": "myapp"}' \
  localhost:9099 arp.v1.WorkspaceService/CreateWorkspace

# Spawn parallel workers
grpcurl -d '{"workspace": "refactor", "template": "crush", "name": "backend"}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent

grpcurl -d '{"workspace": "refactor", "template": "crush", "name": "frontend"}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent

# Assign tasks
grpcurl -d '{"agent_id": "backend-xxx", "message": "Refactor the API layer to use GraphQL"}' \
  localhost:9099 arp.v1.AgentService/CreateAgentTask

grpcurl -d '{"agent_id": "frontend-xxx", "message": "Update React components for the new GraphQL API"}' \
  localhost:9099 arp.v1.AgentService/CreateAgentTask

# Poll both via GetAgentTaskStatus until Task.status.state is terminal
grpcurl -d '{"agent_id": "backend-xxx", "task_id": "..."}' \
  localhost:9099 arp.v1.AgentService/GetAgentTaskStatus

grpcurl -d '{"agent_id": "frontend-xxx", "task_id": "..."}' \
  localhost:9099 arp.v1.AgentService/GetAgentTaskStatus
```

### Pattern 3: Supervisor + Workers

A gRPC client (or agent) acts as supervisor, spawning specialist agents dynamically:

```bash
# Create workspace
grpcurl -d '{"name": "big-refactor", "project": "myapp"}' \
  localhost:9099 arp.v1.WorkspaceService/CreateWorkspace

# Spawn a planning agent first
grpcurl -d '{"workspace": "big-refactor", "template": "crush", "name": "planner"}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent

grpcurl -d '{"agent_id": "planner-xxx", "message": "Analyze the codebase and break this into subtasks: ..."}' \
  localhost:9099 arp.v1.AgentService/SendAgentMessage

# Based on planner's response, spawn worker agents
grpcurl -d '{"workspace": "big-refactor", "template": "crush", "name": "worker-1"}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent

grpcurl -d '{"workspace": "big-refactor", "template": "crush", "name": "worker-2"}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent

# Assign subtasks — each returns a Task for tracking
grpcurl -d '{"agent_id": "worker-1-xxx", "message": "Subtask 1: ..."}' \
  localhost:9099 arp.v1.AgentService/CreateAgentTask

grpcurl -d '{"agent_id": "worker-2-xxx", "message": "Subtask 2: ..."}' \
  localhost:9099 arp.v1.AgentService/CreateAgentTask

# Monitor via WatchAgent (server-streaming)
grpcurl -d '{"agent_id": "worker-1-xxx"}' \
  localhost:9099 arp.v1.DiscoveryService/WatchAgent
```

### Pattern 4: External A2A Client (Direct + Proxied)

An external A2A client discovers and uses agents without gRPC — purely via A2A v1.0 HTTP+JSON endpoints:

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

## gRPC Status Codes

| Condition | gRPC Status | Description |
|-----------|-------------|-------------|
| Agent not found | `NOT_FOUND` | WatchAgent with unknown `agent_id` |
| Workspace not found | `NOT_FOUND` | WatchWorkspace with unknown workspace name |
| Network probe failed | `UNAVAILABLE` | DiscoverAgents NETWORK scope URL unreachable |
| Permission denied | `PERMISSION_DENIED` | Token permission insufficient |
| Scope violation | `PERMISSION_DENIED` | Target not in caller's token scope |
