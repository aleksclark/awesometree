---
title: "ARP — WorkSession Management Tools"
version: 0.3.0
created: 2026-04-06
updated: 2026-04-28
status: draft
tags: [arp, mcp, workspace, worktree]
---

# WorkSession Management Tools

MCP tools for creating and destroying WorkSessions. Local realization creates a material Workspace resource (typically a git worktree). Switchboard is the authority for WorkSession lifecycle.

Hierarchy: **Project → WorkSession → Agent**, with Workspace as a material Resource binding only.

## `work_session/create`

Create a new WorkSession and realize a local Workspace resource.

```json
{
  "name": "work_session/create",
  "description": "Create a new isolated workspace for a project. Creates a git worktree and optionally spawns agents. Does NOT spawn agents by default — use agent/spawn after creation.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "name": { "type": "string", "description": "Workspace name (used as worktree branch name)" },
      "project": { "type": "string", "description": "Project to create workspace for" },
      "branch": { "type": "string", "description": "Git branch (default: project default branch)" },
      "auto_agents": {
        "type": "array",
        "description": "Agent template names to auto-spawn after workspace creation",
        "items": { "type": "string" }
      }
    },
    "required": ["name", "project"]
  }
}
```

**Returns:** `Workspace` object with `status: "active"` and empty `agents` array (unless `auto_agents` specified).

**Behavior:**
1. Creates a git worktree at the resolved directory path for the workspace
2. Allocates a tag index (for window manager integration, if applicable)
3. If `auto_agents` is provided, calls `agent/spawn` for each template name listed
4. Persists workspace state

**Example:**

```
work_session/create  name="feat-auth" project="myapp"
work_session/create  name="feat-auth" project="myapp" auto_agents=["crush", "crush"]
```

## `work_session/list`

List all workspaces with their agents and status.

```json
{
  "name": "work_session/list",
  "description": "List all workspaces with agent status. Filter by project or status.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "project": { "type": "string", "description": "Filter by project name" },
      "status": { "type": "string", "enum": ["active", "inactive"], "description": "Filter by status" }
    }
  }
}
```

**Returns:** Array of `Workspace` objects (see [Data Model](overview.md#workspace)).

**Example response:**

```json
[
  {
    "name": "feat-auth",
    "project": "myapp",
    "dir": "/home/user/src/myapp/worktrees/feat-auth",
    "status": "active",
    "agents": [
      {
        "id": "coder-abc123",
        "template": "crush",
        "status": "ready",
        "port": 9100,
        "direct_url": "http://localhost:9100",
        "proxy_url": "http://localhost:9099/a2a/agents/coder-abc123"
      },
      {
        "id": "reviewer-def456",
        "template": "crush",
        "status": "busy",
        "port": 9101,
        "direct_url": "http://localhost:9101",
        "proxy_url": "http://localhost:9099/a2a/agents/reviewer-def456"
      }
    ],
    "created_at": "2026-04-06T10:00:00Z"
  }
]
```

## `work_session/get`

Get detailed information about a specific workspace.

```json
{
  "name": "work_session/get",
  "description": "Get full details of a workspace including all agent instances and their status.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "name": { "type": "string", "description": "Workspace name" }
    },
    "required": ["name"]
  }
}
```

**Returns:** Full `Workspace` object with resolved `AgentCard` per agent instance.

## `work_session/destroy`

Destroy a workspace, stopping all agents and removing the worktree.

```json
{
  "name": "work_session/destroy",
  "description": "Destroy a workspace. Stops all agents, removes the git worktree, and cleans up all state.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "name": { "type": "string", "description": "Workspace name" },
      "keep_worktree": { "type": "boolean", "description": "Keep the git worktree on disk (default: false)" }
    },
    "required": ["name"]
  }
}
```

**Behavior:**
1. For each agent in the workspace:
   - Cancels any A2A tasks in `TASK_STATE_WORKING` via `CancelTask`
   - Sends SIGTERM, waits grace period, then SIGKILL if needed
2. Removes the workspace from ARP state
3. If `keep_worktree` is false (default), removes the git worktree directory
4. Frees allocated ports and tag indices
