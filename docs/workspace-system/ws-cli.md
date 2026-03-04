# `ws` CLI

Python script at `~/.local/bin/ws`.

## Commands

| Command | Description |
|---------|-------------|
| `ws up [name]` | Start all or one workspace |
| `ws down [name]` | Tear down workspace(s) |
| `ws create <n> [repo] [br]` | Create + start |
| `ws destroy <name>` | Remove worktree + config |
| `ws list` | Show status |
| `ws switch <name>` | Focus workspace tag |
| `ws repos` | Git repos in `~/work/` |
| `ws names` | Active names |
| `ws allnames` | All configured names |
| `ws dir <name>` | Print workspace dir |
| `ws defaults` | Default repo/branch |
| `ws edit` | Open config in `$EDITOR` |

## Flags

- `--no-tag`: Skip AwesomeWM tag creation
- `--no-launch`: Skip launching Zed/GUI apps
- `--keep-worktree`: Keep worktree on `down`

## awesome-client Integration

`ws` evals Lua via `awesome-client` to create/delete tags.
The Lua module calls `ws` via `io.popen` for queries.

See: [Configuration](configuration.md) | [Lifecycle](lifecycle.md)
