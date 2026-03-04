# RFC-0002: Tool Scoping

**Status**: Draft \
**Audience**: Tool proxy implementors, agent host implementors

## 1. Overview

An MCP tool proxy MAY expose hundreds of tools across many integrations.
Agents working on a specific project typically need a small, relevant
subset. Unrestricted tool sets waste context tokens, increase risk of
unintended side effects, and slow agent decision-making.

This document specifies how the `tools` field of a project definition
(see [RFC-0001](rfc-0001-project-definition.md)) controls which MCP
tools are visible to agents and what default arguments are injected.

## 2. The `tools` Object

The `tools` field is a JSON object keyed by **server identifier**. Each
value is a **scoping rule** for that server.

```json
{
  "tools": {
    "<server-id>": {
      "allow": ["<pattern>", ...],
      "deny": ["<pattern>", ...],
      "defaults": {
        "<pattern>": { "<arg>": "<value>", ... }
      }
    }
  }
}
```

### 2.1 Field Reference

| Field | Type | REQUIRED | Description |
|-------|------|----------|-------------|
| `allow` | `string[]` | OPTIONAL | Glob patterns of tool names to permit. |
| `deny` | `string[]` | OPTIONAL | Glob patterns of tool names to forbid. |
| `defaults` | `object` | OPTIONAL | Default arguments keyed by tool name glob pattern. |

If a server key is absent from `tools`, all tools from that server
SHALL be available with no restrictions.

## 3. Server Identity

### 3.1 Server Registry

The key in `tools` (e.g., `"my-proxy"`, `"filesystem"`) maps to an MCP
server. Implementations MUST resolve server identifiers using the
following precedence:

1. **Server registry file** at
   `$XDG_CONFIG_HOME/project-interop/servers.json`
2. **Repo-local MCP config** (`.mcp.json`, `.vscode/mcp.json`) — servers
   defined there are available by their key name
3. **Implementation-defined defaults** — an implementation MAY register
   well-known server names (e.g., mapping a name to `localhost:<port>`)

### 3.2 Server Registry Format

```json
{
  "<server-id>": {
    "transport": "http",
    "url": "http://localhost:3847/mcp"
  },
  "<server-id>": {
    "transport": "stdio",
    "command": "npx",
    "args": ["-y", "@example/mcp-server"]
  }
}
```

| Field | Type | REQUIRED | Description |
|-------|------|----------|-------------|
| `transport` | `string` | REQUIRED | `"http"` or `"stdio"`. |
| `url` | `string` | REQUIRED for `http` | Server URL. |
| `command` | `string` | REQUIRED for `stdio` | Executable path. |
| `args` | `string[]` | OPTIONAL | Command-line arguments. |
| `env` | `object` | OPTIONAL | Environment variables. Values MAY use `${VAR}` or `${VAR:-default}` expansion. |

Implementations MUST NOT start stdio servers or connect to HTTP servers
that are not referenced by at least one project definition or the server
registry.

## 4. Pattern Syntax

`allow`, `deny`, and `defaults` keys use **glob patterns** with the
following rules:

- `*` matches zero or more characters (but not `/`)
- `?` matches exactly one character (but not `/`)
- `[abc]` matches one character from the set
- `[!abc]` matches one character not in the set
- All other characters match literally
- Matching is **case-sensitive**

Patterns are matched against the **full tool name** as returned by the
MCP server's tool listing.

## 5. Resolution Algorithm

For a given `(project, server-id, toolName)` tuple, implementations
MUST resolve tool visibility as follows:

```
1. IF server-id is not present in project.tools
   THEN the tool is PERMITTED (no restrictions for this server).

2. IF allow is absent or empty
   THEN the tool is a CANDIDATE (all tools from this server are eligible).
   ELSE the tool MUST match at least one pattern in allow to be a CANDIDATE.

3. IF the tool is a CANDIDATE AND deny is present
   THEN the tool MUST NOT match any pattern in deny.

4. IF the tool is a CANDIDATE AND does not match any deny pattern
   THEN the tool is PERMITTED.
   ELSE the tool is DENIED.
```

**Deny MUST always take precedence over allow.** If a tool name matches
both an `allow` pattern and a `deny` pattern, it SHALL be denied.

### 5.1 Examples

Given:
```json
{
  "allow": ["github_*", "linear_*"],
  "deny": ["github_delete_*"]
}
```

| Tool Name | Allow Match | Deny Match | Result |
|-----------|-------------|------------|--------|
| `github_list_issues` | yes | no | PERMITTED |
| `github_delete_repo` | yes | yes | DENIED |
| `linear_search_issues` | yes | no | PERMITTED |
| `datadog_search_logs` | no | no | DENIED |

## 6. Default Arguments

### 6.1 Semantics

When `defaults` is present and a tool call matches one or more pattern
keys, the implementation MUST merge default arguments **under** the
agent-provided arguments:

```
final_args = merge(matched_defaults, agent_args)
```

Agent-provided arguments MUST take precedence over defaults. If the
agent supplies a key that also appears in a matching default, the
agent's value SHALL win.

### 6.2 Multiple Pattern Matches

If a tool name matches multiple patterns in `defaults`, implementations
MUST merge them in the order they appear in the JSON object, with later
patterns overriding earlier ones. The agent's explicit arguments are
then applied on top.

### 6.3 Example

```json
{
  "defaults": {
    "github_*": { "owner": "myorg" },
    "github_list_*": { "owner": "myorg", "per_page": "50" }
  }
}
```

An agent calling `github_list_issues` with `{"state": "open"}` produces:

```json
{ "owner": "myorg", "per_page": "50", "state": "open" }
```

An agent calling `github_list_issues` with `{"owner": "other", "state": "open"}`:

```json
{ "owner": "other", "per_page": "50", "state": "open" }
```

## 7. Tool Proxy Responsibilities

### 7.1 Filtering

A tool proxy that is project-aware SHOULD filter its tool listing
(`tools/list` response) to only include tools that are PERMITTED for
the active project. This reduces context usage for agents that rely on
tool discovery.

A tool proxy that is NOT project-aware MAY ignore tool scoping entirely.
In this case, the **agent host** is responsible for applying the
resolution algorithm before presenting tools to the agent.

### 7.2 Default Injection

Default argument injection MAY be performed by the tool proxy, the agent
host, or any middleware in the call chain. Exactly one component in the
chain MUST perform injection; implementations MUST NOT apply defaults
more than once.

### 7.3 Enforcement

Enforcement of `deny` rules is RECOMMENDED at the tool proxy level but
MAY be implemented at the agent host level instead. If both layers
enforce, the deny union applies (a tool denied by either layer is
denied).

## 8. Merging Across Layers

When tool scoping rules exist at multiple discovery layers (see
[RFC-0001 §5](rfc-0001-project-definition.md#5-merging)):

- `allow` arrays MUST be concatenated (union of patterns)
- `deny` arrays MUST be concatenated (union of patterns)
- `defaults` objects MUST be merged per pattern key, with
  higher-precedence layers winning per key
