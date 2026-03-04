use std::process::Command;
use tray::dpi::PhysicalPosition;
use tray::{Icon, TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};
use tray_menu::{PopupMenu, TextEntry, Divider};

pub fn run_tray(workspaces: Vec<(String, bool)>) {
    let icon_bytes = include_bytes!("../assets/tray-icon.png");
    let img = image::load_from_memory(icon_bytes)
        .expect("failed to load tray icon")
        .into_rgba8();
    let (w, h) = img.dimensions();
    let icon = Icon::from_rgba(img.into_raw(), w, h).expect("failed to create icon");

    let _tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("Workspaces")
        .build()
        .expect("failed to build tray icon");

    let receiver = TrayIconEvent::receiver();
    loop {
        if let Ok(event) = receiver.recv() {
            match event {
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
                } => {
                    show_menu(&workspaces, position);
                }
                _ => {}
            }
        }
    }
}

fn show_menu(workspaces: &[(String, bool)], position: PhysicalPosition<f64>) {
    let mut menu = PopupMenu::new();
    menu.add(&TextEntry::of("pick", "Pick Workspace"));
    menu.add(&TextEntry::of("create", "Create Workspace"));
    menu.add(&Divider);

    for (name, active) in workspaces {
        let label = if *active {
            format!("● {name}")
        } else {
            format!("  {name}")
        };
        menu.add(&TextEntry::of(name.clone(), label));
    }

    menu.add(&Divider);
    menu.add(&TextEntry::of("quit", "Quit"));

    if let Some(id) = menu.popup(position) {
        match id.0.as_str() {
            "pick" => {
                let _ = Command::new("awesometree").arg("pick").spawn();
            }
            "create" => {
                let _ = Command::new("awesometree").arg("create-interactive").spawn();
            }
            "quit" => std::process::exit(0),
            ws_name => {
                let is_active = workspaces
                    .iter()
                    .any(|(n, a)| n == ws_name && *a);
                if is_active {
                    let _ = Command::new("awesometree").args(["switch", ws_name]).output();
                } else {
                    let _ = Command::new("awesometree").args(["up", ws_name]).output();
                    let _ = Command::new("awesometree").args(["switch", ws_name]).output();
                }
            }
        }
    }
}
