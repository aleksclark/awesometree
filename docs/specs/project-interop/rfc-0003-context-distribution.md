# RFC-0003: Context Distribution

**Status**: Draft \
**Audience**: Agent host implementors, tool proxy implementors, project manager authors

## 1. Overview

Agents working on a project need context beyond source code: architecture
notes, sprint goals, environment guides, credential hints, and other
documents that may or may not be committed to the repository.

This document specifies how project context is assembled, how it is
delivered to agents via the tool proxy, and how the launch prompt
(RFC-0001 §4.5) directs agents to query the proxy rather than reading
static convention files.

## 2. Context Delivery Model

The primary mechanism for context delivery is **proxy-mediated**: agents
receive context by querying the project-scoped tool proxy, not by
reading AGENTS.md or other convention files from the repository.

The flow is:

```
1. Workspace manager reads the project definition (RFC-0001)
2. Workspace manager expands the server URL template ({project})
3. Workspace manager injects the launch prompt into the agent host
4. Agent starts and receives the launch prompt as system context
5. Launch prompt instructs the agent to use the proxy for context
6. Agent calls project_context tool (or reads MCP resources) at
   the project-scoped URL for relevant context
7. Proxy assembles and returns context from the context store and
   repo includes
```

This model has several advantages over static file delivery:

- **Dynamic**: context is always current; no stale AGENTS.md files
- **Filtered**: the proxy can serve role-specific context subsets
- **Progressive**: agents request context on demand rather than
  receiving everything upfront (see [Section 6](#6-tool-based-context))
- **Controlled**: sensitive context stays server-side until requested

### 2.1 Launch Prompt Role

The `launch.prompt` field (or `launch.promptFile`) in the project
definition is the bridge between the workspace manager and the agent.
It MUST instruct the agent to:

1. Use the project-scoped MCP tools for context discovery
2. NOT read AGENTS.md or other convention files directly
3. Rely on the proxy as the authoritative source of project context

Workspace managers MUST inject the launch prompt via the agent host's
system prompt mechanism (e.g., `--append-system-prompt` for CLI agents,
system message for API-based agents).

## 3. The `context` Object

```json
{
  "context": {
    "files": [
      "architecture-decisions.md",
      "current-sprint.md"
    ],
    "repoIncludes": [
      "AGENTS.md",
      "docs/architecture.md"
    ],
    "maxBytes": 524288
  }
}
```

### 3.1 Field Reference

| Field | Type | REQUIRED | Description |
|-------|------|----------|-------------|
| `files` | `string[]` | OPTIONAL | Paths relative to the project's context store directory. |
| `repoIncludes` | `string[]` | OPTIONAL | Paths relative to the repository root. Glob patterns MUST be supported. |
| `maxBytes` | `integer` | OPTIONAL | Advisory limit on total assembled context size in bytes. |

## 4. Context Store

### 4.1 Location

Each project has a context store directory at:

```
$XDG_CONFIG_HOME/project-interop/context/<project-name>/
```

If `$XDG_CONFIG_HOME` is not set, implementations MUST fall back to
`~/.config/project-interop/context/<project-name>/`.

### 4.2 Properties

Context store files:

- MUST NOT be committed to any repository (they are user-local)
- MAY contain sensitive information (credentials, internal URLs)
- SHOULD be readable by any agent host on the system
- MUST be plain text or Markdown (binary files are not supported)

### 4.3 Management

Project managers SHOULD provide CLI or UI commands for managing context
store files. The RECOMMENDED command surface is:

```
<tool> context list <project>
<tool> context add <project> <file>
<tool> context edit <project> <file>
<tool> context rm <project> <file>
<tool> context bundle <project>
```

The `add` command SHOULD copy the file into the context store. The
`bundle` command SHOULD assemble and print the full context (see
[Section 5](#5-context-assembly)).

## 5. Context Assembly

The tool proxy assembles context server-side when agents request it.
The proxy MUST produce a **context bundle**: an ordered list of
`(path, content)` entries.

### 5.1 Assembly Layers

Context is assembled from three layers. Within each layer, entries
appear in the order declared in the project definition. Across layers,
entries are concatenated in the following order (layer 1 first):

```
Layer 1: repoIncludes    — committed files from the repository
Layer 2: files           — non-committed files from the context store
Layer 3: role overrides  — per-role context restrictions (see RFC-0004)
```

### 5.2 Deduplication

If the same logical file appears in multiple layers (e.g., `AGENTS.md`
is both a repo include and manually added to the context store), the
higher-numbered layer's content SHALL take precedence. Implementations
MUST NOT include duplicate entries in the assembled bundle.

### 5.3 Size Limits

If `maxBytes` is specified and the assembled bundle exceeds this limit,
implementations SHOULD:

1. Warn the user or operator
2. Truncate the bundle by removing entries from the end (lowest
   priority = last added from the highest layer)
3. Include a synthetic entry at the end indicating truncation

Implementations MUST NOT silently drop context without indication.

### 5.4 Missing Files

If a file referenced by `files` does not exist in the context store,
or a `repoIncludes` path does not match any file in the repository:

- Implementations MUST skip the missing entry
- Implementations SHOULD log a warning
- Implementations MUST NOT fail the assembly

## 6. Tool-Based Context (Progressive Disclosure)

The RECOMMENDED approach for serving context is via an MCP tool that
supports search-style progressive disclosure. This approach works with
all agent hosts that support MCP tools.

### 6.1 The `project_context` Tool

A project-aware tool proxy SHOULD expose a `project_context` tool at
the project-scoped endpoint (`{base}/mcp/{project-name}`). Because the
project is implicit in the URL, this tool does NOT require a `name`
parameter.

```json
{
  "name": "project_context",
  "description": "Search and retrieve project context files. Call with no query to list available context entries, or with a query to search for relevant context.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Search query to filter context entries. If omitted, returns a manifest of all available context."
      },
      "path": {
        "type": "string",
        "description": "Exact path of a context file to retrieve its full content."
      },
      "role": {
        "type": "string",
        "description": "Role name to apply context overrides."
      }
    }
  }
}
```

### 6.2 Progressive Disclosure Flow

1. Agent calls `project_context` with no arguments → receives a manifest
   (list of available context files with summaries)
2. Agent identifies relevant entries from the manifest
3. Agent calls `project_context` with `path` → receives full content of
   a specific file
4. Agent calls `project_context` with `query` → receives entries
   matching the search term

This pattern minimizes token usage by letting the agent fetch only the
context it needs for the current task.

## 7. Resource-Based Context

As an alternative to tool-based delivery, agent hosts and project-aware
MCP servers MAY expose context via standard MCP resources.

> **Note**: MCP resources are "application-driven" — the agent host
> decides whether to auto-inject them. Not all agent hosts support
> resource auto-injection. Implementations SHOULD prefer the tool-based
> approach ([Section 6](#6-tool-based-context-progressive-disclosure))
> as the primary delivery mechanism and treat resources as supplementary.

### 7.1 Resource URIs

| URI | Description |
|-----|-------------|
| `project://<name>/context` | Manifest: JSON array of `{path, mimeType, sizeBytes}` entries. |
| `project://<name>/context/<path>` | Content of a single context file. |

### 7.2 Resource Templates

Implementations SHOULD register the following MCP resource template:

```json
{
  "uriTemplate": "project://{projectName}/context/{filePath}",
  "name": "project-context-file",
  "description": "A context file from a project definition",
  "mimeType": "text/plain"
}
```

### 7.3 Manifest Format

The manifest resource (`project://<name>/context`) MUST return a JSON
array:

```json
[
  {
    "path": "AGENTS.md",
    "source": "repo",
    "mimeType": "text/markdown",
    "sizeBytes": 4096
  },
  {
    "path": "current-sprint.md",
    "source": "store",
    "mimeType": "text/markdown",
    "sizeBytes": 1200
  }
]
```

| Field | Type | REQUIRED | Description |
|-------|------|----------|-------------|
| `path` | `string` | REQUIRED | Relative path as declared in the project definition. |
| `source` | `string` | REQUIRED | `"repo"` or `"store"`. Indicates origin layer. |
| `mimeType` | `string` | RECOMMENDED | MIME type of the content. |
| `sizeBytes` | `integer` | RECOMMENDED | Size of the file in bytes. |

## 8. Merging Across Layers

When context configuration exists at multiple discovery layers (see
[RFC-0001 §5](rfc-0001-project-definition.md#5-merging)):

- `files` arrays MUST be concatenated (union)
- `repoIncludes` arrays MUST be concatenated (union)
- `maxBytes` MUST use the higher-precedence layer's value

## 9. Relationship to Existing Conventions

### 9.1 AGENTS.md / CLAUDE.md

Projects using this spec SHOULD set `launch.prompt` to instruct agents
to ignore AGENTS.md and use the proxy instead. This avoids conflicts
between static convention files and proxy-served context.

AGENTS.md MAY still be referenced in `context.repoIncludes` so that
the proxy can serve its content as part of the assembled context bundle.
In this model, the proxy is the authoritative source — agents read
AGENTS.md content through the proxy, not directly from the file.

Projects that do not use a launch prompt MAY fall back to letting agents
read AGENTS.md directly, but this is NOT RECOMMENDED when a tool proxy
is available.

### 9.2 `.cursorrules` / `.github/copilot-instructions.md`

Same as AGENTS.md — include via `repoIncludes` if agents should see
them through the proxy.

### 9.3 Agent Memory Files

Agent hosts that maintain their own memory files (e.g., learned
preferences, command history) operate independently of this spec.
Context distribution provides **project-scoped** context, not
**agent-scoped** memory.
