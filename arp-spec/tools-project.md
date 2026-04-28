---
title: "ARP — Project Management Tools"
version: 0.3.0
created: 2026-04-06
updated: 2026-04-28
status: draft
tags: [arp, mcp, project]
---

# Project Management Tools

MCP tools for registering and managing code repository projects. A project defines what agent templates are available and how agents should be configured when spawned in workspaces for that project.

Projects are the top of the hierarchy: **Project → Workspace → Agent**. You register a project once, then create many workspaces from it, each with their own agents.

## `project/list`

List registered projects.

```json
{
  "name": "project/list",
  "description": "List all registered projects with their agent templates.",
  "inputSchema": {
    "type": "object",
    "properties": {}
  }
}
```

**Returns:** Array of `Project` objects (see [Data Model](overview.md#project)).

**Example response:**

```json
[
  {
    "name": "myapp",
    "repo": "/home/user/src/myapp",
    "branch": "main",
    "agents": [
      {
        "name": "crush",
        "command": "crush serve",
        "port_env": "A2A_PORT",
        "a2a": {
          "name": "Crush",
          "description": "AI coding assistant",
          "skills": [{ "id": "code", "name": "Code", "description": "Write and debug code", "tags": ["coding"] }]
        }
      }
    ]
  }
]
```

## `project/register`

Register a new project or update an existing one.

```json
{
  "name": "project/register",
  "description": "Register a project repository. Defines what agents are available and how they're configured.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "name": { "type": "string", "description": "Unique project identifier" },
      "repo": { "type": "string", "description": "Absolute path to the git repository" },
      "branch": { "type": "string", "description": "Default branch name (default: main)" },
      "agents": {
        "type": "array",
        "description": "Agent templates available for this project. Each template defines a command to start an A2A agent process.",
        "items": { "$ref": "#/definitions/AgentTemplate" }
      }
    },
    "required": ["name", "repo"]
  }
}
```

**Returns:** The registered `Project` object.

**Agent templates** in the `agents` array define:
- `command` — how to start the agent process (e.g., `"crush serve"`)
- `port_env` — which env var receives the assigned port (e.g., `"A2A_PORT"`)
- `health_check.path` — endpoint to probe for readiness (e.g., `"/.well-known/agent-card.json"`)
- `a2a` — fields that populate the agent's A2A `AgentCard` (`name`, `description`, `skills[]`, `capabilities`)

See [AgentTemplate](overview.md#agenttemplate) in the data model for the full type definition.

## `project/unregister`

Remove a project registration. Does not affect running workspaces — agents already spawned in workspaces for this project continue running.

```json
{
  "name": "project/unregister",
  "description": "Unregister a project. Running workspaces for this project are not affected.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "name": { "type": "string", "description": "Project name to unregister" }
    },
    "required": ["name"]
  }
}
```

**Returns:** Confirmation of removal.

**Note:** You cannot unregister a project that has active workspaces with running agents without first stopping those agents or destroying those workspaces.
