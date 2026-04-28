---
title: "ARP — Overview"
version: 0.3.0
created: 2026-04-06
updated: 2026-04-28
status: draft
tags: [arp, a2a, mcp, agent-lifecycle, multi-agent, awesometree]
---

# Agent Registry Protocol (ARP) — Overview

An MCP server that manages the full lifecycle of AI agents within workspaces and provides a unified registry for A2A agent discovery and communication. Fills the gap between MCP (agent-to-tool) and A2A (agent-to-agent): neither protocol defines how to **create, start, stop, or destroy** agent instances. ARP does.

## Problem Statement

The current agent protocol landscape has a lifecycle gap:

- **MCP** connects models to tools. No concept of agent management.
- **A2A** connects agents to each other. Assumes agents already exist — no spawn, no lifecycle.

Every team building multi-agent systems reinvents agent lifecycle management. ARP standardizes it as an MCP server, making agent management available as tools to any MCP-capable host (Claude, Cursor, Zed, VS Code, custom agents). It also exposes managed agents as a standard A2A registry, so any A2A client can discover and communicate with them.

## Design Principles

1. **Workspaces are the unit of isolation.** A workspace is a directory (typically a git worktree) where one or more agents operate. Agents within a workspace share the filesystem but have independent sessions.

2. **A2A is the wire protocol.** Every managed agent speaks A2A v1.0. The ARP server creates agents, and those agents are standard A2A agents — discoverable via `AgentCard`, communicable via `SendMessage` / `SendStreamingMessage`.

3. **MCP is the control plane.** Agent lifecycle operations (spawn, stop, restart) are MCP tools. This means any MCP host can manage agents without custom integrations.

4. **Multi-agent workspaces.** A workspace can host multiple agents (e.g., a coding agent + a review agent + a test agent). Each gets its own port, A2A `context_id` space, and `AgentCard`.

5. **Two paths to every agent.** Clients can talk to agents directly via the agent's A2A URL, or proxied through the ARP server. Direct is lower latency; proxied adds routing, discovery, and lifecycle awareness.

6. **Backend-agnostic.** The spec defines the interface, not the implementation. Backends can be local processes, Docker containers, remote VMs, or cloud services.

7. **Scoped authority flows downward.** Every caller authenticates with a token carrying project scopes and a permission level. Agents inherit tokens from their spawner — scope can only narrow, permission can only lower. This creates a monotonically decreasing privilege chain with no escalation path. See [Identity & Scopes](identity-and-scopes.md).

## Tool Groups

| Group | Tools | Spec |
|-------|-------|------|
| Project Management | `project/list`, `project/register`, `project/unregister` | [tools-project.md](tools-project.md) |
| Workspace Management | `workspace/create`, `workspace/list`, `workspace/get`, `workspace/destroy` | [tools-workspace.md](tools-workspace.md) |
| Agent Lifecycle | `agent/spawn`, `agent/list`, `agent/status`, `agent/message`, `agent/task`, `agent/task_status`, `agent/stop`, `agent/restart` | [tools-agent.md](tools-agent.md) |
| Discovery & Patterns | `agent/discover`, MCP resources, MCP prompts | [tools-discovery.md](tools-discovery.md) |
| Identity & Scopes | `token/create`, scope enforcement, federation | [identity-and-scopes.md](identity-and-scopes.md) |

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
│                       MCP HOST                              │
│  (Claude, Cursor, Zed, custom agent, etc.)                 │
│                                                             │
│  Lifecycle tools:   workspace/create, agent/spawn, ...      │
│  Communication:     agent/message, agent/task, ...          │
└──────────┬─────────────────────────────────────────────────┘
           │ MCP (stdio or HTTP)
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
- When the client already knows the agent URL (from `agent/spawn` or `agent/discover`)
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

Registry endpoints (ARP-specific):
  GET  /a2a/agents                          List all AgentCards for ready agents
  POST /a2a/route/message:send              Route SendMessage by skill/capability match
  GET  /a2a/discover                        Filtered discovery (by capability, workspace, status)
```

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
4. **By skill** — matches `AgentSkill.tags` on the agent's `AgentCard.skills[]`; routes to the first agent with status `ready` whose skills match

If multiple agents match, the proxy prefers agents with status `ready` over `busy`.

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

## Data Model

### Project

A project is a code repository with configuration for how agents should operate within it.

```typescript
interface Project {
  name: string;                    // Unique project identifier
  repo: string;                    // Path to the git repository
  branch?: string;                 // Default branch (default: "main")
  agents?: AgentTemplate[];        // Agent templates available for this project
  context?: {
    files?: string[];              // Context files fed to agents
    repo_includes?: string[];      // Glob patterns for repo context
    max_bytes?: number;            // Max context size
  };
}
```

### Workspace

A workspace is an isolated working directory (typically a git worktree) that hosts one or more agents.

```typescript
interface Workspace {
  name: string;                    // Unique workspace identifier
  project: string;                 // Parent project name
  dir: string;                     // Absolute path to workspace directory
  status: "active" | "inactive";   // Workspace lifecycle state
  agents: AgentInstance[];         // Agents running in this workspace
  created_at: string;              // ISO 8601 timestamp
  metadata?: Record<string, any>;  // Extensible metadata
}
```

### AgentTemplate

Defines how to spawn a particular type of agent. Templates are configured per-project or globally. The `a2a` field populates the `AgentCard` that ARP generates or enriches for this agent.

```typescript
interface AgentTemplate {
  name: string;                    // Template name (e.g., "crush", "claude-code", "reviewer")
  command: string;                 // Command to start the agent process
  port_env?: string;               // Env var name for port assignment (e.g., "A2A_PORT")
  health_check?: {
    path: string;                  // Health endpoint (e.g., "/.well-known/agent-card.json")
    interval_ms?: number;          // Check interval (default: 5000)
    timeout_ms?: number;           // Timeout per check (default: 3000)
    retries?: number;              // Retries before marking unhealthy (default: 3)
  };
  env?: Record<string, string>;    // Additional environment variables
  capabilities?: string[];         // Declared capabilities (e.g., ["code", "review", "test"])
  a2a?: {
    // Fields map to A2A AgentCard:
    name?: string;                 // AgentCard.name override
    description?: string;          // AgentCard.description
    skills?: AgentSkill[];         // AgentCard.skills[]
    input_modes?: string[];        // AgentCard.default_input_modes[]
    output_modes?: string[];       // AgentCard.default_output_modes[]
    capabilities?: {               // AgentCard.capabilities (AgentCapabilities)
      streaming?: boolean;
      push_notifications?: boolean;
      state_transition_history?: boolean;
    };
  };
}

// Maps directly to A2A AgentSkill message
interface AgentSkill {
  id: string;                      // AgentSkill.id
  name: string;                    // AgentSkill.name
  description: string;             // AgentSkill.description
  tags?: string[];                 // AgentSkill.tags[]
  examples?: string[];             // AgentSkill.examples[]
  input_modes?: string[];          // AgentSkill.input_modes[] (overrides AgentCard defaults)
  output_modes?: string[];         // AgentSkill.output_modes[] (overrides AgentCard defaults)
}
```

### AgentInstance

A running agent within a workspace.

```typescript
interface AgentInstance {
  id: string;                      // Unique instance identifier (server-generated)
  template: string;                // Template name used to spawn
  workspace: string;               // Parent workspace name
  status: AgentStatus;             // Current lifecycle state (ARP-specific, not A2A TaskState)
  port: number;                    // Assigned port number
  direct_url: string;              // Direct A2A endpoint (e.g., "http://localhost:9100")
  proxy_url: string;               // Proxied A2A endpoint (e.g., "http://localhost:9099/a2a/agents/{id}")
  pid?: number;                    // Process ID (if local backend)
  context_id?: string;             // Current A2A context_id for multi-turn conversations
  a2a_agent_card?: AgentCard;      // Resolved A2A AgentCard (with ARP metadata)
  token_id: string;                // ARP token issued to this agent (see Identity & Scopes)
  session_id: string;              // Session this agent belongs to (see Identity & Scopes)
  spawned_by: string;              // Token ID of the caller that spawned this agent
  started_at: string;              // ISO 8601 timestamp
  metadata?: Record<string, any>;  // Extensible metadata
}
```

```typescript
// ARP lifecycle states — distinct from A2A TaskState which tracks task execution
type AgentStatus =
  | "starting"      // Process launched, waiting for health check
  | "ready"         // Health check passed, accepting A2A SendMessage
  | "busy"          // Currently processing (has tasks in WORKING state)
  | "error"         // Health check failed or process crashed
  | "stopping"      // Graceful shutdown initiated (SIGTERM sent)
  | "stopped";      // Process terminated
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

Note: ARP `AgentStatus` tracks the **process** lifecycle. A2A `TaskState` tracks **task** execution within the agent. An agent in `ready` status may have completed tasks (`TASK_STATE_COMPLETED`) in its history. An agent transitions to `busy` when it has at least one task in `TASK_STATE_WORKING`.

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
      "a2a": {
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
- Expose both the existing REST API and the new MCP tools
- Serve the A2A registry and proxy endpoints from the existing HTTP server (port 9099)
- Maintain backward compatibility with the current single-agent-per-workspace model

### A2A

ARP is both an A2A client (sending `SendMessageRequest` to managed agents) and an A2A server (exposing `AgentCard` registry and proxying A2A RPCs for managed agents). It fills the lifecycle gap that A2A explicitly does not cover: A2A defines how to *talk* to agents; ARP defines how to *create and manage* them. The two are complementary — ARP creates agents that speak standard A2A v1.0.

### MCP

ARP is an MCP server. The lifecycle tools (spawn, stop, restart) are MCP tools callable by any MCP host. This means Claude, Cursor, Zed, or any custom agent can manage a fleet of A2A agents through standard MCP tool calls — no custom API integration required.
