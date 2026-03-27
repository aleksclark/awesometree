use crate::paths;
use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const TOKEN_LIFETIME_SECS: u64 = 86400 * 30;

static SERVER_SECRET: OnceLock<Vec<u8>> = OnceLock::new();

fn secret_path() -> PathBuf {
    paths::home_dir()
        .join(".config/awesometree")
        .join("server.key")
}

fn load_or_create_secret() -> Vec<u8> {
    let path = secret_path();
    if let Ok(data) = fs::read(&path) {
        if data.len() == 32 {
            return data;
        }
    }
    let secret: Vec<u8> = rand::rng().random::<[u8; 32]>().to_vec();
    let dir = path.parent().unwrap();
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(&path, &secret);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    secret
}

fn get_secret() -> &'static [u8] {
    SERVER_SECRET.get_or_init(load_or_create_secret)
}

pub fn generate_token() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let nonce: u64 = rand::rng().random();
    let payload = format!("{now}:{nonce}");

    let mut mac =
        HmacSha256::new_from_slice(get_secret()).expect("HMAC key");
    mac.update(payload.as_bytes());
    let sig = mac.finalize().into_bytes();
    let sig_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sig);

    format!("{payload}:{sig_b64}")
}

pub fn validate_token(token: &str) -> bool {
    let parts: Vec<&str> = token.splitn(3, ':').collect();
    if parts.len() != 3 {
        return false;
    }

    let Ok(timestamp) = parts[0].parse::<u64>() else {
        return false;
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if now.saturating_sub(timestamp) > TOKEN_LIFETIME_SECS {
        return false;
    }

    let payload = format!("{}:{}", parts[0], parts[1]);
    let mut mac =
        HmacSha256::new_from_slice(get_secret()).expect("HMAC key");
    mac.update(payload.as_bytes());

    let Ok(expected_sig) =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[2])
    else {
        return false;
    };

    mac.verify_slice(&expected_sig).is_ok()
}

pub fn get_local_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

use base64::Engine;

pub fn connection_json(port: u16) -> String {
    let token = generate_token();
    let host = get_local_ip();
    serde_json::json!({
        "host": host,
        "port": port,
        "token": token,
    })
    .to_string()
}

pub fn token_only() -> String {
    generate_token()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_validate_token() {
        let token = generate_token();
        assert!(validate_token(&token));
    }

    #[test]
    fn invalid_token_rejected() {
        assert!(!validate_token("garbage"));
        assert!(!validate_token("1:2:3"));
        assert!(!validate_token(""));
    }

    #[test]
    fn tampered_token_rejected() {
        let token = generate_token();
        let tampered = format!("{token}x");
        assert!(!validate_token(&tampered));
    }

    #[test]
    fn token_has_three_parts() {
        let token = generate_token();
        assert_eq!(token.splitn(4, ':').count(), 3);
    }

    #[test]
    fn connection_json_has_fields() {
        let json = connection_json(9099);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["host"].is_string());
        assert_eq!(parsed["port"], 9099);
        assert!(parsed["token"].is_string());
    }

    #[test]
    fn get_local_ip_not_empty() {
        let ip = get_local_ip();
        assert!(!ip.is_empty());
    }

    #[test]
    fn token_only_is_valid() {
        let token = token_only();
        assert!(validate_token(&token));
    }
}
