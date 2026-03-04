# Keybindings

All use `Super` (Mod4) as modifier.

| Key | Action | Function |
|-----|--------|----------|
| `Super+O` | Open picker | `show_picker()` |
| `Super+I` | Create workspace | `show_create()` |
| `Super+D` | Destroy current | `destroy_current()` |
| `Super+J` | Close current | `close_current()` |
| `Super+L` | Cycle next tag | `cycle_next()` |

## Picker (`Super+O`)

Menu of all workspaces. Active ones marked with `●`.
Selecting inactive runs `ws up` (worktree + Zed).

## Create (`Super+I`)

Three-step prompt: name, repo (tab-complete), branch
(tab-complete). Pre-fills from `[defaults]` in config.

## Destroy vs Close

- **Destroy** (`D`): Blocks if dirty. Removes worktree,
  config entry, clients, and tag.
- **Close** (`J`): Keeps worktree. Removes state, clients,
  tag. Re-open later via picker.

See: [Lifecycle](workspace-system/lifecycle.md)
