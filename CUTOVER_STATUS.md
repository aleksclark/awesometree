# AWM cutover status (local worktree)

Date: 2026-08-18
Branch: awm/grok45-complete
Workspace: `/home/aleks/.paseo/worktrees/1xjojxd2/awm-grok45-complete`
HEAD: (see latest WIP commit)

## Gates

| Gate | Result |
|------|--------|
| `cargo test --workspace` | **PASS** (11 e2e + units) |
| `cargo build --workspace` | **PASS** |
| `cargo clippy --workspace -- -D warnings` | **PASS** |
| `SCREENSHOTS=1 cargo test --test screenshots` | **PASS** (prior run) |
| `make -C agent-work-model check` | **PASS** (prior run) |
| `make -C switchboard/... ci` | **PASS** (prior run) |
| Android `./gradlew test` + compile | **PASS** (prior run) |
| Docker `test/run-arp-tests.sh` | Reworked for WorkSession contract; full docker rebuild not re-run this pass |
| Phase-01 full completion gate | **Not fully claimed** — see remaining gaps |

## Public-boundary create evidence (real Switchboard)

| Surface | Test | Notes |
|---------|------|-------|
| Service | `e2e_default_profile_work_session_create` | default profile, runtime, read-back |
| REST | `e2e_rest_create_work_session` | `/api/work-sessions` |
| gRPC | `e2e_grpc_create_work_session` | `WorkSessionServiceImpl` |
| MCP tool | `e2e_mcp_tool_create_work_session` | `work_session/create` |
| CLI | `e2e_cli_create_work_session` | `awesometree work-session create` |
| core/UniFFI | `e2e_core_client_create_work_session` | `awesometree_core::ApiClient` |
| Daemon IPC | `e2e_daemon_ipc_create_work_session` | Unix socket `work-session-create` → production handler → `WorkSessionService` |
| Daemon handler | `e2e_daemon_ipc_handler_direct` | same handler without socket |
| Missing default | `e2e_missing_default_fails_closed` | typed failure |
| Auth scope | `e2e_auth_scope_denies_other_project` | 403 cross-project |
| Secrets | `e2e_secrets_never_in_list_or_runtime_json` | Bezalel host-local only |

### GPUI status (honest)

- GPUI **picker create form** (`DaemonCmd::Create` / `do_create`) still opens a window and calls the same `WorkSessionService` — **no automated GPUI interaction e2e** (no display-driver create flow).
- **Daemon IPC create is covered** via production Unix socket command `work-session-create` (not the GPUI form). Screenshots cover picker chrome only.
- Do **not** claim “GPUI create e2e done”; claim “daemon IPC create done; GPUI form shares service path”.

## Anti-cheating audit (walked)

| Check | Result |
|-------|--------|
| `interop.rs` removed / no project-interop CRUD | **OK** — only historical docs/tombstone + negative assertions |
| `state.rs` / `arp_store.rs` no Project/Profile/Session authority | **OK** — agent rows by `work_session_id` only |
| Handlers use shared service | **OK** — server/mcp/grpc/daemon/CLI/daemon_main |
| Switchboard client production MCP | **OK** — no shell/fs fallback in `src/switchboard/` |
| No ACP product surface | **OK** — search clean outside plans/status |
| No `/api/workspaces` episode routes | **OK** — openapi asserts absence |
| No dual-write / local fallback store | **OK** |
| FakeCatalog | **unit tests only** in `work_session_service` |
| `WorkspaceServiceImpl` alias | **removed** |
| Discovery `WatchWorkspace` wire names | **explicit leftover** — HTTP path is `/v1/work-sessions/{work_session_id}:watch`; RPC/message names kept for wire stability |
| Docker ARP harness | smoke script updated off project-interop fixtures; asserts old `/api/workspaces` absent |

### Justified leftovers

1. **`docs/specs/project-interop/**`** — historical tombstone (`README` superseded). Not a live contract.
2. **Discovery proto** `WatchWorkspace` / `WorkspaceEvent` / legacy `Workspace` message — wire-stable names; HTTP path renamed.
3. **GPUI form create** — no automated UI create test (needs interaction framework); production path identical to service.
4. **Android instrumented/device create** — not run (unit task empty; compile green).
5. **Compensation / snapshot mismatch / policy** — unit coverage partial; not every phase-01 BDD failure mode has a dedicated multi-boundary e2e.

## This pass changes

- Daemon Unix IPC: `work-session-create …` synchronous production create + `listen_until` for tests.
- E2E: daemon socket + handler create against real Switchboard.
- Removed `WorkspaceServiceImpl` alias.
- MCP instructions/tool blurbs → WorkSession wording.
- `test/run-arp-tests.sh` rewritten for WorkSession API / absence of old routes.

## Commits

- `6238887` WIP: compile, ACP purge, base e2e
- `4b79718` WIP: multi-boundary e2e, REST scope, secrets
- (pending) daemon IPC e2e + audit cleanup
