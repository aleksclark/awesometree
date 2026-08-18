use std::env;

/// Default Switchboard MCP HTTP endpoint (streamable).
pub const DEFAULT_SWITCHBOARD_URL: &str = "http://127.0.0.1:3847/mcp";

#[derive(Debug, Clone)]
pub struct SwitchboardConfig {
    pub endpoint: String,
    pub auth_token: Option<String>,
}

impl Default for SwitchboardConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl SwitchboardConfig {
    pub fn from_env() -> Self {
        let endpoint = env::var("AWESOMETREE_SWITCHBOARD_URL")
            .or_else(|_| env::var("SWITCHBOARD_URL"))
            .unwrap_or_else(|_| DEFAULT_SWITCHBOARD_URL.into());
        let auth_token = env::var("AWESOMETREE_SWITCHBOARD_TOKEN")
            .or_else(|_| env::var("SWITCHBOARD_TOKEN"))
            .ok()
            .filter(|s| !s.is_empty());
        Self {
            endpoint,
            auth_token,
        }
    }

    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            auth_token: None,
        }
    }
}

pub fn switchboard_endpoint() -> String {
    SwitchboardConfig::from_env().endpoint
}
