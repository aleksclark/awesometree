# RFC-0003: Context Distribution

**Status**: Draft \
**Audience**: Agent host implementors, project manager authors

## 1. Overview

Agents working on a project need context beyond source code: architecture
notes, sprint goals, environment guides, credential hints, and other
documents that may or may not be committed to the repository.

This document specifies how the `context` field of a project definition
(see [RFC-0001](rfc-0001-project-definition.md)) configures context
files, how they are assembled into a bundle, and how they are exposed to
agents via the MCP resource protocol.

## 2. The `context` Object

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

### 2.1 Field Reference

| Field | Type | REQUIRED | Description |
|-------|------|----------|-------------|
| `files` | `string[]` | OPTIONAL | Paths relative to the project's context store directory. |
| `repoIncludes` | `string[]` | OPTIONAL | Paths relative to the repository root. Glob patterns MUST be supported. |
| `maxBytes` | `integer` | OPTIONAL | Advisory limit on total assembled context size in bytes. |

## 3. Context Store

### 3.1 Location

Each project has a context store directory at:

```
$XDG_CONFIG_HOME/project-interop/context/<project-name>/
```

If `$XDG_CONFIG_HOME` is not set, implementations MUST fall back to
`~/.config/project-interop/context/<project-name>/`.

### 3.2 Properties

Context store files:

- MUST NOT be committed to any repository (they are user-local)
- MAY contain sensitive information (credentials, internal URLs)
- SHOULD be readable by any agent host on the system
- MUST be plain text or Markdown (binary files are not supported)

### 3.3 Management

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
[Section 4](#4-context-assembly)).

## 4. Context Assembly

When an agent begins work on a project, the agent host MUST assemble
a **context bundle**: an ordered list of `(path, content)` entries.

### 4.1 Assembly Layers

Context is assembled from three layers. Within each layer, entries
appear in the order declared in the project definition. Across layers,
entries are concatenated in the following order (layer 1 first):

```
Layer 1: repoIncludes    — committed files from the repository
Layer 2: files           — non-committed files from the context store
Layer 3: role overrides  — per-role context restrictions (see RFC-0004)
```

### 4.2 Deduplication

If the same logical file appears in multiple layers (e.g., `AGENTS.md`
is both a repo include and manually added to the context store), the
higher-numbered layer's content SHALL take precedence. Implementations
MUST NOT include duplicate entries in the assembled bundle.

### 4.3 Size Limits

If `maxBytes` is specified and the assembled bundle exceeds this limit,
implementations SHOULD:

1. Warn the user or operator
2. Truncate the bundle by removing entries from the end (lowest
   priority = last added from the highest layer)
3. Include a synthetic entry at the end indicating truncation

Implementations MUST NOT silently drop context without indication.

### 4.4 Missing Files

If a file referenced by `files` does not exist in the context store,
or a `repoIncludes` path does not match any file in the repository:

- Implementations MUST skip the missing entry
- Implementations SHOULD log a warning
- Implementations MUST NOT fail the assembly

## 5. MCP Resource Protocol

Agent hosts and project-aware MCP servers SHOULD expose context via
standard MCP resources, enabling any MCP client to read project context
without implementation-specific APIs.

### 5.1 Resource URIs

| URI | Description |
|-----|-------------|
| `project://<name>/context` | Manifest: JSON array of `{path, mimeType, sizeBytes}` entries. |
| `project://<name>/context/<path>` | Content of a single context file. |

### 5.2 Resource Templates

Implementations SHOULD register the following MCP resource template:

```json
{
  "uriTemplate": "project://{projectName}/context/{filePath}",
  "name": "project-context-file",
  "description": "A context file from a project definition",
  "mimeType": "text/plain"
}
```

### 5.3 Manifest Format

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

## 6. Merging Across Layers

When context configuration exists at multiple discovery layers (see
[RFC-0001 §5](rfc-0001-project-definition.md#5-merging)):

- `files` arrays MUST be concatenated (union)
- `repoIncludes` arrays MUST be concatenated (union)
- `maxBytes` MUST use the higher-precedence layer's value

## 7. Relationship to Existing Conventions

### 7.1 AGENTS.md / CLAUDE.md

These files are typically committed to repositories and provide
agent-specific instructions. Projects SHOULD reference them via
`repoIncludes` rather than duplicating their content in the context
store.

### 7.2 `.cursorrules` / `.github/copilot-instructions.md`

Same as AGENTS.md — include via `repoIncludes` if agents should see
them.

### 7.3 Agent Memory Files

Agent hosts that maintain their own memory files (e.g., learned
preferences, command history) operate independently of this spec.
Context distribution provides **project-scoped** context, not
**agent-scoped** memory.
