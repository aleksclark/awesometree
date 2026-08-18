use serde::{Deserialize, Serialize};

/// Stable error codes aligned with Switchboard work-model tool errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    AlreadyExists,
    InvalidInput,
    InvalidReference,
    InvalidTransition,
    Conflict,
    Referenced,
    PolicyBroadening,
    Unavailable,
    InternalError,
    LockTimeout,
    MissingDefaultProfile,
    Unauthorized,
    UnsupportedOldState,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::AlreadyExists => "already_exists",
            Self::InvalidInput => "invalid_input",
            Self::InvalidReference => "invalid_reference",
            Self::InvalidTransition => "invalid_transition",
            Self::Conflict => "conflict",
            Self::Referenced => "referenced",
            Self::PolicyBroadening => "policy_broadening",
            Self::Unavailable => "unavailable",
            Self::InternalError => "internal_error",
            Self::LockTimeout => "lock_timeout",
            Self::MissingDefaultProfile => "missing_default_profile",
            Self::Unauthorized => "unauthorized",
            Self::UnsupportedOldState => "unsupported_old_state",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "not_found" | "project_not_found" => Self::NotFound,
            "already_exists" | "project_already_exists" => Self::AlreadyExists,
            "invalid_input" | "invalid_definition" => Self::InvalidInput,
            "invalid_reference" => Self::InvalidReference,
            "invalid_transition" => Self::InvalidTransition,
            "conflict" | "revision_conflict" => Self::Conflict,
            "referenced" => Self::Referenced,
            "policy_broadening" => Self::PolicyBroadening,
            "unavailable" => Self::Unavailable,
            "lock_timeout" => Self::LockTimeout,
            "missing_default_profile" => Self::MissingDefaultProfile,
            "unauthorized" | "permission_denied" => Self::Unauthorized,
            "unsupported_old_state" => Self::UnsupportedOldState,
            _ => Self::InternalError,
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured error returned across CLI/REST/MCP/gRPC/UI boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct SwitchboardError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    /// Safe human cause (never bearer tokens).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl SwitchboardError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            operation: None,
            entity_kind: None,
            entity_id: None,
            project_id: None,
            work_profile_id: None,
            work_session_id: None,
            expected: None,
            current: None,
            cause: None,
        }
    }

    pub fn with_operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }

    pub fn with_entity(mut self, kind: impl Into<String>, id: impl Into<String>) -> Self {
        self.entity_kind = Some(kind.into());
        self.entity_id = Some(id.into());
        self
    }

    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }

    pub fn unavailable(op: &str, cause: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::Unavailable,
            format!("Switchboard unavailable for {op}"),
        )
        .with_operation(op)
        .with_cause(cause)
    }

    pub fn missing_default() -> Self {
        Self::new(
            ErrorCode::MissingDefaultProfile,
            "WorkProfile with work_profile_id exactly \"default\" is missing in Switchboard; create it or pass an explicit work_profile_id",
        )
        .with_entity("work_profile", "default")
        .with_operation("resolve_default_work_profile")
    }

    pub fn from_switchboard_body(body: &serde_json::Value, operation: &str) -> Self {
        let err = body.get("error").unwrap_or(body);
        let code = err
            .get("code")
            .and_then(|c| c.as_str())
            .map(ErrorCode::parse)
            .unwrap_or(ErrorCode::InternalError);
        let message = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("switchboard error")
            .to_string();
        let mut e = Self::new(code, message).with_operation(operation);
        if let Some(k) = err.get("entity_kind").and_then(|v| v.as_str()) {
            e.entity_kind = Some(k.into());
        }
        if let Some(id) = err.get("entity_id").and_then(|v| v.as_str()) {
            e.entity_id = Some(id.into());
        }
        if let Some(id) = err.get("project_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            e.project_id = Some(id.into());
        }
        if let Some(id) = err
            .get("projectId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            e.project_id = Some(id.into());
        }
        if let Some(id) = err
            .get("work_profile_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            e.work_profile_id = Some(id.into());
        }
        if let Some(id) = err
            .get("work_session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            e.work_session_id = Some(id.into());
        }
        if let Some(v) = err.get("expected").and_then(|v| v.as_str()) {
            e.expected = Some(v.into());
        }
        if let Some(v) = err
            .get("expectedSourceRevision")
            .and_then(|v| v.as_str())
        {
            e.expected = Some(v.into());
        }
        if let Some(v) = err.get("current").and_then(|v| v.as_str()) {
            e.current = Some(v.into());
        }
        if let Some(v) = err
            .get("currentSourceRevision")
            .and_then(|v| v.as_str())
        {
            e.current = Some(v.into());
        }
        e
    }
}

pub type Result<T> = std::result::Result<T, SwitchboardError>;
