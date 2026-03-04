# WM Integration

All WM logic lives in the Rust binary behind the `Adapter` trait
(`src/wm.rs`). The old `workspaces.lua` module has been removed.

## `AwesomeAdapter`

Implements `Adapter` by piping Lua to `awesome-client`. Operations:

| Method | Lua Effect |
|--------|------------|
| `create_tag` | `awful.tag.add("P:<name>", {sharedtagindex=N, layout=…})` |
| `delete_tag` | Find `P:<name>` tag and call `t:delete()` |
| `switch_tag` | `sharedtags.viewonly(t, screen)` |
| `kill_tag_clients` | Kill all clients on `P:<name>` tag |
| `get_current_tag_name` | Write focused tag name to `/tmp/ws-current-tag` |
| `restore_previous_tag` | `awful.tag.history.restore()` |

## rc.lua Setup

See `rc.lua.example` for a minimal integration. It provides:

1. **Keybindings** — `Super+O/I/D/J/L` spawn `awesometree` commands
2. **Window rules** — Float the picker/projects windows, no titlebar
3. **Client assignment** — `manage` signal moves Zed windows to
   matching `P:` tags by title
4. **Autostart** — `awesometree up` + `awesometree daemon`

## Tag Convention

Project tags use `P:` prefix (e.g. `P:feature-x`).
`sharedtagindex` starts at 10+ to avoid collision with static
tags 1–9. The `sharedtags` library handles multi-screen display.

See: [Architecture](../architecture.md) | [Keybindings](../keybindings.md)
