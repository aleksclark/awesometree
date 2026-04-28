---
title: "ARP — Identity Federation & Scopes"
version: 0.3.0
created: 2026-04-28
updated: 2026-04-28
status: draft
tags: [arp, auth, scopes, identity, federation, tokens]
---

# Identity Federation & Scopes

How ARP controls what callers and agents can see and do. Tokens carry scopes that define project access and permission level. Agents inherit scoped tokens from their spawner, creating a permission chain that flows downward.

## Core Concepts

### Tokens

Every authenticated caller to the ARP server presents a **token**. Tokens are opaque bearer strings issued by the ARP server (or an external identity provider via federation). Each token carries:

- **subject** — who this token represents (a user, an external system, or an agent)
- **scopes** — what projects this token can access
- **permission** — what level of control this token grants within those scopes

Tokens are presented via:
- `Authorization: Bearer <token>` header on A2A proxy endpoints
- Injected as an env var (`ARP_TOKEN`) into spawned agent processes
- Passed in MCP tool call context (for MCP hosts)

### Scopes

A scope is a boundary that determines **what entities a caller can see and act on**. Scopes are defined in terms of projects.

```typescript
type Scope =
  | "*"                   // Global: all projects, all workspaces, all agents
  | string[]              // Project list: specific named projects
```

A token scoped to `["myapp", "lib"]` can only interact with workspaces and agents belonging to those two projects. A token scoped to `"*"` has no project restrictions.

### Permissions

Permissions determine **what actions a caller can perform** within its scopes. There are three permission levels, ordered from most restrictive to least:

| Permission | Can do | Cannot do |
|------------|--------|-----------|
| `session` | Manage agents it spawned (or that were spawned in the same session). Create new workspaces/agents within scoped projects. | See or manage agents spawned by other sessions. List agents outside its session. |
| `project` | List, message, manage all agents in scoped projects. Create new workspaces/agents. | See or manage agents in projects outside its scope. Register/unregister projects. |
| `admin` | Everything `project` can do, plus register/unregister projects, issue tokens, manage ARP server configuration. | N/A (unrestricted within scope) |

### Session

A session is an implicit grouping of agents spawned by the same token during a single logical interaction. Every `agent/spawn` call tags the new agent with the spawner's `session_id`. Callers with `session` permission can only see and manage agents that share their `session_id`.

Sessions are created implicitly — the first `agent/spawn` from a token establishes a session if one doesn't exist. Sessions are identified by an opaque server-generated `session_id`.

## Data Model Additions

### Token

```typescript
interface Token {
  id: string;                      // Opaque token identifier
  subject: string;                 // Who this token represents
  scope: "*" | string[];           // Project scope: "*" for global, or list of project names
  permission: "session" | "project" | "admin";
  session_id?: string;             // Set after first agent/spawn; groups agents for session-scoped tokens
  issued_at: string;               // ISO 8601
  expires_at?: string;             // ISO 8601 (optional, tokens can be non-expiring)
  parent_token_id?: string;        // Token that spawned the agent holding this token (for chain tracking)
}
```

### AgentInstance (additions)

These fields are added to the existing `AgentInstance` type (see [Data Model](overview.md#agentinstance)):

```typescript
interface AgentInstance {
  // ... existing fields ...
  token_id: string;                // Token issued to this agent
  session_id: string;              // Session this agent belongs to
  spawned_by: string;              // Token ID of the caller that spawned this agent
}
```

## Permission Model

### How Tokens Flow

```
┌─────────────┐     token: scope=["myapp"], perm=project
│   Human /   │ ───────────────────────────────────────────→ ARP Server
│   MCP Host  │
└─────────────┘
       │
       │  agent/spawn workspace="feat" template="crush"
       ▼
┌─────────────┐     token: scope=["myapp"], perm=project, session_id=sess-1
│  Agent A    │ ←── ARP issues child token, injects as ARP_TOKEN
│  (crush)    │
└──────┬──────┘
       │
       │  Agent A calls ARP: agent/spawn workspace="feat" template="crush" name="helper"
       ▼
┌─────────────┐     token: scope=["myapp"], perm=project, session_id=sess-1
│  Agent B    │ ←── ARP issues grandchild token (same scope, same session)
│  (helper)   │
└─────────────┘
```

Key rules:

1. **Agents get child tokens.** When `agent/spawn` is called, ARP issues a new token to the spawned agent. The child token inherits the caller's scope and permission level (or lower — never higher).

2. **Scope can only narrow, never widen.** A token scoped to `["myapp"]` can spawn an agent with scope `["myapp"]` or narrower, but never `["myapp", "other"]`.

3. **Permission can only lower, never elevate.** A `project`-scoped token can issue `project` or `session` child tokens, but never `admin`.

4. **Session propagates.** Agents spawned by the same root token (or its descendants) share a `session_id`. This is how `session` permission works — it can see everything in the session tree.

5. **Direct A2A access bypasses ARP auth.** If a caller connects directly to an agent's port (direct access), ARP scopes do not apply. The agent itself may have its own auth (A2A `security_schemes`), but ARP's scope enforcement only applies to the proxied path and MCP tools.

### Scope Enforcement Per Tool

| Tool | `session` | `project` | `admin` |
|------|-----------|-----------|---------|
| `project/list` | Only scoped projects | Only scoped projects | All projects |
| `project/register` | ✗ Denied | ✗ Denied | ✓ Within scope |
| `project/unregister` | ✗ Denied | ✗ Denied | ✓ Within scope |
| `workspace/create` | ✓ Within scoped projects | ✓ Within scoped projects | ✓ Any project |
| `workspace/list` | Only workspaces with own-session agents | All in scoped projects | All |
| `workspace/get` | Only if workspace has own-session agents | ✓ Within scoped projects | ✓ Any |
| `workspace/destroy` | Only if all agents are own-session | ✓ Within scoped projects | ✓ Any |
| `agent/spawn` | ✓ Within scoped projects | ✓ Within scoped projects | ✓ Any project |
| `agent/list` | Only own-session agents | All in scoped projects | All |
| `agent/status` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `agent/message` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `agent/task` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `agent/task_status` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `agent/stop` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `agent/restart` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `agent/discover` | Only own-session agents (local) | All in scoped projects (local) | All (local) |
| Proxied A2A | Only own-session agents | ✓ Within scoped projects | ✓ Any |

### Scope Enforcement on Proxied A2A

When an A2A client sends a request through the proxy (e.g., `POST /a2a/agents/{agent_id}/message:send`), the ARP server:

1. Extracts the bearer token from the `Authorization` header
2. Resolves the target agent
3. Checks that the agent's project is within the token's scope
4. For `session` permission, checks that the agent's `session_id` matches the token's
5. If authorized, proxies the request to the agent's direct A2A endpoint
6. If denied, returns HTTP 403 with an A2A-compatible error

## Scenarios

### Scenario 1: Project-scoped agent manages peers

A human operator with `admin` scope spawns a lead agent with `project` permission. The lead agent can then see and manage all agents in that project — including ones it didn't spawn.

```
# Human (admin, scope=*)
agent/spawn workspace="refactor" template="crush" name="lead" \
  → ARP issues token: scope=["myapp"], perm=project

# Lead agent (project, scope=["myapp"])
agent/list workspace="refactor"
  → Sees ALL agents in "refactor" workspace (it's in project "myapp")

agent/spawn workspace="refactor" template="crush" name="worker-1"
  → Creates worker with token: scope=["myapp"], perm=project, session=lead's-session

agent/stop agent_id="some-other-agent-in-myapp"
  → Allowed — project permission lets it manage any agent in myapp
```

### Scenario 2: Session-scoped agent can only manage its tree

A token scoped to two projects with `session` permission launches an agent. That agent can create new agents in either project but can only see/manage agents in its own session.

```
# External system (session, scope=["frontend", "backend"])
agent/spawn workspace="api-v2" template="crush" name="coordinator"
  → ARP issues token: scope=["frontend","backend"], perm=session, session=sess-42

# Coordinator agent (session, scope=["frontend","backend"], session=sess-42)
agent/spawn workspace="api-v2" template="crush" name="api-worker"
  → Allowed — "backend" is in scope. Worker gets session=sess-42.

agent/spawn workspace="ui-v2" template="crush" name="ui-worker"
  → Allowed — "frontend" is in scope. Worker gets session=sess-42.

agent/list
  → Only sees: coordinator, api-worker, ui-worker (all session=sess-42)
  → Does NOT see agents spawned by other sessions, even in the same projects

agent/stop agent_id="some-agent-from-different-session"
  → 403 Denied — different session_id
```

### Scenario 3: Narrowing scope on delegation

A lead agent with broad scope delegates a subtask to a specialist agent with narrowed scope.

```
# Lead agent (project, scope=["myapp", "shared-lib"])
agent/spawn workspace="feat" template="crush" name="specialist" \
  scope=["shared-lib"]  # Narrow: only shared-lib, not myapp
  → Specialist gets token: scope=["shared-lib"], perm=project

# Specialist agent (project, scope=["shared-lib"])
agent/spawn workspace="lib-fix" template="crush" name="helper"
  → Allowed — "shared-lib" is in scope

workspace/list project="myapp"
  → 403 Denied — "myapp" is not in specialist's scope
```

### Scenario 4: Localhost bypass

An agent running locally can always connect directly to another agent's port, bypassing ARP auth entirely. This is by design — ARP scopes protect the **management plane**, not the **data plane**.

```
# Agent A knows Agent B is on port 9101 (from agent/spawn response)
curl -X POST http://localhost:9101/message:send ...
  → Works — direct A2A, no ARP auth involved

# Same request through ARP proxy requires valid token
curl -X POST http://arp:9099/a2a/agents/agent-b/message:send \
  -H "Authorization: Bearer <token>" ...
  → ARP enforces scope + permission
```

## Token Issuance

### Server-Issued Tokens

The ARP server can issue tokens directly:

```json
{
  "name": "token/create",
  "description": "Create an ARP token with specified scope and permission. Requires admin permission.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "subject": { "type": "string", "description": "Who this token represents" },
      "scope": {
        "description": "Project scope: '*' for global, or array of project names",
        "oneOf": [
          { "type": "string", "enum": ["*"] },
          { "type": "array", "items": { "type": "string" } }
        ]
      },
      "permission": { "type": "string", "enum": ["session", "project", "admin"] },
      "expires_in_seconds": { "type": "integer", "description": "Token TTL in seconds (optional, omit for non-expiring)" }
    },
    "required": ["subject", "scope", "permission"]
  }
}
```

### Child Token Issuance (on spawn)

When `agent/spawn` is called, ARP automatically:
1. Creates a child token with scope ≤ parent scope, permission ≤ parent permission
2. Sets `parent_token_id` to the caller's token ID
3. Sets `session_id` to the caller's session (or creates one if first spawn)
4. Injects the child token as `ARP_TOKEN` env var in the agent process
5. Records the `token_id` and `session_id` on the `AgentInstance`

The `agent/spawn` tool accepts an optional `scope` parameter to narrow the child's scope:

```json
{
  "scope": {
    "description": "Narrow the spawned agent's project scope (must be subset of caller's scope). Omit to inherit caller's full scope.",
    "oneOf": [
      { "type": "string", "enum": ["*"] },
      { "type": "array", "items": { "type": "string" } }
    ]
  },
  "permission": {
    "type": "string",
    "enum": ["session", "project"],
    "description": "Permission level for spawned agent (must be ≤ caller's permission). Omit to inherit caller's permission."
  }
}
```

### Identity Federation

ARP tokens can be issued by external identity providers. The ARP server validates external tokens via:

1. **OIDC (OpenID Connect)** — ARP acts as a relying party. External JWT tokens are validated against the provider's JWKS endpoint. Claims map to ARP scope/permission:
   - `arp:scope` claim → project scope
   - `arp:permission` claim → permission level
   - `sub` claim → subject

2. **A2A Security Schemes** — The A2A `AgentCard.security_schemes` field supports `openIdConnect`, `oauth2`, `apiKey`, and `http` auth. ARP's proxied endpoints honor these when present, and can map external A2A auth to internal ARP tokens.

Configuration:

```json
{
  "auth": {
    "mode": "local",
    "federation": [
      {
        "provider": "oidc",
        "issuer": "https://auth.example.com",
        "audience": "arp-server",
        "claim_mapping": {
          "scope": "arp:scope",
          "permission": "arp:permission"
        }
      }
    ],
    "localhost_admin": true
  }
}
```

When `localhost_admin` is true (default), connections from localhost are treated as `admin` with `scope: "*"` — matching the current awesometree behavior where the local user has full control.

## Design Rationale

**Why three permission levels?** Two isn't enough. Without `session`, every multi-agent system either gives agents full project access (dangerous) or requires manual per-agent ACLs (tedious). `session` is the natural default for autonomous agents — they can manage their own spawned sub-agents but can't interfere with other sessions. `project` is for lead/supervisor agents that need to coordinate across sessions. `admin` is for human operators and infrastructure.

**Why scope narrowing only?** This prevents privilege escalation. An agent can't grant itself or its children access to projects it doesn't have. The permission chain is monotonically decreasing, which means you can reason about the maximum blast radius of any agent by looking at its token.

**Why is direct access unscoped?** ARP manages the control plane (lifecycle, registry, routing). The data plane (agent-to-agent communication) is standard A2A. If you need data-plane auth, use A2A's own `security_schemes` on the agent's `AgentCard`. This separation keeps ARP simple and avoids re-inventing A2A's auth model.

**Why sessions rather than explicit groups?** Sessions are implicit — they emerge from the spawn tree. No configuration needed. The first `agent/spawn` from a token creates a session; all descendants join it. This matches how agents actually work: a supervisor spawns workers, and the workers should be able to coordinate with each other but not with unrelated agents.
