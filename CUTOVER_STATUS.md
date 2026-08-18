# AWM cutover status (local worktree)

Date: 2026-08-18
Branch: awm/grok45-complete
Workspace: /home/aleks/.paseo/worktrees/1xjojxd2/awm-grok45-complete

## Gates

| Gate | Result |
|------|--------|
| `cargo test --workspace` | PASS (incl. real Switchboard e2e) |
| `cargo build --workspace` | PASS |
| `cargo clippy --workspace -- -D warnings` | PASS |
| Phase-01 full completion gate | NOT complete (see open items) |

## Done in this session

- Tree compiles; `arp_store` callers fixed (`list_tasks` + active filter).
- ACP product surface removed from Android (`AcpScreen` route, `acpPort`, chat nav) and Rust (`ACP_PORT_*`, `CRUSH_ACP_PORT`).
- Dual-authority: `state`/`arp_store` remain agent-runtime only; worktree create delegated to shared `workspace::ensure_git_worktree` with local-branch fallback.
- E2E `tests/e2e_switchboard_awm.rs`: starts real Switchboard from impl worktree with isolated config; production MCP client; default-profile create; Switchboard read-back; runtime keyed by `work_session_id`; no ACP/secrets in runtime; missing-default fails closed via real delete+create path.
- Tray labels and ARP docs examples updated off workspace-as-episode tool names.
- Clippy `-D warnings` clean.

## Still open vs phase-01 completion gate

- Not every public boundary has a positive production-orchestration creation test (CLI/daemon IPC/REST/MCP stdio/gRPC/core/GPUI/Android each).
- Authorization + secret-redaction end-to-end suites incomplete.
- Full anti-cheating audit / Switchboard `make ci` / AWM `make check` / Android build not run here.
- Leftover term hits (justified or remaining cleanup):
  - `docs/specs/project-interop/**` historical tombstone/samples (README already says superseded).
  - Plan docs under `docs/plans/switchboard-awm-single-pass/**` describe pre-cutover world (expected).
  - `proto/arp/v1/discovery.proto` still has `/v1/workspaces/{workspace_name}:watch` gRPC path (proto surface rename incomplete).
  - `docker-compose.test.yml` no longer mounts project-interop fixture; still a thin ARP docker harness, not full Switchboard e2e.
  - Android package dir still named `ui/workspaces` (screen is `WorkSessionsScreen`).
  - gRPC convert helpers still named `work_session_to_proto_workspace` for wire compat with old proto message names.
  - Server tests assert absence of `/api/workspaces` (good).

## Commit

Local WIP commit intended after green tests (no push).
