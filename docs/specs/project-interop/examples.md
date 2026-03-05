# Example: Full Project Definition

This is a complete, annotated example of a project definition file.

```jsonc
// ~/.config/project-interop/projects/acme-api.project.json
{
  "$schema": "https://project-interop.dev/schemas/v1/project.schema.json",
  "version": "1",
  "name": "acme-api",
  "repo": "~/work/acme-api",
  "branch": "main",

  // Launch configuration: controls how agents are bootstrapped
  "launch": {
    "prompt": "You are working on the acme-api project. Do NOT read AGENTS.md or other convention files directly. Instead, use the project_context tool to retrieve project context and the search tool to discover available operations. Start by calling project_context with no arguments to see what context is available.",
    "env": {
      "PROJECT_NAME": "acme-api",
      "PROJECT_ENV": "development"
    }
  },

  "tools": {
    // Scoping for an MCP tool proxy (registered in servers.json)
    "my-proxy": {
      "allow": [
        "github_*",
        "linear_*",
        "datadog_search_logs",
        "datadog_list_monitors",
        "postgres_*",
        "slack_search_messages",
        "slack_post_message"
      ],
      "deny": [
        "github_delete_repo",
        "*_drop_*"
      ],
      "defaults": {
        "github_*": { "owner": "acme-corp", "repo": "acme-api" },
        "linear_*": { "team_id": "ENG" },
        "postgres_*": { "database": "acme_development" },
        "slack_*": { "channel": "#eng-backend" }
      }
    },
    // A filesystem MCP server — restrict to read-only operations
    "filesystem": {
      "allow": ["read_file", "list_directory", "search_files"],
      "deny": ["write_file", "delete_file"]
    }
  },

  "context": {
    "files": [
      "onboarding.md",
      "current-sprint-goals.md",
      "db-schema-notes.md"
    ],
    "repoIncludes": [
      "AGENTS.md",
      "docs/architecture.md",
      "docs/data-model.md"
    ],
    "maxBytes": 262144
  },

  "agents": {
    "maxConcurrent": 2,
    "roles": {
      "backend": {
        "description": "Backend feature implementation and API development",
        "toolOverrides": {
          "my-proxy": {
            "allow": ["github_*", "postgres_*", "datadog_*"]
          }
        }
      },
      "reviewer": {
        "description": "Code review and quality assurance",
        "toolOverrides": {
          "my-proxy": {
            "allow": ["github_get_*", "github_list_*", "github_search_*"],
            "deny": ["github_create_*", "github_update_*", "github_delete_*", "github_merge_*"]
          }
        },
        "contextOverrides": {
          "files": ["review-checklist.md"]
        }
      }
    }
  },

  "extensions": {
    "com.example.workspace-mgr": {
      "gui": ["firefox https://app.acme.com"],
      "layout": "tile"
    }
  }
}
```

## Server Registry

The server registry uses `{project}` as a template variable in the URL.
Workspace managers expand this to the project name at launch time.

```jsonc
// ~/.config/project-interop/servers.json
{
  "my-proxy": {
    "transport": "http",
    "url": "http://localhost:3847/mcp/{project}"
  },
  "filesystem": {
    "transport": "stdio",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/user"]
  }
}
```

When launching agents for `acme-api`, the workspace manager expands the
URL to `http://localhost:3847/mcp/acme-api`. The proxy at that endpoint
applies tool scoping and serves context for the `acme-api` project.

## Context Store

```
~/.config/project-interop/context/acme-api/
├── onboarding.md
├── current-sprint-goals.md
├── db-schema-notes.md
└── review-checklist.md
```

## Repo-Local Override

```jsonc
// ~/work/acme-api/.project.json  (committed to the repo)
{
  "version": "1",
  "name": "acme-api",
  "context": {
    "repoIncludes": [
      "AGENTS.md",
      "docs/architecture.md",
      "docs/data-model.md",
      "docs/api-reference.md"
    ]
  }
}
```

This repo-local file adds `docs/api-reference.md` to the context
includes. After merging (RFC-0001 §5), the assembled `repoIncludes`
contains all four files.

## Launch Flow Example

A workspace manager launching an agent on `acme-api` performs these
steps:

```
1. Read ~/.config/project-interop/projects/acme-api.project.json
2. Resolve server URLs:
   "http://localhost:3847/mcp/{project}" → "http://localhost:3847/mcp/acme-api"
3. Set environment variables from launch.env:
   PROJECT_NAME=acme-api PROJECT_ENV=development
4. Inject launch.prompt via the agent host's system prompt mechanism
5. Start the agent host, pointing it at the expanded MCP URL
```

### Claude Code

```bash
claude --append-system-prompt "You are working on the acme-api project. ..." \
       --mcp-server http://localhost:3847/mcp/acme-api
```

### Codex / Generic CLI Agent

```bash
PROJECT_NAME=acme-api PROJECT_ENV=development \
  codex --system-prompt "You are working on the acme-api project. ..."
```

The launch prompt is the same across all agent hosts — only the
injection mechanism differs.
