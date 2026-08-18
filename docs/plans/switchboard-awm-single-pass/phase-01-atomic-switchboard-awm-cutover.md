# Phase 1: Atomic Switchboard/AWM cutover

## Goal

Deliver the entire authority and terminology replacement as one coherent release. Switchboard becomes the only mutable source for Projects, WorkProfiles, immutable Project revisions/snapshots, and WorkSessions; awesometree becomes a consumer and host-runtime coordinator. Every first-party boundary changes from the old workspace-as-episode contract to the AWM WorkSession contract at the same time.

The work is intentionally one phase because intermediate production states would create split authority, ambiguous lifecycle ownership, or incompatible first-party clients. Implementation should still proceed in the ordered work packages below and run focused verification after each package, but the branch is not complete or releasable until the final gate passes.

Afterward, future agent/run/resource work can attach to stable `work_session_id`, `work_profile_id`, and pinned ProjectSnapshot identities without reviving project-interop or duplicating Switchboard data.

## BDD Success Criteria

### Scenario: Switchboard is authoritative for Project mutation

- **Given** a running Switchboard MCP server and no awesometree project files
- **When** a user creates, edits, lists, and deletes a Project through each supported awesometree CLI, desktop UI, REST, and Android boundary
- **Then** every boundary performs the operation through Switchboard's Project Catalog tools with its revision and compare-and-swap semantics
- **And** a direct Switchboard read observes the same result
- **And** no project definition is written under project-interop or awesometree local state.

### Scenario: WorkProfiles are authoritative and shared

- **Given** WorkProfiles stored by Switchboard
- **When** any awesometree creation surface lists available WorkProfiles
- **Then** it displays records returned by Switchboard, including stable `work_profile_id`, display name, description, project association, intended resources, and default policy as applicable
- **And** no surface maintains an independent WorkProfile list.

### Scenario: The default WorkProfile is selected

- **Given** Switchboard contains a WorkProfile whose `work_profile_id` is exactly `default`
- **When** the desktop create form, Android create form, CLI, REST client, MCP façade, or gRPC client begins WorkSession creation without an explicit profile choice
- **Then** the UI preselects `default` or the non-visual boundary sends `work_profile_id: "default"`
- **And** selection is based on the exact ID, even when another profile's display name is `default` and the exact-ID profile has a customized display name
- **And** the created authoritative WorkSession references `default`
- **And** the response visibly reports the selected WorkProfile.

### Scenario: Missing default fails explicitly

- **Given** Switchboard does not contain a WorkProfile whose `work_profile_id` is exactly `default`
- **When** a caller omits `work_profile_id` or opens a creation UI
- **Then** creation is disabled or rejected with an actionable error that identifies the missing `default` WorkProfile
- **And** awesometree does not synthesize or persist a local replacement
- **And** no WorkSession or worktree is created.

### Scenario: A user selects a non-default WorkProfile

- **Given** Switchboard contains `default` and at least one other eligible WorkProfile
- **When** a user chooses the other profile in GPUI or Android, or passes its ID through CLI, REST, MCP, or gRPC
- **Then** awesometree creates the WorkSession with that exact `work_profile_id`
- **And** refresh/restart shows the same selection from Switchboard.

### Scenario: A WorkSession pins a real ProjectSnapshot

- **Given** an existing Switchboard Project and immutable resolved revision
- **When** awesometree creates a project-bound WorkSession
- **Then** Switchboard validates that `project_id`, `project_revision`, and `project_snapshot_id` identify the same existing immutable definition
- **And** the WorkSession stores exactly that pin
- **And** later Project edits do not alter the pinned definition returned for the WorkSession.

### Scenario: Creation realizes one material Workspace resource

- **Given** a valid Project, WorkProfile, and free local runtime capacity
- **When** a WorkSession is created through any first-party boundary
- **Then** the shared application service creates or reconciles the authoritative WorkSession and one independently identified host-local Workspace Resource (the git worktree/environment)
- **And** a WorkSession-owned ResourceBinding records `work_session_id`, `resource_id`, a non-secret resolved locator, and a grant that narrows session policy
- **And** local runtime state is keyed by `work_session_id` and references the Workspace Resource and ResourceBinding without becoming another WorkSession store
- **And** GUI mode creates the tag and launches configured applications while headless mode provisions Bezalel through the same orchestration path.

### Scenario: Local realization failure is compensated

- **Given** Switchboard accepts a proposed WorkSession but git, window-manager, application, port, or Bezalel realization fails
- **When** creation cannot reach the open state
- **Then** partial local resources are removed where safe
- **And** the authoritative WorkSession transitions to `aborted`
- **And** the caller receives the original failure plus any compensation failure
- **And** a retry cannot create a duplicate WorkSession or worktree.

### Scenario: Lifecycle operations remain authoritative

- **Given** an open WorkSession with local runtime resources
- **When** a user pauses, resumes, closes, aborts, or deletes it through any first-party boundary
- **Then** Switchboard validates and records the lifecycle transition
- **And** awesometree reconciles local resources to that state
- **And** invalid or terminal-state transitions fail without mutating local runtime into a contradictory state.

### Scenario: Restart reconciliation uses authoritative identities

- **Given** Switchboard holds open or paused WorkSessions and awesometree has host-local runtime records
- **When** the daemon restarts
- **Then** it lists authoritative WorkSessions from Switchboard and reconciles only local records for this host
- **And** missing worktrees/processes are reported or safely recreated according to lifecycle policy
- **And** orphan local runtime records are cleaned or surfaced without creating synthetic WorkSessions.

### Scenario: Concurrent Project edits preserve conflicts

- **Given** two callers read the same Project `sourceRevision`
- **When** both attempt incompatible updates
- **Then** only the first succeeds
- **And** the second receives Switchboard's conflict with expected and current revisions
- **And** awesometree does not overwrite, locally merge, or retry away the conflict.

### Scenario: Referenced records cannot be invalidated

- **Given** a WorkSession references a ProjectSnapshot and WorkProfile
- **When** a caller attempts to delete the live Project or WorkProfile in a way that would violate retained-session integrity
- **Then** Switchboard rejects the operation or applies an explicitly documented archival rule that preserves resolution
- **And** awesometree surfaces that result consistently across all boundaries
- **And** no client-only check is the sole enforcement.

### Scenario: Switchboard outage never falls back to local authority

- **Given** Switchboard is unreachable or returns malformed/error responses
- **When** a user lists or mutates Projects, WorkProfiles, or WorkSessions
- **Then** awesometree reports the authoritative dependency failure with operation context
- **And** does not return stale local data as authoritative
- **And** does not write project-interop JSON, old state WorkSessions, or ARP SQLite rows.

### Scenario: Terms remain semantically distinct

- **Given** an open WorkSession and its git worktree
- **When** the system renders details or serializes APIs
- **Then** `WorkSession` names the bounded episode and `Workspace` names only the material Resource/environment
- **And** MCP connections, host conversations, agent runs, and process instances retain distinct identities and fields (ACP product support is removed entirely).

### Scenario: Old episode terminology and contracts are gone

- **Given** the completed cutover build
- **When** a caller invokes old workspace-as-episode CLI commands, REST routes, MCP tools, gRPC services/messages, JSON fields, or Android client methods
- **Then** those contracts are absent rather than aliased
- **And** all first-party clients use WorkSession contracts
- **And** documentation no longer instructs users to use project-interop storage or merge semantics.

### Scenario: First-party surfaces share one creation path

- **Given** equivalent Project, WorkProfile, WorkSession ID, and runtime options
- **When** creation is invoked from CLI, GPUI, REST, MCP façade, gRPC, Rust core/UniFFI, or Android
- **Then** validation, pinned revision, lifecycle transitions, local realization, error semantics, and persisted result are equivalent
- **And** no handler reimplements git/state creation directly.

### Scenario: Authorization scopes apply to WorkSessions

- **Given** project- or work-session-scoped credentials
- **When** a caller lists, reads, creates, transitions, or deletes a WorkSession outside its allowed Project/WorkSession scope
- **Then** the server denies the operation before local side effects
- **And** broad list responses do not expose another scope's WorkSessions, runtime endpoints, or credentials.

### Scenario: Secrets remain host-local

- **Given** a headless WorkSession with Bezalel runtime credentials
- **When** ProjectSnapshots, WorkProfiles, WorkSessions, list APIs, logs, and exported diagnostics are inspected
- **Then** no bearer token or raw credential is present
- **And** authorized host-local detail access resolves only the minimum required secret reference.

## Implementation Instructions

### Work package 1: Freeze the replacement contract

1. Treat `/home/aleks/work/projects/agent-work-model/model/` as canonical terminology and invariants. Record the concrete mapping in awesometree architecture documentation:
   - current `interop::Project` becomes Switchboard `Project` plus immutable resolved Project revision/snapshot;
   - current workspace-as-episode becomes `WorkSession`;
   - the git worktree/runtime environment becomes an AWM `Workspace` Resource associated with local runtime realization;
   - current agent process records must not be renamed to WorkSession or Workspace.
2. Define a single internal Rust contract shared by all transports. Use explicit names and IDs: `ProjectSummary/ProjectEnvelope`, `WorkProfile`, `WorkSession`, `ProjectSnapshotRef`, `ResourceBinding`, `WorkSessionRuntime`, `WorkspaceResourceRef`, and typed Switchboard errors. Do not mirror arbitrary legacy fields merely to ease migration.
3. Define replacement public contracts before editing handlers:
   - WorkSession creation requires `work_session_id`, `project_id`, and optional `work_profile_id` whose omission resolves only to exact ID `default`;
   - response includes authoritative lifecycle state, Project revision/snapshot pin, selected WorkProfile, and safe local realization status;
   - local headless/GUI options are realization inputs, not fields that redefine WorkSession identity.
4. Decide lifecycle-to-runtime behavior once and encode it in the application service: `proposed` before realization, `open` after successful realization, `paused` for temporarily stopped local execution, `closed` for successful completion/teardown, and `aborted` for failed/cancelled creation. Deletion is record removal only where Switchboard permits it and is not a substitute for closing.
5. Add contract tests for serialization, error mapping, lifecycle transitions, and policy narrowing before replacing public surfaces.

Focused verification:

```sh
cargo test --workspace model
cargo test --workspace worksession
```

Use actual discovered test names once implemented; do not create ignored placeholder tests merely to satisfy these command filters.

### Work package 2: Complete Switchboard's authoritative behavior

Changes in `/home/aleks/work/projects/switchboard/worktrees/impl-project-catalog-mcp` are part of this cutover and must land before or atomically with awesometree:

1. Seed an idempotent WorkProfile with `work_profile_id: "default"`, `display_name: "default"`, version `1`, and explicit empty/default policy and intended resources. Startup and repeated initialization must converge without overwriting an operator-modified existing `default` record.
2. Make `project_work_session_create` resolve and validate a real Project revision archive. Require a project-bound WorkSession to pin one exact existing immutable revision and use a deterministic `project_snapshot_id` derived from or equal to the canonical revision URI/identifier. Reject mismatched Project, revision, and snapshot values.
3. Validate WorkProfile `project_ids` against existing Projects where present. On WorkSession creation, ensure the selected profile is globally applicable or explicitly associated with the Project according to one documented rule.
4. Enforce referential integrity for Project and WorkProfile deletion while retained WorkSessions reference them. Prefer rejection with a stable typed error; do not cascade-delete WorkSessions or immutable revision history.
5. Apply and validate policy narrowing. The effective WorkSession policy must not broaden the pinned Project policy or WorkProfile default policy.
6. Give work-model tool errors stable codes and relevant IDs so awesometree can map not-found, conflict, invalid-transition, invalid-reference, and unavailable failures without parsing prose.
7. Add concurrency/file-lock, restart, invalid-reference, default-seeding, and retained-history tests through the real MCP server. Update Switchboard tool documentation to match the implemented main `/mcp` surface.

Focused verification:

```sh
make -C /home/aleks/work/projects/switchboard/worktrees/impl-project-catalog-mcp ci
make -C /home/aleks/work/projects/agent-work-model check
```

### Work package 3: Add one Switchboard repository and application service

1. Add a Switchboard MCP client module with typed calls for `project_*`, `project_work_profile_*`, and `project_work_session_*`. Follow the repository's existing MCP/HTTP runtime conventions; configure endpoint/authentication once and expose health/readiness diagnostics. Do not shell out or read Switchboard's files directly.
2. Add repository traits only where they improve testability, with the production implementation always using Switchboard. A test fake may exercise pure orchestration failures but must not be selectable in production builds or end-to-end tests.
3. Add one `WorkSessionService` (name may follow local conventions) that owns:
   - Project resolution and immutable revision acquisition;
   - exact default-profile resolution;
   - WorkSession create/get/list/patch/transition/delete requests;
   - authorization checks before side effects;
   - local Workspace resource creation/removal;
   - tag/app/agent/Bezalel realization;
   - compensation, idempotent retry, and restart reconciliation.
4. Move all direct git worktree manipulation and state mutation currently duplicated in `src/workspace.rs`, `src/server.rs`, `src/mcp/tools_workspace.rs`, `src/grpc/workspace.rs`, and `src/daemon_main.rs` behind that service. Platform-specific `wm::Adapter` behavior remains an injected local runtime dependency.
5. Preserve Switchboard Project CAS tokens end-to-end in edit forms and clients. A stale edit must render a conflict and refresh option, not silently merge with a local `.project.json`.
6. Return structured errors with operation, entity ID, and safe cause. Never log or serialize bearer tokens.

Focused verification:

```sh
cargo test --workspace switchboard
cargo test --workspace work_session_service
```

### Work package 4: Split authoritative state from local runtime state

1. Replace `src/state.rs`'s `Store.workspaces` model with a host-local runtime store keyed by `work_session_id`. The local record should contain only realization facts such as Workspace resource ID/type, checkout path, host identity, tag index, process/port status, and secret references required for recovery.
2. Remove copied Project ID/name, WorkProfile definition, WorkSession lifecycle, agent assignment truth, and portable policy from local persistence except immutable foreign keys needed to query Switchboard. Treat Switchboard responses as authoritative each time state is reconciled.
3. Decide the future of `src/arp_store.rs` explicitly:
   - remove its duplicate workspace/session authority; or
   - retain only agent/task/runtime tables that have a distinct AWM owner, renamed and keyed by `work_session_id`.
   It must not store a second Project, WorkProfile, or WorkSession record.
4. Use crash-safe, cross-process-safe local persistence because daemon, CLI, HTTP, MCP, and gRPC may operate concurrently. Prefer consolidating writes in the daemon/application service; if multiple processes still write, use OS-level locking and compare-and-swap/revision checks rather than only a process-local mutex.
5. Reconcile on daemon startup against Switchboard, active processes, worktrees, and window-manager state. Mark discrepancies in observable diagnostics; do not mutate authoritative lifecycle based solely on missing local cache data without the documented reconciliation rule.
6. Do not migrate old JSON/SQLite records. On detection, fail with an explicit unsupported-old-state message and remediation path, or ignore them only after proving they cannot influence production behavior. Never silently import or dual-read.

Focused verification:

```sh
cargo test --workspace state
cargo test --workspace reconciliation
```

### Work package 5: Replace every public and UI surface

1. CLI (`src/main.rs`): replace episode-level `workspace` commands and output with `work-session` commands and AWM fields. Require/accept `--work-profile`, default it to exact ID `default`, expose lifecycle transitions, and keep Workspace terminology only for material resource diagnostics. Remove old aliases.
2. Daemon IPC (`src/daemon.rs`, `src/daemon_main.rs`): rename commands and payloads to WorkSession, include `work_profile_id`, and route all operations through the shared service.
3. GPUI (`src/picker.rs`, `src/projects_ui.rs`, `src/agents_ui.rs`, tray and screenshots):
   - list Projects, WorkProfiles, and WorkSessions from Switchboard-backed view models;
   - add a keyboard-accessible WorkProfile selector to WorkSession creation;
   - preselect exact ID `default`, show display name/description, preserve selection while other fields change, and disable submit with a visible error if no default exists;
   - distinguish authoritative lifecycle state from local Workspace/runtime health;
   - surface save/delete/conflict errors instead of discarding them.
4. REST/OpenAPI (`src/server.rs`): replace `/workspaces` episode routes and schemas with `/work-sessions`; add WorkProfile list/get routes needed by first-party clients or provide an explicitly documented Switchboard-backed façade; remove Project direct-filesystem handlers. Ensure list models omit secrets and detail models enforce authorization.
5. MCP (`src/mcp/`): remove or rename `workspace/*` episode tools. Prefer directing MCP clients to Switchboard's native Project/WorkProfile/WorkSession tools; if awesometree retains realization tools, name them for runtime/Workspace resource operations and have them call the shared service rather than persist authority.
6. gRPC/protobuf (`proto/arp/v1/`, `src/grpc/`): replace Workspace episode messages/services with WorkSession APIs and regenerate checked-in artifacts. Do not reserve old aliases for compatibility unless protobuf tooling requires reserving removed field numbers to prevent accidental reuse.
7. Rust core/UniFFI (`core/`): replace duplicate Workspace models/client methods with WorkSession and WorkProfile contracts, including lifecycle, selected profile, and safe realization status.
8. Android (`android/`): rename screens/navigation/models/client calls to WorkSession, load WorkProfiles, preselect `default`, allow another selection, show missing-default/Switchboard/conflict errors, and update UI tests/snapshots.
9. Keep agent/A2A identity separate. ACP product support is removed entirely. Update authorization helpers that currently infer scope from workspace names to use Project and WorkSession IDs, preserving least privilege.

Focused verification after each surface change:

```sh
cargo test --workspace
cargo build --workspace
SCREENSHOTS=1 cargo test --test screenshots -- --nocapture
```

Also run the Android repository's existing unit/instrumented build tasks discovered from its Gradle configuration; do not claim Android completion from Rust tests alone.

### Work package 6: Remove superseded ownership and documentation

1. Delete `src/interop.rs` and every direct Project filesystem read/write/merge caller after the Switchboard client is wired.
2. Remove `$XDG_CONFIG_HOME/project-interop`, repo-local `.project.json` overlays, context-store assumptions, merge logic, obsolete extension authority, and dependencies used only by that path. Project definitions may still contain resource/launch data if Switchboard's validated Project schema owns it; access it only through resolved Switchboard envelopes/snapshots.
3. Delete `PROJECT_INTEROP_PLAN.md` and `docs/specs/project-interop/` or replace the latter with a short historical tombstone only if repository policy requires preservation. No active documentation may point to it as a contract.
4. Rewrite `AGENTS.md`, `README.md`, `docs/architecture.md`, `docs/workspace-system/*`, API references, images, help output, and release notes around Projects, WorkProfiles, WorkSessions, and material Workspace resources.
5. Search source, generated API artifacts, fixtures, tests, Android resources, screenshots, and docs for old normative uses. Allow `workspace` only where it means an AWM material working environment, Cargo workspace, GPUI framework term, OS virtual desktop, or clearly marked historical text.
6. Remove compatibility branches, fallback paths, import/migration code, aliases, and environment flags that retain the old authority.

Focused verification:

```sh
rg -n 'project-interop|\.project\.json|workspace/(create|list|get|destroy)|/workspaces|WorkspaceInfo|CreateWorkspace' .
cargo test --workspace
```

Every remaining match must be reviewed and justified as a material Workspace/framework/build concept, not ignored by a broad exclusion.

### Work package 7: Final integration and rollout proof

1. Add a real end-to-end harness that starts Switchboard with an isolated temporary config root, waits for its MCP readiness, starts awesometree against that endpoint, and uses a real temporary git repository. Do not use checked-in user state.
2. Exercise the same shared service through at least CLI, REST, GPUI/screenshot harness, and Android client contract tests; exercise native Switchboard MCP directly to prove stored results and immutable revisions.
3. Cover restart reconciliation, concurrent Project edits, duplicate create retries, invalid lifecycle transitions, unavailable Switchboard, missing default, profile/project mismatch, realization compensation, authorization isolation, and secret redaction.
4. Update Docker/spec-torture coverage to the new routes and tool names. Correct stale compose references while preserving real service boundaries.
5. Run generated-code checks for OpenAPI, protobuf, UniFFI, screenshots, and Android models. Check in generated artifacts only through their established generators.
6. Ship as one breaking release. Installation/startup must not advertise successful upgrade while old authoritative stores are in use. Document that old data is not migrated and must be recreated in Switchboard.

## End-to-End Test Plan

### Test environment

- Start the real Switchboard binary from `/home/aleks/work/projects/switchboard/worktrees/impl-project-catalog-mcp` with an isolated temporary config root and its production main MCP transport.
- Start the real awesometree daemon/server configured to that endpoint. Use the production Switchboard client and normal auth middleware. Connect through the real Unix daemon socket for daemon IPC cases and launch the production awesometree MCP stdio server for MCP façade cases.
- Create a real temporary git repository with at least two commits/branches. Use a test window-manager adapter only where CI cannot provide the OS window manager; it may replace window operations, but must not replace Switchboard, filesystem persistence, git, the application service, or transport boundaries.
- Use isolated local runtime/state directories and inspect persisted files only as evidence after actions through public boundaries.

### Test 1: Default-profile WorkSession creation

- Create a Project through awesometree CLI, edit it through GPUI, list/read it through REST and Android, and delete a separate unreferenced Project through each supported mutation surface in isolated cases.
- After every operation, verify through direct Switchboard MCP that the result and revision/CAS behavior match and that no local Project authority was written.
- Store a profile whose display name is `default` but ID is not, customize the exact-ID `default` profile's display name, then open GPUI and a real Compose UI test; assert exact ID `default` is selected before interaction.
- Create through REST without an explicit profile and assert the authoritative WorkSession references `default`, is pinned to the real Project revision, reaches `open`, and has one real git worktree.
- Restart both services and assert the same WorkSession/profile/pin and reconciled local runtime are visible.

### Test 2: Explicit profile selection across clients

- Put a second WorkProfile through Switchboard, associated with the test Project.
- Select it in GPUI and a Compose UI test, and pass it explicitly through CLI, daemon IPC, REST, awesometree MCP stdio, gRPC, and Rust core/UniFFI client methods in separate isolated WorkSessions.
- Assert direct Switchboard reads, awesometree list/detail output, and local runtime all reference the requested profile and unique WorkSession IDs. Exercise the Android API client against the real REST server rather than only serializing fixtures.

### Test 3: Missing and invalid profile failures

- Delete or isolate the `default` WorkProfile before any WorkSession references it.
- Assert visual submit is disabled and non-visual omission returns a typed missing-default error with no WorkSession/worktree.
- Recreate `default`, then request a nonexistent or Project-ineligible profile and assert Switchboard rejects it before local side effects.

### Test 4: Immutable Project pin and CAS conflict

- Create a WorkSession, record its Project revision/snapshot, then update the live Project.
- Assert the WorkSession still resolves the original immutable definition.
- Submit two updates with the same source revision and assert exactly one succeeds; verify the loser receives a conflict and no local merge occurs.

### Test 5: Lifecycle and realization compensation

- Force a real git worktree collision or deterministic window/runtime failure after authoritative proposal creation.
- Assert the WorkSession is `aborted`, partial resources are cleaned, and retry with the same ID reconciles without duplication.
- For a successful session, pause/resume/close through different public boundaries and assert authoritative state and local runtime agree after each transition.
- Attempt a transition from `closed` and assert neither Switchboard nor local runtime changes.

### Test 6: Referenced deletion integrity

- With a retained WorkSession, attempt to delete its WorkProfile and Project through native Switchboard and every awesometree façade.
- Assert consistent rejection or the documented archival behavior, and prove the WorkSession's profile and snapshot remain resolvable after restart.

### Test 7: Dependency failure and no fallback

- Stop Switchboard, then list and mutate all three authoritative entity types through CLI, REST, GPUI, and Android client tests.
- Assert explicit unavailable errors, no success from stale data, and no writes to project-interop, old state WorkSession maps, or ARP duplicate tables.
- Restart Switchboard and assert operations recover without replaying an uncommitted local write queue.

### Test 8: Authorization and secret isolation

- Create two Projects and scoped credentials.
- Assert each scope sees and mutates only allowed WorkSessions through REST/MCP/gRPC.
- Create a headless WorkSession and inspect Switchboard files/resources, REST lists, logs, diagnostics, Android payloads, and ProjectSnapshot exports for Bezalel tokens. Assert none are present.

### Test 9: Contract removal

- Invoke old CLI commands, `/workspaces` routes, `workspace/*` MCP episode tools, and old gRPC calls. Assert they are absent.
- Build all first-party clients and run source/generated-artifact searches. Assert remaining `workspace` uses refer only to material environments, OS/Cargo/GPUI concepts, or explicit historical notes.

### Commands

Run narrow tests while implementing, then the full gate:

```sh
cargo test --workspace
cargo build --workspace
cargo clippy --workspace -- -D warnings
SCREENSHOTS=1 cargo test --test screenshots -- --nocapture
make -C /home/aleks/work/projects/agent-work-model check
make -C /home/aleks/work/projects/switchboard/worktrees/impl-project-catalog-mcp ci
(cd android && ./gradlew testDebugUnitTest assembleDebug)
(cd android && ./gradlew connectedDebugAndroidTest)
```

Run the updated Docker/spec-torture suite through `test/run-arp-tests.sh` after it is renamed/reworked for the WorkSession contract. Add Compose UI instrumentation covering profile loading, exact-ID default selection, explicit selection, missing-default errors, and persistence observed directly from Switchboard. A missing external window manager may justify a narrow adapter at that boundary only; a missing Switchboard process, MCP transport, git executable, or required Android emulator in the completion environment is a blocker, not permission to fake the end-to-end test.

## Anti-Cheating Audit

- Inspect `src/interop.rs` removal and all callers: no Project filesystem CRUD, repo-local overlay, project-interop base path, or hidden fallback may remain.
- Inspect `src/state.rs` and `src/arp_store.rs`: neither may persist Project, WorkProfile, or WorkSession definitions/lifecycle as alternate truth. Runtime rows must be keyed by `work_session_id` and contain only local realization facts.
- Inspect `src/workspace.rs`, `src/server.rs`, `src/mcp/`, `src/grpc/`, and `src/daemon_main.rs`: handlers must call the shared application service, not duplicate worktree creation or return hard-coded success.
- Inspect the Switchboard client: it must use production MCP transport and must not shell out, read Switchboard files, parse rendered prose, swallow typed conflicts, or retry non-idempotent mutations broadly.
- Inspect Switchboard's `default` WorkProfile initialization: it must be durable, idempotent, visible through native MCP, and must not overwrite an existing customized record.
- Inspect ProjectSnapshot validation: tests must use real archived revisions and fail mismatched/fabricated `project_revision` and `project_snapshot_id`; values such as `sha256:abc` must not pass.
- Inspect profile and Project deletion: referential checks must execute in Switchboard, not only in GPUI/Android/awesometree clients.
- Inspect policy validation: no client-only dropdown/filter may substitute for server-side profile eligibility or narrowing enforcement.
- Inspect creation tests: they must assert direct Switchboard records, immutable revision content, ResourceBinding/Workspace identities, real worktree existence, and restart state, not only HTTP status, JSON fixtures, or mocked method calls.
- Inspect default-profile tests with confusable records: exact ID `default` must win over display name, list order, prefix, or first result.
- Inspect public-boundary coverage: CLI, daemon IPC, REST, awesometree MCP stdio, gRPC, core/UniFFI, GPUI, and Android must each invoke production orchestration in a positive creation test.
- Inspect compensation tests: force failures after authoritative creation and prove `aborted` state plus cleanup; do not use a test-only branch that bypasses production orchestration.
- Inspect authorization tests: enforce scopes server-side before filesystem/process side effects. Ensure list filtering is not merely performed in Android or GPUI.
- Inspect logs, REST/OpenAPI schemas, core models, Android payloads, ProjectSnapshot exports, and diagnostics for tokens. A redacted display must not conceal raw serialization elsewhere.
- Search for old names and routes without excluding broad directories. Review generated protobuf/OpenAPI/UniFFI/Kotlin output as well as handwritten code.
- Inspect feature flags and environment variables: none may reactivate project-interop, dual writes, old Workspace episode APIs, or an in-memory authoritative fake.
- Inspect skipped/ignored tests, tolerant parse fallbacks, discarded UI errors, and broad retries. The cutover must fail visibly when Switchboard or validation fails.
- Verify documentation claims against executable tests. No matrix may claim a surface is migrated unless that exact surface creates and reads a WorkSession through Switchboard.

## Completion Gate

- [ ] Every BDD scenario passes through at least one real public boundary; CLI, daemon IPC, REST, awesometree MCP stdio, gRPC, core/UniFFI, GPUI, and Android each have positive production-orchestration creation evidence.
- [ ] Switchboard directly proves durable Projects, WorkProfiles, WorkSessions, immutable Project pins, referential integrity, and the exact `default` profile.
- [ ] All creation and lifecycle paths use one application service and production Switchboard client.
- [ ] Local state contains only host-runtime realization keyed by `work_session_id`; duplicate JSON/SQLite authority is absent.
- [ ] GPUI and Android can select a WorkProfile and initially select exact ID `default`.
- [ ] Missing default, dependency outage, stale Project edit, invalid transition, invalid reference, and realization failure produce observable typed failures without partial authority.
- [ ] Old workspace-as-episode commands/routes/tools/protos/models and project-interop guidance/fallbacks are removed.
- [ ] Authorization and secret-redaction end-to-end tests pass.
- [ ] OpenAPI, protobuf, UniFFI, Android, screenshots, and other generated artifacts are current.
- [ ] The anti-cheating audit finds no hard-coded success, fake first-party persistence, hidden compatibility mode, swallowed errors, or tests bypassing public boundaries.
- [ ] `cargo test --workspace`, `cargo build --workspace`, `cargo clippy --workspace -- -D warnings`, screenshot tests, AWM `make check`, Switchboard `make ci`, updated Docker/spec-torture tests, and Android build/tests all pass.
- [ ] Repository status contains only intentional implementation, generated, test, and documentation changes for this breaking cutover.
