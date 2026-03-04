# Architecture

## Data Flow

```
Super+O → awesometree pick → Unix socket → awesometree-daemon
  → GPUI picker window → user selects workspace
  → Manager::up() → git worktree + awesome-client Lua eval + Zed launch
```

## Binaries

**awesometree** (`src/main.rs`): Stateless CLI. Parses args via
`clap`, loads JSON config, dispatches to `Manager` or sends a
command to the daemon socket. Interactive commands (`pick`,
`create-interactive`, `projects-ui`) require the daemon.

**awesometree-daemon** (`src/daemon_main.rs`): Long-running GPUI
application. Listens on `/tmp/awesometree.sock`, spawns the tray
icon thread, and opens GPUI windows for the picker, create form,
projects editor, and error notifications.

## Key Abstractions

**`Config`** (`src/config.rs`): Serialized to
`~/.config/awesometree/config.json`. Contains a `Vec<Project>`,
each with nested `Vec<WorkspaceEntry>`.

**`Manager`** (`src/workspace.rs`): Owns a `Config` and a
`Box<dyn Adapter>`. Methods: `up`, `down`, `switch`, `is_dirty`.
Handles worktree creation/removal and config persistence.

**`Adapter`** trait (`src/wm.rs`): WM operations —
`create_tag`, `delete_tag`, `switch_tag`, `kill_tag_clients`,
`get_current_tag_name`, `restore_previous_tag`.
`AwesomeAdapter` implements these via `awesome-client` Lua eval.

**`DaemonCmd`** (`src/daemon.rs`): Enum of socket commands
(`Pick`, `Create`, `Projects`, `Restart`, `Reload`).

## State

| File | Format | Purpose |
|------|--------|---------|
| `~/.config/awesometree/config.json` | JSON | Projects + workspaces |
| `/tmp/awesometree.sock` | Unix socket | CLI ↔ daemon IPC |
| `~/worktrees/<project>/<name>` | dirs | Git worktree checkouts |
| `/tmp/ws-current-tag` | text | Transient tag name relay |

See: [Lifecycle](workspace-system/lifecycle.md) |
[CLI](workspace-system/ws-cli.md) |
[Configuration](workspace-system/configuration.md)
