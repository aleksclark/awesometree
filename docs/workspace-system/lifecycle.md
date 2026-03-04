# Workspace Lifecycle

## States

```
[no config] ──create──▶ [inactive] ──up──▶ [active]
                 [active] ──close──▶ [inactive]
                 [active] ──destroy──▶ [removed]
```

## Create (`awesometree create` / create form)

1. Validate project exists (or create new project)
2. Append `WorkspaceEntry` to the project in config JSON
3. Run `Manager::up` (see below)

## Up (`Manager::up`)

1. `git fetch origin <branch>` in source repo
2. `git worktree add` at `~/worktrees/<project>/<name>`
   (reuses existing branch or creates from `origin/<branch>`)
3. `git branch --unset-upstream` on the new branch
4. Eval Lua via `awesome-client` to create `P:<name>` tag
   with `sharedtagindex` (offset from `TAG_OFFSET = 10`)
5. Launch `zeditor -n <dir>` + any `gui` commands
6. Mark workspace active in config, save JSON

On startup, `awesometree up` (no name) brings up all
previously-active workspaces without launching apps.

## Close (`awesometree close`)

1. Detect current `P:` tag via `awesome-client`
2. Restore previous tag (`awful.tag.history.restore`)
3. Kill tag clients, delete tag
4. Mark workspace inactive; worktree stays on disk

## Destroy (`awesometree destroy-current`)

1. Detect current `P:` tag
2. Abort if `git status --porcelain` shows changes
3. Restore previous tag, kill clients, delete tag
4. `git worktree remove`
5. Remove workspace entry from config JSON

See: [CLI Reference](ws-cli.md) | [Configuration](configuration.md)
