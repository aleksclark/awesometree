# Architecture

## Agent Work Model (AWM)

awesometree conforms to the canonical Agent Work Model. Terminology:

| AWM term | Role in awesometree |
|---|---|
| **Project** | Durable collaboration scope. Authoritative store: **Switchboard** Project Catalog. |
| **ProjectSnapshot** | Immutable pinned Project revision (`project_revision` + deterministic `project_snapshot_id`). |
| **WorkProfile** | Reusable WorkSession blueprint. Authoritative store: Switchboard. Exact ID `default` is preselected when omitted. |
| **WorkSession** | Bounded work episode (formerly called "workspace" in episode sense). Authoritative lifecycle in Switchboard. |
| **Workspace** | Material Resource only (git worktree / runtime environment). Host-local realization. |
| **Agent instance** | Running process — not a WorkSession or Workspace. |

Canonical reference: `/home/aleks/work/projects/agent-work-model/model/`.

### Authority boundary

- **Switchboard** is the sole mutable authority for Projects, WorkProfiles, ProjectSnapshots/revisions, and WorkSessions.
- **awesometree** is a consumer and host-runtime coordinator. It never caches a writable copy of those records.
- Local `runtime.json` holds only realization facts keyed by `work_session_id` (paths, tags, ports, secret refs). Lifecycle truth always comes from Switchboard.
- Switchboard unavailability is a hard failure for authoritative CRUD. No local fallback store.

### Mapping from pre-cutover names

| Former | Now |
|---|---|
| ~~`interop::Project` under project-interop~~ (removed) | Switchboard `Project` + immutable revision/snapshot |
| workspace-as-episode (`state.workspaces`) | `WorkSession` |
| git worktree / tag / apps | AWM `Workspace` Resource + `WorkSessionRuntime` |
| agent process records | unchanged agent instances (not renamed to WorkSession) |

## Data Flow

```
Hotkey → awesometree pick → Unix socket → awesometree-daemon
  → GPUI picker → user selects WorkSession / creates with WorkProfile
  → WorkSessionService
       → Switchboard MCP (project_work_session_create, …)
       → local Workspace realization (git worktree, WM tag, apps, Bezalel)
```

## Binaries

**awesometree** (`src/main.rs`): CLI. WorkSession / Project / WorkProfile
commands call `WorkSessionService` (Switchboard-backed). Interactive
commands still require the daemon.

**awesometree-daemon** (`src/daemon_main.rs`): Long-running GPUI app.
Socket listener, tray, picker, create form (WorkProfile selector),
projects UI, agents UI, HTTP server, QR window.

**awesometree-mcp**: MCP façade. `work_session/*` and `project/*` tools
delegate to the shared service (not a second store).

## Key Abstractions

**`model`** (`src/model/`): Shared AWM contracts — `WorkSession`,
`WorkProfile`, `ProjectEnvelope`, `WorkSessionRuntime`, typed
`SwitchboardError`, lifecycle states, policy narrowing.

**`switchboard::SwitchboardClient`** (`src/switchboard/`): Production
MCP streamable-HTTP client for `project_*`, `project_work_profile_*`,
`project_work_session_*`. Configurable via `AWESOMETREE_SWITCHBOARD_URL`
(default `http://127.0.0.1:3847/mcp`).

**`WorkSessionService`** (`src/work_session_service.rs`): Single
application service for all transports. Owns Project resolution,
default-profile resolution, WorkSession lifecycle, local realization,
compensation (`aborted` on failure), and restart reconciliation.

**`runtime_store`** (`src/runtime_store.rs`): Host-local
`~/.config/awesometree/runtime.json` keyed by `work_session_id`.
Secrets (Bezalel tokens) in `runtime-secrets.json`. Rejects legacy
`state.json` workspace-episode documents.

**`Adapter`** (`src/wm.rs`): Window-manager operations for local tags.

## HTTP / OpenAPI

| Path | Entity |
|---|---|
| `/api/work-sessions` | WorkSession list/create |
| `/api/work-sessions/{id}` | get/delete |
| `/api/work-sessions/{id}/transition` | lifecycle |
| `/api/work-profiles` | WorkProfile list (from Switchboard) |
| `/api/projects` | Project Catalog façade |

Old `/api/workspaces` episode routes are removed (not aliased).

## State

| File | Purpose |
|------|---------|
| Switchboard config root | Authoritative Projects, WorkProfiles, WorkSessions, revisions |
| `~/.config/awesometree/runtime.json` | Host-local realization only |
| `~/.config/awesometree/runtime-secrets.json` | Host-local secret refs (never Switchboard) |
| `~/.config/awesometree/daemon.sock` | CLI ↔ daemon IPC |
| git worktree paths | Material Workspace Resource checkouts |

## Lifecycle

WorkSession: `proposed` → `open` ↔ `paused` → `closed` | `aborted`.

Creation: propose in Switchboard → realize Workspace → transition `open`.
Realization failure: clean partial local resources → transition `aborted`.
