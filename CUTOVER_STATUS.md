# AWM cutover status (local worktree)

Date: 2026-08-18
Branch: awm/grok45-complete
Workspace: `/home/aleks/.paseo/worktrees/1xjojxd2/awm-grok45-complete`

## Gates

| Gate | Result |
|------|--------|
| `cargo test --workspace` | **PASS** (9 e2e + unit suite) |
| `cargo build --workspace` | **PASS** |
| `cargo clippy --workspace -- -D warnings` | **PASS** |
| `SCREENSHOTS=1 cargo test --test screenshots` | **PASS** (picker.png, create-form.png) |
| `make -C agent-work-model check` | **PASS** (validate/lint/generate + 52 pytest) |
| `make -C switchboard/.../impl-project-catalog-mcp ci` | **PASS** (incl. gosec 0 issues, govulncheck) |
| Android `./gradlew test` + `compileDebugKotlin` | **PASS** (no unit sources; compile green) |
| Phase-01 full completion gate | **Near-complete** — see remaining gaps |

## Done

### Authority / runtime
- Switchboard sole authority for Project/WorkProfile/WorkSession.
- Host-local `runtime_store` + `state`/`arp_store` agent rows keyed by `work_session_id` only.
- Shared `WorkSessionService` + production MCP client; no local fallback store.
- Git worktree realization via `workspace::ensure_git_worktree` (local-branch fallback when no `origin/*`).

### ACP purge
- No `AcpScreen`, `acp_supervisor`, `ACP_PORT_*`, `CRUSH_ACP_PORT`, Android ACP nav/fields.

### Public boundaries — positive production create (real Switchboard)
| Surface | Evidence |
|---------|----------|
| Service | `e2e_default_profile_work_session_create` |
| REST `/api/work-sessions` | `e2e_rest_create_work_session` |
| gRPC `WorkSessionService` | `e2e_grpc_create_work_session` |
| MCP tool `work_session/create` | `e2e_mcp_tool_create_work_session` |
| CLI `work-session create` | `e2e_cli_create_work_session` |
| core/UniFFI `ApiClient` | `e2e_core_client_create_work_session` |
| Missing default fails closed | `e2e_missing_default_fails_closed` |

GPUI picker/daemon IPC and full Android device create still lack dedicated e2e here (manual/UI paths use same `WorkSessionService`).

### Auth + secret redaction
- REST enforces project scope on list/get/create/delete/transition (`Extension<ScopedToken>`).
- `e2e_auth_scope_denies_other_project` — foreign project get/create/list blocked (403).
- `e2e_secrets_never_in_list_or_runtime_json` — headless Bezalel token host-local only; absent from runtime.json, REST list/detail, create payload, Switchboard WorkSession JSON; no `acp_*`.

### Surface cleanup
- discovery.proto watch HTTP path → `/v1/work-sessions/{work_session_id}:watch`; field `work_session_id`.
- gRPC helper `work_session_to_discovery_payload` (legacy alias deprecated).
- Android package `ui/workspaces` → `ui/worksessions` + `WorkSessionsScreen.kt`.
- Tray labels / ARP docs off workspace-as-episode tool names.
- `docs/specs/project-interop/**` left as historical tombstone (README says superseded).

## Still open / not claimed

- GPUI picker + daemon socket IPC do not have automated Switchboard create e2e (same service path; not exercised via UI harness).
- Android instrumented/device create not run (unit test task empty).
- Full anti-cheating audit checklist from phase-01 not exhaustively re-walked line-by-line.
- Discovery stream still uses wire message names `WatchWorkspace` / `WorkspaceEvent` (HTTP path renamed; major proto break deferred).
- `WorkspaceServiceImpl` type alias remains for compile-compat; not a public route.
- Docker ARP harness is still not full Switchboard e2e.

## Commits

- `6238887` WIP: finish Switchboard AWM cutover compile, ACP purge, e2e
- (pending) multi-boundary e2e + auth/redaction + proto/Android rename
