---
title: "ARP — Identity Federation & Scopes"
version: 0.4.0
created: 2026-04-28
updated: 2026-05-01
status: draft
tags: [arp, grpc, protobuf, auth, scopes, identity, federation, tokens]
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
- gRPC metadata key `authorization` with value `Bearer <token>` on all ARP gRPC RPCs
- `Authorization: Bearer <token>` HTTP header on A2A proxy endpoints and gRPC-Web transcoded requests
- Injected as an env var (`ARP_TOKEN`) into spawned agent processes

### Scopes

A scope is a boundary that determines **what entities a caller can see and act on**. Scopes are defined in terms of projects.

```protobuf
message Scope {
  // If true, this is a global scope covering all projects.
  bool global = 1;

  // Specific project names (ignored if global is true).
  repeated string projects = 2;
}
```

A token scoped to `projects: ["myapp", "lib"]` can only interact with workspaces and agents belonging to those two projects. A token with `global: true` has no project restrictions.

### Permissions

Permissions determine **what actions a caller can perform** within its scopes. There are three permission levels, ordered from most restrictive to least:

```protobuf
enum Permission {
  PERMISSION_UNSPECIFIED = 0;
  PERMISSION_SESSION = 1;
  PERMISSION_PROJECT = 2;
  PERMISSION_ADMIN = 3;
}
```

| Permission | Can do | Cannot do |
|------------|--------|-----------|
| `SESSION` | Manage agents it spawned (or that were spawned in the same session). Create new workspaces/agents within scoped projects. | See or manage agents spawned by other sessions. List agents outside its session. |
| `PROJECT` | List, message, manage all agents in scoped projects. Create new workspaces/agents. | See or manage agents in projects outside its scope. Register/unregister projects. |
| `ADMIN` | Everything `PROJECT` can do, plus register/unregister projects, issue tokens, manage ARP server configuration. | N/A (unrestricted within scope) |

### Session

A session is an implicit grouping of agents spawned by the same token during a single logical interaction. Every `SpawnAgent` call tags the new agent with the spawner's `session_id`. Callers with `SESSION` permission can only see and manage agents that share their `session_id`.

Sessions are created implicitly — the first `SpawnAgent` from a token establishes a session if one doesn't exist. Sessions are identified by an opaque server-generated `session_id`.

## Protobuf Data Model

### Token

```protobuf
message Token {
  string id = 1;                                       // Opaque token identifier
  string subject = 2;                                  // Who this token represents
  Scope scope = 3;                                     // Project scope
  Permission permission = 4;                           // Permission level
  string session_id = 5;                               // Set after first SpawnAgent
  google.protobuf.Timestamp issued_at = 6;             // When issued
  google.protobuf.Timestamp expires_at = 7;            // Optional expiry
  string parent_token_id = 8;                          // Token that spawned the agent holding this token
}
```

### AgentInstance (additions)

These fields are part of the `AgentInstance` message (see [Data Model](overview.md#agentinstance)):

```protobuf
message AgentInstance {
  // ... existing fields ...
  string token_id = 11;       // Token issued to this agent
  string session_id = 12;     // Session this agent belongs to
  string spawned_by = 13;     // Token ID of the caller that spawned this agent
}
```

## Token Service

```protobuf
// TokenService manages ARP token issuance.
service TokenService {
  // CreateToken creates an ARP token with specified scope and permission.
  // Requires admin permission.
  rpc CreateToken(CreateTokenRequest) returns (CreateTokenResponse) {
    option (google.api.http) = {
      post: "/v1/tokens"
      body: "*"
    };
  }
}

message CreateTokenRequest {
  // Who this token represents.
  string subject = 1 [(google.api.field_behavior) = REQUIRED];

  // Project scope.
  Scope scope = 2 [(google.api.field_behavior) = REQUIRED];

  // Permission level.
  Permission permission = 3 [(google.api.field_behavior) = REQUIRED];

  // Token TTL in seconds (optional, omit for non-expiring).
  int32 expires_in_seconds = 4;
}

message CreateTokenResponse {
  // The issued token metadata.
  Token token = 1;

  // The opaque bearer string to use in gRPC metadata / HTTP Authorization header.
  string bearer_token = 2;
}
```

## Permission Model

### How Tokens Flow

```
┌─────────────┐     gRPC metadata: authorization = "Bearer <token>"
│   Human /   │     token: scope={projects:["myapp"]}, perm=PROJECT
│   CLI       │ ───────────────────────────────────────────→ ARP Server
└─────────────┘
       │
       │  SpawnAgent(workspace="feat", template="crush")
       ▼
┌─────────────┐     token: scope={projects:["myapp"]}, perm=PROJECT, session_id=sess-1
│  Agent A    │ ←── ARP issues child token, injects as ARP_TOKEN
│  (crush)    │
└──────┬──────┘
       │
       │  Agent A calls ARP: SpawnAgent(workspace="feat", template="crush", name="helper")
       ▼
┌─────────────┐     token: scope={projects:["myapp"]}, perm=PROJECT, session_id=sess-1
│  Agent B    │ ←── ARP issues grandchild token (same scope, same session)
│  (helper)   │
└─────────────┘
```

Key rules:

1. **Agents get child tokens.** When `SpawnAgent` is called, ARP issues a new token to the spawned agent. The child token inherits the caller's scope and permission level (or lower — never higher).

2. **Scope can only narrow, never widen.** A token scoped to `["myapp"]` can spawn an agent with scope `["myapp"]` or narrower, but never `["myapp", "other"]`.

3. **Permission can only lower, never elevate.** A `PROJECT`-scoped token can issue `PROJECT` or `SESSION` child tokens, but never `ADMIN`.

4. **Session propagates.** Agents spawned by the same root token (or its descendants) share a `session_id`. This is how `SESSION` permission works — it can see everything in the session tree.

5. **Direct A2A access bypasses ARP auth.** If a caller connects directly to an agent's port (direct access), ARP scopes do not apply. The agent itself may have its own auth (A2A `security_schemes`), but ARP's scope enforcement only applies to the proxied path and gRPC RPCs.

### Scope Enforcement Per RPC

| RPC | `SESSION` | `PROJECT` | `ADMIN` |
|-----|-----------|-----------|---------|
| `ListProjects` | Only scoped projects | Only scoped projects | All projects |
| `RegisterProject` | ✗ `PERMISSION_DENIED` | ✗ `PERMISSION_DENIED` | ✓ Within scope |
| `UnregisterProject` | ✗ `PERMISSION_DENIED` | ✗ `PERMISSION_DENIED` | ✓ Within scope |
| `CreateWorkspace` | ✓ Within scoped projects | ✓ Within scoped projects | ✓ Any project |
| `ListWorkspaces` | Only workspaces with own-session agents | All in scoped projects | All |
| `GetWorkspace` | Only if workspace has own-session agents | ✓ Within scoped projects | ✓ Any |
| `DestroyWorkspace` | Only if all agents are own-session | ✓ Within scoped projects | ✓ Any |
| `SpawnAgent` | ✓ Within scoped projects | ✓ Within scoped projects | ✓ Any project |
| `ListAgents` | Only own-session agents | All in scoped projects | All |
| `GetAgentStatus` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `SendAgentMessage` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `CreateAgentTask` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `GetAgentTaskStatus` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `StopAgent` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `RestartAgent` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `DiscoverAgents` | Only own-session agents (local) | All in scoped projects (local) | All (local) |
| `WatchAgent` | Only own-session agents | ✓ Within scoped projects | ✓ Any |
| `WatchWorkspace` | Only if workspace has own-session agents | ✓ Within scoped projects | ✓ Any |
| Proxied A2A | Only own-session agents | ✓ Within scoped projects | ✓ Any |

### Scope Enforcement on Proxied A2A

When an A2A client sends a request through the proxy (e.g., `POST /a2a/agents/{agent_id}/message:send`), the ARP server:

1. Extracts the bearer token from the `Authorization` HTTP header
2. Resolves the target agent
3. Checks that the agent's project is within the token's scope
4. For `SESSION` permission, checks that the agent's `session_id` matches the token's
5. If authorized, proxies the request to the agent's direct A2A endpoint
6. If denied, returns HTTP 403 with an A2A-compatible error

### gRPC Metadata Convention

For native gRPC clients, authentication tokens are passed via gRPC metadata (equivalent to HTTP/2 headers):

```
Key:   authorization
Value: Bearer <token>
```

This is consistent with the standard `Authorization` HTTP header convention and works seamlessly with gRPC-Web transcoding, Envoy proxies, and grpc-gateway.

## Scenarios

### Scenario 1: Project-scoped agent manages peers

A human operator with `ADMIN` scope spawns a lead agent with `PROJECT` permission. The lead agent can then see and manage all agents in that project — including ones it didn't spawn.

```bash
# Human (ADMIN, scope=global)
grpcurl -H "authorization: Bearer <admin-token>" \
  -d '{"workspace": "refactor", "template": "crush", "name": "lead", "scope": {"projects": ["myapp"]}, "permission": "PERMISSION_PROJECT"}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent
# → ARP issues token: scope={projects:["myapp"]}, perm=PROJECT

# Lead agent (PROJECT, scope={projects:["myapp"]})
grpcurl -H "authorization: Bearer <lead-token>" \
  -d '{"workspace": "refactor"}' \
  localhost:9099 arp.v1.AgentService/ListAgents
# → Sees ALL agents in "refactor" workspace (it's in project "myapp")

grpcurl -H "authorization: Bearer <lead-token>" \
  -d '{"agent_id": "some-other-agent-in-myapp"}' \
  localhost:9099 arp.v1.AgentService/StopAgent
# → Allowed — PROJECT permission lets it manage any agent in myapp
```

### Scenario 2: Session-scoped agent can only manage its tree

A token scoped to two projects with `SESSION` permission launches an agent. That agent can create new agents in either project but can only see/manage agents in its own session.

```bash
# External system (SESSION, scope={projects:["frontend","backend"]})
grpcurl -H "authorization: Bearer <ext-token>" \
  -d '{"workspace": "api-v2", "template": "crush", "name": "coordinator"}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent
# → ARP issues token: scope={projects:["frontend","backend"]}, perm=SESSION, session=sess-42

# Coordinator agent (SESSION, scope={projects:["frontend","backend"]}, session=sess-42)
grpcurl -H "authorization: Bearer <coordinator-token>" \
  -d '{}' localhost:9099 arp.v1.AgentService/ListAgents
# → Only sees: coordinator, api-worker, ui-worker (all session=sess-42)
# → Does NOT see agents spawned by other sessions, even in the same projects

grpcurl -H "authorization: Bearer <coordinator-token>" \
  -d '{"agent_id": "some-agent-from-different-session"}' \
  localhost:9099 arp.v1.AgentService/StopAgent
# → PERMISSION_DENIED — different session_id
```

### Scenario 3: Narrowing scope on delegation

A lead agent with broad scope delegates a subtask to a specialist agent with narrowed scope.

```bash
# Lead agent (PROJECT, scope={projects:["myapp","shared-lib"]})
grpcurl -H "authorization: Bearer <lead-token>" \
  -d '{"workspace": "feat", "template": "crush", "name": "specialist", "scope": {"projects": ["shared-lib"]}}' \
  localhost:9099 arp.v1.AgentService/SpawnAgent
# → Specialist gets token: scope={projects:["shared-lib"]}, perm=PROJECT

# Specialist agent (PROJECT, scope={projects:["shared-lib"]})
grpcurl -H "authorization: Bearer <specialist-token>" \
  -d '{"project": "myapp"}' localhost:9099 arp.v1.WorkspaceService/ListWorkspaces
# → PERMISSION_DENIED — "myapp" is not in specialist's scope
```

### Scenario 4: Localhost bypass

An agent running locally can always connect directly to another agent's port, bypassing ARP auth entirely. This is by design — ARP scopes protect the **management plane**, not the **data plane**.

```bash
# Agent A knows Agent B is on port 9101 (from SpawnAgent response)
curl -X POST http://localhost:9101/message:send ...
# → Works — direct A2A, no ARP auth involved

# Same request through ARP proxy requires valid token
curl -X POST http://arp:9099/a2a/agents/agent-b/message:send \
  -H "Authorization: Bearer <token>" ...
# → ARP enforces scope + permission
```

## Child Token Issuance (on spawn)

When `SpawnAgent` is called, ARP automatically:
1. Creates a child token with scope ≤ parent scope, permission ≤ parent permission
2. Sets `parent_token_id` to the caller's token ID
3. Sets `session_id` to the caller's session (or creates one if first spawn)
4. Injects the child token as `ARP_TOKEN` env var in the agent process
5. Records the `token_id` and `session_id` on the `AgentInstance`

The `SpawnAgentRequest` accepts optional `scope` and `permission` fields to narrow the child's scope:

```protobuf
message SpawnAgentRequest {
  // ... other fields ...

  // Narrow the spawned agent's project scope (must be subset of caller's scope).
  // Omit to inherit caller's full scope.
  Scope scope = 6;

  // Permission level for spawned agent (must be ≤ caller's permission).
  // Omit to inherit caller's permission.
  Permission permission = 7;
}
```

## Identity Federation

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

When `localhost_admin` is true (default), connections from localhost are treated as `ADMIN` with global scope — matching the current awesometree behavior where the local user has full control.

## Design Rationale

**Why three permission levels?** Two isn't enough. Without `SESSION`, every multi-agent system either gives agents full project access (dangerous) or requires manual per-agent ACLs (tedious). `SESSION` is the natural default for autonomous agents — they can manage their own spawned sub-agents but can't interfere with other sessions. `PROJECT` is for lead/supervisor agents that need to coordinate across sessions. `ADMIN` is for human operators and infrastructure.

**Why scope narrowing only?** This prevents privilege escalation. An agent can't grant itself or its children access to projects it doesn't have. The permission chain is monotonically decreasing, which means you can reason about the maximum blast radius of any agent by looking at its token.

**Why is direct access unscoped?** ARP manages the control plane (lifecycle, registry, routing). The data plane (agent-to-agent communication) is standard A2A. If you need data-plane auth, use A2A's own `security_schemes` on the agent's `AgentCard`. This separation keeps ARP simple and avoids re-inventing A2A's auth model.

**Why sessions rather than explicit groups?** Sessions are implicit — they emerge from the spawn tree. No configuration needed. The first `SpawnAgent` from a token creates a session; all descendants join it. This matches how agents actually work: a supervisor spawns workers, and the workers should be able to coordinate with each other but not with unrelated agents.

**Why gRPC metadata for auth?** gRPC metadata is the standard mechanism for passing request-scoped key-value pairs — directly analogous to HTTP headers. Using the `authorization` metadata key with `Bearer <token>` value is consistent with HTTP conventions, works with gRPC-Web transcoding out of the box, and is supported by every gRPC client library and proxy (Envoy, grpc-gateway, etc.).
