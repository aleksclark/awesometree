# Architecture

## Data Flow

```
Super+O -> rc.lua -> workspaces.lua -> `ws` CLI
  -> worktree + Lua eval via awesome-client
  -> Zed launches; manage signal assigns to P: tag
```

## Layers

**rc.lua**: Binds `Super+O/I/D/J/L` to workspace functions.
Connects `manage` signal for auto-assigning clients.

**workspaces.lua**: Creates/finds `P:` tags, shows picker,
cycles tags, matches Zed windows by title to projects.

**ws (Python)**: TOML config, JSON state, git worktree
creation/removal, evals Lua via `awesome-client`.

**sharedtags**: Third-party lib for tags shared across
screens, sorted by `sharedtagindex`.

## State Files

| File | Format | Purpose |
|------|--------|---------|
| `~/.config/workspaces.toml` | TOML | Definitions |
| `~/.local/state/workspaces.json` | JSON | Runtime |
| `~/worktrees/` | dirs | Checkouts |

See: [Lifecycle](workspace-system/lifecycle.md) |
[CLI](workspace-system/ws-cli.md) |
[Lua Module](workspace-system/lua-module.md)
