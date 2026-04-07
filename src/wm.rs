use std::process::Command;

pub fn tag_name(project: &str, workspace: &str) -> String {
    format!("{project}:{workspace}")
}

pub fn parse_tag_name(tag: &str) -> Option<(&str, &str)> {
    tag.split_once(':')
}

pub trait Adapter {
    fn create_tag(&self, tag: &str, index: i32, layout: &str) -> Result<(), String>;
    fn delete_tag(&self, tag: &str) -> Result<(), String>;
    fn switch_tag(&self, tag: &str) -> Result<(), String>;
    fn kill_tag_clients(&self, tag: &str) -> Result<(), String>;
    fn eval(&self, lua: &str) -> Result<(), String>;
    fn get_current_tag_name(&self) -> Result<Option<String>, String>;
    fn restore_previous_tag(&self) -> Result<(), String>;
}

pub fn platform_adapter() -> Box<dyn Adapter> {
    #[cfg(target_os = "linux")]
    {
        Box::new(AwesomeAdapter::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(MacosAdapter::new())
    }
}

// ---------------------------------------------------------------------------
// Linux: AwesomeWM adapter
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
pub struct AwesomeAdapter;

#[cfg(target_os = "linux")]
impl Default for AwesomeAdapter {
    fn default() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl AwesomeAdapter {
    pub fn new() -> Self {
        Self
    }

    fn awesome_eval(&self, lua: &str) -> Result<(), String> {
        let mut child = Command::new("awesome-client")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn awesome-client: {e}"))?;
        use std::io::Write;
        if let Some(ref mut stdin) = child.stdin {
            stdin
                .write_all(lua.as_bytes())
                .map_err(|e| format!("write to awesome-client: {e}"))?;
        }
        child.wait().map_err(|e| format!("awesome-client: {e}"))?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn layout_to_lua(layout: &str) -> &str {
    match layout {
        "fair" => "awful.layout.suit.fair",
        "max" => "awful.layout.suit.max",
        "floating" => "awful.layout.suit.floating",
        _ => "awful.layout.suit.tile",
    }
}

#[cfg(target_os = "linux")]
impl Adapter for AwesomeAdapter {
    fn create_tag(&self, tag: &str, index: i32, layout: &str) -> Result<(), String> {
        let lua_layout = layout_to_lua(layout);
        let lua = format!(
            r#"
local awful = require("awful")
local sharedtags = require("sharedtags")
local target_tag = nil
for _, t in ipairs(root.tags()) do
    if t.name == "{tag}" then
        target_tag = t
        break
    end
end
if not target_tag then
    target_tag = awful.tag.add("{tag}", {{
        screen = awful.screen.focused(),
        layout = {lua_layout},
        sharedtagindex = {index},
    }})
end
"#
        );
        self.awesome_eval(&lua)
    }

    fn delete_tag(&self, tag: &str) -> Result<(), String> {
        let lua = format!(
            r#"
local awful = require("awful")
for _, t in ipairs(root.tags()) do
    if t.name == "{tag}" then
        if t.selected then
            awful.tag.history.restore()
        end
        local clients = t:clients()
        for _, c in ipairs(clients) do
            c:kill()
        end
        local function try_delete()
            if #t:clients() == 0 then
                t:delete()
            else
                require("gears.timer").start_new(0.2, function()
                    try_delete()
                    return false
                end)
            end
        end
        try_delete()
        break
    end
end
"#
        );
        self.awesome_eval(&lua)
    }

    fn switch_tag(&self, tag: &str) -> Result<(), String> {
        let lua = format!(
            r#"
local awful = require("awful")
local sharedtags = require("sharedtags")
for _, t in ipairs(root.tags()) do
    if t.name == "{tag}" then
        sharedtags.viewonly(t, awful.screen.focused())
        break
    end
end
"#
        );
        self.awesome_eval(&lua)
    }

    fn kill_tag_clients(&self, tag: &str) -> Result<(), String> {
        let lua = format!(
            r#"
for _, c in ipairs(client.get()) do
    for _, t in ipairs(c:tags()) do
        if t.name == "{tag}" then
            c:kill()
            break
        end
    end
end
"#
        );
        self.awesome_eval(&lua)
    }

    fn eval(&self, lua: &str) -> Result<(), String> {
        self.awesome_eval(lua)
    }

    fn get_current_tag_name(&self) -> Result<Option<String>, String> {
        self.awesome_eval(
            r#"
local awful = require("awful")
local s = awful.screen.focused()
local tag = s.selected_tag
if tag then
    local f = io.open("/tmp/ws-current-tag", "w")
    if f then
        f:write(tag.name)
        f:close()
    end
end
"#,
        )?;
        let path = std::path::Path::new("/tmp/ws-current-tag");
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(path);
        let raw = data.trim();
        if parse_tag_name(raw).is_some() {
            Ok(Some(raw.to_string()))
        } else {
            Ok(None)
        }
    }

    fn restore_previous_tag(&self) -> Result<(), String> {
        self.awesome_eval(r#"require("awful").tag.history.restore()"#)
    }
}

// ---------------------------------------------------------------------------
// macOS adapter
//
// macOS has no scriptable tiling WM equivalent to AwesomeWM.  We support two
// modes:
//
//   1. **yabai** (optional) — a third-party tiling WM for macOS that exposes
//      a CLI.  When yabai is installed we create/destroy spaces and manage
//      window focus through it.
//
//   2. **Fallback** — When yabai is not available we keep a lightweight
//      bookkeeping file (`/tmp/awesometree-macos-tags.json`) that maps tag
//      names to macOS Space indices.  We can still switch spaces via
//      AppleScript (`tell application "System Events" …`) and quit apps via
//      `osascript`, but we cannot programmatically *create* Mission Control
//      spaces without accessibility permissions.
//
// The `eval` method accepts AppleScript instead of Lua on macOS.
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::sync::Mutex;

#[cfg(target_os = "macos")]
static MACOS_TAGS: std::sync::OnceLock<Mutex<MacosTagState>> = std::sync::OnceLock::new();

#[cfg(target_os = "macos")]
struct MacosTagState {
    tags: HashMap<String, MacosTag>,
    previous_tag: Option<String>,
    current_tag: Option<String>,
}

#[cfg(target_os = "macos")]
struct MacosTag {
    _index: i32,
    _layout: String,
}

#[cfg(target_os = "macos")]
fn tag_state() -> &'static Mutex<MacosTagState> {
    MACOS_TAGS.get_or_init(|| {
        let state = load_macos_tag_state().unwrap_or_else(|| MacosTagState {
            tags: HashMap::new(),
            previous_tag: None,
            current_tag: None,
        });
        Mutex::new(state)
    })
}

#[cfg(target_os = "macos")]
const MACOS_TAG_FILE: &str = "/tmp/awesometree-macos-tags.json";

#[cfg(target_os = "macos")]
fn load_macos_tag_state() -> Option<MacosTagState> {
    let data = std::fs::read_to_string(MACOS_TAG_FILE).ok()?;
    let json: serde_json::Value = serde_json::from_str(&data).ok()?;
    let obj = json.as_object()?;
    let mut tags = HashMap::new();
    if let Some(t) = obj.get("tags").and_then(|v| v.as_object()) {
        for (name, val) in t {
            let index = val.get("index").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let layout = val
                .get("layout")
                .and_then(|v| v.as_str())
                .unwrap_or("tile")
                .to_string();
            tags.insert(
                name.clone(),
                MacosTag {
                    _index: index,
                    _layout: layout,
                },
            );
        }
    }
    let previous_tag = obj
        .get("previous_tag")
        .and_then(|v| v.as_str())
        .map(String::from);
    let current_tag = obj
        .get("current_tag")
        .and_then(|v| v.as_str())
        .map(String::from);
    Some(MacosTagState {
        tags,
        previous_tag,
        current_tag,
    })
}

#[cfg(target_os = "macos")]
fn save_macos_tag_state(state: &MacosTagState) {
    let mut tags = serde_json::Map::new();
    for (name, tag) in &state.tags {
        let mut entry = serde_json::Map::new();
        entry.insert("index".into(), serde_json::Value::from(tag._index));
        entry.insert("layout".into(), serde_json::Value::from(tag._layout.clone()));
        tags.insert(name.clone(), serde_json::Value::Object(entry));
    }
    let mut root = serde_json::Map::new();
    root.insert("tags".into(), serde_json::Value::Object(tags));
    if let Some(ref prev) = state.previous_tag {
        root.insert("previous_tag".into(), serde_json::Value::from(prev.clone()));
    }
    if let Some(ref cur) = state.current_tag {
        root.insert("current_tag".into(), serde_json::Value::from(cur.clone()));
    }
    let json = serde_json::Value::Object(root);
    let _ = std::fs::write(MACOS_TAG_FILE, json.to_string());
}

#[cfg(target_os = "macos")]
pub struct MacosAdapter {
    has_yabai: bool,
}

#[cfg(target_os = "macos")]
impl MacosAdapter {
    pub fn new() -> Self {
        let has_yabai = Command::new("which")
            .arg("yabai")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        Self { has_yabai }
    }

    fn osascript(&self, script: &str) -> Result<String, String> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| format!("osascript: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("osascript error: {}", stderr.trim()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn yabai_cmd(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("yabai")
            .args(args)
            .output()
            .map_err(|e| format!("yabai: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("yabai error: {}", stderr.trim()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[cfg(target_os = "macos")]
impl Adapter for MacosAdapter {
    fn create_tag(&self, tag: &str, index: i32, layout: &str) -> Result<(), String> {
        if self.has_yabai {
            let _ = self.yabai_cmd(&["-m", "space", "--create"]);
            if !layout.is_empty() && layout != "tile" {
                let yabai_layout = match layout {
                    "max" => "stack",
                    "floating" => "float",
                    _ => "bsp",
                };
                let _ = self.yabai_cmd(&["-m", "space", "last", "--layout", yabai_layout]);
            }
            let _ = self.yabai_cmd(&["-m", "space", "last", "--label", tag]);
        }

        let mut state = tag_state().lock().unwrap();
        state.tags.insert(
            tag.to_string(),
            MacosTag {
                _index: index,
                _layout: layout.to_string(),
            },
        );
        save_macos_tag_state(&state);
        Ok(())
    }

    fn delete_tag(&self, tag: &str) -> Result<(), String> {
        if self.has_yabai {
            let _ = self.yabai_cmd(&["-m", "space", "--destroy", tag]);
        }

        let mut state = tag_state().lock().unwrap();
        state.tags.remove(tag);
        if state.current_tag.as_deref() == Some(tag) {
            state.current_tag = state.previous_tag.take();
        }
        save_macos_tag_state(&state);
        Ok(())
    }

    fn switch_tag(&self, tag: &str) -> Result<(), String> {
        if self.has_yabai {
            self.yabai_cmd(&["-m", "space", "--focus", tag])?;
        } else {
            let state = tag_state().lock().unwrap();
            if let Some(macos_tag) = state.tags.get(tag) {
                let idx = macos_tag._index;
                drop(state);
                let script = format!(
                    r#"tell application "System Events" to key code {}"#,
                    mission_control_key_code(idx)
                );
                let _ = self.osascript(&script);
            }
        }

        let mut state = tag_state().lock().unwrap();
        state.previous_tag = state.current_tag.take();
        state.current_tag = Some(tag.to_string());
        save_macos_tag_state(&state);
        Ok(())
    }

    fn kill_tag_clients(&self, _tag: &str) -> Result<(), String> {
        Ok(())
    }

    fn eval(&self, script: &str) -> Result<(), String> {
        self.osascript(script)?;
        Ok(())
    }

    fn get_current_tag_name(&self) -> Result<Option<String>, String> {
        let state = tag_state().lock().unwrap();
        Ok(state.current_tag.clone())
    }

    fn restore_previous_tag(&self) -> Result<(), String> {
        let prev = {
            let state = tag_state().lock().unwrap();
            state.previous_tag.clone()
        };
        if let Some(tag) = prev {
            self.switch_tag(&tag)?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn mission_control_key_code(index: i32) -> u8 {
    match index {
        1 => 18,  // key code for '1'
        2 => 19,  // '2'
        3 => 20,  // '3'
        4 => 21,  // '4'
        5 => 23,  // '5'
        6 => 22,  // '6'
        7 => 26,  // '7'
        8 => 28,  // '8'
        9 => 25,  // '9'
        _ => 18,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_name_format() {
        assert_eq!(tag_name("myproj", "feat-1"), "myproj:feat-1");
    }

    #[test]
    fn parse_tag_name_valid() {
        let (project, ws) = parse_tag_name("myproj:feat-1").unwrap();
        assert_eq!(project, "myproj");
        assert_eq!(ws, "feat-1");
    }

    #[test]
    fn parse_tag_name_no_colon() {
        assert!(parse_tag_name("nocolon").is_none());
    }

    #[test]
    fn parse_tag_name_empty_parts() {
        let (a, b) = parse_tag_name(":ws").unwrap();
        assert_eq!(a, "");
        assert_eq!(b, "ws");
    }

    #[test]
    fn parse_tag_name_multiple_colons() {
        let (project, ws) = parse_tag_name("proj:ws:extra").unwrap();
        assert_eq!(project, "proj");
        assert_eq!(ws, "ws:extra");
    }

    #[test]
    fn tag_name_roundtrip() {
        let tag = tag_name("proj", "ws");
        let (p, w) = parse_tag_name(&tag).unwrap();
        assert_eq!(p, "proj");
        assert_eq!(w, "ws");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn layout_to_lua_known() {
        assert_eq!(layout_to_lua("fair"), "awful.layout.suit.fair");
        assert_eq!(layout_to_lua("max"), "awful.layout.suit.max");
        assert_eq!(layout_to_lua("floating"), "awful.layout.suit.floating");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn layout_to_lua_default() {
        assert_eq!(layout_to_lua("tile"), "awful.layout.suit.tile");
        assert_eq!(layout_to_lua("unknown"), "awful.layout.suit.tile");
        assert_eq!(layout_to_lua(""), "awful.layout.suit.tile");
    }
}
