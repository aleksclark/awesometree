use awesometree::picker::{self, PickerItem, PickerMode};
use awesometree::projects_ui;
use awesometree::qr;
use gpui::*;
use std::env;
use std::sync::mpsc;

fn sample_picker_items() -> Vec<PickerItem> {
    vec![
        PickerItem { name: "aleks/make-docs-fancy".into(), project: "awesometree".into(), active: true, acp_status: Some("running".into()) },
        PickerItem { name: "remote-protocol".into(), project: "awesometree".into(), active: false, acp_status: None },
        PickerItem { name: "systemd-integration".into(), project: "awesometree".into(), active: false, acp_status: None },
        PickerItem { name: "fix-data-node-compliance".into(), project: "blockyard".into(), active: false, acp_status: None },
        PickerItem { name: "mgmt-api".into(), project: "blockyard".into(), active: false, acp_status: None },
        PickerItem { name: "ublk-driver".into(), project: "blockyard".into(), active: false, acp_status: None },
        PickerItem { name: "aleks/some-cool-thing".into(), project: "curri".into(), active: true, acp_status: None },
        PickerItem { name: "aleks/booking-flow-slow".into(), project: "curri".into(), active: false, acp_status: None },
        PickerItem { name: "aleks/graphql-ast-lint".into(), project: "curri".into(), active: false, acp_status: None },
        PickerItem { name: "phase-3".into(), project: "streamlate".into(), active: true, acp_status: Some("running".into()) },
        PickerItem { name: "phase-4".into(), project: "streamlate".into(), active: true, acp_status: None },
    ]
}

fn sample_project_names() -> Vec<String> {
    vec![
        "awesometree".into(),
        "blockyard".into(),
        "crush".into(),
        "curri".into(),
        "streamlate".into(),
    ]
}

fn signal_ready(name: &str) {
    let dir = env::var("SCREENSHOT_SIGNAL_DIR").unwrap_or_else(|_| "/tmp/awesometree-screenshots".into());
    let path = format!("{dir}/{name}.ready");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&path, "");
}

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "picker".into());

    let app = Application::new();
    app.run(move |cx: &mut App| {
        awesometree::text_input::bind_text_input_keys(cx);
        cx.bind_keys([
            KeyBinding::new("escape", picker::Cancel, None),
            KeyBinding::new("enter", picker::Confirm, None),
            KeyBinding::new("down", picker::SelectNext, None),
            KeyBinding::new("up", picker::SelectPrev, None),
            KeyBinding::new("tab", picker::TabForward, None),
            KeyBinding::new("shift-tab", picker::TabBack, None),
            KeyBinding::new("ctrl-n", picker::OpenCreate, None),
            KeyBinding::new("ctrl-d", picker::DestroySelected, None),
            KeyBinding::new("escape", projects_ui::Dismiss, None),
            KeyBinding::new("escape", qr::DismissQr, None),
            KeyBinding::new("enter", projects_ui::ConfirmAction, None),
            KeyBinding::new("tab", projects_ui::NextField, None),
            KeyBinding::new("shift-tab", projects_ui::PrevField, None),
        ]);

        let (tx, _rx) = mpsc::channel::<String>();

        match mode.as_str() {
            "picker" => {
                let items = sample_picker_items();
                picker::open_picker_window(cx, PickerMode::List { items }, tx);
                signal_ready("picker");
            }
            "create" => {
                let projects = sample_project_names();
                picker::open_picker_window(cx, PickerMode::CreateForm { projects }, tx);
                signal_ready("create-form");
            }
            _ => {
                eprintln!("Unknown mode: {mode}. Use: picker, create");
                std::process::exit(1);
            }
        }
    });
}
