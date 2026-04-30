---
title: "Agent Registry Protocol (ARP)"
created: 2026-04-06
updated: 2026-05-01
---

# Agent Registry Protocol (ARP)

A gRPC service that manages the full lifecycle of A2A agents within workspaces. Fills the gap between the control plane (agent lifecycle management) and A2A (agent-to-agent communication): no existing protocol defines how to create, start, stop, or destroy agent instances. ARP does. The control plane speaks gRPC + protobuf; agents speak A2A.

## Spec Documents

### Overview

Problem statement, design principles, architecture, A2A v1.0 reference tables, the two-interface model (direct vs proxied access), protobuf data model, agent status state machine, configuration, and relationship to existing systems.

→ [Overview](overview.md)

### Project Management Service

gRPC `ProjectService` for registering and managing code repository projects — the templates that define what agents are available and how they're configured.

RPCs: `ListProjects`, `RegisterProject`, `UnregisterProject`

→ [Project Service](services-project.md)

### Workspace Management Service

gRPC `WorkspaceService` for creating and destroying isolated workspaces (git worktrees) where agents operate.

RPCs: `CreateWorkspace`, `ListWorkspaces`, `GetWorkspace`, `DestroyWorkspace`

→ [Workspace Service](services-workspace.md)

### Agent Lifecycle Service

gRPC `AgentService` for spawning, monitoring, messaging, and stopping A2A agents within workspaces. Includes A2A `SendMessage` / `GetTask` integration and multi-agent patterns.

RPCs: `SpawnAgent`, `ListAgents`, `GetAgentStatus`, `SendAgentMessage`, `CreateAgentTask`, `GetAgentTaskStatus`, `StopAgent`, `RestartAgent`

→ [Agent Lifecycle Service](services-agent.md)

### Discovery & Routing Service

gRPC `DiscoveryService` for discovering agents across workspaces and the network. Server-streaming RPCs for real-time monitoring. Multi-agent workflow patterns.

RPCs: `DiscoverAgents`, `WatchAgent` (server-streaming), `WatchWorkspace` (server-streaming)

→ [Discovery & Routing](services-discovery.md)

### Identity Federation & Scopes

Token-based auth model for ARP. Tokens are passed via gRPC metadata (`authorization` key) and carry project scopes and permission levels (session, project, admin). Agents inherit scoped tokens from their spawner — scope can only narrow, never widen. Session-scoped agents see only their own spawn tree; project-scoped agents manage all agents in their projects. Covers token issuance, child token flow, per-RPC enforcement, OIDC federation, and scope narrowing on delegation.

→ [Identity & Scopes](identity-and-scopes.md)
