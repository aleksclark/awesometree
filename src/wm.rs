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

pub struct AwesomeAdapter;

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

fn layout_to_lua(layout: &str) -> &str {
    match layout {
        "fair" => "awful.layout.suit.fair",
        "max" => "awful.layout.suit.max",
        "floating" => "awful.layout.suit.floating",
        _ => "awful.layout.suit.tile",
    }
}

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

    #[test]
    fn layout_to_lua_known() {
        assert_eq!(layout_to_lua("fair"), "awful.layout.suit.fair");
        assert_eq!(layout_to_lua("max"), "awful.layout.suit.max");
        assert_eq!(layout_to_lua("floating"), "awful.layout.suit.floating");
    }

    #[test]
    fn layout_to_lua_default() {
        assert_eq!(layout_to_lua("tile"), "awful.layout.suit.tile");
        assert_eq!(layout_to_lua("unknown"), "awful.layout.suit.tile");
        assert_eq!(layout_to_lua(""), "awful.layout.suit.tile");
    }
}
