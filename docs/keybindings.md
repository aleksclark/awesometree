# Keybindings

All bound in `rc.lua` via `awful.key` (see `rc.lua.example`).

| Key | Command | Effect |
|-----|---------|--------|
| `Super+O` | `awesometree pick` | Open picker (daemon) |
| `Super+I` | `awesometree create-interactive` | Create form (daemon) |
| `Super+D` | `awesometree destroy-current` | Dirty-check + destroy |
| `Super+J` | `awesometree close` | Close, keep worktree |
| `Super+L` | `awesometree cycle` | Cycle project tags |

## Picker (`Super+O`)

GPUI window with fuzzy filtering. Workspaces grouped by project;
active ones marked with a green dot. Selecting an inactive
workspace runs `Manager::up`. `Ctrl+N` or the `+` button opens
the create form.

## Create Form (`Super+I`)

GPUI form: name, project (fuzzy dropdown), and — for new
projects — repo path and branch. Tab between fields; Enter to
confirm.

## Destroy vs Close

- **Destroy** (`Super+D` / `awesometree destroy-current`): Aborts if
  `git status --porcelain` shows changes. Removes worktree, kills tag
  clients, deletes tag, deletes the Switchboard WorkSession.
- **Close** (`Super+J` / `awesometree close`): Keeps worktree on disk.
  Kills clients, deletes tag, transitions WorkSession to `closed`.
  Re-open via picker / create.

See: [Lifecycle](workspace-system/lifecycle.md)
