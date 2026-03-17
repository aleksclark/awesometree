# awesometree

Rust workspace manager: AwesomeWM + Zed + git worktrees.

Cargo workspace with two crates. `awesometree` is the main crate
producing two binaries: `awesometree` (CLI) and `awesometree-daemon`
(GPUI process with picker, projects UI, system tray, QR code window,
and HTTP server). `awesometree-core` is a shared Rust API client
library with UniFFI bindings for the Android mobile app.

## How It Works

`Super+O` sends `pick` to the daemon, which opens a GPUI picker.
Selecting an inactive workspace creates a git worktree, creates an
AwesomeWM `P:` tag via `awesome-client`, and launches Zed.
`Super+L` cycles between active project tags.

The daemon also runs an HTTP server (port 9099) with a REST API for
workspace/project CRUD and an ACP reverse proxy. The mobile app
connects by scanning a QR code from the tray menu.

## Components

| Layer | Source | Role |
|-------|--------|------|
| CLI | `src/main.rs` | All subcommands (`up`, `down`, `create`, …) |
| Daemon | `src/daemon_main.rs` | GPUI app, socket listener, tray |
| Config | `src/config.rs` | JSON load/save, project/workspace model |
| Workspace | `src/workspace.rs` | `Manager` — worktree, tag, app lifecycle |
| WM adapter | `src/wm.rs` | `Adapter` trait; `AwesomeAdapter` via `awesome-client` |
| HTTP/ACP | `src/server.rs` | REST API, ACP reverse proxy (axum + tokio) |
| Auth | `src/auth.rs` | HMAC token generation/validation for remote clients |
| QR code | `src/qr.rs` | QR code generation + GPUI display window |
| Picker | `src/picker.rs` | GPUI fuzzy picker + create form |
| Projects UI | `src/projects_ui.rs` | GPUI project CRUD window |
| Tray | `src/tray.rs` | System tray icon + popup menu |
| Notifications | `src/notify.rs` | Error windows, background task runner |
| Core lib | `core/` | Shared API client crate with UniFFI for Android |
| Android | `android/` | Kotlin/Compose mobile app |

## Build & Install

```sh
make install   # cargo build --release → ~/.local/bin/
make test      # cargo test --workspace
make openapi   # print OpenAPI spec to stdout
```

## Android App

The mobile app lives in `android/`. It uses Jetpack Compose with
Material 3 (Catppuccin Mocha theme) and connects to the desktop server
via the REST API. Core API client logic is in `core/` (Rust + UniFFI).

Screens: Workspaces, Projects, ACP Agent Chat, Settings/QR Scanner.

## Detailed Docs

- [Architecture](docs/architecture.md)
- [Keybindings](docs/keybindings.md)
- [Lifecycle](docs/workspace-system/lifecycle.md)
- [CLI Reference](docs/workspace-system/ws-cli.md)
- [Configuration](docs/workspace-system/configuration.md)
- [WM Integration](docs/workspace-system/lua-module.md)
