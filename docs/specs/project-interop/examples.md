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

```jsonc
// ~/.config/project-interop/servers.json
{
  "my-proxy": {
    "transport": "http",
    "url": "http://localhost:3847/mcp"
  },
  "filesystem": {
    "transport": "stdio",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/user"]
  }
}
```

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
