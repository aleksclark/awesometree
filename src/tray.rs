use std::process::Command;
use std::time::Duration;
use tray::dpi::PhysicalPosition;
use tray::{Icon, TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};
use tray_menu::{PopupMenu, TextEntry, Divider};

pub fn run_tray(_workspaces: Vec<(String, bool)>) {
    gtk::init().expect("failed to init gtk");

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
        while gtk::events_pending() {
            gtk::main_iteration();
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn show_menu(position: PhysicalPosition<f64>) {
    let mut menu = PopupMenu::new();
    menu.add(&TextEntry::of("create", "Create Workspace"));
    menu.add(&TextEntry::of("pick", "Open Workspace"));
    menu.add(&Divider);
    menu.add(&TextEntry::of("projects", "Projects"));
    menu.add(&TextEntry::of("logs", "Logs"));
    menu.add(&Divider);
    menu.add(&TextEntry::of("restart", "Restart"));
    menu.add(&TextEntry::of("exit", "Exit"));

    if let Some(id) = menu.popup(position) {
        match id.0.as_str() {
            "create" => {
                let _ = Command::new("awesometree").arg("create-interactive").spawn();
            }
            "pick" => {
                let _ = Command::new("awesometree").arg("pick").spawn();
            }
            "projects" => {
                let _ = Command::new("awesometree").arg("projects-ui").spawn();
            }
            "logs" => {
                crate::log::request_log_window();
            }
            "restart" => {
                let _ = Command::new("awesometree").arg("restart-daemon").spawn();
            }
            "exit" => std::process::exit(0),
            _ => {}
        }
    }
}
