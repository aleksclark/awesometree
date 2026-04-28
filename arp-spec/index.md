---
title: "Agent Registry Protocol (ARP)"
created: 2026-04-06
updated: 2026-04-28
---

# Agent Registry Protocol (ARP)

An MCP server that manages the full lifecycle of A2A agents within workspaces. Fills the gap between MCP (agent-to-tool) and A2A (agent-to-agent): neither protocol defines how to create, start, stop, or destroy agent instances. ARP does.

## Spec Documents

### Overview

Problem statement, design principles, architecture, A2A v1.0 reference tables, the two-interface model (direct vs proxied access), data model, agent status state machine, configuration, and relationship to existing systems.

→ [Overview](overview.md)

### Project Management Tools

MCP tools for registering and managing code repository projects — the templates that define what agents are available and how they're configured.

Tools: `project/list`, `project/register`, `project/unregister`

→ [Project Tools](tools-project.md)

### Workspace Management Tools

MCP tools for creating and destroying isolated workspaces (git worktrees) where agents operate.

Tools: `workspace/create`, `workspace/list`, `workspace/get`, `workspace/destroy`

→ [Workspace Tools](tools-workspace.md)

### Agent Lifecycle Tools

MCP tools for spawning, monitoring, messaging, and stopping A2A agents within workspaces. Includes A2A `SendMessage` / `GetTask` integration and multi-agent patterns.

Tools: `agent/spawn`, `agent/list`, `agent/status`, `agent/message`, `agent/task`, `agent/task_status`, `agent/stop`, `agent/restart`

→ [Agent Lifecycle Tools](tools-agent.md)

### Discovery Tools

MCP tools and A2A registry endpoints for discovering agents across workspaces and the network. Plus MCP resources, MCP prompts, and multi-agent workflow patterns.

Tools: `agent/discover`
Resources: `agent://{id}/status`, `agent://{id}/card`, `workspace://{name}`

→ [Discovery & Patterns](tools-discovery.md)

### Identity Federation & Scopes

Token-based auth model for ARP. Tokens carry project scopes and permission levels (session, project, admin). Agents inherit scoped tokens from their spawner — scope can only narrow, never widen. Session-scoped agents see only their own spawn tree; project-scoped agents manage all agents in their projects. Covers token issuance, child token flow, per-tool enforcement, OIDC federation, and scope narrowing on delegation.

→ [Identity & Scopes](identity-and-scopes.md)
