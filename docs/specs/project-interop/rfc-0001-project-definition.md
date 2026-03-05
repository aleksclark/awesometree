# RFC-0001: Project Definition

**Status**: Draft \
**Audience**: All implementors

## 1. Overview

A **project definition** is a JSON document that binds a named unit of
work to its source repositories, tool access policies, context files,
and agent coordination metadata.

This document specifies the file format, discovery rules, schema, and
merging semantics. Tool scoping, context distribution, and multi-agent
features are specified in subsequent RFCs and referenced by field name.

## 2. File Format

### 2.1 Encoding

A project definition MUST be a valid JSON document encoded as UTF-8.
Implementations MAY accept JSONC (JSON with comments and trailing commas)
when reading human-authored files but MUST produce strict JSON when
serializing.

### 2.2 File Extension

Project definition files MUST use the extension `.project.json`.

### 2.3 Schema Identifier

A conforming document SHOULD include a `$schema` field pointing to the
canonical schema URL for validation tooling:

```json
{
  "$schema": "https://project-interop.dev/schemas/v1/project.schema.json"
}
```

## 3. Discovery

Implementations MUST support the following discovery locations, checked
in order. When multiple locations yield a definition for the same project
name, the merge rules in [Section 5](#5-merging) apply.

### 3.1 User-Level Store

```
$XDG_CONFIG_HOME/project-interop/projects/<name>.project.json
```

If `$XDG_CONFIG_HOME` is not set, implementations MUST fall back to
`~/.config/project-interop/projects/`.

The user-level store is the primary location for project definitions. A
**project manager** (any tool that creates/edits project definitions)
SHOULD write to this location by default.

### 3.2 Repo-Local Override

```
.project.json    # in repository root
```

A repository MAY contain a `.project.json` at its root. When an agent
operates within a repository that contains this file, the repo-local
definition is merged on top of the user-level definition (see
[Section 5](#5-merging)).

Repo-local files are OPTIONAL. They are intended for committed,
team-shared project configuration.

### 3.3 Workspace Manager Integration

Workspace managers that maintain their own project registries (e.g., in
an application-specific config file) MAY expose those definitions through
this spec by either:

1. Writing standalone `.project.json` files to the user-level store, or
2. Implementing the programmatic API defined in
   [RFC-0005](rfc-0005-api.md).

Workspace managers SHOULD NOT require consumers to parse their
application-specific config formats.

## 4. Schema

### 4.1 Top-Level Fields

```json
{
  "$schema": "https://project-interop.dev/schemas/v1/project.schema.json",
  "version": "1",
  "name": "myproject",
  "repo": "~/work/myproject",
  "branch": "main",
  "launch": {},
  "tools": {},
  "context": {},
  "agents": {},
  "extensions": {}
}
```

### 4.2 Field Reference

| Field | Type | REQUIRED | Description |
|-------|------|----------|-------------|
| `$schema` | `string` | RECOMMENDED | Schema URL for validation. |
| `version` | `string` | REQUIRED | Schema version. MUST be `"1"`. |
| `name` | `string` | REQUIRED | Unique project identifier. MUST match `^[a-zA-Z0-9][a-zA-Z0-9._-]*$`. |
| `repo` | `string` | OPTIONAL | Path to source repository. `~/` MUST be expanded to `$HOME`. |
| `branch` | `string` | OPTIONAL | Default branch for new worktrees or checkouts. |
| `launch` | `object` | OPTIONAL | Agent launch configuration. See [Section 4.5](#45-launch-configuration). |
| `tools` | `object` | OPTIONAL | Tool scoping rules. See [RFC-0002](rfc-0002-tool-scoping.md). |
| `context` | `object` | OPTIONAL | Context file configuration. See [RFC-0003](rfc-0003-context-distribution.md). |
| `agents` | `object` | OPTIONAL | Multi-agent configuration. See [RFC-0004](rfc-0004-multi-agent.md). |
| `extensions` | `object` | OPTIONAL | Implementation-specific data. See [Section 6](#6-extensions). |

### 4.3 Name Constraints

The `name` field:
- MUST be between 1 and 128 characters
- MUST start with an alphanumeric character
- MUST contain only alphanumeric characters, `.`, `_`, or `-`
- MUST be unique within a given user-level store
- SHOULD be treated as case-sensitive by implementations

### 4.4 Repo Path Resolution

When `repo` is present:
- Implementations MUST expand a leading `~/` to the value of `$HOME`
- Implementations MUST resolve relative paths against the directory
  containing the project definition file
- Implementations SHOULD validate that the resolved path exists and
  contains a `.git` directory (or is otherwise recognizable as a
  repository) before performing repository operations

### 4.5 Launch Configuration

The `launch` object controls how agents are bootstrapped when they
start work on a project. It allows the project definition to specify
the system prompt or append-prompt that the workspace manager passes to
the agent host at launch time.

```json
{
  "launch": {
    "prompt": "You are working on the acme-api project. Do NOT read AGENTS.md. Instead, use the project_context tool to retrieve project context, and the search tool to discover available operations.",
    "promptFile": "launch-prompt.md",
    "env": {
      "PROJECT_NAME": "acme-api"
    }
  }
}
```

| Field | Type | REQUIRED | Description |
|-------|------|----------|-------------|
| `prompt` | `string` | OPTIONAL | Inline prompt text injected at agent launch. |
| `promptFile` | `string` | OPTIONAL | Path to a prompt file, relative to the context store. |
| `env` | `object` | OPTIONAL | Environment variables set in the agent process. Values are strings. |

If both `prompt` and `promptFile` are present, the implementation MUST
concatenate them (`prompt` first, then the contents of `promptFile`,
separated by a newline).

The launch prompt is the mechanism by which a project definition
instructs agents to query the tool proxy for context rather than
reading static convention files. See
[RFC-0003 §2](rfc-0003-context-distribution.md#2-context-delivery-model)
for the rationale.

## 5. Merging

When a project definition exists at multiple discovery locations,
implementations MUST merge them with the following precedence (highest
wins):

```
1. Repo-local (.project.json in repo root)         ← highest
2. User-level store (<name>.project.json)           ← lowest
```

### 5.1 Merge Semantics

- **Scalar fields** (`name`, `repo`, `branch`): higher-precedence value
  wins. The `name` field MUST NOT differ between layers; if it does,
  the implementation MUST reject the merge and report an error.
- **`launch`**: `prompt` and `promptFile` from higher-precedence layer
  win (replace, not concatenate). `env` objects are merged with
  higher-precedence values winning per key.
- **`tools`**: merged per server key. Within a server, `allow` and
  `deny` arrays are concatenated (not replaced). `defaults` objects are
  merged with higher-precedence values winning per pattern key.
- **`context`**: `files` and `repoIncludes` arrays are concatenated.
  `maxBytes` uses the higher-precedence value.
- **`agents`**: `roles` are merged by role name; higher-precedence role
  definitions replace lower-precedence ones entirely.
- **`extensions`**: merged by top-level key; higher-precedence values
  replace lower-precedence ones per key.

### 5.2 Merge Errors

If merging produces an invalid document (e.g., conflicting `name`
fields), implementations MUST report the error and MUST NOT silently
use a partial result.

## 6. Extensions

The `extensions` field provides a namespace for implementation-specific
data that is not part of this spec. Workspace managers, CI systems, and
other tools MAY store arbitrary configuration here.

```json
{
  "extensions": {
    "com.example.workspace-manager": {
      "gui": ["firefox"],
      "layout": "tile"
    },
    "com.example.ci": {
      "pipeline": "backend-deploy"
    }
  }
}
```

### 6.1 Naming

Extension keys SHOULD use reverse domain notation to avoid collisions
(e.g., `com.example.mytool`). Implementations MUST preserve unrecognized
extension keys when reading and writing project definitions.

### 6.2 Compatibility

Implementations MUST NOT reject a project definition that contains
unrecognized extension keys. Implementations MUST NOT reject a project
definition that contains unrecognized top-level fields, to allow
forward-compatible additions to this spec.

## 7. Filesystem Layout

A conforming user-level store has the following structure:

```
$XDG_CONFIG_HOME/project-interop/
├── projects/
│   ├── my-app.project.json
│   ├── my-lib.project.json
│   └── infra.project.json
├── context/                            # See RFC-0003
│   ├── my-app/
│   └── my-lib/
├── servers.json                        # See RFC-0002 §3
└── state/                              # See RFC-0004 (future)
```

Implementations MUST create the `projects/` directory if it does not
exist when writing a new project definition.

Implementations MUST NOT create other directories (`context/`, `state/`)
until the corresponding feature is used.

## 8. Versioning

The `version` field enables schema evolution. The current version is
`"1"`.

- Implementations MUST reject documents with a `version` value they do
  not support.
- Future versions of this spec MAY add new OPTIONAL fields without
  incrementing the version number.
- A version number increment indicates a breaking change to REQUIRED
  fields or semantics.
