use crate::paths;
use crate::state::AgentInstanceState;
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
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

// ---------------------------------------------------------------------------
// Legacy simple tokens (kept for backward compat with existing bearer auth)
// ---------------------------------------------------------------------------

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
    // Try scoped token first
    if validate_scoped_token(token).is_some() {
        return true;
    }

    // Fall back to legacy simple token
    validate_legacy_token(token)
}

fn validate_legacy_token(token: &str) -> bool {
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

// ---------------------------------------------------------------------------
// Scoped Token System
// ---------------------------------------------------------------------------

/// Project scope for a token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum TokenScope {
    /// `"*"` — all projects
    Global,
    /// Specific project names
    Projects(Vec<String>),
}

/// Permission level. Admin > Project > Session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Session,
    Project,
    Admin,
}

impl Permission {
    /// Numeric rank for comparison (higher = more permissive).
    fn rank(&self) -> u8 {
        match self {
            Permission::Session => 0,
            Permission::Project => 1,
            Permission::Admin => 2,
        }
    }
}

/// A scoped ARP token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedToken {
    pub id: String,
    pub subject: String,
    pub scope: TokenScope,
    pub permission: Permission,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub issued_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_token_id: Option<String>,
}

// In-memory token store
static TOKEN_STORE: OnceLock<Mutex<HashMap<String, ScopedToken>>> = OnceLock::new();

fn token_store() -> &'static Mutex<HashMap<String, ScopedToken>> {
    TOKEN_STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Create a scoped token with the given parameters.
/// Persists the token in the in-memory store.
pub fn create_scoped_token(
    subject: &str,
    scope: TokenScope,
    permission: Permission,
    expires_in: Option<u64>,
) -> ScopedToken {
    let now = chrono::Utc::now();
    let expires_at = expires_in.map(|secs| {
        (now + chrono::Duration::seconds(secs as i64)).to_rfc3339()
    });

    let token = ScopedToken {
        id: uuid::Uuid::new_v4().to_string(),
        subject: subject.to_string(),
        scope,
        permission,
        session_id: None,
        issued_at: now.to_rfc3339(),
        expires_at,
        parent_token_id: None,
    };

    let mut store = token_store().lock().unwrap();
    store.insert(token.id.clone(), token.clone());
    token
}

/// Create a child token that can only narrow scope/permission, never widen.
/// Returns Err if the child would exceed the parent's scope or permission.
pub fn create_child_token(
    parent: &ScopedToken,
    scope: Option<TokenScope>,
    permission: Option<Permission>,
) -> Result<ScopedToken, String> {
    let child_scope = scope.unwrap_or_else(|| parent.scope.clone());
    let child_perm = permission.unwrap_or_else(|| parent.permission.clone());

    // Validate permission doesn't escalate
    if child_perm.rank() > parent.permission.rank() {
        return Err(format!(
            "child permission {:?} exceeds parent {:?}",
            child_perm, parent.permission
        ));
    }

    // Validate scope doesn't widen
    if !scope_is_subset(&child_scope, &parent.scope) {
        return Err(
            "child scope exceeds parent scope".to_string(),
        );
    }

    let now = chrono::Utc::now();
    let expires_at = parent.expires_at.clone();
    let session_id = parent.session_id.clone();

    let token = ScopedToken {
        id: uuid::Uuid::new_v4().to_string(),
        subject: parent.subject.clone(),
        scope: child_scope,
        permission: child_perm,
        session_id,
        issued_at: now.to_rfc3339(),
        expires_at,
        parent_token_id: Some(parent.id.clone()),
    };

    let mut store = token_store().lock().unwrap();
    store.insert(token.id.clone(), token.clone());
    Ok(token)
}

/// Encode a ScopedToken as an HMAC-signed bearer string.
/// Format: `base64url(json).base64url(hmac_sig)`
pub fn encode_scoped_token(token: &ScopedToken) -> String {
    let json = serde_json::to_string(token).expect("serialize token");
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(json.as_bytes());

    let mut mac =
        HmacSha256::new_from_slice(get_secret()).expect("HMAC key");
    mac.update(payload_b64.as_bytes());
    let sig = mac.finalize().into_bytes();
    let sig_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sig);

    format!("{payload_b64}.{sig_b64}")
}

/// Validate and decode a scoped token string.
/// Returns None if the token is invalid, expired, or tampered with.
pub fn validate_scoped_token(token_str: &str) -> Option<ScopedToken> {
    let parts: Vec<&str> = token_str.splitn(2, '.').collect();
    if parts.len() != 2 {
        return None;
    }

    let payload_b64 = parts[0];
    let sig_b64 = parts[1];

    // Verify HMAC
    let mut mac =
        HmacSha256::new_from_slice(get_secret()).expect("HMAC key");
    mac.update(payload_b64.as_bytes());

    let sig_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(sig_b64)
        .ok()?;

    mac.verify_slice(&sig_bytes).ok()?;

    // Decode payload
    let json_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let json_str = String::from_utf8(json_bytes).ok()?;
    let token: ScopedToken = serde_json::from_str(&json_str).ok()?;

    // Check expiry
    if let Some(ref expires_at) = token.expires_at {
        if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires_at) {
            if exp < chrono::Utc::now() {
                return None;
            }
        }
    }

    Some(token)
}

/// Extract Bearer token from Authorization header, validate as scoped token.
pub fn resolve_token_from_header(auth_header: Option<&str>) -> Option<ScopedToken> {
    let token_str = auth_header?
        .strip_prefix("Bearer ")?;
    validate_scoped_token(token_str)
}

/// Return a synthetic admin/*-scoped token for localhost callers.
pub fn localhost_admin_token() -> ScopedToken {
    ScopedToken {
        id: "localhost-admin".to_string(),
        subject: "localhost".to_string(),
        scope: TokenScope::Global,
        permission: Permission::Admin,
        session_id: None,
        issued_at: chrono::Utc::now().to_rfc3339(),
        expires_at: None,
        parent_token_id: None,
    }
}

/// Look up a token by ID in the in-memory store.
pub fn get_token_by_id(id: &str) -> Option<ScopedToken> {
    let store = token_store().lock().unwrap();
    store.get(id).cloned()
}

/// Ensure a token has a session_id, creating one if needed.
/// Returns the session_id.
pub fn ensure_session(token: &mut ScopedToken) -> String {
    if let Some(ref sid) = token.session_id {
        return sid.clone();
    }
    let session_id = format!("sess-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    token.session_id = Some(session_id.clone());

    // Update in store
    let mut store = token_store().lock().unwrap();
    if let Some(stored) = store.get_mut(&token.id) {
        stored.session_id = Some(session_id.clone());
    }
    session_id
}

// ---------------------------------------------------------------------------
// Scope enforcement helpers
// ---------------------------------------------------------------------------

/// Returns true if `scope` includes the given project.
pub fn scope_includes_project(scope: &TokenScope, project: &str) -> bool {
    match scope {
        TokenScope::Global => true,
        TokenScope::Projects(projects) => projects.iter().any(|p| p == project),
    }
}

/// Returns true if `token_perm` >= `required` (admin > project > session).
pub fn permission_allows(token_perm: &Permission, required: &Permission) -> bool {
    token_perm.rank() >= required.rank()
}

/// For session-scoped tokens, checks that the agent's session_id matches.
pub fn session_matches(token: &ScopedToken, agent: &AgentInstanceState) -> bool {
    match token.permission {
        Permission::Session => {
            match (&token.session_id, &agent.session_id) {
                (Some(ts), Some(as_)) => ts == as_,
                _ => false,
            }
        }
        // Non-session tokens don't need session matching
        _ => true,
    }
}

/// Check if child_scope is a subset of parent_scope.
fn scope_is_subset(child: &TokenScope, parent: &TokenScope) -> bool {
    match parent {
        TokenScope::Global => true, // Parent is global, any child is a subset
        TokenScope::Projects(parent_projects) => {
            match child {
                TokenScope::Global => false, // Can't widen to global
                TokenScope::Projects(child_projects) => {
                    child_projects.iter().all(|cp| parent_projects.contains(cp))
                }
            }
        }
    }
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

    // Scoped token tests

    #[test]
    fn create_and_validate_scoped_token() {
        let token = create_scoped_token(
            "test-user",
            TokenScope::Global,
            Permission::Admin,
            None,
        );
        let encoded = encode_scoped_token(&token);
        let decoded = validate_scoped_token(&encoded).unwrap();
        assert_eq!(decoded.id, token.id);
        assert_eq!(decoded.subject, "test-user");
        assert_eq!(decoded.permission, Permission::Admin);
        assert_eq!(decoded.scope, TokenScope::Global);
    }

    #[test]
    fn scoped_token_with_project_scope() {
        let token = create_scoped_token(
            "test-user",
            TokenScope::Projects(vec!["myapp".into(), "lib".into()]),
            Permission::Project,
            None,
        );
        let encoded = encode_scoped_token(&token);
        let decoded = validate_scoped_token(&encoded).unwrap();
        assert_eq!(decoded.scope, TokenScope::Projects(vec!["myapp".into(), "lib".into()]));
        assert_eq!(decoded.permission, Permission::Project);
    }

    #[test]
    fn scoped_token_expiry() {
        // Token that expires in 0 seconds (already expired)
        let mut token = create_scoped_token(
            "test-user",
            TokenScope::Global,
            Permission::Admin,
            None,
        );
        token.expires_at = Some("2020-01-01T00:00:00+00:00".into());
        let encoded = encode_scoped_token(&token);
        assert!(validate_scoped_token(&encoded).is_none());
    }

    #[test]
    fn scoped_token_tampered() {
        let token = create_scoped_token(
            "test-user",
            TokenScope::Global,
            Permission::Admin,
            None,
        );
        let encoded = encode_scoped_token(&token);
        let tampered = format!("{encoded}x");
        assert!(validate_scoped_token(&tampered).is_none());
    }

    #[test]
    fn scoped_token_detected_by_validate_token() {
        let token = create_scoped_token(
            "test-user",
            TokenScope::Global,
            Permission::Admin,
            None,
        );
        let encoded = encode_scoped_token(&token);
        assert!(validate_token(&encoded));
    }

    #[test]
    fn child_token_inherits_parent() {
        let parent = create_scoped_token(
            "operator",
            TokenScope::Projects(vec!["myapp".into(), "lib".into()]),
            Permission::Project,
            None,
        );
        let child = create_child_token(&parent, None, None).unwrap();
        assert_eq!(child.scope, parent.scope);
        assert_eq!(child.permission, parent.permission);
        assert_eq!(child.parent_token_id, Some(parent.id.clone()));
    }

    #[test]
    fn child_token_narrows_scope() {
        let parent = create_scoped_token(
            "operator",
            TokenScope::Projects(vec!["myapp".into(), "lib".into()]),
            Permission::Project,
            None,
        );
        let child = create_child_token(
            &parent,
            Some(TokenScope::Projects(vec!["myapp".into()])),
            None,
        )
        .unwrap();
        assert_eq!(child.scope, TokenScope::Projects(vec!["myapp".into()]));
    }

    #[test]
    fn child_token_narrows_permission() {
        let parent = create_scoped_token(
            "operator",
            TokenScope::Global,
            Permission::Admin,
            None,
        );
        let child = create_child_token(
            &parent,
            None,
            Some(Permission::Session),
        )
        .unwrap();
        assert_eq!(child.permission, Permission::Session);
    }

    #[test]
    fn child_token_cannot_widen_scope() {
        let parent = create_scoped_token(
            "operator",
            TokenScope::Projects(vec!["myapp".into()]),
            Permission::Project,
            None,
        );
        let result = create_child_token(
            &parent,
            Some(TokenScope::Projects(vec!["myapp".into(), "other".into()])),
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn child_token_cannot_escalate_permission() {
        let parent = create_scoped_token(
            "operator",
            TokenScope::Global,
            Permission::Session,
            None,
        );
        let result = create_child_token(
            &parent,
            None,
            Some(Permission::Admin),
        );
        assert!(result.is_err());
    }

    #[test]
    fn child_cannot_widen_to_global() {
        let parent = create_scoped_token(
            "operator",
            TokenScope::Projects(vec!["myapp".into()]),
            Permission::Project,
            None,
        );
        let result = create_child_token(
            &parent,
            Some(TokenScope::Global),
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn scope_includes_project_global() {
        assert!(scope_includes_project(&TokenScope::Global, "anything"));
    }

    #[test]
    fn scope_includes_project_listed() {
        let scope = TokenScope::Projects(vec!["myapp".into(), "lib".into()]);
        assert!(scope_includes_project(&scope, "myapp"));
        assert!(scope_includes_project(&scope, "lib"));
        assert!(!scope_includes_project(&scope, "other"));
    }

    #[test]
    fn permission_allows_checks() {
        assert!(permission_allows(&Permission::Admin, &Permission::Admin));
        assert!(permission_allows(&Permission::Admin, &Permission::Project));
        assert!(permission_allows(&Permission::Admin, &Permission::Session));
        assert!(permission_allows(&Permission::Project, &Permission::Project));
        assert!(permission_allows(&Permission::Project, &Permission::Session));
        assert!(!permission_allows(&Permission::Project, &Permission::Admin));
        assert!(permission_allows(&Permission::Session, &Permission::Session));
        assert!(!permission_allows(&Permission::Session, &Permission::Project));
    }

    #[test]
    fn session_matches_for_session_perm() {
        let token = ScopedToken {
            id: "t1".into(),
            subject: "user".into(),
            scope: TokenScope::Global,
            permission: Permission::Session,
            session_id: Some("sess-1".into()),
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            parent_token_id: None,
        };
        let agent_match = AgentInstanceState {
            session_id: Some("sess-1".into()),
            ..Default::default()
        };
        let agent_no_match = AgentInstanceState {
            session_id: Some("sess-2".into()),
            ..Default::default()
        };
        let agent_no_session = AgentInstanceState {
            session_id: None,
            ..Default::default()
        };
        assert!(session_matches(&token, &agent_match));
        assert!(!session_matches(&token, &agent_no_match));
        assert!(!session_matches(&token, &agent_no_session));
    }

    #[test]
    fn session_matches_non_session_perm_always_true() {
        let token = ScopedToken {
            id: "t1".into(),
            subject: "user".into(),
            scope: TokenScope::Global,
            permission: Permission::Project,
            session_id: None,
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            parent_token_id: None,
        };
        let agent = AgentInstanceState {
            session_id: Some("sess-99".into()),
            ..Default::default()
        };
        assert!(session_matches(&token, &agent));
    }

    #[test]
    fn localhost_admin_token_is_admin_global() {
        let token = localhost_admin_token();
        assert_eq!(token.permission, Permission::Admin);
        assert_eq!(token.scope, TokenScope::Global);
        assert_eq!(token.subject, "localhost");
    }

    #[test]
    fn resolve_token_from_header_works() {
        let token = create_scoped_token(
            "user",
            TokenScope::Global,
            Permission::Admin,
            None,
        );
        let encoded = encode_scoped_token(&token);
        let header = format!("Bearer {encoded}");
        let resolved = resolve_token_from_header(Some(&header)).unwrap();
        assert_eq!(resolved.id, token.id);
    }

    #[test]
    fn resolve_token_from_header_none() {
        assert!(resolve_token_from_header(None).is_none());
        assert!(resolve_token_from_header(Some("garbage")).is_none());
        assert!(resolve_token_from_header(Some("Bearer invalid")).is_none());
    }

    #[test]
    fn get_token_by_id_works() {
        let token = create_scoped_token(
            "user",
            TokenScope::Global,
            Permission::Admin,
            None,
        );
        let found = get_token_by_id(&token.id).unwrap();
        assert_eq!(found.id, token.id);
        assert!(get_token_by_id("nonexistent").is_none());
    }

    #[test]
    fn ensure_session_creates_session() {
        let mut token = create_scoped_token(
            "user",
            TokenScope::Global,
            Permission::Admin,
            None,
        );
        assert!(token.session_id.is_none());
        let sid = ensure_session(&mut token);
        assert!(sid.starts_with("sess-"));
        assert_eq!(token.session_id, Some(sid.clone()));

        // Calling again returns same session
        let sid2 = ensure_session(&mut token);
        assert_eq!(sid, sid2);
    }

    #[test]
    fn scope_subset_checks() {
        // Global is superset of everything
        assert!(scope_is_subset(&TokenScope::Global, &TokenScope::Global));
        assert!(scope_is_subset(
            &TokenScope::Projects(vec!["a".into()]),
            &TokenScope::Global,
        ));

        // Projects cannot widen to global
        assert!(!scope_is_subset(
            &TokenScope::Global,
            &TokenScope::Projects(vec!["a".into()]),
        ));

        // Subset check
        assert!(scope_is_subset(
            &TokenScope::Projects(vec!["a".into()]),
            &TokenScope::Projects(vec!["a".into(), "b".into()]),
        ));
        assert!(!scope_is_subset(
            &TokenScope::Projects(vec!["a".into(), "c".into()]),
            &TokenScope::Projects(vec!["a".into(), "b".into()]),
        ));
    }
}
