# Workspace Lifecycle

## States

```
[unconfigured] --create--> [inactive] --up--> [active]
[active] --close--> [inactive]
[active] --destroy--> [unconfigured]
```

## Create (`ws create` / `Super+I`)

1. Append `[[workspace]]` to `workspaces.toml`
2. `git fetch origin <branch>` in source repo
3. Create worktree at `~/worktrees/<name>`
4. Unset upstream (push defaults to `origin/<ws-branch>`)
5. Eval Lua to add `P:<name>` tag
6. Launch `zeditor -n <dir>` + configured `gui` commands
7. Write state JSON

## Up (`ws up`)

Same as create steps 2-7 from existing config. On startup,
`setup_autostart()` runs `ws up` for all workspaces.

## Close (`ws down --keep-worktree`)

1. Restore previous tag, kill clients, delete tag
2. Remove from state; worktree stays on disk

## Destroy (`ws destroy`)

1. Abort if `git status --porcelain` shows changes
2. Close (kill clients, delete tag)
3. `git worktree remove`
4. Remove from TOML config

See: [ws CLI](ws-cli.md) | [Lua module](lua-module.md)
