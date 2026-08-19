use awesometree::auth;
use awesometree::daemon;
use awesometree::model::lifecycle::WorkSessionState;
use awesometree::model::project::definition_for_create;
use awesometree::model::work_session::{
    CreateWorkSessionRequest, RealizationOptions, DEFAULT_WORK_PROFILE_ID,
};
use awesometree::server;
use awesometree::service_access;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(
    name = "awesometree",
    about = "Agent Work Model host: Switchboard Projects/WorkProfiles/WorkSessions + local Workspace realization"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// WorkSession lifecycle operations (create/list/get/transition/destroy).
    #[command(subcommand, name = "work-session")]
    WorkSession(WorkSessionCmd),
    /// Project Catalog operations (via Switchboard).
    #[command(subcommand, name = "project")]
    Project(ProjectCmd),
    /// List WorkProfiles from Switchboard.
    #[command(name = "work-profiles")]
    WorkProfiles,
    /// Cycle focus between open WorkSession tags.
    Cycle,
    /// Focus a WorkSession's window-manager tag.
    Switch {
        work_session_id: String,
    },
    Pick,
    #[command(name = "create-interactive")]
    CreateInteractive,
    #[command(name = "launch-agent")]
    LaunchAgent {
        work_session_id: String,
        #[arg(long, default_value = "claude")]
        agent: String,
    },
    /// Destroy the focused WorkSession after a dirty-worktree check.
    #[command(name = "destroy-current")]
    DestroyCurrent,
    /// Close the focused WorkSession, keep the worktree.
    Close,
    /// List open WorkSessions (shortcut).
    List,
    /// Print worktree path for a WorkSession.
    Dir {
        work_session_id: String,
    },
    Projects,
    #[command(name = "projects-ui")]
    ProjectsUi,
    #[command(name = "agents-ui")]
    AgentsUi,
    Cleanup,
    #[command(name = "restart-daemon")]
    RestartDaemon,
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
enum WorkSessionCmd {
    Create {
        work_session_id: String,
        #[arg(long)]
        project: String,
        /// WorkProfile ID; defaults to exact ID "default".
        #[arg(long)]
        work_profile: Option<String>,
        #[arg(long)]
        display_name: Option<String>,
        #[arg(long)]
        no_tag: bool,
        #[arg(long)]
        no_launch: bool,
        #[arg(long)]
        headless: bool,
    },
    List {
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        project: Option<String>,
    },
    Get {
        work_session_id: String,
    },
    Transition {
        work_session_id: String,
        state: String,
    },
    Destroy {
        work_session_id: String,
        #[arg(long)]
        keep_worktree: bool,
    },
    Close {
        work_session_id: String,
    },
    Pause {
        work_session_id: String,
    },
    Resume {
        work_session_id: String,
    },
}

#[derive(Subcommand)]
enum ProjectCmd {
    List,
    Show { project_id: String },
    Create {
        project_id: String,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    Delete {
        project_id: String,
        #[arg(long)]
        expected_source_revision: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::WorkSession(sub) => cmd_work_session(sub),
        Commands::Project(sub) => cmd_project(sub),
        Commands::WorkProfiles => cmd_work_profiles(),
        Commands::Cycle => send_daemon_cmd("cycle"),
        Commands::Switch { work_session_id } => send_daemon_cmd(&format!("switch {work_session_id}")),
        Commands::DestroyCurrent => cmd_destroy_current(),
        Commands::Close => cmd_close_current(),
        Commands::Pick => cmd_pick(),
        Commands::CreateInteractive => cmd_create_interactive(),
        Commands::LaunchAgent {
            work_session_id,
            agent,
        } => cmd_launch_agent(&work_session_id, &agent),
        Commands::List => cmd_work_session(WorkSessionCmd::List {
            state: None,
            project: None,
        }),
        Commands::Dir { work_session_id } => cmd_dir(&work_session_id),
        Commands::Projects => cmd_project(ProjectCmd::List),
        Commands::ProjectsUi => send_daemon_cmd("projects-ui"),
        Commands::AgentsUi => send_daemon_cmd("agents-ui"),
        Commands::Cleanup => send_daemon_cmd("cleanup"),
        Commands::RestartDaemon => cmd_restart_daemon(),
        Commands::Daemon => cmd_daemon(),
        Commands::Openapi { output } => cmd_openapi(output),
        Commands::MobileQr => send_daemon_cmd("mobile-qr"),
        Commands::GenerateToken => println!("{}", auth::generate_token()),
    }
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

fn cmd_work_session(cmd: WorkSessionCmd) {
    let svc = service_access::service_blocking();
    match cmd {
        WorkSessionCmd::Create {
            work_session_id,
            project,
            work_profile,
            display_name,
            no_tag,
            no_launch,
            headless,
        } => {
            let req = CreateWorkSessionRequest {
                work_session_id: work_session_id.clone(),
                project_id: project,
                work_profile_id: work_profile,
                display_name,
                realization: RealizationOptions {
                    create_tag: !no_tag && !headless,
                    launch_apps: !no_launch && !headless,
                    headless,
                    no_wm: headless,
                },
            };
            match rt_block_on(svc.create_work_session(req)) {
                Ok(resp) => {
                    println!(
                        "work_session={} state={} work_profile={} revision={}",
                        resp.work_session.work_session_id,
                        resp.work_session.state,
                        resp.work_profile_id,
                        resp.project_revision.as_deref().unwrap_or("-")
                    );
                    if let Some(rt) = resp.runtime
                        && let Some(ws) = rt.workspace
                    {
                        println!("workspace_path={}", ws.path);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }
        WorkSessionCmd::List { state, project } => {
            match rt_block_on(svc.list_work_sessions(state.as_deref(), project.as_deref())) {
                Ok(list) => {
                    for v in list {
                        let dir = v
                            .runtime
                            .as_ref()
                            .and_then(|r| r.workspace.as_ref())
                            .map(|w| w.path.as_str())
                            .unwrap_or("-");
                        println!(
                            "{}\t{}\t{}\t{}\t{}",
                            v.work_session.work_session_id,
                            v.work_session.state,
                            v.work_session.project_id.as_deref().unwrap_or("-"),
                            v.work_session
                                .work_profile_id
                                .as_deref()
                                .unwrap_or(DEFAULT_WORK_PROFILE_ID),
                            dir
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }
        WorkSessionCmd::Get { work_session_id } => {
            match rt_block_on(svc.get_work_session(&work_session_id)) {
                Ok(v) => {
                    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }
        WorkSessionCmd::Transition {
            work_session_id,
            state,
        } => {
            let st = WorkSessionState::parse(&state).unwrap_or_else(|| {
                eprintln!("invalid state: {state}");
                process::exit(1);
            });
            match rt_block_on(svc.transition(&work_session_id, st)) {
                Ok(v) => println!(
                    "{} -> {}",
                    v.work_session.work_session_id, v.work_session.state
                ),
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }
        WorkSessionCmd::Destroy {
            work_session_id,
            keep_worktree,
        } => {
            if let Err(e) = rt_block_on(svc.destroy(&work_session_id, keep_worktree)) {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        }
        WorkSessionCmd::Close { work_session_id } => {
            if let Err(e) = rt_block_on(svc.transition(&work_session_id, WorkSessionState::Closed))
            {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        }
        WorkSessionCmd::Pause { work_session_id } => {
            if let Err(e) = rt_block_on(svc.transition(&work_session_id, WorkSessionState::Paused))
            {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        }
        WorkSessionCmd::Resume { work_session_id } => {
            if let Err(e) = rt_block_on(svc.transition(&work_session_id, WorkSessionState::Open)) {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        }
    }
}

fn cmd_project(cmd: ProjectCmd) {
    let svc = service_access::service_blocking();
    match cmd {
        ProjectCmd::List => match rt_block_on(svc.list_projects(None)) {
            Ok(list) => {
                for p in list {
                    println!(
                        "{}\t{}\t{}",
                        p.project_id,
                        p.title,
                        p.description.as_deref().unwrap_or("")
                    );
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        ProjectCmd::Show { project_id } => match rt_block_on(svc.get_project(&project_id)) {
            Ok(env) => {
                println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
            }
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        ProjectCmd::Create {
            project_id,
            repo,
            branch,
            description,
        } => {
            let def = definition_for_create(
                &project_id,
                description.as_deref(),
                repo.as_deref(),
                branch.as_deref(),
                None,
            );
            match rt_block_on(svc.create_project(def)) {
                Ok(s) => println!(
                    "created project {} revision={}",
                    s.project_id,
                    s.revision.as_deref().unwrap_or("-")
                ),
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }
        ProjectCmd::Delete {
            project_id,
            expected_source_revision,
        } => {
            if let Err(e) = rt_block_on(svc.delete_project(&project_id, &expected_source_revision))
            {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        }
    }
}

fn cmd_work_profiles() {
    let svc = service_access::service_blocking();
    match rt_block_on(svc.list_work_profiles()) {
        Ok(list) => {
            for p in list {
                let marker = if p.work_profile_id == DEFAULT_WORK_PROFILE_ID {
                    " *"
                } else {
                    ""
                };
                println!(
                    "{}{}\t{}",
                    p.work_profile_id,
                    marker,
                    p.display()
                );
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

fn cmd_dir(work_session_id: &str) {
    match awesometree::runtime_store::get(work_session_id) {
        Ok(Some(rt)) => {
            if let Some(ws) = rt.workspace {
                println!("{}", ws.path);
            } else {
                eprintln!("no workspace path for {work_session_id}");
                process::exit(1);
            }
        }
        Ok(None) => {
            eprintln!("no local runtime for {work_session_id}");
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

fn cmd_launch_agent(work_session_id: &str, agent: &str) {
    // Prefer daemon path when available; fall back to local.
    if daemon::send_command(&format!("launch-agent {work_session_id} {agent}")).is_ok() {
        return;
    }
    eprintln!("daemon unavailable; launch-agent requires daemon for GUI agents");
    process::exit(1);
}

fn cmd_destroy_current() {
    let svc = service_access::service_blocking();
    match rt_block_on(svc.destroy_current()) {
        Ok(id) => println!("destroyed {id}"),
        Err(e) => {
            awesometree::notify::report_error(e.to_string());
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

fn cmd_close_current() {
    let svc = service_access::service_blocking();
    match rt_block_on(svc.close_current()) {
        Ok(id) => println!("closed {id}"),
        Err(e) => {
            awesometree::notify::report_error(e.to_string());
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

fn cmd_pick() {
    send_daemon_cmd("pick");
}

fn cmd_create_interactive() {
    send_daemon_cmd("create");
}

fn send_daemon_cmd(cmd: &str) {
    match daemon::send_command(cmd) {
        Ok(resp) if resp == "ok" || resp.starts_with("ok ") => {}
        Ok(resp) => {
            eprintln!("Error: daemon: {resp}");
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

fn cmd_restart_daemon() {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "restart", "awesometree"])
            .status();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("launchctl")
            .args(["kickstart", "-k", "gui/$(id -u)/dev.awesometree.daemon"])
            .status();
    }
}

fn cmd_daemon() {
    // Hand off to daemon binary behavior via library entry if present.
    eprintln!("use awesometree-daemon binary");
    process::exit(1);
}

fn cmd_openapi(output: Option<PathBuf>) {
    let spec = server::openapi_spec();
    match output {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, spec) {
                eprintln!("write {}: {e}", path.display());
                process::exit(1);
            }
        }
        None => print!("{spec}"),
    }
}
