---
title: "ARP — Workspace Management Service"
version: 0.4.0
created: 2026-04-06
updated: 2026-05-01
status: draft
tags: [arp, grpc, protobuf, workspace, worktree]
---

# Workspace Management Service

gRPC service for creating and destroying isolated workspaces where agents operate. A workspace is typically a git worktree — an independent working directory branched from a project's repository.

Workspaces sit in the middle of the hierarchy: **Project → Workspace → Agent**. One project can have many workspaces. Each workspace can host multiple agents that share the filesystem but have independent A2A sessions.

## Service Definition

```protobuf
syntax = "proto3";

package arp.v1;

import "google/api/annotations.proto";
import "google/api/field_behavior.proto";
import "google/protobuf/empty.proto";

// WorkspaceService manages isolated workspaces (git worktrees) where agents operate.
service WorkspaceService {
  // CreateWorkspace creates a new isolated workspace for a project.
  // Creates a git worktree and optionally auto-spawns agents.
  rpc CreateWorkspace(CreateWorkspaceRequest) returns (Workspace) {
    option (google.api.http) = {
      post: "/v1/workspaces"
      body: "*"
    };
  }

  // ListWorkspaces lists all workspaces with their agents and status.
  rpc ListWorkspaces(ListWorkspacesRequest) returns (ListWorkspacesResponse) {
    option (google.api.http) = {
      get: "/v1/workspaces"
    };
  }

  // GetWorkspace returns detailed information about a specific workspace.
  rpc GetWorkspace(GetWorkspaceRequest) returns (Workspace) {
    option (google.api.http) = {
      get: "/v1/workspaces/{name}"
    };
  }

  // DestroyWorkspace destroys a workspace, stopping all agents and removing the worktree.
  rpc DestroyWorkspace(DestroyWorkspaceRequest) returns (google.protobuf.Empty) {
    option (google.api.http) = {
      delete: "/v1/workspaces/{name}"
    };
  }
}
```

## Messages

### CreateWorkspace

```protobuf
message CreateWorkspaceRequest {
  // Workspace name (used as worktree branch name).
  string name = 1 [(google.api.field_behavior) = REQUIRED];

  // Project to create workspace for.
  string project = 2 [(google.api.field_behavior) = REQUIRED];

  // Git branch (default: project default branch).
  string branch = 3;

  // Agent template names to auto-spawn after workspace creation.
  repeated string auto_agents = 4;
}
```

**Returns:** `Workspace` message with `status: WORKSPACE_STATUS_ACTIVE` and empty `agents` list (unless `auto_agents` specified).

**Behavior:**
1. Creates a git worktree at the resolved directory path for the workspace
2. Allocates a tag index (for window manager integration, if applicable)
3. If `auto_agents` is provided, calls `SpawnAgent` for each template name listed
4. Persists workspace state

**Example (via gRPC-Web / HTTP transcoding):**

```bash
# Create a workspace
grpcurl -d '{"name": "feat-auth", "project": "myapp"}' \
  localhost:9099 arp.v1.WorkspaceService/CreateWorkspace

# Create with auto-spawned agents
grpcurl -d '{"name": "feat-auth", "project": "myapp", "auto_agents": ["crush", "crush"]}' \
  localhost:9099 arp.v1.WorkspaceService/CreateWorkspace
```

### ListWorkspaces

```protobuf
message ListWorkspacesRequest {
  // Filter by project name.
  string project = 1;

  // Filter by status.
  WorkspaceStatus status = 2;
}

message ListWorkspacesResponse {
  repeated Workspace workspaces = 1;
}
```

**Example response (JSON transcoding):**

```json
{
  "workspaces": [
    {
      "name": "feat-auth",
      "project": "myapp",
      "dir": "/home/user/src/myapp/worktrees/feat-auth",
      "status": "WORKSPACE_STATUS_ACTIVE",
      "agents": [
        {
          "id": "coder-abc123",
          "template": "crush",
          "status": "AGENT_STATUS_READY",
          "port": 9100,
          "direct_url": "http://localhost:9100",
          "proxy_url": "http://localhost:9099/a2a/agents/coder-abc123"
        },
        {
          "id": "reviewer-def456",
          "template": "crush",
          "status": "AGENT_STATUS_BUSY",
          "port": 9101,
          "direct_url": "http://localhost:9101",
          "proxy_url": "http://localhost:9099/a2a/agents/reviewer-def456"
        }
      ],
      "created_at": "2026-04-06T10:00:00Z"
    }
  ]
}
```

### GetWorkspace

```protobuf
message GetWorkspaceRequest {
  // Workspace name.
  string name = 1 [(google.api.field_behavior) = REQUIRED];
}
```

**Returns:** Full `Workspace` message with resolved `AgentCard` per agent instance.

### DestroyWorkspace

```protobuf
message DestroyWorkspaceRequest {
  // Workspace name.
  string name = 1 [(google.api.field_behavior) = REQUIRED];

  // Keep the git worktree on disk (default: false).
  bool keep_worktree = 2;
}
```

**Behavior:**
1. For each agent in the workspace:
   - Cancels any A2A tasks in `TASK_STATE_WORKING` via `CancelTask`
   - Sends SIGTERM, waits grace period, then SIGKILL if needed
2. Removes the workspace from ARP state
3. If `keep_worktree` is false (default), removes the git worktree directory
4. Frees allocated ports and tag indices

## gRPC Status Codes

| Condition | gRPC Status | Description |
|-----------|-------------|-------------|
| Workspace not found | `NOT_FOUND` | GetWorkspace/DestroyWorkspace with unknown name |
| Project not found | `NOT_FOUND` | CreateWorkspace referencing unknown project |
| Workspace already exists | `ALREADY_EXISTS` | CreateWorkspace with duplicate name |
| Missing required field | `INVALID_ARGUMENT` | name or project not provided |
| Permission denied | `PERMISSION_DENIED` | Token permission insufficient |
| Scope violation | `PERMISSION_DENIED` | Project not in caller's token scope |
