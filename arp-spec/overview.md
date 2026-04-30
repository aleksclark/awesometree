---
title: "ARP — Overview"
version: 0.4.0
created: 2026-04-06
updated: 2026-05-01
status: draft
tags: [arp, a2a, grpc, protobuf, agent-lifecycle, multi-agent, awesometree]
---

# Agent Registry Protocol (ARP) — Overview

A gRPC service that manages the full lifecycle of AI agents within workspaces and provides a unified registry for A2A agent discovery and communication. Fills the gap between the control plane (how you **create, start, stop, and destroy** agent instances) and A2A (how agents **talk to each other**). ARP is the control plane; A2A is the wire protocol.

## Problem Statement

The current agent protocol landscape has a lifecycle gap:

- **A2A** connects agents to each other. Assumes agents already exist — no spawn, no lifecycle.
- **No standard control plane** exists for agent lifecycle management.

Every team building multi-agent systems reinvents agent lifecycle management. ARP standardizes it as a gRPC service with full protobuf definitions, making agent management available to any gRPC client — CLI tools, orchestrators, dashboards, or other agents. It also exposes managed agents as a standard A2A registry, so any A2A client can discover and communicate with them.

## Design Principles

1. **Workspaces are the unit of isolation.** A workspace is a directory (typically a git worktree) where one or more agents operate. Agents within a workspace share the filesystem but have independent sessions.

2. **A2A is the wire protocol.** Every managed agent speaks A2A v1.0. The ARP server creates agents, and those agents are standard A2A agents — discoverable via `AgentCard`, communicable via `SendMessage` / `SendStreamingMessage`.

3. **gRPC is the control plane.** Agent lifecycle operations (spawn, stop, restart) are gRPC RPCs defined in protobuf. Any gRPC client can manage agents without custom integrations. HTTP/JSON access is available via gRPC-Web transcoding with `google.api.http` annotations on every RPC.

4. **Multi-agent workspaces.** A workspace can host multiple agents (e.g., a coding agent + a review agent + a test agent). Each gets its own port, A2A `context_id` space, and `AgentCard`.

5. **Two paths to every agent.** Clients can talk to agents directly via the agent's A2A URL, or proxied through the ARP server. Direct is lower latency; proxied adds routing, discovery, and lifecycle awareness.

6. **Backend-agnostic.** The spec defines the interface, not the implementation. Backends can be local processes, Docker containers, remote VMs, or cloud services.

7. **Scoped authority flows downward.** Every caller authenticates with a token passed via gRPC metadata (`authorization` key). Tokens carry project scopes and a permission level. Agents inherit tokens from their spawner — scope can only narrow, permission can only lower. This creates a monotonically decreasing privilege chain with no escalation path. See [Identity & Scopes](identity-and-scopes.md).

## gRPC Service Groups

| Group | Service | RPCs | Spec |
|-------|---------|------|------|
| Project Management | `ProjectService` | `ListProjects`, `RegisterProject`, `UnregisterProject` | [services-project.md](services-project.md) |
| Workspace Management | `WorkspaceService` | `CreateWorkspace`, `ListWorkspaces`, `GetWorkspace`, `DestroyWorkspace` | [services-workspace.md](services-workspace.md) |
| Agent Lifecycle | `AgentService` | `SpawnAgent`, `ListAgents`, `GetAgentStatus`, `SendAgentMessage`, `CreateAgentTask`, `GetAgentTaskStatus`, `StopAgent`, `RestartAgent` | [services-agent.md](services-agent.md) |
| Discovery & Routing | `DiscoveryService` | `DiscoverAgents`, `WatchAgent`, `WatchWorkspace` | [services-discovery.md](services-discovery.md) |
| Identity & Scopes | `TokenService` | `CreateToken`; scope enforcement, federation | [identity-and-scopes.md](identity-and-scopes.md) |

## A2A v1.0 Reference

This spec builds on the A2A v1.0 protocol (`lf.a2a.v1`). Key types and RPCs referenced throughout:

### Types Used

| A2A Type | Description | Key Fields |
|----------|-------------|------------|
| `AgentCard` | Agent's discovery document | `name`, `description`, `version`, `supported_interfaces[]`, `capabilities`, `skills[]`, `default_input_modes[]`, `default_output_modes[]`, `security_schemes`, `metadata` |
| `AgentInterface` | Service endpoint binding | `url`, `transport` (JSONRPC, GRPC, HTTP_JSON) |
| `AgentCapabilities` | What the agent supports | `streaming`, `push_notifications`, `state_transition_history`, `extensions[]` |
| `AgentSkill` | A capability the agent offers | `id`, `name`, `description`, `tags[]`, `examples[]`, `input_modes[]`, `output_modes[]` |
| `Task` | A tracked unit of work | `id`, `context_id`, `status`, `history[]`, `artifacts[]`, `metadata` |
| `TaskStatus` | Current state + timestamp | `state` (TaskState enum), `timestamp`, `message` |
| `TaskState` | Lifecycle enum | `SUBMITTED`, `WORKING`, `INPUT_REQUIRED`, `COMPLETED`, `CANCELED`, `FAILED`, `REJECTED`, `AUTH_REQUIRED` |
| `Message` | A communication unit | `message_id`, `role` (USER, AGENT), `parts[]`, `task_id`, `context_id`, `reference_task_ids[]`, `metadata` |
| `Part` | Content within a message | oneof: `TextPart` {`text`}, `FilePart` {`file`}, `DataPart` {`data`} |
| `Artifact` | Output produced by a task | `artifact_id`, `name`, `description`, `parts[]`, `metadata` |
| `SendMessageRequest` | Request to send a message | `message` (Message), `configuration` (SendMessageConfiguration) |
| `SendMessageResponse` | Response from agent | oneof: `task` (Task) or `message` (Message) |
| `StreamResponse` | Streaming response event | oneof: `task` (Task) or `message` (Message) |

### RPCs Used

| A2A RPC | HTTP Binding | Description |
|---------|-------------|-------------|
| `SendMessage` | `POST /message:send` | Send a message, get sync response |
| `SendStreamingMessage` | `POST /message:stream` | Send a message, get SSE stream |
| `GetTask` | `GET /tasks/{id}` | Get current task state |
| `ListTasks` | `GET /tasks` | List tasks with filters |
| `CancelTask` | `POST /tasks/{id}:cancel` | Cancel a running task |
| `SubscribeToTask` | `GET /tasks/{id}:subscribe` | SSE stream of task updates |
| `GetExtendedAgentCard` | `GET /extendedAgentCard` | Authenticated extended card |

### Discovery

| Endpoint | Description |
|----------|-------------|
| `GET /.well-known/agent-card.json` | Public `AgentCard` discovery (RFC 8615) |
| `GET /extendedAgentCard` | Authenticated `AgentCard` with additional capabilities/skills |

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│                     gRPC CLIENTS                            │
│  (CLI tools, orchestrators, dashboards, other agents)       │
│                                                             │
│  Lifecycle RPCs:  SpawnAgent, StopAgent, RestartAgent, ...  │
│  Communication:   SendAgentMessage, CreateAgentTask, ...    │
└──────────┬─────────────────────────────────────────────────┘
           │ gRPC (H2) or gRPC-Web (HTTP/1.1 transcoding)
           ▼
┌────────────────────────────────────────────────────────────┐
│                    ARP SERVER                                │
│              (Agent Registry Protocol)                       │
│                                                             │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────┐   │
│  │  Workspace   │  │    Agent     │  │   A2A Registry  │   │
│  │  Manager     │  │  Supervisor  │  │   & Proxy       │   │
│  └──────┬──────┘  └──────┬───────┘  └────────┬────────┘   │
│         │                │                     │            │
│         │      ┌─────────▼─────────┐           │            │
│         │      │  Process Backend  │           │            │
│         │      │  (local/docker/   │           │            │
│         │      │   remote/cloud)   │           │            │
│         │      └─────────┬─────────┘           │            │
└─────────┼────────────────┼─────────────────────┼────────────┘
          │                │                     │
          ▼                ▼                     ▼
    ┌──────────┐    ┌──────────────┐    ┌────────────────┐
    │Workspace │    │ Agent        │    │ A2A Endpoints   │
    │Directory │    │ Processes    │    │                 │
    │(worktree)│    │              │    │ Direct:         │
    │          │    │ :9100 crush  │    │   agent:9100    │
    │          │    │ :9101 claude │    │                 │
    │          │    │ :9102 review │    │ Proxied:        │
    └──────────┘    └──────────────┘    │   arp:9099/a2a/ │
                                        └────────────────┘
```

## Two Interfaces to Agents

Every agent managed by ARP is accessible via two paths. Both speak A2A v1.0.

### Direct Access

The agent process binds to its own port and exposes A2A v1.0 HTTP+JSON endpoints directly. Clients connect to the agent's URL without going through the ARP server.

```
Agent URL: http://localhost:{agent_port}

A2A v1.0 HTTP+JSON Endpoints:
  GET  /.well-known/agent-card.json       AgentCard discovery (RFC 8615)
  POST /message:send                       SendMessage → SendMessageResponse
  POST /message:stream                     SendStreamingMessage → stream StreamResponse
  GET  /tasks/{id}                         GetTask → Task
  GET  /tasks                              ListTasks → ListTasksResponse
  POST /tasks/{id}:cancel                  CancelTask → Task
  GET  /tasks/{id}:subscribe               SubscribeToTask → stream StreamResponse
  GET  /extendedAgentCard                  GetExtendedAgentCard → AgentCard
```

**When to use direct access:**
- Lowest latency — no proxy hop
- Agent-to-agent communication where both agents are local
- When the client already knows the agent URL (from `SpawnAgent` response or `DiscoverAgents`)
- Long-lived SSE connections (`SubscribeToTask`, `SendStreamingMessage`)

**Limitations:**
- Client must know the specific port
- No lifecycle awareness — if the agent restarts on a new port, direct URLs break
- No routing by name or skill — client must resolve agent identity itself
- No aggregated discovery across workspaces

### Proxied Access (via ARP Server)

The ARP server proxies A2A requests to the appropriate agent, routing by agent ID, name, or skill. The proxy adds lifecycle awareness: if an agent is restarting, the proxy can queue or retry.

```
ARP Server URL: http://localhost:{arp_port}

Proxied A2A endpoints (per-agent, mirrors A2A v1.0 HTTP bindings):
  GET  /a2a/agents/{agent_id}/.well-known/agent-card.json  AgentCard
  POST /a2a/agents/{agent_id}/message:send                  SendMessage
  POST /a2a/agents/{agent_id}/message:stream                SendStreamingMessage
  GET  /a2a/agents/{agent_id}/tasks/{task_id}               GetTask
  GET  /a2a/agents/{agent_id}/tasks                         ListTasks
  POST /a2a/agents/{agent_id}/tasks/{task_id}:cancel        CancelTask
  GET  /a2a/agents/{agent_id}/tasks/{task_id}:subscribe     SubscribeToTask
  GET  /a2a/agents/{agent_id}/extendedAgentCard             GetExtendedAgentCard

Registry endpoints (ARP-specific, also exposed via gRPC-Web HTTP transcoding):
  GET  /a2a/agents                          List all AgentCards for ready agents
  POST /a2a/route/message:send              Route SendMessage by skill/capability match
  GET  /a2a/discover                        Filtered discovery (by capability, workspace, status)
```

These HTTP endpoints are defined as `google.api.http` annotations on the gRPC RPCs in the protobuf definitions. See the individual service specs for the full bindings.

**When to use proxied access:**
- External A2A clients discovering agents for the first time
- Routing by capability/skill rather than specific agent ID
- Resilience — proxy handles agent restarts, port changes, retries
- Aggregated view across all workspaces
- When the ARP server is the only known endpoint (e.g., remote access)

### Routing Behavior

The ARP proxy resolves agents in this order:

1. **By agent_id** — exact match to a managed agent instance
2. **By name** — matches against the `AgentCard.name` field
3. **By workspace/name** — `{workspace}/{instance_name}` composite key
4. **By skill** — matches `AgentSkill.tags` on the agent's `AgentCard.skills[]`; routes to the first agent with status `AGENT_STATUS_READY` whose skills match

If multiple agents match, the proxy prefers agents with status `AGENT_STATUS_READY` over `AGENT_STATUS_BUSY`.

### Agent Card Enrichment

When serving `AgentCard` through the proxy, the ARP server enriches it with lifecycle metadata in the `metadata` field (a `google.protobuf.Struct` in the A2A proto, a JSON object over HTTP+JSON):

```json
{
  "name": "crush-coder",
  "description": "Crush AI coding assistant",
  "version": "1.0.0",
  "supported_interfaces": [
    {
      "url": "http://localhost:9099/a2a/agents/coder-abc123",
      "transport": "HTTP_JSON"
    }
  ],
  "capabilities": {
    "streaming": true,
    "push_notifications": false,
    "state_transition_history": false
  },
  "skills": [
    {
      "id": "code",
      "name": "Code",
      "description": "Write, review, and debug code",
      "tags": ["coding", "debugging", "refactoring"],
      "examples": ["Implement OAuth2 login flow", "Fix the null pointer in auth.rs"]
    }
  ],
  "default_input_modes": ["text/plain"],
  "default_output_modes": ["text/plain"],
  "metadata": {
    "arp": {
      "agent_id": "coder-abc123",
      "workspace": "feat-auth",
      "project": "myapp",
      "template": "crush",
      "status": "ready",
      "direct_url": "http://localhost:9100",
      "started_at": "2026-04-06T10:30:00Z"
    }
  }
}
```

The `supported_interfaces[0].url` in the enriched card points to the **proxied** endpoint. The `metadata.arp.direct_url` gives clients the option to bypass the proxy and connect to the agent's `AgentInterface` directly.

## Protobuf Data Model

All ARP data types are defined as proto3 messages in `arp/v1/arp.proto`. The following are the core message types; full protobuf definitions are in the individual service specs.

### Project

A project is a code repository with configuration for how agents should operate within it.

```protobuf
message Project {
  string name = 1 [(google.api.field_behavior) = REQUIRED];     // Unique project identifier
  string repo = 2 [(google.api.field_behavior) = REQUIRED];     // Path to the git repository
  string branch = 3;                                             // Default branch (default: "main")
  repeated AgentTemplate agents = 4;                             // Agent templates available
  ProjectContext context = 5;                                    // Context configuration
}

message ProjectContext {
  repeated string files = 1;           // Context files fed to agents
  repeated string repo_includes = 2;   // Glob patterns for repo context
  int64 max_bytes = 3;                 // Max context size
}
```

### Workspace

A workspace is an isolated working directory (typically a git worktree) that hosts one or more agents.

```protobuf
message Workspace {
  string name = 1 [(google.api.field_behavior) = REQUIRED];     // Unique workspace identifier
  string project = 2 [(google.api.field_behavior) = REQUIRED];  // Parent project name
  string dir = 3;                                                // Absolute path to workspace directory
  WorkspaceStatus status = 4;                                    // Workspace lifecycle state
  repeated AgentInstance agents = 5;                             // Agents running in this workspace
  google.protobuf.Timestamp created_at = 6;                     // Creation timestamp
  google.protobuf.Struct metadata = 7;                          // Extensible metadata
}

enum WorkspaceStatus {
  WORKSPACE_STATUS_UNSPECIFIED = 0;
  WORKSPACE_STATUS_ACTIVE = 1;
  WORKSPACE_STATUS_INACTIVE = 2;
}
```

### AgentTemplate

Defines how to spawn a particular type of agent. Templates are configured per-project or globally. The `a2a_card_config` field populates the `AgentCard` that ARP generates or enriches for this agent.

```protobuf
message AgentTemplate {
  string name = 1 [(google.api.field_behavior) = REQUIRED];     // Template name (e.g., "crush", "claude-code")
  string command = 2 [(google.api.field_behavior) = REQUIRED];  // Command to start the agent process
  string port_env = 3;                                           // Env var name for port assignment
  HealthCheckConfig health_check = 4;                            // Health check configuration
  map<string, string> env = 5;                                   // Additional environment variables
  repeated string capabilities = 6;                              // Declared capabilities
  A2ACardConfig a2a_card_config = 7;                             // Fields for AgentCard generation
}

message HealthCheckConfig {
  string path = 1;            // Health endpoint (e.g., "/.well-known/agent-card.json")
  int32 interval_ms = 2;     // Check interval (default: 5000)
  int32 timeout_ms = 3;      // Timeout per check (default: 3000)
  int32 retries = 4;         // Retries before marking unhealthy (default: 3)
}

message A2ACardConfig {
  string name = 1;                                    // AgentCard.name override
  string description = 2;                             // AgentCard.description
  repeated lf.a2a.v1.AgentSkill skills = 3;           // AgentCard.skills[]
  repeated string input_modes = 4;                    // AgentCard.default_input_modes[]
  repeated string output_modes = 5;                   // AgentCard.default_output_modes[]
  lf.a2a.v1.AgentCapabilities capabilities = 6;       // AgentCard.capabilities
}
```

### AgentInstance

A running agent within a workspace.

```protobuf
message AgentInstance {
  string id = 1;                                      // Unique instance identifier (server-generated)
  string template = 2;                                // Template name used to spawn
  string workspace = 3;                               // Parent workspace name
  AgentStatus status = 4;                             // Current lifecycle state
  int32 port = 5;                                     // Assigned port number
  string direct_url = 6;                              // Direct A2A endpoint
  string proxy_url = 7;                               // Proxied A2A endpoint via ARP
  int32 pid = 8;                                      // Process ID (if local backend)
  string context_id = 9;                              // Current A2A context_id
  lf.a2a.v1.AgentCard a2a_agent_card = 10;            // Resolved A2A AgentCard (with ARP metadata)
  string token_id = 11;                               // ARP token issued to this agent
  string session_id = 12;                             // Session this agent belongs to
  string spawned_by = 13;                             // Token ID of the caller that spawned this agent
  google.protobuf.Timestamp started_at = 14;          // Start timestamp
  google.protobuf.Struct metadata = 15;               // Extensible metadata
}

// ARP lifecycle states — distinct from A2A TaskState which tracks task execution
enum AgentStatus {
  AGENT_STATUS_UNSPECIFIED = 0;
  AGENT_STATUS_STARTING = 1;     // Process launched, waiting for health check
  AGENT_STATUS_READY = 2;        // Health check passed, accepting A2A SendMessage
  AGENT_STATUS_BUSY = 3;         // Currently processing (has tasks in WORKING state)
  AGENT_STATUS_ERROR = 4;        // Health check failed or process crashed
  AGENT_STATUS_STOPPING = 5;     // Graceful shutdown initiated (SIGTERM sent)
  AGENT_STATUS_STOPPED = 6;      // Process terminated
}
```

### AgentStatus State Machine

```
             spawn
               │
               ▼
          ┌─────────┐
          │starting  │──── health check fails (retries exhausted) ──→ error
          └────┬─────┘                                                  │
               │ health check passes                                    │
               ▼                                                  restart│
          ┌─────────┐                                                   │
     ┌──→ │  ready   │ ←──────── task reaches terminal state ──┐       │
     │    └────┬─────┘                                          │       │
     │         │ A2A SendMessage received                       │       │
     │         ▼                                                │       │
     │    ┌─────────┐                                           │       │
     │    │  busy    │ ──── crash ──→ error ────── restart ─────┘       │
     │    └────┬─────┘                                                  │
     │         │     (task → COMPLETED / FAILED / CANCELED)             │
     │         └────────────────────────────────────────────────────────┘
     │
     │    terminate
     │         │
     │         ▼
     │    ┌─────────┐         ┌─────────┐
     └──→ │stopping  │ ──────→│ stopped  │
          └─────────┘  grace  └─────────┘
                       period
```

Note: ARP `AgentStatus` tracks the **process** lifecycle. A2A `TaskState` tracks **task** execution within the agent. An agent in `AGENT_STATUS_READY` may have completed tasks (`TASK_STATE_COMPLETED`) in its history. An agent transitions to `AGENT_STATUS_BUSY` when it has at least one task in `TASK_STATE_WORKING`.

## Configuration

The ARP server is configured via a JSON file:

```json
{
  "port": 9099,
  "port_range": { "min": 9100, "max": 9199 },
  "state_dir": "~/.config/arp/",
  "templates": {
    "crush": {
      "command": "crush serve",
      "port_env": "A2A_PORT",
      "health_check": { "path": "/.well-known/agent-card.json", "interval_ms": 5000 },
      "a2a_card_config": {
        "name": "Crush",
        "description": "AI coding assistant",
        "skills": [
          {
            "id": "code",
            "name": "Code",
            "description": "Write, review, and debug code",
            "tags": ["coding"]
          }
        ],
        "capabilities": {
          "streaming": true,
          "push_notifications": false,
          "state_transition_history": false
        }
      }
    },
    "claude-code": {
      "command": "claude --a2a",
      "port_env": "A2A_PORT",
      "health_check": { "path": "/.well-known/agent-card.json", "interval_ms": 5000 }
    }
  },
  "process": {
    "grace_period_ms": 5000,
    "restart_delay_ms": 2000,
    "auto_restart": true,
    "max_restart_attempts": 3
  },
  "a2a": {
    "registry_enabled": true,
    "proxy_enabled": true
  },
  "auth": {
    "mode": "local",
    "localhost_admin": true,
    "federation": []
  }
}
```

## Relationship to Existing Systems

### Awesometree

ARP generalizes awesometree's workspace and agent supervisor model. An awesometree-backed implementation would:
- Use awesometree's `Manager` for workspace creation (git worktrees, WM tags)
- Extend the agent `Supervisor` to manage multiple agents per workspace (currently one)
- Expose both the existing REST API and the new gRPC service
- Serve the A2A registry and proxy endpoints from the existing HTTP server (port 9099)
- Maintain backward compatibility with the current single-agent-per-workspace model

### A2A

ARP is both an A2A client (sending `SendMessageRequest` to managed agents) and an A2A server (exposing `AgentCard` registry and proxying A2A RPCs for managed agents). It fills the lifecycle gap that A2A explicitly does not cover: A2A defines how to *talk* to agents; ARP defines how to *create and manage* them. The two are complementary — ARP creates agents that speak standard A2A v1.0.

### gRPC + HTTP Transcoding

ARP's control plane is a set of gRPC services defined in protobuf. Every RPC carries `google.api.http` annotations, so the same server can serve both native gRPC clients (high-performance, streaming) and HTTP/JSON clients (via gRPC-Web transcoding or an Envoy/grpc-gateway sidecar). This mirrors the pattern used by A2A v1.0 itself, which defines its RPCs in protobuf with HTTP bindings.
