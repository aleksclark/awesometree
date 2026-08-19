use awesometree::agents_ui;
use awesometree::cleanup_ui;
use awesometree::bezalel_supervisor;
use awesometree::agent_supervisor;
use awesometree::daemon::{self, DaemonCmd};
use awesometree::log as dlog;
use awesometree::model::work_session::{
    CreateWorkSessionRequest, RealizationOptions, DEFAULT_WORK_PROFILE_ID,
};
use awesometree::notify;
use awesometree::picker::{
    self, parse_create_result, PickerItem, PickerMode, CREATE_SENTINEL, DESTROY_PREFIX, STOP_PREFIX,
};
use awesometree::projects_ui;
use awesometree::qr;
use awesometree::runtime_store;
use awesometree::server;
use awesometree::service_access;
use awesometree::tray;
use awesometree::wm;
use awesometree::work_session_service::WorkSessionService;
use futures_channel::mpsc;
use futures_util::StreamExt;
use gpui::*;
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::thread;

extern crate libc;

fn main() {
    if daemon::is_running() {
        eprintln!("awesometree-daemon is already running");
        std::process::exit(1);
    }

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
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        // Initialize the shared WorkSessionService before HTTP/gRPC start.
        rt.block_on(async {
            let catalog = awesometree::switchboard::live_catalog();
            let wm = Some(wm::platform_adapter());
            let svc = Arc::new(WorkSessionService::new(catalog, wm));
            if let Ok(notes) = svc.reconcile_on_startup().await {
                for n in notes {
                    dlog::log(format!("reconcile: {n}"));
                }
            }
            service_access::set_service(svc).await;
        });

        bezalel_supervisor::init(rt.handle().clone());
        agent_supervisor::init(rt.handle().clone());
        rt.block_on(async {
            bezalel_supervisor::start_active_workspaces();
            bezalel_supervisor::start_sync_loop(std::time::Duration::from_secs(5));
            tokio::spawn(server::run_grpc(server::DEFAULT_GRPC_PORT));
            server::run(server::DEFAULT_PORT).await;
        });
    });

    thread::spawn(|| {
        let workspaces: Vec<(String, bool)> = runtime_store::load_all()
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, rt)| {
                matches!(
                    rt.realization_status,
                    awesometree::model::runtime::RealizationStatus::Ready
                        | awesometree::model::runtime::RealizationStatus::Degraded
                )
            })
            .map(|(id, _)| (id, true))
            .collect();
        if let Err(e) = std::panic::catch_unwind(|| {
            tray::run_tray(workspaces);
        }) {
            eprintln!("tray thread panicked: {e:?}");
        }
    });

    awesometree::user_env::load();
    dlog::log("Daemon starting (Switchboard-backed AWM)");

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
            KeyBinding::new("escape", cleanup_ui::DismissCleanup, None),
            KeyBinding::new("escape", qr::DismissQr, None),
            KeyBinding::new("escape", agents_ui::DismissAgents, None),
            KeyBinding::new("enter", projects_ui::ConfirmAction, None),
            KeyBinding::new("tab", projects_ui::NextField, None),
            KeyBinding::new("shift-tab", projects_ui::PrevField, None),
        ]);

        notify::open_sentinel_window(cx);

        let mut log_rx = dlog::setup_log_listener(cx);
        let mut error_rx = notify::setup_error_listener(cx);
        let mut progress_rx = notify::setup_progress_listener(cx);

        cx.spawn(async move |cx: &mut AsyncApp| {
            while let Some(()) = log_rx.next().await {
                let _ = cx.update(dlog::show_log_window);
            }
        })
        .detach();

        cx.spawn(async move |cx: &mut AsyncApp| {
            while let Some(msg) = error_rx.next().await {
                let _ = cx.update(|cx| notify::show_error_window(cx, msg));
            }
        })
        .detach();

        let progress_handle: std::sync::Arc<
            std::sync::Mutex<Option<WindowHandle<notify::ProgressView>>>,
        > = std::sync::Arc::new(std::sync::Mutex::new(None));

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
                        dlog::log("Picker opened");
                        let cmd_tx = fut_tx.clone();
                        let _ = cx.update(|cx| do_pick(cx, cmd_tx));
                    }
                    DaemonCmd::Create => {
                        dlog::log("Create form opened");
                        let _ = cx.update(do_create);
                    }
                    DaemonCmd::Projects => {
                        dlog::log("Projects UI opened");
                        let _ = cx.update(projects_ui::open_projects_window);
                    }
                    DaemonCmd::Agents => {
                        dlog::log("Agents UI opened");
                        let _ = cx.update(agents_ui::open_agents_window);
                    }
                    DaemonCmd::Cleanup => {
                        dlog::log("Cleanup UI opened");
                        let _ = cx.update(cleanup_ui::open_cleanup_window);
                    }
                    DaemonCmd::LaunchAgent => {}
                    DaemonCmd::Restart => {
                        dlog::log("Daemon restarting");
                        bezalel_supervisor::stop_all();
                        daemon::cleanup();
                        std::process::exit(0);
                    }
                    DaemonCmd::Reload => {}
                    DaemonCmd::Logs => {
                        let _ = cx.update(dlog::show_log_window);
                    }
                    DaemonCmd::MobileQr => {
                        dlog::log("QR code window opened");
                        let _ = cx.update(qr::show_qr_window);
                    }
                }
            }
        })
        .detach();
    });

    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

extern "C" fn handle_signal(_sig: libc::c_int) {
    bezalel_supervisor::stop_all();
    std::process::exit(0);
}

fn rt_block_on<F: std::future::Future>(f: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(h) => tokio::task::block_in_place(|| h.block_on(f)),
        Err(_) => {
            let rt = tokio::runtime::Runtime::new().expect("tokio");
            rt.block_on(f)
        }
    }
}

fn do_pick(cx: &mut App, cmd_tx: mpsc::UnboundedSender<DaemonCmd>) {
    let items = rt_block_on(async {
        let svc = service_access::service().await;
        match svc.list_work_sessions(None, None).await {
            Ok(list) => {
                let mut items: Vec<PickerItem> = list
                    .into_iter()
                    .map(|v| {
                        let active = v
                            .runtime
                            .as_ref()
                            .map(|r| {
                                matches!(
                                    r.realization_status,
                                    awesometree::model::runtime::RealizationStatus::Ready
                                        | awesometree::model::runtime::RealizationStatus::Degraded
                                )
                            })
                            .unwrap_or(false);
                        PickerItem {
                            name: v.work_session.work_session_id.clone(),
                            project: v
                                .work_session
                                .project_id
                                .clone()
                                .unwrap_or_default(),
                            active,
                            lifecycle: v.work_session.state.to_string(),
                            work_profile_id: v
                                .work_session
                                .work_profile_id
                                .clone()
                                .unwrap_or_default(),
                        }
                    })
                    .collect();
                items.sort_by(|a, b| a.project.cmp(&b.project).then(a.name.cmp(&b.name)));
                items
            }
            Err(e) => {
                dlog::log(format!("list work sessions failed: {e}"));
                Vec::new()
            }
        }
    });

    let (tx, rx) = std_mpsc::channel::<String>();
    picker::open_picker_window(cx, PickerMode::List { items }, tx);

    notify::spawn_task("Open WorkSession", move || {
        let Ok(selection) = rx.recv() else {
            dlog::log("Picker dismissed");
            return Ok(());
        };

        if selection == CREATE_SENTINEL {
            dlog::log("Picker: switching to create form");
            let _ = cmd_tx.unbounded_send(DaemonCmd::Create);
            return Ok(());
        }

        if let Some(ws_id) = selection.strip_prefix(DESTROY_PREFIX) {
            dlog::log(format!("Picker: destroying work session {ws_id}"));
            return do_destroy_session(ws_id);
        }

        if let Some(ws_id) = selection.strip_prefix(STOP_PREFIX) {
            dlog::log(format!("Picker: pausing work session {ws_id}"));
            return do_pause_session(ws_id);
        }

        let name = selection;
        dlog::log(format!("Switching to work session: {name}"));
        let svc = service_access::service_blocking();
        let view = rt_block_on(svc.get_work_session(&name))
            .map_err(|e| format!("get work session: {e}"))?;
        if let Some(rt) = view.runtime
            && let Some(tag) = rt.tag_name
        {
            let wm = wm::platform_adapter();
            wm.switch_tag(&tag)
                .map_err(|e| format!("switch tag: {e}"))?;
        }
        Ok(())
    });
}

fn do_create(cx: &mut App) {
    let (projects, profiles, default_missing) = rt_block_on(async {
        let svc = service_access::service().await;
        let projects = svc
            .list_projects(None)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.project_id)
            .collect::<Vec<_>>();
        let profiles = svc.list_work_profiles().await.unwrap_or_default();
        let default_missing = !profiles.iter().any(|p| p.work_profile_id == DEFAULT_WORK_PROFILE_ID);
        (projects, profiles, default_missing)
    });

    let (tx, rx) = std_mpsc::channel::<String>();
    picker::open_picker_window(
        cx,
        PickerMode::CreateForm {
            projects,
            work_profiles: profiles,
            default_missing,
        },
        tx,
    );

    thread::spawn(move || {
        let Ok(result_str) = rx.recv() else {
            dlog::log("Create form dismissed");
            return;
        };

        let result = match parse_create_result(&result_str) {
            Some(r) => r,
            None => {
                notify::report_error("Create WorkSession: invalid form result");
                return;
            }
        };

        dlog::log(format!(
            "Creating work session: {} (project: {}, profile: {})",
            result.name, result.project, result.work_profile_id
        ));

        let progress = notify::open_progress("Creating WorkSession");
        progress.update("Contacting Switchboard...");

        let svc = service_access::service_blocking();
        let req = CreateWorkSessionRequest {
            work_session_id: result.name.clone(),
            project_id: result.project.clone(),
            work_profile_id: if result.work_profile_id.is_empty() {
                None
            } else {
                Some(result.work_profile_id.clone())
            },
            display_name: Some(result.name.clone()),
            realization: RealizationOptions {
                create_tag: true,
                launch_apps: true,
                headless: false,
                no_wm: false,
            },
        };

        match rt_block_on(svc.create_work_session(req)) {
            Ok(resp) => {
                dlog::log(format!(
                    "WorkSession {} created profile={} state={}",
                    resp.work_session.work_session_id,
                    resp.work_profile_id,
                    resp.work_session.state
                ));
                if let Some(rt) = resp.runtime
                    && let Some(tag) = rt.tag_name
                {
                    let wm = wm::platform_adapter();
                    let _ = wm.switch_tag(&tag);
                }
                progress.done();
            }
            Err(e) => {
                progress.error(format!("Create WorkSession failed: {e}"));
            }
        }
    });
}

fn do_destroy_session(ws_id: &str) -> Result<(), String> {
    dlog::log(format!("Destroying work session: {ws_id}"));
    let svc = service_access::service_blocking();
    let _ = wm::platform_adapter().restore_previous_tag();
    rt_block_on(svc.destroy(ws_id, false)).map_err(|e| format!("destroy {ws_id}: {e}"))?;
    dlog::log(format!("WorkSession {ws_id} destroyed"));
    Ok(())
}

fn do_pause_session(ws_id: &str) -> Result<(), String> {
    dlog::log(format!("Pausing work session: {ws_id}"));
    let svc = service_access::service_blocking();
    let _ = wm::platform_adapter().restore_previous_tag();
    rt_block_on(svc.transition(
        ws_id,
        awesometree::model::lifecycle::WorkSessionState::Paused,
    ))
    .map_err(|e| format!("pause {ws_id}: {e}"))?;
    dlog::log(format!("WorkSession {ws_id} paused"));
    Ok(())
}
