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
- `--nogui`: Shorthand for `--no-tag --no-launch`
- `--headless`: Set up the worktree and launch a bezalel MCP server for the
  workspace without creating a window-manager tag or launching GUI apps. The
  workspace is still marked active; its bezalel URL and auth token appear in
  `list`. Implies `--no-tag --no-launch`.
- `--keep-worktree`: Keep worktree on `down`

## Headless Workspaces

`up`/`create --headless` provisions a workspace for remote/agent use:

- A git worktree is created exactly as for a normal workspace.
- No AwesomeWM tag is created and no configured apps are launched.
- A bezalel MCP server is started (managed by the daemon), bound to
  `127.0.0.1` on a port in the `9200-9299` range, with `--workdir` set to the
  worktree and a freshly generated bearer token.
- `awesometree list` marks the workspace `(headless)` and prints the bezalel
  URL (`http://127.0.0.1:<port>/mcp`) and token. The REST API exposes the same
  via `headless`, `bezalel_port`, `bezalel_url`, and `bezalel_token` fields, and
  `POST /api/work-sessions` accepts a `"headless": true` flag.

See: [Configuration](configuration.md) | [Lifecycle](lifecycle.md)
