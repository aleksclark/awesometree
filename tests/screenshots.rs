use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn output_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs").join("images");
    fs::create_dir_all(&dir).expect("create output dir");
    dir
}

fn screenshot_binary() -> PathBuf {
    let release = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target").join("release").join("screenshot");
    if release.exists() { return release; }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target").join("debug").join("screenshot")
}

fn cli_binary() -> PathBuf {
    let release = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target").join("release").join("awesometree");
    if release.exists() { return release; }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target").join("debug").join("awesometree")
}

fn find_window_by_class(class: &str, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(output) = Command::new("xdotool")
            .args(["search", "--class", class])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(id) = stdout.lines().next() {
                let id = id.trim();
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    None
}

fn capture_window(window_id: &str, output_path: &Path) -> bool {
    Command::new("import")
        .args(["-window", window_id, output_path.to_str().unwrap()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn capture_gpui_window(mode: &str, class: &str, output_path: &Path) -> bool {
    let mut child = Command::new(screenshot_binary())
        .arg(mode)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("launch screenshot binary");

    let result = if let Some(wid) = find_window_by_class(class, Duration::from_secs(5)) {
        thread::sleep(Duration::from_millis(500));
        capture_window(&wid, output_path)
    } else {
        false
    };

    let _ = child.kill();
    let _ = child.wait();
    result
}

fn capture_cli_svg(args: &[&str], output_path: &Path) {
    let output = Command::new(cli_binary())
        .args(args)
        .output()
        .expect("run awesometree");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = if output.stderr.is_empty() {
        stdout.to_string()
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!("{stdout}{stderr}")
    };

    let cmd = format!("awesometree {}", args.join(" "));
    fs::write(output_path, terminal_svg(&cmd, &text)).expect("write SVG");
}

const C_BG: &str      = "\x231e1e2e";
const C_BG_DARK: &str  = "\x23181825";
const C_BORDER: &str   = "\x23313244";
const C_FG: &str       = "\x23cdd6f4";
const C_DIM: &str      = "\x236c7086";
const C_BLUE: &str     = "\x2389b4fa";
const C_GREEN: &str    = "\x23a6e3a1";
const C_RED: &str      = "\x23f38ba8";
const C_YELLOW: &str   = "\x23f9e2af";
const FONT: &str       = "JetBrains Mono,Menlo,monospace";

fn terminal_svg(command: &str, output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let lh: i32 = 18;
    let pad: i32 = 20;
    let tb: i32 = 36;
    let cs = tb + pad;
    let clh = lh + 8;
    let n = lines.len() as i32;
    let h = cs + clh + n * lh + pad + 10;
    let max_w = lines.iter().map(|l| l.len()).max().unwrap_or(40).max(command.len()) as i32;
    let w = (max_w * 8).max(500) + pad * 2;

    let mut s = String::with_capacity(4096);
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n\
         <defs><filter id=\"shadow\" x=\"-5%\" y=\"-5%\" width=\"110%\" height=\"110%\">\
         <feDropShadow dx=\"0\" dy=\"4\" stdDeviation=\"8\" flood-opacity=\"0.3\"/>\
         </filter></defs>\n\
         <rect width=\"{w}\" height=\"{h}\" rx=\"10\" fill=\"{C_BG}\" filter=\"url({f}shadow)\" stroke=\"{C_BORDER}\"/>\n\
         <rect width=\"{w}\" height=\"{tb}\" rx=\"10\" fill=\"{C_BG_DARK}\"/>\n\
         <rect y=\"26\" width=\"{w}\" height=\"10\" fill=\"{C_BG_DARK}\"/>\n\
         <circle cx=\"18\" cy=\"18\" r=\"6\" fill=\"{C_RED}\"/>\n\
         <circle cx=\"38\" cy=\"18\" r=\"6\" fill=\"{C_YELLOW}\"/>\n\
         <circle cx=\"58\" cy=\"18\" r=\"6\" fill=\"{C_GREEN}\"/>\n",
        f = "\x23",
    ));

    s.push_str(&format!(
        "<text x=\"{pad}\" y=\"{cs}\" font-family=\"{FONT}\" font-size=\"13\">\
         <tspan fill=\"{C_GREEN}\">$</tspan>\
         <tspan fill=\"{C_FG}\"> {}</tspan></text>\n",
        xml_escape(command),
    ));

    for (i, line) in lines.iter().enumerate() {
        let y = cs + clh + i as i32 * lh;
        s.push_str(&format!(
            "<text x=\"{pad}\" y=\"{y}\" font-family=\"{FONT}\" font-size=\"12\">{}</text>\n",
            colorize(line),
        ));
    }

    s.push_str("</svg>\n");
    s
}

fn colorize(line: &str) -> String {
    let e = xml_escape(line);
    if line.starts_with("    [UP]") {
        let rest = &e[8..];
        if let Some(tp) = rest.find("[tag ") {
            let (name, tag) = rest.split_at(tp);
            return format!(
                "<tspan fill=\"{C_FG}\">    [</tspan><tspan fill=\"{C_GREEN}\">UP</tspan>\
                 <tspan fill=\"{C_FG}\">{name}</tspan><tspan fill=\"{C_DIM}\">{tag}</tspan>"
            );
        }
        return format!(
            "<tspan fill=\"{C_FG}\">    [</tspan><tspan fill=\"{C_GREEN}\">UP</tspan>\
             <tspan fill=\"{C_FG}\">{rest}</tspan>"
        );
    }
    if line.starts_with("    [  ]") {
        return format!("<tspan fill=\"{C_FG}\">{e}</tspan>");
    }
    if line.contains('(') && line.contains("branch:") {
        if let Some(p) = e.find("  (") {
            let (name, rest) = e.split_at(p);
            return format!(
                "<tspan fill=\"{C_BLUE}\" font-weight=\"bold\">{name}</tspan>\
                 <tspan fill=\"{C_DIM}\">{rest}</tspan>"
            );
        }
    }
    format!("<tspan fill=\"{C_FG}\">{e}</tspan>")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[test]
fn generate_screenshots() {
    if env::var("SCREENSHOTS").is_err() {
        println!("Skipping screenshot tests (set SCREENSHOTS=1 to enable)");
        return;
    }

    let out = output_dir();

    capture_cli_svg(&["list"], &out.join("terminal-list.svg"));
    capture_cli_svg(&["--help"], &out.join("terminal-help.svg"));

    if capture_gpui_window("picker", "awesometree-picker", &out.join("picker.png")) {
        println!("Saved picker.png");
    } else {
        eprintln!("WARN: failed to capture picker (needs GPU display)");
    }

    if capture_gpui_window("create", "awesometree-picker", &out.join("create-form.png")) {
        println!("Saved create-form.png");
    } else {
        eprintln!("WARN: failed to capture create form (needs GPU display)");
    }
}
