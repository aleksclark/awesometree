# Reference Implementations

Two minimal Go programs demonstrating how to build a project-scoped MCP
proxy per the project-interop spec.

## tool-proxy

Serves context via a `project_context` MCP tool (progressive disclosure).
This is the **recommended** approach — it works with all agent hosts that
support MCP tools.

```bash
cd tool-proxy
go run main.go -addr :4567 -config ~/.config/project-interop
```

Agents connect to `http://localhost:4567/mcp/{project-name}` and call:
- `project_context()` — list available context files
- `project_context(query: "architecture")` — search context by keyword
- `project_context(path: "AGENTS.md")` — fetch a specific file

## resource-proxy

Serves context via MCP resources using `project://` URIs. This approach
is more idiomatic for MCP but depends on the agent host supporting
resource auto-injection.

```bash
cd resource-proxy
go run main.go -addr :4568 -config ~/.config/project-interop
```

Resources exposed at `http://localhost:4568/mcp/{project-name}`:
- `project://{name}/context` — manifest of all context files
- `project://{name}/context/{path}` — individual file content

## Prerequisites

Both require the Go MCP SDK:

```
go 1.22+
github.com/modelcontextprotocol/go-sdk v1.3.1
```

Run `go mod tidy` in either directory to resolve dependencies before
building.
