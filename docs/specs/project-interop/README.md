# Project Interop Specification

**Version**: 0.1.0-draft \
**Status**: Draft \
**Date**: 2026-03-03

## Abstract

This specification defines a portable, tool-agnostic data format for
**project definitions** in agentic development workflows. A project
definition binds together source repositories, scoped tool access, shared
context files, and multi-agent coordination metadata into a single
interoperable contract.

The spec is designed for consumption by workspace managers, MCP tool
proxies, agent hosts, and CI/CD systems without coupling to any specific
implementation.

## Terminology

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in these
documents are to be interpreted as described in
[RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

| Term | Definition |
|------|------------|
| **Project** | A named unit of work binding source, tools, context, and agents. |
| **Tool proxy** | Any MCP server that aggregates or routes tool calls (e.g., an MCP aggregator, a gateway). |
| **Workspace manager** | Any system that manages development environments, worktrees, or editor sessions for a project. |
| **Agent host** | Any runtime that executes one or more AI agents (e.g., a CLI agent, an IDE copilot, an orchestrator). |
| **Context store** | A directory of non-committed files distributed to agents working on a project. |
| **Role** | A named set of tool and context restrictions for an agent operating within a project. |
| **Session** | A runtime binding of one agent to one project, optionally under a role. |

## Documents

This specification is split into focused sections for progressive
disclosure. Implementors SHOULD read documents in order but MAY skip to
the section relevant to their use case.

| Document | Audience | Summary |
|----------|----------|---------|
| [RFC-0001: Project Definition](rfc-0001-project-definition.md) | All implementors | File format, discovery, schema, merging rules. |
| [RFC-0002: Tool Scoping](rfc-0002-tool-scoping.md) | Tool proxy implementors | Allow/deny lists, default arguments, resolution algorithm. |
| [RFC-0003: Context Distribution](rfc-0003-context-distribution.md) | Agent host implementors | Context store, assembly layers, MCP resource protocol. |
| [RFC-0004: Multi-Agent Coordination](rfc-0004-multi-agent.md) | Orchestrator implementors | Roles, sessions, shared state. Status: **future**. |
| [RFC-0005: Programmatic API](rfc-0005-api.md) | Tooling authors | MCP tools, MCP resources, filesystem conventions. |

## Goals

1. Bind sets of MCP tools per project to N agents
2. Distribute non-committed context files per project to N agents
3. Manage project definitions via files, CLI, and programmatic API
4. Layer on top of AGENTS.md, `.mcp.json`, and existing conventions
5. (Future) Plan for multi-agent flows with shared state and coordination

## Non-Goals

- Replacing AGENTS.md, `.mcp.json`, or any agent/editor-specific format
- Defining a new MCP transport or protocol extension
- Specifying an agent orchestration runtime

## Compatibility

This spec is designed to compose with, not replace, existing conventions:

| Convention | Relationship |
|---|---|
| **AGENTS.md / CLAUDE.md** | Referenced via `context.repoIncludes`. Not replaced. |
| **`.mcp.json` / `.vscode/mcp.json`** | Servers defined there are referenceable in `tools`. Scoping is additive. |
| **`devcontainer.json`** | Orthogonal. Defines the environment; this spec defines agent config within it. |
| **`mise.toml` / `.tool-versions`** | Orthogonal. Build-time tool versions, not agent-time tool access. |

## Design Rationale

### Why not extend `.mcp.json`?

`.mcp.json` defines which MCP servers to connect to. Project definitions
are a layer above: they scope *which tools from those servers* are
relevant, add non-committed context, and support multi-agent coordination.
Extending `.mcp.json` would conflate server configuration with project
semantics.

### Why JSON?

JSON is the native format of the MCP protocol itself. Using JSON avoids
additional parser dependencies and provides direct compatibility with
JSON Schema validation. Implementations MAY accept JSONC (JSON with
comments) for human-authored files but MUST produce valid JSON when
serializing.

### Why glob patterns for tool scoping?

MCP tools are commonly namespaced by integration prefix (e.g.,
`github_*`, `datadog_*`). Glob patterns are the simplest way to scope by
namespace without requiring knowledge of every individual tool name.
They are also forward-compatible as new tools are added to a server.

### Why a standalone file?

The project definition needs to be consumable by tools other than any
single workspace manager. A standalone JSON file with a schema is more
interoperable than being embedded in an application-specific config.

## Implementation Phases

| Phase | Scope | Documents |
|-------|-------|-----------|
| **1** | Project definitions + tool scoping | RFC-0001, RFC-0002 |
| **2** | Context distribution | RFC-0003 |
| **3** | Multi-agent coordination | RFC-0004 |

RFC-0005 (API) applies across all phases.
