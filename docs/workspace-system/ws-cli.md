# CLI Reference

Binary: `awesometree` (installed to `~/.local/bin/`).

## Workspace Commands

| Command | Description |
|---------|-------------|
| `up [name]` | Start one or all active workspaces |
| `down [name]` | Tear down one or all workspaces |
| `create <name> --project <p>` | Create workspace under project |
| `destroy <name>` | Remove worktree + config entry |
| `destroy-current` | Destroy workspace of focused tag |
| `close` | Close focused workspace, keep worktree |
| `cycle` | Focus next active project tag |
| `switch <name>` | Focus a specific workspace tag |
| `list` | Print projects and workspace status |

## Interactive (require daemon)

| Command | Description |
|---------|-------------|
| `pick` | Open GPUI workspace picker |
| `create-interactive` | Open GPUI create form |
| `projects-ui` | Open GPUI project manager |

## Query Commands

| Command | Description |
|---------|-------------|
| `repos` | Git repos in `~/work/` |
| `names` | Active workspace names |
| `allnames` | All configured workspace names |
| `dir <name>` | Print workspace directory |
| `projects` | List project names |
| `edit` | Open config in `$EDITOR` |

## Daemon Commands

| Command | Description |
|---------|-------------|
| `daemon` | Fork `awesometree-daemon` |
| `restart-daemon` | Send restart + relaunch |

## Common Flags

- `--no-tag`: Skip AwesomeWM tag creation/deletion
- `--no-launch`: Skip launching Zed and GUI apps
- `--keep-worktree`: Keep worktree on `down`

See: [Configuration](configuration.md) | [Lifecycle](lifecycle.md)
