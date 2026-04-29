use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tray::{Icon, TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};

#[cfg(target_os = "linux")]
use tray::dpi::PhysicalPosition;
#[cfg(target_os = "linux")]
use tray_menu::{PopupMenu, TextEntry, Divider};

pub fn run_tray(_workspaces: Vec<(String, bool)>) {
    #[cfg(target_os = "linux")]
    {
        gtk::init().expect("failed to init gtk");
    }

    let icon_bytes = include_bytes!("../assets/tray-icon.png");
    let img = image::load_from_memory(icon_bytes)
        .expect("failed to load tray icon")
        .into_rgba8();
    let (w, h) = img.dimensions();
    let icon = Icon::from_rgba(img.into_raw(), w, h).expect("failed to create icon");

    let _tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("awesometree")
        .build()
        .expect("failed to build tray icon");

    let receiver = TrayIconEvent::receiver();
    loop {
        while let Ok(event) = receiver.try_recv() {
            let position = match &event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    position,
                    ..
                }
                | TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    position,
                    ..
                } => Some(*position),
                _ => None,
            };

            if let Some(pos) = position {
                show_menu(pos);
            }
        }
        #[cfg(target_os = "linux")]
        {
            while gtk::events_pending() {
                gtk::main_iteration();
            }
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(target_os = "linux")]
fn show_menu(position: PhysicalPosition<f64>) {
    let mut menu = PopupMenu::new();
    menu.add(&TextEntry::of("create", "Create Workspace"));
    menu.add(&TextEntry::of("pick", "Open Workspace"));
    menu.add(&Divider);
    menu.add(&TextEntry::of("projects", "Projects"));
    menu.add(&TextEntry::of("agents", "Agents"));
    menu.add(&TextEntry::of("cleanup", "Cleanup Workspaces"));
    menu.add(&TextEntry::of("mobile-qr", "Mobile Connect"));
    menu.add(&TextEntry::of("logs", "Logs"));
    menu.add(&Divider);
    menu.add(&TextEntry::of("restart", "Restart"));
    menu.add(&TextEntry::of("exit", "Exit"));

    if let Some(id) = menu.popup(position) {
        handle_menu_action(id.0.as_str());
    }
}

#[cfg(target_os = "macos")]
fn show_menu(_position: tray::dpi::PhysicalPosition<f64>) {
    let script = r#"
set menuItems to {"Create Workspace", "Open Workspace", "-", "Projects", "Agents", "Cleanup Workspaces", "Mobile Connect", "Logs", "-", "Restart", "Exit"}
set menuIds to {"create", "pick", "", "projects", "agents", "cleanup", "mobile-qr", "logs", "", "restart", "exit"}
set chosen to choose from list menuItems with prompt "awesometree" without multiple selections allowed and empty selection allowed
if chosen is false then return ""
set chosenItem to item 1 of chosen
repeat with i from 1 to count of menuItems
    if item i of menuItems is chosenItem then
        return item i of menuIds
    end if
end repeat
return ""
"#;
    let output = Command::new("osascript")
        .args(["-e", script])
        .output();
    if let Ok(output) = output {
        let choice = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !choice.is_empty() {
            handle_menu_action(&choice);
        }
    }
}

fn awesometree_bin() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("awesometree")))
        .unwrap_or_else(|| PathBuf::from("awesometree"))
}

fn handle_menu_action(id: &str) {
    let bin = awesometree_bin();
    match id {
        "create" => {
            let _ = Command::new(&bin).arg("create-interactive").spawn();
        }
        "pick" => {
            let _ = Command::new(&bin).arg("pick").spawn();
        }
        "projects" => {
            let _ = Command::new(&bin).arg("projects-ui").spawn();
        }
        "agents" => {
            let _ = Command::new(&bin).arg("agents-ui").spawn();
        }
        "cleanup" => {
            let _ = Command::new(&bin).arg("cleanup").spawn();
        }
        "mobile-qr" => {
            let _ = Command::new(&bin).arg("mobile-qr").spawn();
        }
        "logs" => {
            crate::log::request_log_window();
        }
        "restart" => {
            let _ = Command::new(&bin).arg("restart-daemon").spawn();
        }
        "exit" => std::process::exit(0),
        _ => {}
    }
}
