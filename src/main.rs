use awesometree::config::{self, Config};
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
        #[arg(long)]
        project: String,
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
    Projects,
    #[command(name = "projects-ui")]
    ProjectsUi,
    #[command(name = "restart-daemon")]
    RestartDaemon,
    Edit,
    Daemon,
}

fn main() {
    let cli = Cli::parse();
    let cfg = config::load_config().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        process::exit(1);
    });

    match cli.command {
        Commands::Up {
            name,
            no_tag,
            no_launch,
        } => cmd_up(cfg, name, no_tag, no_launch),
        Commands::Down {
            name,
            no_tag,
            keep_worktree,
        } => cmd_down(cfg, name, no_tag, keep_worktree),
        Commands::Create {
            name,
            project,
            no_tag,
            no_launch,
        } => cmd_create(cfg, name, project, no_tag, no_launch),
        Commands::Destroy { name, no_tag } => cmd_destroy(cfg, name, no_tag),
        Commands::DestroyCurrent => cmd_destroy_current(cfg),
        Commands::Close => cmd_close(cfg),
        Commands::Cycle => cmd_cycle(cfg),
        Commands::List => cmd_list(&cfg),
        Commands::Switch { name } => cmd_switch(&cfg, &name),
        Commands::Pick => cmd_pick(),
        Commands::CreateInteractive => cmd_create_interactive(),
        Commands::Repos => cmd_repos(),
        Commands::Names => cmd_names(&cfg),
        Commands::Allnames => cmd_allnames(&cfg),
        Commands::Dir { name } => cmd_dir(&cfg, &name),
        Commands::Projects => cmd_projects(&cfg),
        Commands::ProjectsUi => cmd_projects_ui(),
        Commands::RestartDaemon => cmd_restart_daemon(),
        Commands::Edit => cmd_edit(),
        Commands::Daemon => cmd_daemon(),
    }
}

fn make_manager(cfg: Config) -> Manager {
    Manager::new(cfg, Box::new(AwesomeAdapter::new()))
}

fn cmd_up(cfg: Config, name: Option<String>, no_tag: bool, no_launch: bool) {
    match name {
        Some(n) => {
            let ws = cfg.find_workspace(&n).unwrap_or_else(|| {
                eprintln!("Workspace not found: {n}");
                process::exit(1);
            });
            let mut mgr = make_manager(cfg);
            let opts = UpOptions {
                create_tag: !no_tag,
                launch_apps: !no_launch,
            };
            if let Err(e) = mgr.up(&ws, &opts) {
                eprintln!("Error: {e}");
            }
        }
        None => {
            let active_ws: Vec<_> = cfg
                .all_workspaces()
                .into_iter()
                .filter(|ws| ws.active)
                .collect();
            let mut mgr = make_manager(cfg);
            for ws in &active_ws {
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

fn cmd_down(cfg: Config, name: Option<String>, no_tag: bool, keep_worktree: bool) {
    let opts = DownOptions {
        manage_tag: !no_tag,
        keep_worktree,
    };
    let workspaces = match &name {
        Some(n) => {
            let ws = cfg.find_workspace(n).unwrap_or_else(|| {
                eprintln!("Workspace not found: {n}");
                process::exit(1);
            });
            vec![ws]
        }
        None => cfg.all_workspaces(),
    };
    let mut mgr = make_manager(cfg);
    for ws in &workspaces {
        if let Err(e) = mgr.down(ws, &opts) {
            eprintln!("Error: {e}");
        }
    }
}

fn cmd_create(mut cfg: Config, name: String, project: String, no_tag: bool, no_launch: bool) {
    if cfg.find_workspace(&name).is_some() {
        eprintln!("Workspace already exists: {name}");
        process::exit(1);
    }
    let proj = cfg.find_project(&project).unwrap_or_else(|| {
        eprintln!("Project not found: {project}");
        process::exit(1);
    });
    let ws = config::Workspace {
        name: name.clone(),
        project: project.clone(),
        repo: proj.repo.clone(),
        branch: proj.branch.clone(),
        gui: proj.gui.clone(),
        layout: proj.layout.clone(),
        active: false,
        tag_index: 0,
        dir: String::new(),
    };
    cfg.append_workspace_to_project(&project, &name).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        process::exit(1);
    });
    let mut mgr = make_manager(cfg);
    let opts = UpOptions {
        create_tag: !no_tag,
        launch_apps: !no_launch,
    };
    if let Err(e) = mgr.up(&ws, &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_destroy(cfg: Config, name: String, no_tag: bool) {
    let ws = cfg.find_workspace(&name).unwrap_or_else(|| {
        eprintln!("Workspace not found: {name}");
        process::exit(1);
    });
    let mut mgr = make_manager(cfg);
    let opts = DownOptions {
        manage_tag: !no_tag,
        keep_worktree: false,
    };
    if let Err(e) = mgr.down(&ws, &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
    mgr.config.remove_workspace(&name);
    if let Err(e) = config::save_config(&mgr.config) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_destroy_current(cfg: Config) {
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
    let mut mgr = make_manager(cfg);
    if let Ok(true) = mgr.is_dirty(&ws) {
        eprintln!("Cannot destroy {name}: uncommitted changes");
        process::exit(1);
    }
    let _ = wm.restore_previous_tag();
    let opts = DownOptions {
        manage_tag: true,
        keep_worktree: false,
    };
    if let Err(e) = mgr.down(&ws, &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
    mgr.config.remove_workspace(&name);
    if let Err(e) = config::save_config(&mgr.config) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_close(cfg: Config) {
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
    let mut mgr = make_manager(cfg);
    let opts = DownOptions {
        manage_tag: true,
        keep_worktree: true,
    };
    if let Err(e) = mgr.down(&ws, &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_cycle(cfg: Config) {
    let active = cfg.active_names();
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
    let mgr = make_manager(cfg);
    let _ = mgr.switch(&active[next_idx]);
}

fn cmd_list(cfg: &Config) {
    for proj in &cfg.projects {
        println!("{}  ({}  branch:{})", proj.name, proj.repo, proj.branch);
        for ws in &proj.workspaces {
            let status = if ws.active { "UP" } else { "  " };
            let tag = if ws.active {
                format!(" [tag P:{}]", ws.name)
            } else {
                String::new()
            };
            println!("    [{status}] {}{tag}", ws.name);
        }
    }
}

fn cmd_switch(cfg: &Config, name: &str) {
    let ws = cfg.find_workspace(name);
    if !ws.is_some_and(|w| w.active) {
        eprintln!("Workspace not active: {name}");
        process::exit(1);
    }
    let wm = AwesomeAdapter::new();
    if let Err(e) = wm.switch_tag(name) {
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

fn cmd_names(cfg: &Config) {
    for n in cfg.active_names() {
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

fn cmd_projects(cfg: &Config) {
    for p in &cfg.projects {
        println!("{}", p.name);
    }
}

fn cmd_edit() {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
    let path = config::config_path();
    let _ = process::Command::new(editor).arg(path).status();
}

fn cmd_projects_ui() {
    if daemon::is_running() {
        match daemon::send_command("projects") {
            Ok(_) => {}
            Err(e) => eprintln!("Error: {e}"),
        }
    } else {
        eprintln!("awesometree-daemon is not running");
        process::exit(1);
    }
}

fn cmd_restart_daemon() {
    if daemon::is_running() {
        match daemon::send_command("restart") {
            Ok(_) => {}
            Err(e) => eprintln!("Error: {e}"),
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    cmd_daemon();
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
