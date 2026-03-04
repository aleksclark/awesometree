use awesometree::config;
use awesometree::daemon::{self, DaemonCmd};
use awesometree::picker::{run_picker, PickerMode};
use awesometree::tray;
use awesometree::wm::AwesomeAdapter;
use awesometree::workspace::{Manager, UpOptions};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

extern crate libc;

fn main() {
    if daemon::is_running() {
        eprintln!("awesometree-daemon is already running");
        std::process::exit(1);
    }

    daemon::cleanup();

    let (tx, rx) = mpsc::channel::<DaemonCmd>();

    let tx_sock = tx.clone();
    thread::spawn(move || {
        daemon::listen(tx_sock);
    });

    thread::spawn(|| {
        let cfg = config::load_config().unwrap_or_default();
        let state = config::load_state().unwrap_or_default();
        let workspaces: Vec<(String, bool)> = cfg
            .workspaces
            .iter()
            .map(|ws| {
                let active = state.get(&ws.name).is_some_and(|s| s.active);
                (ws.name.clone(), active)
            })
            .collect();
        if let Err(e) = std::panic::catch_unwind(|| {
            tray::run_tray(workspaces);
        }) {
            eprintln!("tray thread panicked: {e:?}");
        }
    });

    unsafe {
        libc::signal(libc::SIGTERM, handle_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, handle_signal as *const () as libc::sighandler_t);
    }

    loop {
        match rx.recv() {
            Ok(DaemonCmd::Pick) => do_pick(),
            Ok(DaemonCmd::Create) => do_create(),
            Ok(DaemonCmd::Reload) => {}
            Err(_) => break,
        }
    }

    daemon::cleanup();
}

extern "C" fn handle_signal(_sig: libc::c_int) {
    daemon::cleanup();
    std::process::exit(0);
}

fn do_pick() {
    let cfg = config::load_config().unwrap_or_default();
    let state = config::load_state().unwrap_or_default();

    let all_names = cfg.all_names();
    let active_names = cfg.active_names(&state);
    let active_set: HashSet<&str> = active_names.iter().map(|s| s.as_str()).collect();

    let items: Vec<String> = all_names
        .iter()
        .map(|n| {
            if active_set.contains(n.as_str()) {
                format!("● {n}")
            } else {
                format!("  {n}")
            }
        })
        .collect();

    let Some(selection) = run_picker(PickerMode::List {
        items,
        prompt: "workspace".into(),
    }) else {
        return;
    };

    let name = selection
        .strip_prefix("● ")
        .or_else(|| selection.strip_prefix("  "))
        .unwrap_or(&selection);

    let wm = Box::new(AwesomeAdapter::new());

    if active_set.contains(name) {
        let mgr = Manager::new(&cfg, state, wm);
        let _ = mgr.switch(name);
    } else {
        let ws = match cfg.find_workspace(name) {
            Some(ws) => ws.clone(),
            None => return,
        };
        let mut mgr = Manager::new(&cfg, state, wm);
        let _ = mgr.up(
            &ws,
            &UpOptions {
                create_tag: true,
                launch_apps: true,
            },
        );
        let _ = mgr.switch(name);
    }
}

fn do_create() {
    let cfg = config::load_config().unwrap_or_default();
    let state = config::load_state().unwrap_or_default();

    let Some(name) = run_picker(PickerMode::Freeform {
        prompt: "workspace name".into(),
    }) else {
        return;
    };

    let repos = config::list_repos();
    let mut repo_items: Vec<String> = repos
        .iter()
        .filter_map(|r| r.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    if !cfg.defaults.repo.is_empty() {
        repo_items.insert(0, cfg.defaults.repo.clone());
    }

    let repo_map: std::collections::HashMap<String, PathBuf> = repos
        .iter()
        .filter_map(|r| {
            r.file_name()
                .map(|n| (n.to_string_lossy().into_owned(), r.clone()))
        })
        .collect();

    let Some(repo_sel) = run_picker(PickerMode::List {
        items: repo_items,
        prompt: "repo".into(),
    }) else {
        return;
    };

    let repo_path = repo_map
        .get(&repo_sel)
        .cloned()
        .unwrap_or_else(|| PathBuf::from(&repo_sel));

    let mut branch_items = config::list_branches(&repo_path);
    if !cfg.defaults.branch.is_empty() {
        branch_items.insert(0, cfg.defaults.branch.clone());
    }

    let Some(branch) = run_picker(PickerMode::List {
        items: branch_items,
        prompt: "branch".into(),
    }) else {
        return;
    };

    let abs_repo = std::fs::canonicalize(&repo_path)
        .unwrap_or(repo_path)
        .to_string_lossy()
        .into_owned();

    if let Err(e) = config::append_to_config(&name, &abs_repo, &branch) {
        eprintln!("Error: {e}");
        return;
    }

    let ws = config::Workspace {
        name: name.clone(),
        repo: abs_repo,
        branch,
        path: String::new(),
        gui: vec![],
        layout: String::new(),
    };

    let wm = Box::new(AwesomeAdapter::new());
    let mut mgr = Manager::new(&cfg, state, wm);
    let _ = mgr.up(
        &ws,
        &UpOptions {
            create_tag: true,
            launch_apps: true,
        },
    );
    let _ = mgr.switch(&name);
}
