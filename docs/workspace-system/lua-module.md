# Lua Module: `workspaces.lua`

At `~/.config/awesome/workspaces.lua`, imported in `rc.lua`.

## Functions

| Function | Purpose |
|----------|---------|
| `show_picker(s)` | Menu of all workspaces |
| `show_create(s)` | 3-step prompt |
| `switch(name)` | Focus `P:<name>` tag |
| `cycle_next()` | Cycle project tags |
| `destroy_current()` | Dirty-check + remove |
| `close_current()` | Keep worktree, kill clients |
| `assign_client(c)` | Auto-assign Zed windows |
| `ensure_tag(name)` | Find or create `P:` tag |
| `get_active_names()` | `ws names` |
| `get_all_names()` | `ws allnames` |
| `get_dir(name)` | `ws dir` |
| `get_repos()` | `ws repos` |
| `get_branches(repo)` | `git branch -a` |

## Client Assignment

Connected to `manage` signal. Matches Zed window titles
against `P:` tag names to auto-move windows.

## Tag Convention

Project tags use `P:` prefix. `sharedtagindex` starts at
1000+ to avoid collision with static tags 1-9.

See: [ws CLI](ws-cli.md) | [Lifecycle](lifecycle.md)
