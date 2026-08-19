use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;

pub fn sock_path() -> PathBuf {
    let dir = crate::paths::home_dir().join(".config/awesometree");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("daemon.sock")
}

#[derive(Debug)]
pub enum DaemonCmd {
    Pick,
    Create,
    Projects,
    Cleanup,
    LaunchAgent,
    Agents,
    Restart,
    Reload,
    Logs,
    MobileQr,
}

pub fn send_command(cmd: &str) -> Result<String, String> {
    let path = sock_path();
    let mut stream =
        UnixStream::connect(&path).map_err(|e| format!("connect to daemon: {e}"))?;
    stream
        .write_all(format!("{cmd}\n").as_bytes())
        .map_err(|e| format!("write to daemon: {e}"))?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|e| format!("shutdown write: {e}"))?;
    let mut response = String::new();
    BufReader::new(&stream)
        .read_line(&mut response)
        .map_err(|e| format!("read from daemon: {e}"))?;
    Ok(response.trim().to_string())
}

pub fn is_running() -> bool {
    let path = sock_path();
    let stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let timeout = std::time::Duration::from_secs(2);
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let mut s = stream;
    if s.write_all(b"ping\n").is_err() {
        return false;
    }
    let _ = s.shutdown(std::net::Shutdown::Write);
    let mut buf = String::new();
    if BufReader::new(&s).read_line(&mut buf).is_err() {
        return false;
    }
    !buf.is_empty()
}

pub fn listen(tx: mpsc::Sender<DaemonCmd>) {
    loop {
        let sock = sock_path();
        let _ = std::fs::remove_file(&sock);

        let listener = match UnixListener::bind(&sock) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("failed to bind daemon socket at {}: {e}", sock.display());
                std::thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
        };

        listener
            .set_nonblocking(true)
            .expect("set_nonblocking failed");

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    if let Some(cmd) = handle_client(stream, &tx) {
                        let _ = tx.send(cmd);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }

            if !sock.exists() {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

fn handle_client(stream: UnixStream, _tx: &mpsc::Sender<DaemonCmd>) -> Option<DaemonCmd> {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return None;
    }
    let input = line.trim();

    // Synchronous production create over IPC (no GPUI). Used by CLI/automation
    // and e2e; still goes through WorkSessionService → Switchboard.
    if let Some(args) = input.strip_prefix("work-session-create ") {
        let response = handle_work_session_create(args);
        let mut writer = stream;
        let _ = writer.write_all(format!("{response}\n").as_bytes());
        return None;
    }

    let cmd = parse_command(input);
    let mut writer = stream;
    let response = if cmd.is_some() { "ok" } else { "unknown command" };
    let _ = writer.write_all(format!("{response}\n").as_bytes());
    cmd
}

/// Parse and execute `work-session-create <id> <project_id> [--profile ID] [--headless]`.
/// Public for integration tests that exercise the same path as the Unix socket handler.
pub fn work_session_create_ipc(args: &str) -> String {
    handle_work_session_create(args)
}

fn handle_work_session_create(args: &str) -> String {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 2 {
        return "error: usage: work-session-create <work_session_id> <project_id> [--profile ID] [--headless]".into();
    }
    let work_session_id = parts[0].to_string();
    let project_id = parts[1].to_string();
    let mut work_profile_id = None;
    let mut headless = false;
    let mut i = 2;
    while i < parts.len() {
        match parts[i] {
            "--profile" if i + 1 < parts.len() => {
                work_profile_id = Some(parts[i + 1].to_string());
                i += 2;
            }
            "--headless" => {
                headless = true;
                i += 1;
            }
            other => return format!("error: unknown arg {other}"),
        }
    }

    use crate::model::work_session::{CreateWorkSessionRequest, RealizationOptions};
    use crate::service_access;

    let req = CreateWorkSessionRequest {
        work_session_id: work_session_id.clone(),
        project_id,
        work_profile_id,
        display_name: Some(work_session_id.clone()),
        realization: RealizationOptions {
            create_tag: !headless,
            launch_apps: !headless,
            headless,
            no_wm: headless,
        },
    };

    let svc = service_access::service_blocking();
    let result = match tokio::runtime::Handle::try_current() {
        Ok(h) => tokio::task::block_in_place(|| h.block_on(svc.create_work_session(req))),
        Err(_) => {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => return format!("error: runtime: {e}"),
            };
            rt.block_on(svc.create_work_session(req))
        }
    };

    match result {
        Ok(resp) => format!(
            "ok work_session={} state={} work_profile={} revision={}",
            resp.work_session.work_session_id,
            resp.work_session.state,
            resp.work_profile_id,
            resp.project_revision.as_deref().unwrap_or("-")
        ),
        Err(e) => format!("error: {e}"),
    }
}

/// Bind the daemon socket and serve until `stop` is set or the socket file is removed.
/// Used by e2e tests; production uses [`listen`].
pub fn listen_until(
    tx: mpsc::Sender<DaemonCmd>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;
    let sock = sock_path();
    let _ = std::fs::remove_file(&sock);
    let listener = match UnixListener::bind(&sock) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("failed to bind daemon socket at {}: {e}", sock.display());
            return;
        }
    };
    listener
        .set_nonblocking(true)
        .expect("set_nonblocking failed");
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                if let Some(cmd) = handle_client(stream, &tx) {
                    let _ = tx.send(cmd);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(_) => break,
        }
        if !sock.exists() {
            break;
        }
    }
    let _ = std::fs::remove_file(&sock);
}

pub fn parse_command(input: &str) -> Option<DaemonCmd> {
    match input {
        "pick" => Some(DaemonCmd::Pick),
        "create" => Some(DaemonCmd::Create),
        // CLI/tray send "projects-ui"; keep "projects" as the socket verb.
        "projects" | "projects-ui" => Some(DaemonCmd::Projects),
        "launch-agent" => Some(DaemonCmd::LaunchAgent),
        "agents" | "agents-ui" => Some(DaemonCmd::Agents),
        "restart" => Some(DaemonCmd::Restart),
        "reload" => Some(DaemonCmd::Reload),
        "logs" => Some(DaemonCmd::Logs),
        "mobile-qr" => Some(DaemonCmd::MobileQr),
        "cleanup" => Some(DaemonCmd::Cleanup),
        _ => None,
    }
}

pub fn cleanup() {
    let _ = std::fs::remove_file(sock_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_pick() {
        assert!(matches!(parse_command("pick"), Some(DaemonCmd::Pick)));
    }

    #[test]
    fn parse_command_create() {
        assert!(matches!(parse_command("create"), Some(DaemonCmd::Create)));
    }

    #[test]
    fn parse_command_projects() {
        assert!(matches!(parse_command("projects"), Some(DaemonCmd::Projects)));
        assert!(matches!(parse_command("projects-ui"), Some(DaemonCmd::Projects)));
    }

    #[test]
    fn parse_command_launch_agent() {
        assert!(matches!(parse_command("launch-agent"), Some(DaemonCmd::LaunchAgent)));
    }

    #[test]
    fn parse_command_agents() {
        assert!(matches!(parse_command("agents"), Some(DaemonCmd::Agents)));
        assert!(matches!(parse_command("agents-ui"), Some(DaemonCmd::Agents)));
    }

    #[test]
    fn parse_command_restart() {
        assert!(matches!(parse_command("restart"), Some(DaemonCmd::Restart)));
    }

    #[test]
    fn parse_command_reload() {
        assert!(matches!(parse_command("reload"), Some(DaemonCmd::Reload)));
    }

    #[test]
    fn parse_command_logs() {
        assert!(matches!(parse_command("logs"), Some(DaemonCmd::Logs)));
    }

    #[test]
    fn parse_command_mobile_qr() {
        assert!(matches!(parse_command("mobile-qr"), Some(DaemonCmd::MobileQr)));
    }

    #[test]
    fn parse_command_cleanup() {
        assert!(matches!(parse_command("cleanup"), Some(DaemonCmd::Cleanup)));
    }

    #[test]
    fn parse_command_unknown() {
        assert!(parse_command("unknown").is_none());
    }

    #[test]
    fn work_session_create_is_not_async_daemon_cmd() {
        // Handled synchronously in handle_client; must not map to UI Create.
        assert!(parse_command("work-session-create ws p").is_none());
    }

    #[test]
    fn parse_command_empty() {
        assert!(parse_command("").is_none());
    }

    #[test]
    fn parse_command_case_sensitive() {
        assert!(parse_command("Pick").is_none());
        assert!(parse_command("PICK").is_none());
    }

    #[test]
    fn sock_path_ends_with_daemon_sock() {
        let p = sock_path();
        assert!(p.to_string_lossy().ends_with("daemon.sock"));
        assert!(p.to_string_lossy().contains("awesometree"));
    }
}
