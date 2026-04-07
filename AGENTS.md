# awesometree

Cross-platform workspace manager: Zed + git worktrees + window management.

Cargo workspace with two crates. `awesometree` is the main crate
producing two binaries: `awesometree` (CLI) and `awesometree-daemon`
(GPUI process with picker, projects UI, system tray, QR code window,
and HTTP server). `awesometree-core` is a shared Rust API client
library with UniFFI bindings for the Android mobile app.

## How It Works

A hotkey sends `pick` to the daemon, which opens a GPUI picker.
Selecting an inactive workspace creates a git worktree, creates a
virtual desktop/tag, and launches Zed. Another hotkey cycles between
active project tags.

The daemon also runs an HTTP server (port 9099) with a REST API for
workspace/project CRUD and an ACP reverse proxy. The mobile app
connects by scanning a QR code from the tray menu.

## Platform Support

| Platform | WM Adapter | Tray | Daemon Service | Install |
|----------|-----------|------|----------------|---------|
| Linux | `AwesomeAdapter` via `awesome-client` | GTK `tray-menu` | systemd user unit | `make install` |
| macOS | `MacosAdapter` via yabai/AppleScript | osascript menu | launchd plist | `make install` / `make bundle` |
| Android | — | — | — | `make android-lib` |

### macOS Notes

The macOS adapter supports two modes:

1. **yabai** (recommended) — When [yabai](https://github.com/koekeishiya/yabai)
   is installed, spaces are created/destroyed/focused via its CLI. The
   `layout` field maps to yabai layouts (`bsp`, `stack`, `float`).

2. **Fallback** — Without yabai, workspace state is tracked in
   `/tmp/awesometree-macos-tags.json`. Space switching uses AppleScript
   key codes for Mission Control. Creating spaces programmatically
   requires accessibility permissions.

The `eval` method on macOS accepts AppleScript instead of Lua.

## Components

| Layer | Source | Role |
|-------|--------|------|
| CLI | `src/main.rs` | All subcommands (`up`, `down`, `create`, …) |
| Daemon | `src/daemon_main.rs` | GPUI app, socket listener, tray |
| Config | `src/config.rs` | JSON load/save, project/workspace model |
| Workspace | `src/workspace.rs` | `Manager` — worktree, tag, app lifecycle |
| WM adapter | `src/wm.rs` | `Adapter` trait; Linux `AwesomeAdapter`, macOS `MacosAdapter` |
| HTTP/ACP | `src/server.rs` | REST API, ACP reverse proxy (axum + tokio) |
| Auth | `src/auth.rs` | HMAC token generation/validation for remote clients |
| QR code | `src/qr.rs` | QR code generation + GPUI display window |
| Picker | `src/picker.rs` | GPUI fuzzy picker + create form |
| Projects UI | `src/projects_ui.rs` | GPUI project CRUD window |
| Tray | `src/tray.rs` | System tray icon + popup menu (GTK on Linux, osascript on macOS) |
| Notifications | `src/notify.rs` | Error windows, background task runner |
| Core lib | `core/` | Shared API client crate with UniFFI for Android |
| Android | `android/` | Kotlin/Compose mobile app |
| macOS bundle | `macos/` | Info.plist for .app bundle |

## Build & Install

```sh
make install   # cargo build --release → ~/.local/bin/ + service setup
make test      # cargo test --workspace
make openapi   # print OpenAPI spec to stdout
```

### macOS-specific

```sh
make bundle          # build + create Awesometree.app in target/release/
make install-bundle  # copy .app to /Applications/
make enable          # register launchd agent
make disable         # unregister launchd agent
make restart         # kickstart daemon via launchctl
```

### Linux-specific

```sh
make enable    # systemctl --user enable
make disable   # systemctl --user disable
make restart   # systemctl --user restart
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
