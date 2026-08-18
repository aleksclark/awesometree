# awesometree

Cross-platform Agent Work Model host: Switchboard-backed Projects,
WorkProfiles, and WorkSessions, plus local Workspace realization
(Zed + git worktrees + window management).

Cargo workspace with two crates. `awesometree` is the main crate
producing two binaries: `awesometree` (CLI) and `awesometree-daemon`
(GPUI process with picker, projects UI, system tray, QR code window,
and HTTP server). `awesometree-core` is a shared Rust API client
library with UniFFI bindings for the Android mobile app.

## How It Works

Switchboard is the sole mutable authority for Projects, WorkProfiles,
and WorkSessions. awesometree coordinates host-local realization
(git worktrees, WM tags, apps, ports, credentials).

A hotkey sends `pick` to the daemon, which opens a GPUI picker.
Creating a WorkSession (WorkProfile selection defaults to exact ID
`default`) writes the episode to Switchboard, creates a git worktree
(material Workspace Resource), creates a virtual desktop/tag, and
launches Zed. Another hotkey cycles between open WorkSession tags.

The daemon also runs an HTTP server (port 9099) with a REST API for
WorkSession/Project façades The mobile app
connects by scanning a QR code from the tray menu.

Configure Switchboard via `AWESOMETREE_SWITCHBOARD_URL` (default
`http://127.0.0.1:3847/mcp`).

## Agent Work Model

See [docs/architecture.md](docs/architecture.md) for the AWM mapping.
Canonical terms: Project, ProjectSnapshot, WorkProfile, WorkSession,
Workspace (material Resource only). Do not call a WorkSession a
"workspace".

## Agent Registry Protocol (ARP)

awesometree implements the Agent Registry Protocol — an MCP server
that manages the full lifecycle of A2A agents within WorkSessions.
ARP fills the gap between MCP (agent-to-tool) and A2A
(agent-to-agent): neither protocol defines how to create, start,
stop, or destroy agent instances. ARP does.

The spec lives in `arp-spec/`. Protobuf definitions in `proto/arp/v1/`.

### Interfaces

- **MCP tools** — lifecycle operations callable by any MCP host
- **A2A proxy** — HTTP endpoints at `/a2a/agents/` that proxy
  standard A2A v1.0 RPCs to managed agents

### Tool Groups

| Group | Tools | Notes |
|-------|-------|------|
| Project | `project/list`, `project/register`, `project/unregister` | Switchboard-backed façade |
| WorkSession | `work_session/create`, `work_session/list`, `work_session/get`, `work_session/transition`, `work_session/destroy`, `work_profile/list` | Shared `WorkSessionService` |
| Agent Lifecycle | `agent/spawn`, `agent/list`, `agent/status`, … | `arp-spec/tools-agent.md` |
| Discovery | `agent/discover`, MCP resources, MCP prompts | `arp-spec/tools-discovery.md` |
| Identity | `token/create`, scope enforcement | `arp-spec/identity-and-scopes.md` |

Native Switchboard tools: `project_*`, `project_work_profile_*`,
`project_work_session_*`.

### Agent Lifecycle State Machine

`starting` → `ready` ↔ `busy` → `stopping` → `stopped`

Any state can transition to `error` on crash/health failure.

### Key Implementation Files

| Layer | Source | Role |
|-------|--------|------|
| Model | `src/model/` | AWM contracts |
| Switchboard client | `src/switchboard/` | Production MCP client |
| Application service | `src/work_session_service.rs` | Single orchestration path |
| Runtime store | `src/runtime_store.rs` | Host-local realization by `work_session_id` |
| Agent supervisor | `src/agent_supervisor.rs` | Process spawn, health, stop |
| ARP store | `src/arp_store.rs` | Agent/task runtime (not WorkSession authority) |
| A2A proxy | `src/a2a_proxy.rs` | A2A v1.0 HTTP proxy |
| Auth | `src/auth.rs` | HMAC tokens |

### Identity & Scopes

Tokens carry project and work-session scopes and permission levels
(`session`, `project`, `admin`). Scope can only narrow, never widen.

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

2. **Fallback** — Without yabai, tag state is tracked in
   `/tmp/awesometree-macos-tags.json`. Space switching uses AppleScript
   key codes for Mission Control. Creating spaces programmatically
   requires accessibility permissions.

The `eval` method on macOS accepts AppleScript instead of Lua.

## Components

| Layer | Source | Role |
|-------|--------|------|
| CLI | `src/main.rs` | `work-session`, `project`, `work-profiles`, … |
| Daemon | `src/daemon_main.rs` | GPUI app, socket listener, tray |
| Model | `src/model/` | AWM contracts |
| Switchboard | `src/switchboard/` | MCP client |
| Service | `src/work_session_service.rs` | Shared orchestration |
| Runtime store | `src/runtime_store.rs` | Host-local realization |
| Workspace helpers | `src/workspace.rs` | git/WM realization helpers |
| WM adapter | `src/wm.rs` | `Adapter` trait |
| HTTP | `src/server.rs` | REST `/api/work-sessions`, projects |
| Agent supervisor | `src/agent_supervisor.rs` | Agent process lifecycle |
| ARP store | `src/arp_store.rs` | Agent/task runtime tables |
| A2A proxy | `src/a2a_proxy.rs` | A2A v1.0 HTTP proxy |
| Auth | `src/auth.rs` | HMAC tokens |
| QR code | `src/qr.rs` | QR display window |
| Picker | `src/picker.rs` | GPUI picker + create form |
| Projects UI | `src/projects_ui.rs` | GPUI project CRUD |
| Agents UI | `src/agents_ui.rs` | Agent status dashboard |
| Tray | `src/tray.rs` | System tray |
| Core lib | `core/` | UniFFI Android client |
| Android | `android/` | Kotlin/Compose app |
| Packaging | `packaging/` | Homebrew + AUR |

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

Screens: WorkSessions, Projects, Settings/QR Scanner.

## Detailed Docs

- [Architecture](docs/architecture.md)
- [Keybindings](docs/keybindings.md)
- [ARP Spec](arp-spec/index.md)
- [AWM cutover plan](docs/plans/switchboard-awm-single-pass/index.md)

## CI/CD

Two GitHub Actions workflows in `.github/workflows/`:

- **`ci.yml`** — Runs on push/PR. Linux build+test, macOS build+test, clippy.
- **`release.yml`** — Triggered by `v*` tags. Builds release binaries for
  Linux x86_64, macOS arm64, and macOS x86_64, then:
  1. Creates a GitHub Release with tarballs + checksums
  2. Updates the Homebrew tap (`aleksclark/homebrew-tap`)
  3. Publishes to AUR (`awesometree`)

### Releasing

Uses CalVer (`YYYY.M.D`). Bump version in `Cargo.toml`, `core/Cargo.toml`,
and `macos/Info.plist`, then tag and push:

```sh
git tag v2026.4.8
git push origin v2026.4.8
```

### Required Secrets

| Secret | Purpose |
|--------|---------|
| `HOMEBREW_TAP_GITHUB_TOKEN` | GitHub PAT with write access to `aleksclark/homebrew-tap` |
| `AUR_SSH_KEY` | SSH key registered with AUR for pushing PKGBUILD |

### Package Installation

```sh
# Homebrew (macOS/Linux)
brew tap aleksclark/tap
brew install awesometree

# AUR (Arch Linux)
yay -S awesometree
```
