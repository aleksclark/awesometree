use awesometree::auth;
use awesometree::daemon;
use awesometree::interop::{self, Project};
use awesometree::server;
use awesometree::state;
use awesometree::wm::{self, Adapter};
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
        #[arg(long)]
        nogui: bool,
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
        #[arg(long)]
        nogui: bool,
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
    #[command(subcommand, name = "project")]
    Project(ProjectCmd),
    #[command(subcommand)]
    Context(ContextCmd),
    #[command(name = "launch-agent")]
    LaunchAgent {
        workspace: String,
        #[arg(long, default_value = "claude")]
        agent: String,
    },
    Repos,
    Names,
    Allnames,
    Dir { name: String },
    Projects,
    #[command(name = "projects-ui")]
    ProjectsUi,
    #[command(name = "agents-ui")]
    AgentsUi,
    #[command(name = "restart-daemon")]
    RestartDaemon,
    Edit { name: String },
    Daemon,
    Openapi {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    #[command(name = "mobile-qr")]
    MobileQr,
    #[command(name = "generate-token")]
    GenerateToken,
}

#[derive(Subcommand)]
enum ProjectCmd {
    List,
    Show { name: String },
    Create {
        name: String,
        #[arg(long)]
        repo: String,
        #[arg(long, default_value = "master")]
        branch: String,
    },
    Edit { name: String },
    Delete { name: String },
}

#[derive(Subcommand)]
enum ContextCmd {
    List { project: String },
    Add { project: String, file: String },
    Edit { project: String, file: String },
    Rm { project: String, file: String },
    Bundle { project: String },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Up {
            name,
            no_tag,
            no_launch,
            nogui,
        } => cmd_up(name, no_tag || nogui, no_launch || nogui),
        Commands::Down {
            name,
            no_tag,
            keep_worktree,
        } => cmd_down(name, no_tag, keep_worktree),
        Commands::Create {
            name,
            project,
            no_tag,
            no_launch,
            nogui,
        } => cmd_create(name, project, no_tag || nogui, no_launch || nogui),
        Commands::Destroy { name, no_tag } => cmd_destroy(name, no_tag),
        Commands::DestroyCurrent => cmd_destroy_current(),
        Commands::Close => cmd_close(),
        Commands::Cycle => cmd_cycle(),
        Commands::List => cmd_list(),
        Commands::Switch { name } => cmd_switch(&name),
        Commands::Pick => cmd_pick(),
        Commands::CreateInteractive => cmd_create_interactive(),
        Commands::Project(sub) => cmd_project(sub),
        Commands::Context(sub) => cmd_context(sub),
        Commands::LaunchAgent { workspace, agent } => cmd_launch_agent(&workspace, &agent),
        Commands::Repos => cmd_repos(),
        Commands::Names => cmd_names(),
        Commands::Allnames => cmd_allnames(),
        Commands::Dir { name } => cmd_dir(&name),
        Commands::Projects => cmd_projects(),
        Commands::ProjectsUi => cmd_projects_ui(),
        Commands::AgentsUi => cmd_agents_ui(),
        Commands::RestartDaemon => cmd_restart_daemon(),
        Commands::Edit { name } => cmd_edit(&name),
        Commands::Daemon => cmd_daemon(),
        Commands::Openapi { output } => cmd_openapi(output),
        Commands::MobileQr => send_daemon_cmd("mobile-qr"),
        Commands::GenerateToken => println!("{}", auth::generate_token()),
    }
}

fn make_manager() -> Manager {
    let st = state::load().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        process::exit(1);
    });
    Manager::new(st, wm::platform_adapter())
}

fn cmd_up(name: Option<String>, no_tag: bool, no_launch: bool) {
    let mut mgr = make_manager();
    match name {
        Some(n) => {
            let rw = mgr.resolve_workspace(&n).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let opts = UpOptions {
                create_tag: !no_tag,
                launch_apps: !no_launch,
            };
            if let Err(e) = mgr.up(&n, &rw.project, &opts) {
                eprintln!("Error: {e}");
            }
        }
        None => {
            let active: Vec<_> = mgr.state.active_names();
            for name in active {
                let rw = match mgr.resolve_workspace(&name) {
                    Ok(rw) => rw,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        continue;
                    }
                };
                let opts = UpOptions {
                    create_tag: true,
                    launch_apps: false,
                };
                if let Err(e) = mgr.up(&name, &rw.project, &opts) {
                    eprintln!("Error: {e}");
                }
            }
        }
    }
}

fn cmd_down(name: Option<String>, no_tag: bool, keep_worktree: bool) {
    let opts = DownOptions {
        manage_tag: !no_tag,
        keep_worktree,
    };
    let mut mgr = make_manager();
    let names = match name {
        Some(n) => vec![n],
        None => mgr.state.all_names(),
    };
    for n in &names {
        if let Err(e) = mgr.down(n, &opts) {
            eprintln!("Error: {e}");
        }
    }
}

fn cmd_create(name: String, project_name: String, no_tag: bool, no_launch: bool) {
    let mut mgr = make_manager();
    if mgr.state.workspace(&name).is_some() {
        eprintln!("Workspace already exists: {name}");
        process::exit(1);
    }
    let proj = interop::load(&project_name).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        process::exit(1);
    });
    let opts = UpOptions {
        create_tag: !no_tag,
        launch_apps: !no_launch,
    };
    if let Err(e) = mgr.up(&name, &proj, &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_destroy(name: String, no_tag: bool) {
    let mut mgr = make_manager();
    let opts = DownOptions {
        manage_tag: !no_tag,
        keep_worktree: false,
    };
    if let Err(e) = mgr.destroy(&name, &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn resolve_current_workspace() -> (Box<dyn Adapter>, String) {
    let wm = wm::platform_adapter();
    let tag_full = wm
        .get_current_tag_name()
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            process::exit(1);
        })
        .unwrap_or_else(|| {
            eprintln!("Not a project workspace");
            process::exit(1);
        });
    let (_project, name) = wm::parse_tag_name(&tag_full).unwrap_or_else(|| {
        eprintln!("Not a project workspace");
        process::exit(1);
    });
    (wm, name.to_string())
}

fn cmd_destroy_current() {
    let (wm, name) = resolve_current_workspace();
    let mut mgr = make_manager();
    if let Ok(true) = mgr.is_dirty(&name) {
        eprintln!("Cannot destroy {name}: uncommitted changes");
        process::exit(1);
    }
    let _ = wm.restore_previous_tag();
    let opts = DownOptions {
        manage_tag: true,
        keep_worktree: false,
    };
    if let Err(e) = mgr.destroy(&name, &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_close() {
    let (wm, name) = resolve_current_workspace();
    let _ = wm.restore_previous_tag();
    let mut mgr = make_manager();
    let opts = DownOptions {
        manage_tag: true,
        keep_worktree: true,
    };
    if let Err(e) = mgr.down(&name, &opts) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_cycle() {
    let mgr = make_manager();
    let active = mgr.state.active_names();
    if active.is_empty() {
        return;
    }
    let wm = wm::platform_adapter();
    let current = wm.get_current_tag_name().ok().flatten();
    let current_ws = current.as_deref().and_then(wm::parse_tag_name).map(|(_, ws)| ws);
    let next_idx = match current_ws {
        Some(name) => {
            let pos = active.iter().position(|n| n == name).unwrap_or(0);
            (pos + 1) % active.len()
        }
        None => 0,
    };
    let _ = mgr.switch(&active[next_idx]);
}

fn cmd_list() {
    let projects = interop::list().unwrap_or_default();
    let st = state::load().unwrap_or_default();
    for proj in &projects {
        println!(
            "{}  ({}  branch:{})",
            proj.name,
            proj.repo.as_deref().unwrap_or(""),
            proj.branch_or_default()
        );
        for (ws_name, ws) in st.workspaces_for_project(&proj.name) {
            let status = if ws.active { "UP" } else { "  " };
            let tag = if ws.active {
                format!(" [tag {}:{}]", ws.project, ws_name)
            } else {
                String::new()
            };
            println!("    [{status}] {ws_name}{tag}");
        }
    }
}

fn cmd_switch(name: &str) {
    let mgr = make_manager();
    match mgr.state.workspace(name) {
        Some(ws) if ws.active => {
            if let Err(e) = mgr.switch(name) {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        }
        _ => {
            eprintln!("Workspace not active: {name}");
            process::exit(1);
        }
    }
}

fn send_daemon_cmd(cmd: &str) {
    if daemon::is_running() {
        match daemon::send_command(cmd) {
            Ok(_) => {}
            Err(e) => eprintln!("Error: {e}"),
        }
    } else {
        eprintln!("awesometree-daemon is not running");
        process::exit(1);
    }
}

fn cmd_pick() {
    send_daemon_cmd("pick");
}

fn cmd_create_interactive() {
    send_daemon_cmd("create");
}

fn cmd_project(sub: ProjectCmd) {
    match sub {
        ProjectCmd::List => {
            for proj in interop::list().unwrap_or_default() {
                println!("{}", proj.name);
            }
        }
        ProjectCmd::Show { name } => {
            let proj = interop::load(&name).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let json = serde_json::to_string_pretty(&proj).unwrap();
            println!("{json}");
        }
        ProjectCmd::Create {
            name,
            repo,
            branch,
        } => {
            if interop::load(&name).is_ok() {
                eprintln!("Project already exists: {name}");
                process::exit(1);
            }
            let proj = Project::new(&name, repo, branch);
            interop::save(&proj).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            println!("Created project: {name}");
        }
        ProjectCmd::Edit { name } => {
            let path = interop::projects_dir().join(format!("{name}.project.json"));
            if !path.exists() {
                eprintln!("Project not found: {name}");
                process::exit(1);
            }
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
            let _ = process::Command::new(editor).arg(&path).status();
        }
        ProjectCmd::Delete { name } => {
            interop::delete(&name).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            println!("Deleted project: {name}");
        }
    }
}

fn cmd_context(sub: ContextCmd) {
    match sub {
        ContextCmd::List { project } => {
            let dir = interop::context_dir(&project);
            if !dir.exists() {
                return;
            }
            let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            for entry in entries.flatten() {
                println!("{}", entry.file_name().to_string_lossy());
            }
        }
        ContextCmd::Add { project, file } => {
            let dir = interop::context_dir(&project);
            std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let src = PathBuf::from(&file);
            let dest = dir.join(src.file_name().unwrap_or(src.as_ref()));
            std::fs::copy(&src, &dest).unwrap_or_else(|e| {
                eprintln!("Error copying {file}: {e}");
                process::exit(1);
            });
            println!(
                "Added {} to {project} context",
                dest.file_name().unwrap().to_string_lossy()
            );
        }
        ContextCmd::Edit { project, file } => {
            let path = interop::context_dir(&project).join(&file);
            if !path.exists() {
                eprintln!("Context file not found: {file}");
                process::exit(1);
            }
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
            let _ = process::Command::new(editor).arg(&path).status();
        }
        ContextCmd::Rm { project, file } => {
            let path = interop::context_dir(&project).join(&file);
            std::fs::remove_file(&path).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            println!("Removed {file} from {project} context");
        }
        ContextCmd::Bundle { project } => {
            let proj = interop::load(&project).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let bundle = interop::assemble_context_bundle(&proj).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            for (path, content) in &bundle {
                println!("--- {path} ---");
                println!("{content}");
            }
        }
    }
}

fn cmd_launch_agent(workspace: &str, agent: &str) {
    let mgr = make_manager();
    if let Err(e) = mgr.launch_agent(workspace, agent) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn cmd_repos() {
    for r in interop::list_repos() {
        println!("{}", r.display());
    }
}

fn cmd_names() {
    let st = state::load().unwrap_or_default();
    for n in st.active_names() {
        println!("{n}");
    }
}

fn cmd_allnames() {
    let st = state::load().unwrap_or_default();
    for n in st.all_names() {
        println!("{n}");
    }
}

fn cmd_dir(name: &str) {
    let mgr = make_manager();
    let rw = mgr.resolve_workspace(name).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        process::exit(1);
    });
    println!("{}", rw.dir.display());
}

fn cmd_projects() {
    cmd_project(ProjectCmd::List);
}

fn cmd_edit(name: &str) {
    cmd_project(ProjectCmd::Edit { name: name.to_string() });
}

fn cmd_projects_ui() {
    send_daemon_cmd("projects");
}

fn cmd_agents_ui() {
    send_daemon_cmd("agents");
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
    let log_err = log
        .try_clone()
        .unwrap_or_else(|_| std::fs::File::open("/dev/null").unwrap());

    #[cfg(target_os = "linux")]
    {
        let _ = process::Command::new("setsid")
            .arg("--fork")
            .arg(&exe)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::from(log))
            .stderr(process::Stdio::from(log_err))
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = process::Command::new(&exe)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::from(log))
            .stderr(process::Stdio::from(log_err))
            .spawn();
    }
}

fn cmd_openapi(output: Option<PathBuf>) {
    let spec = server::openapi_spec();
    match output {
        Some(path) => {
            std::fs::write(&path, &spec).unwrap_or_else(|e| {
                eprintln!("Error writing {}: {e}", path.display());
                process::exit(1);
            });
            eprintln!("Wrote OpenAPI spec to {}", path.display());
        }
        None => print!("{spec}"),
    }
}
