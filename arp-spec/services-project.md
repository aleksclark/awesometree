---
title: "ARP — Project Management Service"
version: 0.4.0
created: 2026-04-06
updated: 2026-05-01
status: draft
tags: [arp, grpc, protobuf, project]
---

# Project Management Service

gRPC service for registering and managing code repository projects. A project defines what agent templates are available and how agents should be configured when spawned in workspaces for that project.

Projects are the top of the hierarchy: **Project → Workspace → Agent**. You register a project once, then create many workspaces from it, each with their own agents.

## Service Definition

```protobuf
syntax = "proto3";

package arp.v1;

import "google/api/annotations.proto";
import "google/api/field_behavior.proto";
import "google/protobuf/empty.proto";

// ProjectService manages code repository projects and their agent templates.
service ProjectService {
  // ListProjects returns all registered projects with their agent templates.
  rpc ListProjects(ListProjectsRequest) returns (ListProjectsResponse) {
    option (google.api.http) = {
      get: "/v1/projects"
    };
  }

  // RegisterProject registers a new project or updates an existing one.
  rpc RegisterProject(RegisterProjectRequest) returns (Project) {
    option (google.api.http) = {
      post: "/v1/projects"
      body: "*"
    };
  }

  // UnregisterProject removes a project registration.
  // Running workspaces for this project are not affected.
  rpc UnregisterProject(UnregisterProjectRequest) returns (google.protobuf.Empty) {
    option (google.api.http) = {
      delete: "/v1/projects/{name}"
    };
  }
}
```

## Messages

### ListProjects

```protobuf
message ListProjectsRequest {
  // No fields — returns all projects visible to the caller's token scope.
}

message ListProjectsResponse {
  repeated Project projects = 1;
}
```

### RegisterProject

```protobuf
message RegisterProjectRequest {
  // Unique project identifier.
  string name = 1 [(google.api.field_behavior) = REQUIRED];

  // Absolute path to the git repository.
  string repo = 2 [(google.api.field_behavior) = REQUIRED];

  // Default branch name (default: "main").
  string branch = 3;

  // Agent templates available for this project. Each template defines
  // a command to start an A2A agent process.
  repeated AgentTemplate agents = 4;
}
```

**Returns:** The registered `Project` message.

**Agent templates** in the `agents` field define:
- `command` — how to start the agent process (e.g., `"crush serve"`)
- `port_env` — which env var receives the assigned port (e.g., `"A2A_PORT"`)
- `health_check.path` — endpoint to probe for readiness (e.g., `"/.well-known/agent-card.json"`)
- `a2a_card_config` — fields that populate the agent's A2A `AgentCard` (`name`, `description`, `skills[]`, `capabilities`)

See [AgentTemplate](overview.md#agenttemplate) in the data model for the full message definition.

### UnregisterProject

```protobuf
message UnregisterProjectRequest {
  // Project name to unregister.
  string name = 1 [(google.api.field_behavior) = REQUIRED];
}
```

**Returns:** `google.protobuf.Empty` on success.

**Note:** You cannot unregister a project that has active workspaces with running agents without first stopping those agents or destroying those workspaces. The server returns `FAILED_PRECONDITION` if active agents exist.

## Example: ListProjects Response

```json
{
  "projects": [
    {
      "name": "myapp",
      "repo": "/home/user/src/myapp",
      "branch": "main",
      "agents": [
        {
          "name": "crush",
          "command": "crush serve",
          "port_env": "A2A_PORT",
          "a2a_card_config": {
            "name": "Crush",
            "description": "AI coding assistant",
            "skills": [
              {
                "id": "code",
                "name": "Code",
                "description": "Write and debug code",
                "tags": ["coding"]
              }
            ]
          }
        }
      ]
    }
  ]
}
```

## gRPC Status Codes

| Condition | gRPC Status | Description |
|-----------|-------------|-------------|
| Project not found | `NOT_FOUND` | UnregisterProject with unknown name |
| Active agents exist | `FAILED_PRECONDITION` | UnregisterProject when agents are running |
| Missing required field | `INVALID_ARGUMENT` | name or repo not provided |
| Permission denied | `PERMISSION_DENIED` | Token lacks `admin` permission (register/unregister require admin) |
| Scope violation | `PERMISSION_DENIED` | Project not in caller's token scope |
