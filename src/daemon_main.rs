use awesometree::daemon::{self, DaemonCmd};
use awesometree::interop;
use awesometree::notify;
use awesometree::picker::{self, parse_create_result, PickerItem, PickerMode, CREATE_SENTINEL, DESTROY_PREFIX};
use awesometree::projects_ui;
use awesometree::state;
use awesometree::tray;
use awesometree::wm::AwesomeAdapter;
use awesometree::workspace::{self as ws, DownOptions, Manager, UpOptions};
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
        let st = state::load().unwrap_or_default();
        let workspaces: Vec<(String, bool)> = st
            .workspaces
            .iter()
            .filter(|(_, ws)| ws.active)
            .map(|(name, ws)| (name.clone(), ws.active))
            .collect();
        if let Err(e) = std::panic::catch_unwind(|| {
            tray::run_tray(workspaces);
        }) {
            eprintln!("tray thread panicked: {e:?}");
        }
    });

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
            KeyBinding::new("enter", projects_ui::ConfirmAction, None),
            KeyBinding::new("tab", projects_ui::NextField, None),
            KeyBinding::new("shift-tab", projects_ui::PrevField, None),
        ]);

        notify::open_sentinel_window(cx);

        let mut error_rx = notify::setup_error_listener(cx);
        let mut progress_rx = notify::setup_progress_listener(cx);

        cx.spawn(async move |cx: &mut AsyncApp| {
            while let Some(msg) = error_rx.next().await {
                let _ = cx.update(|cx| notify::show_error_window(cx, msg));
            }
        })
        .detach();

        let progress_handle: std::sync::Arc<std::sync::Mutex<Option<WindowHandle<notify::ProgressView>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));

        cx.spawn(async move |cx: &mut AsyncApp| {
            while let Some(msg) = progress_rx.next().await {
                match msg {
                    notify::ProgressMsg::Open { title } => {
                        let handle_ref = progress_handle.clone();
                        let _ = cx.update(|cx| {
                            let wh = notify::show_progress_window(cx, title);
                            *handle_ref.lock().unwrap() = wh;
                        });
                    }
                    notify::ProgressMsg::Update(status) => {
                        let handle_ref = progress_handle.clone();
                        let _ = cx.update(|cx| {
                            if let Some(ref h) = *handle_ref.lock().unwrap() {
                                notify::update_progress_window(h, status, cx);
                            }
                        });
                    }
                    notify::ProgressMsg::Done => {
                        let handle_ref = progress_handle.clone();
                        let _ = cx.update(|cx| {
                            if let Some(ref h) = *handle_ref.lock().unwrap() {
                                notify::close_progress_window(h, cx);
                            }
                            *handle_ref.lock().unwrap() = None;
                        });
                    }
                    notify::ProgressMsg::Error(e) => {
                        let handle_ref = progress_handle.clone();
                        let _ = cx.update(|cx| {
                            if let Some(ref h) = *handle_ref.lock().unwrap() {
                                notify::close_progress_window(h, cx);
                            }
                            *handle_ref.lock().unwrap() = None;
                            notify::show_error_window(cx, e);
                        });
                    }
                }
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
                    DaemonCmd::LaunchAgent => {}
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
    let projects = interop::list().unwrap_or_default();
    let st = state::load().unwrap_or_default();

    let mut items: Vec<PickerItem> = Vec::new();
    for (ws_name, ws) in &st.workspaces {
        items.push(PickerItem {
            name: ws_name.clone(),
            project: ws.project.clone(),
            active: ws.active,
        });
    }
    items.sort_by(|a, b| a.project.cmp(&b.project).then(a.name.cmp(&b.name)));

    let project_names: Vec<String> = projects.iter().map(|p| p.name.clone()).collect();
    let _ = project_names;

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

        if let Some(ws_name) = selection.strip_prefix(DESTROY_PREFIX) {
            return do_destroy_workspace(ws_name);
        }

        let st = state::load().map_err(|e| format!("load state: {e}"))?;
        let name = selection;

        let ws = st.workspace(&name);
        let is_active = ws.map(|w| w.active).unwrap_or(false);
        let wm = Box::new(AwesomeAdapter::new());

        if is_active {
            let mgr = Manager::new(st, wm);
            mgr.switch(&name).map_err(|e| format!("switch to {name}: {e}"))?;
        } else {
            let project_name = ws
                .map(|w| w.project.clone())
                .ok_or_else(|| format!("workspace not found: {name}"))?;
            let project = interop::load(&project_name)
                .map_err(|e| format!("load project: {e}"))?;
            let mut mgr = Manager::new(st, wm);
            mgr.up(
                &name,
                &project,
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
    let projects = interop::list().unwrap_or_default();
    let project_names: Vec<String> = projects.iter().map(|p| p.name.clone()).collect();

    let (tx, rx) = std_mpsc::channel::<String>();

    picker::open_picker_window(cx, PickerMode::CreateForm { projects: project_names }, tx);

    thread::spawn(move || {
        let Ok(result_str) = rx.recv() else { return };

        let result = match parse_create_result(&result_str) {
            Some(r) => r,
            None => {
                notify::report_error("Create workspace: invalid form result");
                return;
            }
        };

        let progress = notify::open_progress("Creating Workspace");

        if result.is_new_project {
            progress.update(format!("Saving project {}...", result.project));
            let proj = interop::Project {
                schema: Some(
                    "https://project-interop.dev/schemas/v1/project.schema.json".into(),
                ),
                version: "1".into(),
                name: result.project.clone(),
                repo: Some(result.repo_path.clone()),
                branch: Some(result.branch.clone()),
                ..Default::default()
            };
            if let Err(e) = interop::save(&proj) {
                progress.error(format!("Save project: {e}"));
                return;
            }
        }

        let project = match interop::load(&result.project) {
            Ok(p) => p,
            Err(e) => {
                progress.error(format!("Load project: {e}"));
                return;
            }
        };

        let dir = ws::resolve_dir(&result.name, &project);
        progress.update(format!(
            "Creating worktree in {}...",
            dir.display()
        ));

        if let Err(e) = ws::ensure_worktree(&result.name, &project, &dir) {
            progress.error(format!("Worktree creation failed: {e}"));
            return;
        }

        progress.update("Creating tag and launching apps...");

        let st = match state::load() {
            Ok(s) => s,
            Err(e) => {
                progress.error(format!("Load state: {e}"));
                return;
            }
        };
        let wm = Box::new(AwesomeAdapter::new());
        let mut mgr = Manager::new(st, wm);

        if let Err(e) = mgr.up(
            &result.name,
            &project,
            &UpOptions {
                create_tag: true,
                launch_apps: true,
            },
        ) {
            progress.error(format!("Bring up {}: {e}", result.name));
            return;
        }

        if let Err(e) = mgr.switch(&result.name) {
            progress.error(format!("Switch to {}: {e}", result.name));
            return;
        }

        progress.done();
    });
}

fn do_destroy_workspace(ws_name: &str) -> Result<(), String> {
    let st = state::load().map_err(|e| format!("load state: {e}"))?;
    let wm = Box::new(AwesomeAdapter::new());
    let mut mgr = Manager::new(st, wm);

    if let Ok(true) = mgr.is_dirty(ws_name) {
        return Err(format!(
            "Cannot destroy {ws_name}: uncommitted changes.\nCommit or stash your changes first."
        ));
    }

    let _ = mgr.wm.restore_previous_tag();

    mgr.destroy(
        ws_name,
        &DownOptions {
            manage_tag: true,
            keep_worktree: false,
        },
    )
    .map_err(|e| format!("destroy {ws_name}: {e}"))
}
