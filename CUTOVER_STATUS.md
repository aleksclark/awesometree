# AWM cutover status (local worktree)

Date: 2026-08-18
Branch: awm/grok45-complete
Workspace: `/home/aleks/.paseo/worktrees/1xjojxd2/awm-grok45-complete`
HEAD: (see latest commit)

## Phase-01 decision

**COMPLETE for product cutover**, with the documented harness-only leftovers below.
No remaining product-facing dual-authority, ACP, local fallback store, or missing
Switchboard-backed create/lifecycle evidence on the public boundaries we can drive
in this environment.

## Gates (refreshed)

| Gate | Result |
|------|--------|
| `cargo test --workspace` | **PASS** (18 e2e + units) |
| `cargo build --workspace` | **PASS** |
| `cargo clippy --workspace -- -D warnings` | **PASS** |
| `SCREENSHOTS=1 cargo test --test screenshots` | **PASS** (picker.png, create-form.png) |
| `make -C agent-work-model check` | **PASS** (validate/lint/generate + 52 pytest) |
| `make -C switchboard/.../impl-project-catalog-mcp ci` | **PASS** (gosec 0, govulncheck clean) |
| Android `./gradlew test` + `compileDebugKotlin` | **PASS** (earlier; unit sources empty) |
| `test/run-arp-tests.sh` (Docker Compose ARP smoke) | **PASS** after Dockerfile/smoke fixes |
| Android instrumented/device create | **BLOCKED** — see below |

## Public-boundary create evidence (real Switchboard)

| Surface | Test |
|---------|------|
| Service | `e2e_default_profile_work_session_create` |
| REST | `e2e_rest_create_work_session` |
| gRPC | `e2e_grpc_create_work_session` |
| MCP tool | `e2e_mcp_tool_create_work_session` |
| CLI | `e2e_cli_create_work_session` |
| core/UniFFI | `e2e_core_client_create_work_session` |
| Daemon IPC | `e2e_daemon_ipc_create_work_session` (+ handler direct) |
| Missing default | `e2e_missing_default_fails_closed` |
| Auth scope | `e2e_auth_scope_denies_other_project` |
| Secrets | `e2e_secrets_never_in_list_or_runtime_json` |

## Phase-01 BDD gaps (real Switchboard)

| Scenario | Test |
|----------|------|
| Realization failure → aborted; retry same id not Open | `e2e_realization_failure_aborts_and_no_duplicate_on_retry` |
| Invalid transition from closed | `e2e_invalid_transition_from_closed` |
| Snapshot/revision pin survives live Project edit | `e2e_project_snapshot_pin_survives_live_edit` |
| CAS conflict on stale Project update | `e2e_project_cas_conflict_on_stale_update` |
| Referenced Project delete rejected | `e2e_referenced_project_delete_rejected` |
| Referenced WorkProfile delete rejected when in use | `e2e_referenced_work_profile_delete_rejected_when_in_use` |
| Switchboard outage hard-fail; no local write; REST 503 | `e2e_switchboard_outage_is_hard_failure_no_local_write` |

## Documented leftovers (not product bugs)

1. **GPUI picker form create UI e2e** — no interaction framework. Production path is `do_create` → same `WorkSessionService` as daemon IPC (covered). Screenshots cover chrome only. **Not faked.**
2. **Android instrumented/device create** — **BLOCKED: no running emulator/device.**
   - `adb devices` empty (after adding SDK platform-tools to PATH).
   - AVDs exist on disk (`pixel`, `test_device`) but none booted; starting a full emulator session was not part of this pass.
   - Compile/unit Gradle tasks green; app package is `ui/worksessions`.
3. **Discovery wire names** `WatchWorkspace` / `WorkspaceEvent` / legacy `Workspace` message — **intentionally kept** for wire stability. HTTP path is `/v1/work-sessions/{work_session_id}:watch`.
4. **Docker ARP smoke** is surface/route smoke (WorkSession API, no `/api/workspaces`); full Switchboard-in-compose create is covered by host e2e, not the thin ARP container.

## This pass (finalization)

- Fixed Docker image build: copy `proto/` + `build.rs`, install `protobuf-compiler` + `libprotobuf-dev`, headless `wm` import cfg.
- Fixed ARP smoke healthcheck (`curl -f` was treating 503 as down).
- `test/run-arp-tests.sh` **PASS**: work-sessions 503 (no SB in container), `/api/workspaces` 404, `/a2a/agents` 200, OpenAPI clean.
- Refreshed screenshots, AWM check, Switchboard ci.

## Anti-cheating (holds)

- No dual-authority Project/WorkProfile/WorkSession in `state`/`arp_store`.
- No ACP product surface.
- No `/api/workspaces` episode routes; no local fallback on Switchboard outage.
- All lifecycle/create paths use `WorkSessionService` + production MCP client.

## Commits (local, no push)

- `6238887` … `54806ce` cutover + multi-boundary + daemon IPC + BDD gaps
- (pending) Docker ARP harness fix + phase-01 COMPLETE status
