# awesometree

Rust workspace manager: AwesomeWM + Zed + git worktrees.

Two binaries built from one crate. `awesometree` is the CLI.
`awesometree-daemon` is a long-running GPUI process that owns the
picker window, create form, projects UI, system tray, and error
notifications.

## How It Works

`Super+O` sends `pick` to the daemon, which opens a GPUI picker.
Selecting an inactive workspace creates a git worktree, creates an
AwesomeWM `P:` tag via `awesome-client`, and launches Zed.
`Super+L` cycles between active project tags.

## Components

| Layer | Source | Role |
|-------|--------|------|
| CLI | `src/main.rs` | All subcommands (`up`, `down`, `create`, …) |
| Daemon | `src/daemon_main.rs` | GPUI app, socket listener, tray |
| Config | `src/config.rs` | JSON load/save, project/workspace model |
| Workspace | `src/workspace.rs` | `Manager` — worktree, tag, app lifecycle |
| WM adapter | `src/wm.rs` | `Adapter` trait; `AwesomeAdapter` via `awesome-client` |
| Picker | `src/picker.rs` | GPUI fuzzy picker + create form |
| Projects UI | `src/projects_ui.rs` | GPUI project CRUD window |
| Tray | `src/tray.rs` | System tray icon + popup menu |
| Notifications | `src/notify.rs` | Error windows, background task runner |

## Build & Install

```sh
make install   # cargo build --release → ~/.local/bin/
```

## Detailed Docs

- [Architecture](docs/architecture.md)
- [Keybindings](docs/keybindings.md)
- [Lifecycle](docs/workspace-system/lifecycle.md)
- [CLI Reference](docs/workspace-system/ws-cli.md)
- [Configuration](docs/workspace-system/configuration.md)
- [WM Integration](docs/workspace-system/lua-module.md)
