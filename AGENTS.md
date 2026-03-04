# AwesomeWM Workspace System

Dynamic project workspaces: AwesomeWM + Zed + git worktrees.

## What It Does

`Super+O` picks a project. The system creates a git worktree,
opens Zed, and assigns everything to a dedicated `P:` tag.
`Super+L` cycles between project tags.

## Components

| Layer | File | Role |
|-------|------|------|
| WM config | `~/.config/awesome/rc.lua` | Keybindings, tags |
| Lua module | `~/.config/awesome/workspaces.lua` | Tag management |
| Python CLI | `~/.local/bin/ws` | Worktree + config |
| Shared tags | `~/.config/awesome/sharedtags/` | Multi-screen |
| Config | `~/.config/workspaces.toml` | Definitions |

## Detailed Docs

- [Architecture](docs/architecture.md)
- [Keybindings](docs/keybindings.md)
- [Lifecycle](docs/workspace-system/lifecycle.md)
- [ws CLI](docs/workspace-system/ws-cli.md)
- [Lua Module](docs/workspace-system/lua-module.md)
- [Configuration](docs/workspace-system/configuration.md)
