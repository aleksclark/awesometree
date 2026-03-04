# Configuration

## `~/.config/workspaces.toml`

```toml
[defaults]
repo = "/home/user/work/myrepo"
branch = "master"

[[workspace]]
name = "feature-x"
repo = "/home/user/work/myrepo"
branch = "master"
# layout = "tile"   # tile|fair|max|floating
# path = "~/custom" # overrides worktree
# gui = ["firefox"] # extra apps
```

## Fields

| Field | Req | Description |
|-------|-----|-------------|
| `name` | yes | Used in tag name and worktree dir |
| `repo` | no | Git repo (fallback: `[defaults]`) |
| `branch` | no | Base branch; omit for path-only |
| `path` | no | Explicit dir (skips worktree) |
| `gui` | no | Shell commands launched with Zed |
| `layout` | no | WM layout (default: `tile`) |

## Runtime State

`~/.local/state/workspaces.json` tracks `tag_index`, `dir`,
`active` per workspace. Managed by `ws`, not user-edited.

## Worktrees

At `~/worktrees/<name>` (`/` -> `-`). Removed on `destroy`;
kept on `close`.

See: [ws CLI](ws-cli.md) | [Lifecycle](lifecycle.md)
