# Plan: awesometree on `.project.json`

## Data Model

**Project definitions** → `$XDG_CONFIG_HOME/project-interop/projects/<name>.project.json` (spec format)
**Workspace runtime state** → `~/.config/awesometree/state.json` (awesometree-specific)
**Context store** → `$XDG_CONFIG_HOME/project-interop/context/<name>/`

No `config.json`. No `servers.json`. No migration command.

### `.project.json` (what awesometree reads)

```json
{
  "version": "1",
  "name": "acme-api",
  "repo": "~/work/acme-api",
  "branch": "main",
  "launch": {
    "prompt": "Use project_context for context. Do not read AGENTS.md.",
    "env": { "PROJECT_NAME": "acme-api" }
  },
  "context": {
    "files": ["onboarding.md"],
    "repoIncludes": ["AGENTS.md", "docs/architecture.md"]
  },
  "extensions": {
    "dev.awesometree": {
      "mcp": "http://localhost:4567/mcp/{project}",
      "gui": ["firefox https://app.acme.com"],
      "layout": "tile"
    }
  }
}
```

awesometree reads: `name`, `repo`, `branch`, `launch`, `context`, `extensions.dev.awesometree`. It passes through `tools`/`agents` without interpreting them.

### `state.json` (workspace runtime)

```json
{
  "workspaces": {
    "feature-x": {
      "project": "acme-api",
      "active": true,
      "tag_index": 10,
      "dir": "/home/aleks/worktrees/acme-api/feature-x"
    }
  }
}
```

Keyed by workspace name. Entirely awesometree's concern.

## File-Level Changes

| File | Change |
|---|---|
| `Cargo.toml` | Add `glob` crate |
| `src/lib.rs` | Replace `pub mod config` with `pub mod interop; pub mod state;` |
| `src/interop.rs` | **New** — `.project.json` CRUD, merge, MCP URL expansion, context helpers |
| `src/state.rs` | **New** — workspace runtime state load/save |
| `src/config.rs` | **Delete** — replaced entirely by `interop` + `state` |
| `src/workspace.rs` | Rewrite `Manager` to use `interop::Project` + `state::Store`; add `launch_agent()` |
| `src/main.rs` | Rewrite CLI: new subcommand tree, all commands use new data layer |
| `src/daemon.rs` | Add `DaemonCmd::LaunchAgent` |
| `src/daemon_main.rs` | Handle `LaunchAgent`; load from new data layer |
| `src/picker.rs` | Read from `interop` + `state`; add agent launch action |
| `src/projects_ui.rs` | Read/write `.project.json`; form fields for launch prompt, MCP URL, context |
| `src/wm.rs` | Unchanged |
| `src/notify.rs` | Unchanged |
| `src/tray.rs` | Unchanged |

## Module Design

### `src/interop.rs`

```
pub struct Project       — mirrors .project.json schema
pub struct Launch        — prompt, promptFile, env
pub struct ContextConfig — files, repoIncludes, maxBytes
pub struct AwesometreeExt — mcp, gui, layout (from extensions.dev.awesometree)

pub fn base_dir() -> PathBuf                          — $XDG_CONFIG_HOME/project-interop
pub fn projects_dir() -> PathBuf                      — base_dir/projects
pub fn context_dir(name) -> PathBuf                   — base_dir/context/<name>
pub fn load(name) -> Result<Project>                  — read from user store
pub fn load_merged(name, repo_path) -> Result<Project> — user store + repo-local .project.json
pub fn save(project) -> Result<()>                    — atomic write-rename
pub fn list() -> Result<Vec<Project>>                 — glob projects/*.project.json
pub fn delete(name) -> Result<()>                     — remove file
pub fn expand_mcp_url(template, name) -> String       — {project} → name
pub fn assemble_launch_prompt(project) -> Result<String> — prompt + promptFile content
pub fn assemble_context_bundle(project) -> Result<Vec<(String, String)>> — layers 1+2
```

Merge logic per RFC-0001 §5: scalars (repo-local wins), arrays (concatenate),
`launch.prompt`/`promptFile` (repo-local wins), `launch.env` (merge per key),
`extensions` (merge per key).

### `src/state.rs`

```
pub struct Store         — HashMap<String, WorkspaceState>
pub struct WorkspaceState — project, active, tag_index, dir

pub fn load() -> Result<Store>       — read ~/.config/awesometree/state.json
pub fn save(store) -> Result<()>     — atomic write
impl Store:
  fn workspace(name) -> Option<&WorkspaceState>
  fn set_active(name, project, tag_index, dir)
  fn set_inactive(name)
  fn remove(name)
  fn active_workspaces() -> Vec<(String, WorkspaceState)>
  fn all_workspaces() -> Vec<(String, WorkspaceState)>
  fn workspaces_for_project(project) -> Vec<(String, WorkspaceState)>
```

### `src/workspace.rs` (revised Manager)

```
pub struct Manager { projects: Vec<Project>, state: Store, wm: Box<dyn Adapter> }

Manager::up(workspace_name, project, opts)   — worktree + tag + gui (reads AwesometreeExt)
Manager::down(workspace_name, opts)          — kill clients + tag + optional worktree removal
Manager::switch(workspace_name)              — WM tag switch
Manager::is_dirty(workspace_name)            — git status check
Manager::launch_agent(workspace_name, agent_host) — assemble prompt + URL, exec agent
```

`launch_agent` flow:
1. Load merged project definition
2. `assemble_launch_prompt()` → full prompt string
3. `expand_mcp_url()` → resolved URL (if `mcp` present in extensions)
4. Set `launch.env` in child process environment
5. Exec agent host (hardcoded patterns for claude/codex initially)

### `src/main.rs` (revised CLI)

```
awesometree up [name] [--no-tag] [--no-launch]
awesometree down [name] [--no-tag] [--keep-worktree]
awesometree create <name> --project <p> [--no-tag] [--no-launch]
awesometree destroy <name> [--no-tag]
awesometree destroy-current
awesometree close
awesometree cycle
awesometree switch <name>
awesometree list

awesometree project list
awesometree project show <name>
awesometree project create <name> --repo <path> [--branch <branch>]
awesometree project edit <name>
awesometree project delete <name>

awesometree context list <project>
awesometree context add <project> <file>
awesometree context edit <project> <file>
awesometree context rm <project> <file>
awesometree context bundle <project>

awesometree launch-agent <workspace> [--agent claude|codex]

awesometree pick                    (daemon)
awesometree create-interactive      (daemon)
awesometree projects-ui             (daemon)
awesometree daemon
awesometree restart-daemon

awesometree repos
awesometree names
awesometree allnames
awesometree dir <name>
awesometree edit <name>             (opens .project.json in $EDITOR)
```

## Implementation Order

1. `src/interop.rs` — pure data: structs, load/save/list/delete/merge, URL expansion, prompt assembly, context bundle
2. `src/state.rs` — pure data: workspace runtime state
3. `src/workspace.rs` — rewrite Manager on new data layer, including launch_agent()
4. `src/main.rs` — rewrite CLI with full subcommand tree
5. `src/daemon_main.rs` + `src/daemon.rs` — update to use new data layer
6. `src/picker.rs` — update to read from interop + state
7. `src/projects_ui.rs` — update to read/write .project.json
8. Delete `src/config.rs`

## What's NOT Changing

- `src/wm.rs` — Adapter trait and AwesomeAdapter unchanged
- `src/notify.rs` — error reporting unchanged
- `src/tray.rs` — tray icon unchanged (reads workspace list differently but same shape)
- Makefile — same build
- rc.lua.example — same keybindings
- Worktree layout (`~/worktrees/<project>/<name>`) — same
- Tag convention (`P:<name>`) — same
- Daemon socket protocol — same commands plus `launch-agent`
