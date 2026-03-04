use awesometree::config;
use awesometree::daemon::{self, DaemonCmd};
use awesometree::notify;
use awesometree::picker::{self, parse_create_result, PickerItem, PickerMode, CREATE_SENTINEL};
use awesometree::projects_ui;
use awesometree::tray;
use awesometree::wm::AwesomeAdapter;
use awesometree::workspace::{Manager, UpOptions};
use futures_channel::mpsc;
use futures_util::StreamExt;
use gpui::*;
use std::sync::mpsc as std_mpsc;
use std::thread;

extern crate libc;

fn main() {
    if daemon::is_running() {
        eprintln!("awesometree-daemon is already running");
        std::process::exit(1);
    }

    daemon::cleanup();

    unsafe {
        libc::signal(libc::SIGTERM, handle_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, handle_signal as *const () as libc::sighandler_t);
    }

    let (fut_tx, fut_rx) = mpsc::unbounded::<DaemonCmd>();

    let sock_tx = fut_tx.clone();
    thread::spawn(move || {
        let (std_tx, std_rx) = std_mpsc::channel::<DaemonCmd>();
        thread::spawn(move || {
            daemon::listen(std_tx);
        });
        for cmd in std_rx {
            if sock_tx.unbounded_send(cmd).is_err() {
                break;
            }
        }
    });

    thread::spawn(|| {
        let cfg = config::load_config().unwrap_or_default();
        let workspaces: Vec<(String, bool)> = cfg
            .all_workspaces()
            .iter()
            .map(|ws| (ws.name.clone(), ws.active))
            .collect();
        if let Err(e) = std::panic::catch_unwind(|| {
            tray::run_tray(workspaces);
        }) {
            eprintln!("tray thread panicked: {e:?}");
        }
    });

    let app = Application::new();
    app.run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("escape", picker::Cancel, None),
            KeyBinding::new("enter", picker::Confirm, None),
            KeyBinding::new("down", picker::SelectNext, None),
            KeyBinding::new("up", picker::SelectPrev, None),
            KeyBinding::new("tab", picker::TabForward, None),
            KeyBinding::new("shift-tab", picker::TabBack, None),
            KeyBinding::new("ctrl-n", picker::OpenCreate, None),
            KeyBinding::new("escape", projects_ui::Dismiss, None),
            KeyBinding::new("enter", projects_ui::ConfirmAction, None),
            KeyBinding::new("tab", projects_ui::NextField, None),
            KeyBinding::new("shift-tab", projects_ui::PrevField, None),
        ]);

        notify::open_sentinel_window(cx);

        let mut error_rx = notify::setup_error_listener(cx);

        cx.spawn(async move |cx: &mut AsyncApp| {
            while let Some(msg) = error_rx.next().await {
                let _ = cx.update(|cx| notify::show_error_window(cx, msg));
            }
        })
        .detach();

        cx.spawn(async move |cx: &mut AsyncApp| {
            let mut rx = fut_rx;
            while let Some(cmd) = rx.next().await {
                match cmd {
                    DaemonCmd::Pick => {
                        let cmd_tx = fut_tx.clone();
                        let _ = cx.update(|cx| do_pick(cx, cmd_tx));
                    }
                    DaemonCmd::Create => {
                        let _ = cx.update(|cx| do_create(cx));
                    }
                    DaemonCmd::Projects => {
                        let _ = cx.update(|cx| projects_ui::open_projects_window(cx));
                    }
                    DaemonCmd::Restart => {
                        daemon::cleanup();
                        std::process::exit(0);
                    }
                    DaemonCmd::Reload => {}
                }
            }
        })
        .detach();
    });

    daemon::cleanup();
}

extern "C" fn handle_signal(_sig: libc::c_int) {
    daemon::cleanup();
    std::process::exit(0);
}

fn do_pick(cx: &mut App, cmd_tx: mpsc::UnboundedSender<DaemonCmd>) {
    let cfg = config::load_config().unwrap_or_default();
    let all_workspaces = cfg.all_workspaces();

    let items: Vec<PickerItem> = all_workspaces
        .iter()
        .map(|ws| PickerItem {
            name: ws.name.clone(),
            project: ws.project.clone(),
            active: ws.active,
        })
        .collect();

    let (tx, rx) = std_mpsc::channel::<String>();

    picker::open_picker_window(
        cx,
        PickerMode::List { items },
        tx,
    );

    notify::spawn_task("Open workspace", move || {
        let Ok(selection) = rx.recv() else { return Ok(()) };

        if selection == CREATE_SENTINEL {
            let _ = cmd_tx.unbounded_send(DaemonCmd::Create);
            return Ok(());
        }

        let cfg = config::load_config().map_err(|e| format!("load config: {e}"))?;
        let all_workspaces = cfg.all_workspaces();

        let name = selection;

        let is_active = all_workspaces.iter().any(|ws| ws.name == name && ws.active);
        let wm = Box::new(AwesomeAdapter::new());

        if is_active {
            let mgr = Manager::new(cfg, wm);
            mgr.switch(&name).map_err(|e| format!("switch to {name}: {e}"))?;
        } else {
            let ws = cfg
                .find_workspace(&name)
                .ok_or_else(|| format!("workspace not found: {name}"))?;
            let mut mgr = Manager::new(cfg, wm);
            mgr.up(
                &ws,
                &UpOptions {
                    create_tag: true,
                    launch_apps: true,
                },
            )
            .map_err(|e| format!("bring up {name}: {e}"))?;
            mgr.switch(&name).map_err(|e| format!("switch to {name}: {e}"))?;
        }

        Ok(())
    });
}

fn do_create(cx: &mut App) {
    let cfg = config::load_config().unwrap_or_default();
    let project_names = cfg.project_names();

    let (tx, rx) = std_mpsc::channel::<String>();

    picker::open_picker_window(cx, PickerMode::CreateForm { projects: project_names }, tx);

    notify::spawn_task("Create workspace", move || {
        let Ok(result_str) = rx.recv() else { return Ok(()) };

        let result =
            parse_create_result(&result_str).ok_or_else(|| "invalid form result".to_string())?;

        let mut cfg = config::load_config().map_err(|e| format!("load config: {e}"))?;

        if result.is_new_project {
            cfg.add_project(&result.project, &result.repo_path, &result.branch);
        }

        let proj = cfg
            .find_project(&result.project)
            .ok_or_else(|| format!("project not found: {}", result.project))?
            .clone();

        cfg.append_workspace_to_project(&result.project, &result.name)?;
        config::save_config(&cfg).map_err(|e| format!("save config: {e}"))?;

        let ws = config::Workspace {
            name: result.name.clone(),
            project: proj.name.clone(),
            repo: proj.repo.clone(),
            branch: proj.branch.clone(),
            gui: proj.gui.clone(),
            layout: proj.layout.clone(),
            active: false,
            tag_index: 0,
            dir: String::new(),
        };

        let wm = Box::new(AwesomeAdapter::new());
        let mut mgr = Manager::new(cfg, wm);
        mgr.up(
            &ws,
            &UpOptions {
                create_tag: true,
                launch_apps: true,
            },
        )
        .map_err(|e| format!("bring up {}: {e}", result.name))?;
        mgr.switch(&result.name)
            .map_err(|e| format!("switch to {}: {e}", result.name))?;

        Ok(())
    });
}
