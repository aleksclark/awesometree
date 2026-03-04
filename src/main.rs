use awesometree::config::{self, Config, State};
use awesometree::daemon;
use awesometree::wm::{Adapter, AwesomeAdapter};
use awesometree::workspace::{DownOptions, Manager, UpOptions};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(name = "awesometree", about = "Workspace manager for window managers + Zed + git worktrees")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Up {
        name: Option<String>,
        #[arg(long)]
        no_tag: bool,
        #[arg(long)]
        no_launch: bool,
    },
    Down {
        name: Option<String>,
        #[arg(long)]
        no_tag: bool,
        #[arg(long)]
        keep_worktree: bool,
    },
    Create {
        name: String,
        repo: Option<String>,
        branch: Option<String>,
        #[arg(long)]
        no_tag: bool,
        #[arg(long)]
        no_launch: bool,
    },
    Destroy {
        name: String,
        #[arg(long)]
        no_tag: bool,
    },
    #[command(name = "destroy-current")]
    DestroyCurrent,
    Close,
    Cycle,
    List,
    Switch { name: String },
    Pick,
    #[command(name = "create-interactive")]
    CreateInteractive,
    Repos,
    Names,
    Allnames,
    Dir { name: String },
    Defaults,
    Edit,
    Daemon,
}

fn main() {
    let cli = Cli::parse();
    let cfg = config::load_config().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        process::exit(1);
    });
    let state = config::load_state().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        process::exit(1);
    });

    match cli.command {
        Commands::Up {
            name,
            no_tag,
            no_launch,
        } => cmd_up(&cfg, state, name, no_tag, no_launch),
        Commands::Down {
            name,
            no_tag,
            keep_worktree,
        } => cmd_down(&cfg, state, name, no_tag, keep_worktree),
        Commands::Create {
            name,
            repo,
            branch,
            no_tag,
            no_launch,
        } => cmd_create(&cfg, state, name, repo, branch, no_tag, no_launch),
        Commands::Destroy { name, no_tag } => cmd_destroy(&cfg, state, name, no_tag),
        Commands::DestroyCurrent => cmd_destroy_current(&cfg, state),
        Commands::Close => cmd_close(&cfg, state),
        Commands::Cycle => cmd_cycle(&cfg, state),
        Commands::List => cmd_list(&cfg, &state),
        Commands::Switch { name } => cmd_switch(&cfg, state, &name),
        Commands::Pick => cmd_pick(),
        Commands::CreateInteractive => cmd_create_interactive(),
        Commands::Repos => cmd_repos(),
        Commands::Names => cmd_names(&cfg, &state),
        Commands::Allnames => cmd_allnames(&cfg),
        Commands::Dir { name } => cmd_dir(&cfg, &name),
        Commands::Defaults => cmd_defaults(&cfg),
        Commands::Edit => cmd_edit(),
        Commands::Daemon => cmd_daemon(),
    }
}

fn make_manager<'a>(cfg: &'a Config, state: State) -> Manager<'a> {
    Manager::new(cfg, state, Box::new(AwesomeAdapter::new()))
}

fn cmd_up(cfg: &Config, state: State, name: Option<String>, no_tag: bool, no_launch: bool) {
    let mut mgr = make_manager(cfg, state);
    match name {
        Some(n) => {
            let ws = cfg.find_workspace(&n).unwrap_or_else(|| {
                eprintln!("Workspace not found: {n}");
                process::exit(1);
            });
            let opts = UpOptions {
                create_tag: !no_tag,
                launch_apps: !no_launch,
            };
            if let Err(e) = mgr.up(&ws.clone(), &opts) {
                eprintln!("Error: {e}");
            }
        }
        None => {
            for ws in &cfg.workspaces {
                let is_active = mgr.state.get(&ws.name).is_some_and(|s| s.active);
                if is_active {
                    let opts = UpOptions {
                        create_tag: true,
                        launch_apps: false,
                    };
                    if let Err(e) = mgr.up(ws, &opts) {
                        eprintln!("Error: {e}");
                    }
                }
            }
        }
    }
}

fn cmd_down(
    cfg: &Config,
    state: State,
    name: Option<String>,
    no_tag: bool,
    keep_worktree: bool,
) {
    let mut mgr = make_manager(cfg, state);
    let opts = DownOptions {
        manage_tag: !no_tag,
        keep_worktree,
    };
    let workspaces: Vec<_> = match &name {
        Some(n) => {
            let ws = cfg.find_workspace(n).unwrap_or_else(|| {
                eprintln!("Workspace not found: {n}");
                process::exit(1);
            });
            vec![ws.clone()]
        }
        None => cfg.workspaces.clone(),
    };
    for ws in &workspaces {
        if let Err(e) = mgr.down(ws, &opts) {
            eprintln!("Error: {e}");
        }
    }
}

fn cmd_create(
    cfg: &Config,
    state: State,
    name: String,
    repo: Option<String>,
    branch: Option<String>,
    no_tag: bool,
    no_launch: bool,
) {
    if cfg.find_workspace(&name).is_some() {
        eprintln!("Workspace already exists: {name}");
        process::exit(1);
    }
    let repo = repo.unwrap_or_else(|| cfg.defaults.repo.clone());
    let branch = branch.unwrap_or_else(|| cfg.defaults.branch.clone());
    if repo.is_empty() || branch.is_empty() {
        eprintln!("No repo/branch given and no defaults configured.");
        process::exit(1);
    }
    let abs_repo = std::fs::canonicalize(&repo)
        .unwrap_or_else(|_| PathBuf::from(&repo))
        .to_string_lossy()
        .into_owned();
    config::append_to_config(&name, &abs_repo, &branch).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        process::exit(1);
    });
    let ws = config::Workspace {
        name: name.clone(),
        repo: abs_repo,
        branch,
        path: String::new(),
        gui: vec![],
        layout: String::new(),
    };
    let mut mgr = make_manager(cfg, state);
    let opts = UpOptions {
        create_tag: !no_tag,
        launch_apps: !no_launch,
    };
    if let Err(e) = mgr.up(&ws, &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_destroy(cfg: &Config, state: State, name: String, no_tag: bool) {
    let ws = cfg.find_workspace(&name).unwrap_or_else(|| {
        eprintln!("Workspace not found: {name}");
        process::exit(1);
    });
    let mut mgr = make_manager(cfg, state);
    let opts = DownOptions {
        manage_tag: !no_tag,
        keep_worktree: false,
    };
    if let Err(e) = mgr.down(&ws.clone(), &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
    if let Err(e) = config::remove_from_config(&name) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_destroy_current(cfg: &Config, state: State) {
    let wm = AwesomeAdapter::new();
    let name = wm
        .get_current_tag_name()
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            process::exit(1);
        })
        .unwrap_or_else(|| {
            eprintln!("Not a project workspace");
            process::exit(1);
        });
    let ws = cfg.find_workspace(&name).unwrap_or_else(|| {
        eprintln!("Workspace not found: {name}");
        process::exit(1);
    });
    let mut mgr = make_manager(cfg, state);
    if let Ok(true) = mgr.is_dirty(ws) {
        eprintln!("Cannot destroy {name}: uncommitted changes");
        process::exit(1);
    }
    let _ = wm.restore_previous_tag();
    let opts = DownOptions {
        manage_tag: true,
        keep_worktree: false,
    };
    if let Err(e) = mgr.down(&ws.clone(), &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
    if let Err(e) = config::remove_from_config(&name) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_close(cfg: &Config, state: State) {
    let wm = AwesomeAdapter::new();
    let name = wm
        .get_current_tag_name()
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            process::exit(1);
        })
        .unwrap_or_else(|| {
            eprintln!("Not a project workspace");
            process::exit(1);
        });
    let ws = cfg.find_workspace(&name).unwrap_or_else(|| {
        eprintln!("Workspace not found: {name}");
        process::exit(1);
    });
    let _ = wm.restore_previous_tag();
    let mut mgr = make_manager(cfg, state);
    let opts = DownOptions {
        manage_tag: true,
        keep_worktree: true,
    };
    if let Err(e) = mgr.down(&ws.clone(), &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_cycle(cfg: &Config, state: State) {
    let active = cfg.active_names(&state);
    if active.is_empty() {
        return;
    }
    let wm = AwesomeAdapter::new();
    let current = wm.get_current_tag_name().ok().flatten();
    let next_idx = match &current {
        Some(name) => {
            let pos = active.iter().position(|n| n == name).unwrap_or(0);
            (pos + 1) % active.len()
        }
        None => 0,
    };
    let mgr = make_manager(cfg, state);
    let _ = mgr.switch(&active[next_idx]);
}

fn cmd_list(cfg: &Config, state: &State) {
    for ws in &cfg.workspaces {
        let active = state.get(&ws.name).is_some_and(|s| s.active);
        let status = if active { "UP" } else { "  " };
        let dir = ws.resolve_dir().display().to_string();
        let tag = if active {
            format!(" [tag P:{}]", ws.name)
        } else {
            String::new()
        };
        println!("  [{status}] {:<20} {dir}{tag}", ws.name);
    }
}

fn cmd_switch(cfg: &Config, state: State, name: &str) {
    let s = state.get(name);
    if !s.is_some_and(|s| s.active) {
        eprintln!("Workspace not active: {name}");
        process::exit(1);
    }
    let mgr = make_manager(cfg, state);
    if let Err(e) = mgr.switch(name) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_pick() {
    if daemon::is_running() {
        match daemon::send_command("pick") {
            Ok(_) => {}
            Err(e) => eprintln!("Error: {e}"),
        }
    } else {
        eprintln!("awesometree-daemon is not running");
        process::exit(1);
    }
}

fn cmd_create_interactive() {
    if daemon::is_running() {
        match daemon::send_command("create") {
            Ok(_) => {}
            Err(e) => eprintln!("Error: {e}"),
        }
    } else {
        eprintln!("awesometree-daemon is not running");
        process::exit(1);
    }
}

fn cmd_repos() {
    for r in config::list_repos() {
        println!("{}", r.display());
    }
}

fn cmd_names(cfg: &Config, state: &State) {
    for n in cfg.active_names(state) {
        println!("{n}");
    }
}

fn cmd_allnames(cfg: &Config) {
    for n in cfg.all_names() {
        println!("{n}");
    }
}

fn cmd_dir(cfg: &Config, name: &str) {
    let ws = cfg.find_workspace(name).unwrap_or_else(|| {
        eprintln!("Workspace not found: {name}");
        process::exit(1);
    });
    println!("{}", ws.resolve_dir().display());
}

fn cmd_defaults(cfg: &Config) {
    println!("{}", cfg.defaults.repo);
    println!("{}", cfg.defaults.branch);
}

fn cmd_edit() {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
    let path = config::config_path();
    let _ = process::Command::new(editor).arg(path).status();
}

fn cmd_daemon() {
    if daemon::is_running() {
        eprintln!("awesometree-daemon is already running");
        process::exit(1);
    }
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("awesometree-daemon")))
        .unwrap_or_else(|| PathBuf::from("awesometree-daemon"));
    let log = std::fs::File::create("/tmp/awesometree-daemon.log")
        .unwrap_or_else(|_| std::fs::File::open("/dev/null").unwrap());
    let log_err = log.try_clone().unwrap_or_else(|_| std::fs::File::open("/dev/null").unwrap());
    let _ = process::Command::new("setsid")
        .arg("--fork")
        .arg(&exe)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::from(log))
        .stderr(process::Stdio::from(log_err))
        .spawn();
}
