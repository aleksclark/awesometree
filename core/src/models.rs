use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct WorkSessionInfo {
    pub work_session_id: String,
    pub project_id: Option<String>,
    pub work_profile_id: Option<String>,
    pub state: String,
    pub project_revision: Option<String>,
    pub project_snapshot_id: Option<String>,
    pub display_name: Option<String>,
    pub dir: Option<String>,
    pub realization_status: Option<String>,
    pub headless: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct WorkProfileInfo {
    pub work_profile_id: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct ProjectInfo {
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    pub revision: Option<String>,
    pub source_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct ProjectDetail {
    pub project_id: String,
    pub revision: String,
    pub source_revision: String,
    /// JSON string of the definition object.
    pub definition_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct CreateWorkSessionReq {
    pub work_session_id: String,
    pub project_id: String,
    /// Empty string means omit (resolve exact ID "default").
    pub work_profile_id: String,
    pub display_name: String,
    pub headless: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct CreateProjectReq {
    pub project_id: String,
    pub description: String,
    pub repo: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct ServerConnection {
    pub host: String,
    pub port: u16,
    pub token: String,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ApiError {
    #[error("Network error: {message}")]
    Network { message: String },
    #[error("Server error ({status}): {message}")]
    Server { status: u16, message: String },
    #[error("Parse error: {message}")]
    Parse { message: String },
    #[error("Authentication failed")]
    AuthFailed,
}
