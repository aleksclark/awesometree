use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct WorkspaceInfo {
    pub name: String,
    pub project: String,
    pub active: bool,
    pub tag_index: i32,
    pub dir: String,
    pub acp_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct ProjectInfo {
    pub name: String,
    pub repo: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct ProjectDetail {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub version: String,
    pub name: String,
    pub repo: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct CreateWorkspaceReq {
    pub name: String,
    pub project: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct AcpMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct AcpSendRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct AcpConversation {
    pub messages: Vec<AcpMessage>,
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
