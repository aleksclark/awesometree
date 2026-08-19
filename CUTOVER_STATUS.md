# AWM cutover status (local worktree)

Date: 2026-08-18
Branch: awm/grok45-complete
Workspace: `/home/aleks/.paseo/worktrees/1xjojxd2/awm-grok45-complete`

## Gates

| Gate | Result |
|------|--------|
| `cargo test --workspace` | **PASS** (18 e2e + units) |
| `cargo build --workspace` | **PASS** |
| `cargo clippy --workspace -- -D warnings` | **PASS** |
| Screenshots / AWM check / Switchboard ci / Android | **PASS** (earlier this cutover; not re-run every commit) |
| Phase-01 full completion gate | **Substantially covered**; residual gaps below |

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

## Phase-01 BDD gap tests (this pass)

| Scenario | Test | Result |
|----------|------|--------|
| Realization failure → aborted + no Ready runtime; retry same id no Open duplicate | `e2e_realization_failure_aborts_and_no_duplicate_on_retry` | PASS |
| Invalid transition from `closed` | `e2e_invalid_transition_from_closed` | PASS |
| Project snapshot/revision pin survives live Project edit | `e2e_project_snapshot_pin_survives_live_edit` | PASS |
| CAS conflict on stale Project update | `e2e_project_cas_conflict_on_stale_update` | PASS |
| Referenced Project delete rejected | `e2e_referenced_project_delete_rejected` | PASS |
| Referenced WorkProfile delete rejected when in use | `e2e_referenced_work_profile_delete_rejected_when_in_use` | PASS |
| Switchboard outage hard-fails; no local runtime write; REST 503 | `e2e_switchboard_outage_is_hard_failure_no_local_write` | PASS |

## Honest leftovers (not claimed done)

1. **GPUI picker form create** — no automated UI interaction e2e. Production path is `do_create` → `WorkSessionService` (same as daemon IPC). Screenshots only cover chrome. **Not faked.**
2. **Android instrumented/device create** — not run (unit task empty; compile green earlier).
3. **Discovery wire names** `WatchWorkspace` / `WorkspaceEvent` / legacy `Workspace` message — **kept for wire stability**; HTTP path is `/v1/work-sessions/{work_session_id}:watch`. Renaming RPCs would be a breaking proto change; documented, not done.
4. **Referenced delete error code** — Switchboard may return `Referenced` or another non-success code; tests assert rejection + entity still present, not only one code enum.
5. **Docker compose full rebuild** after smoke rewrite — not re-run every pass.
6. **Compensation worktree path cleanup** — create path calls `cleanup_partial` + aborted; e2e asserts aborted/not-Open and non-Ready runtime (bad-repo case may never create a dir).

## Anti-cheating (still holds)

- No dual-authority Project/WorkProfile/WorkSession in `state`/`arp_store`.
- No ACP product surface.
- No `/api/workspaces` episode routes.
- No local fallback store on Switchboard outage.
- Handlers go through `WorkSessionService` + production MCP client.

## Commits (local, no push)

- `6238887` base cutover compile/ACP/e2e
- `4b79718` multi-boundary + auth/secrets
- `c5989db` daemon IPC create
- (pending) BDD gap e2e suite
