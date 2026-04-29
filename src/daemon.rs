use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;

pub const SOCK_PATH: &str = "/tmp/awesometree.sock";

#[derive(Debug)]
pub enum DaemonCmd {
    Pick,
    Create,
    Projects,
    LaunchAgent,
    Agents,
    Restart,
    Reload,
    Logs,
    MobileQr,
}

pub fn send_command(cmd: &str) -> Result<String, String> {
    let mut stream =
        UnixStream::connect(SOCK_PATH).map_err(|e| format!("connect to daemon: {e}"))?;
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
    let stream = match UnixStream::connect(SOCK_PATH) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let timeout = std::time::Duration::from_secs(2);
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let mut s = stream;
    if s.write_all(b"ping\n").is_err() {
        let _ = std::fs::remove_file(SOCK_PATH);
        return false;
    }
    let _ = s.shutdown(std::net::Shutdown::Write);
    let mut buf = String::new();
    if BufReader::new(&s).read_line(&mut buf).is_err() {
        let _ = std::fs::remove_file(SOCK_PATH);
        return false;
    }
    !buf.is_empty()
}

pub fn listen(tx: mpsc::Sender<DaemonCmd>) {
    let sock = PathBuf::from(SOCK_PATH);
    let _ = std::fs::remove_file(&sock);

    let listener = UnixListener::bind(&sock).expect("failed to bind daemon socket");

    for stream in listener.incoming().flatten() {
        if let Some(cmd) = handle_client(stream, &tx) {
            let _ = tx.send(cmd);
        }
    }
}

fn handle_client(stream: UnixStream, _tx: &mpsc::Sender<DaemonCmd>) -> Option<DaemonCmd> {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return None;
    }
    let cmd = parse_command(line.trim());

    let mut writer = stream;
    let response = if cmd.is_some() { "ok" } else { "unknown command" };
    let _ = writer.write_all(format!("{response}\n").as_bytes());

    cmd
}

pub fn parse_command(input: &str) -> Option<DaemonCmd> {
    match input {
        "pick" => Some(DaemonCmd::Pick),
        "create" => Some(DaemonCmd::Create),
        "projects" => Some(DaemonCmd::Projects),
        "launch-agent" => Some(DaemonCmd::LaunchAgent),
        "agents" => Some(DaemonCmd::Agents),
        "restart" => Some(DaemonCmd::Restart),
        "reload" => Some(DaemonCmd::Reload),
        "logs" => Some(DaemonCmd::Logs),
        "mobile-qr" => Some(DaemonCmd::MobileQr),
        _ => None,
    }
}

pub fn cleanup() {
    let _ = std::fs::remove_file(SOCK_PATH);
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
    }

    #[test]
    fn parse_command_launch_agent() {
        assert!(matches!(parse_command("launch-agent"), Some(DaemonCmd::LaunchAgent)));
    }

    #[test]
    fn parse_command_agents() {
        assert!(matches!(parse_command("agents"), Some(DaemonCmd::Agents)));
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
    fn parse_command_unknown() {
        assert!(parse_command("unknown").is_none());
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
    fn sock_path_is_tmp() {
        assert!(SOCK_PATH.starts_with("/tmp/"));
    }
}
