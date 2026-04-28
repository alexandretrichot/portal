use std::path::Path;

use anyhow::Result;
use serde::Deserialize;
use serde_json;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default)]
    pub server: ServerSection,
    #[serde(default)]
    pub tokens: TokensSection,
    #[serde(default)]
    pub limits: LimitsSection,
    #[serde(default)]
    pub heartbeat: HeartbeatSection,
    #[serde(default)]
    pub offline_response: OfflineResponseSection,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            server: ServerSection::default(),
            tokens: TokensSection::default(),
            limits: LimitsSection::default(),
            heartbeat: HeartbeatSection::default(),
            offline_response: OfflineResponseSection::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerSection {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_ws_path")]
    pub ws_path: String,
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            ws_path: default_ws_path(),
        }
    }
}

fn default_bind() -> String {
    "0.0.0.0:8080".into()
}

fn default_ws_path() -> String {
    "/tunnel".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokensSection {
    #[serde(default = "default_token_backend")]
    pub backend: String,
    pub file: Option<String>,
}

impl Default for TokensSection {
    fn default() -> Self {
        Self {
            backend: default_token_backend(),
            file: None,
        }
    }
}

fn default_token_backend() -> String {
    "allow_all".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct LimitsSection {
    #[serde(default = "default_max_connections_per_token")]
    pub max_connections_per_token: u32,
    #[serde(default = "default_max_total_connections")]
    pub max_total_connections: u32,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
}

impl Default for LimitsSection {
    fn default() -> Self {
        Self {
            max_connections_per_token: default_max_connections_per_token(),
            max_total_connections: default_max_total_connections(),
            request_timeout_secs: default_request_timeout_secs(),
        }
    }
}

fn default_max_connections_per_token() -> u32 {
    1
}

fn default_max_total_connections() -> u32 {
    10000
}

fn default_request_timeout_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatSection {
    #[serde(default = "default_heartbeat_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_heartbeat_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_missed")]
    pub max_missed: u32,
}

impl Default for HeartbeatSection {
    fn default() -> Self {
        Self {
            interval_secs: default_heartbeat_interval(),
            timeout_secs: default_heartbeat_timeout(),
            max_missed: default_max_missed(),
        }
    }
}

fn default_heartbeat_interval() -> u64 {
    30
}

fn default_heartbeat_timeout() -> u64 {
    10
}

fn default_max_missed() -> u32 {
    3
}

#[derive(Debug, Clone, Deserialize)]
pub struct OfflineResponseSection {
    #[serde(default = "default_offline_message")]
    pub message: String,
    #[serde(default = "default_retry_after")]
    pub retry_after_secs: u32,
}

impl Default for OfflineResponseSection {
    fn default() -> Self {
        Self {
            message: default_offline_message(),
            retry_after_secs: default_retry_after(),
        }
    }
}

fn default_offline_message() -> String {
    "The device is currently offline. Please try again later.".into()
}

fn default_retry_after() -> u32 {
    30
}

pub fn load_config(path: &str) -> Result<ServerConfig> {
    let path = Path::new(path);
    if !path.exists() {
        return Ok(ServerConfig::default());
    }
    let content = std::fs::read_to_string(path)?;
    let config: ServerConfig = serde_json::from_str(&content)?;
    Ok(config)
}
