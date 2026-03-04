# Configuration

## `~/.config/awesometree/config.json`

```json
{
  "projects": [
    {
      "name": "myrepo",
      "repo": "~/work/myrepo",
      "branch": "master",
      "gui": ["firefox"],
      "layout": "tile",
      "workspaces": [
        { "name": "feature-x", "active": true, "tag_index": 10, "dir": "..." },
        { "name": "bugfix-y", "active": false, "tag_index": 0, "dir": "" }
      ]
    }
  ]
}
```

## Project Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Project identifier |
| `repo` | yes | Path to git repo (`~/` expanded) |
| `branch` | no | Base branch for new worktrees (default: `master`) |
| `gui` | no | Extra shell commands launched alongside Zed |
| `layout` | no | WM layout: `tile`, `fair`, `max`, `floating` |

## Workspace Entry Fields

| Field | Managed | Description |
|-------|---------|-------------|
| `name` | user | Workspace name (used in tag and worktree dir) |
| `active` | auto | Whether workspace is currently up |
| `tag_index` | auto | AwesomeWM `sharedtagindex` (≥ `TAG_OFFSET`) |
| `dir` | auto | Resolved worktree path when active |

## Worktree Layout

```
~/worktrees/<project>/<workspace-name>/
```

Slashes in workspace names become hyphens. Created on `up`;
removed on `destroy`; kept on `close`.

## Tag Indexing

Tag indices start at `TAG_OFFSET = 10` to avoid collision
with static AwesomeWM tags (1–9). Allocated by finding the
first unused index.

See: [CLI Reference](ws-cli.md) | [Lifecycle](lifecycle.md)
