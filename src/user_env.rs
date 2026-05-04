use crate::paths;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn env_path() -> PathBuf {
    paths::home_dir()
        .join(".config/awesometree")
        .join("env.json")
}

const CAPTURED_VARS: &[&str] = &[
    "PATH",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_TYPE",
    "XDG_CURRENT_DESKTOP",
    "DBUS_SESSION_BUS_ADDRESS",
    "HOME",
    "USER",
    "SHELL",
    "LANG",
    "LC_ALL",
    "TERM",
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
    "GNOME_KEYRING_CONTROL",
    "XDG_DATA_DIRS",
    "XDG_CONFIG_DIRS",
    "GDK_BACKEND",
    "QT_QPA_PLATFORM",
    "XCURSOR_SIZE",
    "XCURSOR_THEME",
];

pub fn snapshot() {
    let mut env: HashMap<String, String> = HashMap::new();
    for key in CAPTURED_VARS {
        if let Ok(val) = std::env::var(key) {
            env.insert(key.to_string(), val);
        }
    }
    let path = env_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&env) {
        Ok(data) => {
            let _ = fs::write(&path, data);
        }
        Err(e) => eprintln!("user_env: failed to serialize: {e}"),
    }
}

pub fn load() {
    let path = env_path();
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return,
    };
    let env: HashMap<String, String> = match serde_json::from_str(&data) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("user_env: failed to parse {}: {e}", path.display());
            return;
        }
    };
    for (key, val) in &env {
        if std::env::var(key).is_err() || key == "PATH" {
            unsafe { std::env::set_var(key, val) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_vars_includes_path() {
        assert!(CAPTURED_VARS.contains(&"PATH"));
    }

    #[test]
    fn captured_vars_includes_display() {
        assert!(CAPTURED_VARS.contains(&"DISPLAY"));
    }

    #[test]
    fn snapshot_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env.json");

        let mut env: HashMap<String, String> = HashMap::new();
        env.insert("PATH".into(), "/usr/bin:/custom/bin".into());
        env.insert("DISPLAY".into(), ":99".into());
        let data = serde_json::to_string_pretty(&env).unwrap();
        fs::write(&path, &data).unwrap();

        let loaded: HashMap<String, String> =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.get("PATH").unwrap(), "/usr/bin:/custom/bin");
        assert_eq!(loaded.get("DISPLAY").unwrap(), ":99");
    }

    #[test]
    fn load_missing_file_is_noop() {
        load();
    }
}
