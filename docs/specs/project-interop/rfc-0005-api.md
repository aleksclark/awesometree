# RFC-0005: Programmatic API

**Status**: Draft \
**Audience**: Tooling authors (tool proxies, workspace managers, agent hosts, CLI tools)

## 1. Overview

Project definitions are JSON files on disk. This is sufficient for many
use cases. However, tool proxies, agent hosts, and orchestrators benefit
from higher-level APIs that abstract over file I/O, merging, and the MCP
resource protocol.

This document specifies three API surfaces: MCP tools, MCP resources,
and the filesystem convention. Implementations MAY support any
combination of these.

## 2. MCP Tools

A project-aware MCP server (standalone or integrated into a tool proxy)
MAY expose the following tools. Tool names use the `project_` prefix to
avoid collision with integration-specific tools.

### 2.1 Tool Definitions

| Tool | Parameters | Description |
|------|-----------|-------------|
| `project_list` | `{}` | List all project names and summaries. |
| `project_get` | `{name}` | Return the fully merged project definition. |
| `project_create` | `{name, repo?, branch?}` | Create a new project definition in the user-level store. |
| `project_update` | `{name, patch}` | Apply a JSON merge patch (RFC 7396) to the project definition. |
| `project_delete` | `{name}` | Delete the project definition from the user-level store. |
| `project_tools` | `{name, role?}` | Return the resolved tool manifest (after allow/deny/role filtering). |
| `project_context` | `{name, role?}` | Return the assembled context bundle. |
| `project_defaults` | `{name, toolName}` | Return the resolved default arguments for a specific tool. |

### 2.2 Tool Semantics

- `project_list` MUST return all projects discoverable via the rules in
  [RFC-0001 §3](rfc-0001-project-definition.md#3-discovery).
- `project_get` MUST return the result of merging all layers per
  [RFC-0001 §5](rfc-0001-project-definition.md#5-merging).
- `project_create` MUST write to the user-level store. It MUST NOT
  overwrite an existing project with the same name (return an error).
- `project_update` MUST apply a JSON merge patch to the user-level
  store file only. Repo-local files MUST NOT be modified.
- `project_delete` MUST only delete from the user-level store. Repo-local
  files MUST NOT be deleted.
- `project_tools` MUST apply the resolution algorithm from
  [RFC-0002 §5](rfc-0002-tool-scoping.md#5-resolution-algorithm) and,
  if `role` is provided, the role override rules from
  [RFC-0004 §3](rfc-0004-multi-agent.md#3-role-resolution).
- `project_context` MUST assemble the context bundle per
  [RFC-0003 §4](rfc-0003-context-distribution.md#4-context-assembly) and,
  if `role` is provided, apply role context overrides.
- `project_defaults` MUST return the merged default arguments that would
  be injected for the given tool name, accounting for all matching
  patterns.

### 2.3 Error Handling

Tools MUST return errors as structured results (not transport-level
errors) for the following cases:

| Condition | Error Message Pattern |
|-----------|----------------------|
| Project not found | `project "<name>" not found` |
| Name conflict on create | `project "<name>" already exists` |
| Invalid project definition | `invalid project definition: <detail>` |
| Missing context file | `context file not found: <path>` (warning, not fatal) |

## 3. MCP Resources

A project-aware MCP server SHOULD expose project data as MCP resources.
Resources are read-only and suitable for agent context injection.

### 3.1 Resource URIs

| URI Pattern | Content Type | Description |
|-------------|-------------|-------------|
| `project://list` | `application/json` | Array of `{name, repo, branch}` for all projects. |
| `project://<name>/definition` | `application/json` | Full merged project definition. |
| `project://<name>/tools` | `application/json` | Resolved tool manifest (array of tool names). |
| `project://<name>/context` | `application/json` | Context manifest (see RFC-0003 §5.3). |
| `project://<name>/context/<path>` | `text/*` | Individual context file content. |
| `project://<name>/agents` | `application/json` | Active sessions (see RFC-0004 §4). |

### 3.2 Resource Templates

Implementations SHOULD register the following MCP resource templates for
parameterized access:

```json
[
  {
    "uriTemplate": "project://{name}/definition",
    "name": "project-definition",
    "description": "Full merged project definition"
  },
  {
    "uriTemplate": "project://{name}/context/{path}",
    "name": "project-context-file",
    "description": "A context file from a project's context bundle"
  }
]
```

### 3.3 Subscriptions

If the MCP server supports resource subscriptions, it SHOULD emit
`notifications/resources/updated` when:

- A project definition file is modified on disk
- A context store file is added, removed, or modified
- An agent session is created or removed

## 4. Filesystem Convention

All project data is stored under a well-known directory. Any tool MAY
read these files directly without going through MCP.

### 4.1 Directory Structure

```
$XDG_CONFIG_HOME/project-interop/
├── projects/
│   └── <name>.project.json       # Project definitions (RFC-0001)
├── context/
│   └── <name>/                   # Per-project context stores (RFC-0003)
│       └── <file>
├── servers.json                  # MCP server registry (RFC-0002 §3)
└── state/
    └── <name>/                   # Per-project runtime state (RFC-0004)
        └── sessions/
            └── <session-id>.json
```

### 4.2 Permissions

- The `project-interop/` directory SHOULD be created with mode `0700`
- Project definition files SHOULD be created with mode `0600`
- Context store files SHOULD be created with mode `0600` (may contain
  sensitive data)
- Session files SHOULD be created with mode `0600`

### 4.3 Concurrency

Multiple processes MAY read project definitions concurrently without
coordination. Writes to project definitions SHOULD use atomic
write-rename (write to a temporary file, then rename) to prevent partial
reads.

Session files (RFC-0004) are designed for concurrent access: each agent
writes only its own file. No file-level locking is required.

## 5. CLI Convention

Implementations that provide a CLI SHOULD follow this command structure:

### 5.1 Project Management

```
<tool> project list
<tool> project show <name>
<tool> project create <name> [--repo <path>] [--branch <branch>]
<tool> project edit <name>
<tool> project delete <name>
<tool> project import <path>
<tool> project export <name>
```

### 5.2 Tool Scoping

```
<tool> tools list <project>
<tool> tools allow <project> <pattern>
<tool> tools deny <project> <pattern>
<tool> tools defaults <project> <pattern> '<json>'
```

### 5.3 Context Management

```
<tool> context list <project>
<tool> context add <project> <file>
<tool> context edit <project> <file>
<tool> context rm <project> <file>
<tool> context bundle <project>
```

### 5.4 Agent Sessions (Future)

```
<tool> agents list <project>
<tool> agents attach <project> [--role <role>]
<tool> agents detach <project> --session <id>
<tool> agents status <project>
```

These are RECOMMENDED command names. Implementations MAY use different
subcommand structures as long as the underlying operations map to the
MCP tools defined in [Section 2](#2-mcp-tools).

## 6. Integration Patterns

### 6.1 Tool Proxy Integration

A tool proxy that supports project scoping SHOULD:

1. Accept a `project` parameter (or header, or session context) that
   identifies the active project
2. Load and merge the project definition
3. Filter `tools/list` responses per RFC-0002
4. Inject default arguments per RFC-0002 §6
5. Optionally enforce `deny` rules on `tools/call`

### 6.2 Agent Host Integration

An agent host that supports project scoping SHOULD:

1. Resolve the active project (from CLI flag, working directory, or
   user selection)
2. Assemble the context bundle per RFC-0003
3. Inject context into the agent's initial prompt or system message
4. If the tool proxy is not project-aware, filter tool listings
   client-side before presenting to the agent

### 6.3 Workspace Manager Integration

A workspace manager MAY synchronize its internal project model with
project definitions by:

1. Writing `.project.json` files to the user-level store (using the
   `extensions` field for workspace-specific data)
2. Reading `.project.json` files and importing them into its internal
   model
3. Exposing project operations via the MCP tools in Section 2
