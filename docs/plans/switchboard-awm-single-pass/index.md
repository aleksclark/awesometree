# Switchboard-backed Agent Work Model refactor

## Outcome

Refactor awesometree in one coordinated cutover so its product language and public contracts conform to the canonical Agent Work Model (AWM), while Switchboard becomes the only mutable authority for `Project`, `WorkProfile`, and `WorkSession` records. Awesometree remains responsible for material workspace resources and local runtime realization such as git worktrees, window-manager tags, launched applications, ports, processes, and credentials.

This is intentionally a single implementation phase. The phase may contain ordered internal work packages, but it is merged and released only after every caller, UI, API, test, and document uses the replacement model. There is no period in which old and new stores are both supported production authorities.

## Current-state summary

### Existing behavior

- `src/interop.rs:8-32,125-219` defines and directly persists project-interop `Project` records under `$XDG_CONFIG_HOME/project-interop`; awesometree is currently a mutable Project authority.
- `src/state.rs:7-35,116-159` persists a separate JSON `Store` keyed by workspace name. Each `WorkspaceState` combines durable episode identity with local runtime details, including project, active state, worktree directory, ACP state, Bezalel credentials, and agents.
- `src/arp_store.rs:9-92,135-369` provides a second SQLite workspace/agent/task store, but its global initialization is not wired as the primary runtime store.
- Workspace creation is independently implemented by `Manager`, HTTP, MCP, gRPC, and the GPUI daemon (`src/workspace.rs:59-105`, `src/server.rs:299-401`, `src/mcp/tools_workspace.rs:40-70`, `src/grpc/workspace.rs:18-130`, `src/daemon_main.rs:321-416`). Their validation and side effects differ.
- GPUI creation currently collects name, Project, repository, and branch (`src/picker.rs:20-34,81-120`); Android creates a workspace from only name and Project (`android/app/src/main/kotlin/dev/awesometree/mobile/ui/workspaces/WorkspacesScreen.kt:323-330`). Neither surface can select a WorkProfile.
- REST/OpenAPI, CLI, MCP, gRPC/protobuf, the Rust core client, Android models, screenshots, and documentation expose `workspace` terminology for the bounded episode (`src/server.rs:32-119`, `src/main.rs:12-122`, `src/mcp/tools_workspace.rs:12-211`, `proto/arp/v1/workspace.proto`, `core/src/models.rs:3-69`).
- Awesometree already materializes genuine AWM `Workspace` resources as git worktrees and runtime environments (`src/workspace.rs`), but currently uses the same word for the coordinating episode. AWM explicitly distinguishes a material `Workspace` Resource from a bounded `WorkSession`.

### Switchboard capabilities already available

- Switchboard's main MCP surface implements authoritative Project Catalog tools with revisions, immutable archived definitions, validation, compare-and-swap mutation, filesystem locking, and atomic writes.
- It implements `project_work_profile_*` and `project_work_session_*` tools with WorkSession lifecycle validation and reference checks.
- WorkSession fields include `work_session_id`, `project_id`, `project_snapshot_id`, `project_revision`, `work_profile_id`, lifecycle state, policy, agent profile IDs, and timestamps.

### Partial or missing behavior

- Awesometree has no Switchboard client or repository abstraction for these records.
- Switchboard does not seed a default WorkProfile, automatically apply one, validate Project revisions/snapshot identifiers on WorkSession creation, or protect referenced Projects/WorkProfiles from deletion. Those gaps must be resolved in Switchboard as part of the same cutover rather than compensated for with a second awesometree store.
- The current local state model does not separate authoritative WorkSession state from host-local runtime realization.
- No public awesometree creation boundary accepts a WorkProfile.
- Existing tests are strongest around local helpers and serialization; the HTTP tests do not prove authoritative persistence through Switchboard.
- `PROJECT_INTEROP_PLAN.md` and `docs/specs/project-interop/` encode superseded ownership and compatibility guidance.

## Scope boundaries

### In scope

- Adopt AWM names, identities, relationships, and lifecycle semantics for Project, ProjectSnapshot, WorkProfile, WorkSession, and material Workspace resources.
- Make Switchboard the sole mutable and durable store accessed by awesometree for Projects, WorkProfiles, and WorkSessions.
- Complete the required Switchboard contract gaps for pinned Project revisions/snapshots, referential integrity, and the `default` WorkProfile.
- Replace all duplicate creation paths with one awesometree application service that coordinates Switchboard writes and local runtime realization.
- Separate local host runtime state from WorkSession records and key it by `work_session_id`.
- Rename CLI, daemon IPC, REST/OpenAPI, MCP, gRPC/protobuf, core/UniFFI, Android, GPUI, tray, screenshots, fixtures, tests, logs, and documentation where they currently call a WorkSession a workspace.
- Add WorkProfile selection to every WorkSession creation UI and public API. Preselect the WorkProfile whose `work_profile_id` is exactly `default`; creation fails clearly if it is absent rather than silently inventing local state.
- Remove obsolete project-interop code, schemas, plans, paths, merge behavior, and compatibility aliases.
- Delete or repurpose duplicate JSON/SQLite authority after the new path is proven; do not retain dual writes or fallback reads.

### Out of scope

- Backward-compatible commands, endpoint aliases, protobuf fields/services, JSON field aliases, migration commands, or automatic import of old project-interop/state/ARP data.
- Preserving records under `$XDG_CONFIG_HOME/project-interop`, `~/.config/awesometree/state.json`, or the old awesometree ARP SQLite schema.
- Treating a WorkSession as an MCP transport session, ACP conversation, HostConversation, AgentRun, or Workspace Resource.
- Making Switchboard authoritative for host-local processes, ports, bearer tokens, window-manager tags, checkout health, or worktree filesystem contents.
- Broadening the effort to every AWM term beyond those touched by the current awesometree product flow.
- Maintaining the standalone awesometree Project/workspace CRUD MCP service as an alternate store. Any retained MCP façade must delegate to the same Switchboard-backed application service.

## Global constraints

- The canonical semantic reference is `/home/aleks/work/projects/agent-work-model/model/`, especially `Project.yaml`, `ProjectSnapshot.yaml`, `WorkProfile.yaml`, `WorkSession.yaml`, `Workspace.yaml`, and `rules/architecture.yaml`. The project-interop mapping is not implementation guidance.
- One mutable authority per entity: Switchboard owns Projects, WorkProfiles, ProjectSnapshots/revisions, and WorkSessions. Awesometree observes and requests changes through Switchboard; it does not cache a writable copy.
- Project-bound WorkSessions pin an existing immutable Switchboard Project revision/snapshot. A live Project edit must not rewrite an existing WorkSession's pinned definition.
- Runtime details remain in a local `WorkSessionRuntime`-style store keyed by `work_session_id`; it may contain worktree path/resource ID, tag index, ports, process IDs, and secret references, but not a second copy of Project/WorkProfile/WorkSession definitions or lifecycle truth.
- Credentials and Bezalel tokens must never be written to Switchboard's portable WorkSession or ProjectSnapshot records or returned from general list APIs.
- Policy can only narrow from ProjectSnapshot to WorkProfile defaults to WorkSession policy. The authoritative service validates the effective policy before opening the WorkSession.
- WorkSession lifecycle uses only `proposed`, `open`, `paused`, `closed`, and `aborted`. Local realization failure leaves observable authoritative evidence: compensate a newly created session to `aborted` and clean partial runtime resources, or return an error proving why compensation failed.
- Creation and teardown must be retry-safe. Repeating a request with the same `work_session_id` must reconcile the existing authoritative record and local runtime instead of producing duplicate sessions or worktrees.
- Switchboard unavailability is a hard failure for authoritative CRUD. No local fallback store, stale writable cache, or success response is allowed.
- Because compatibility is a non-goal, remove old public names atomically and update all first-party clients in the same change.
- Tests must exercise production MCP transport and real filesystem/git boundaries. In-process fakes may cover narrow unit cases but cannot satisfy completion evidence.

## Phase overview

| Phase | Goal | Depends on |
|---|---|---|
| [Phase 1: Atomic Switchboard/AWM cutover](./phase-01-atomic-switchboard-awm-cutover.md) | Replace the model, authority boundary, lifecycle orchestration, and every product surface in one releasable change | None |

## Requirement traceability

| Requested capability | Success criteria and evidence |
|---|---|
| Single-pass data model and store refactor | Phase 1 scenarios "No dual authority after cutover" and "First-party surfaces share one creation path"; deletion audit and full repository gate |
| AWM terminology conformance | Phase 1 scenarios "Terms remain semantically distinct" and "Old episode terminology is gone"; source/proto/OpenAPI/UI/docs search audit |
| Switchboard authoritative for Projects, WorkProfiles, WorkSessions | Phase 1 scenarios covering authoritative reads/writes, outages, restart durability, conflict handling, and no dual authority |
| WorkProfile selection during WorkSession creation | Phase 1 desktop, CLI/API, and Android creation scenarios plus public-boundary end-to-end tests |
| Default selection of profile named `default` | Phase 1 default selection and missing-default failure scenarios; Switchboard seeded-record test |
| Obsolete project-interop guidance | Phase 1 obsolete-contract removal scenario and anti-cheating source search |
| Backward compatibility is a non-goal | Phase 1 old-contract rejection scenario and removal of aliases/fallbacks/migration code |

## Completion rule

The plan is complete only when the single phase lands as one coherent cutover, all BDD scenarios pass through real public boundaries, Switchboard is the only mutable store for the three authoritative entities, all first-party UIs select a WorkProfile and default to `default`, local runtime recovery works across restarts, old project-interop/workspace-episode contracts are absent, and these gates pass without skipped tests or compatibility flags:

```sh
cargo test --workspace
cargo build --workspace
cargo clippy --workspace -- -D warnings
SCREENSHOTS=1 cargo test --test screenshots -- --nocapture
make -C /home/aleks/work/projects/agent-work-model check
make -C /home/aleks/work/projects/switchboard/worktrees/impl-project-catalog-mcp ci
```
