use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelError {
    pub code: i32,
    pub message: String,
}

impl TunnelError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(-32603, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(-32000, message)
    }

    pub fn device_offline(message: impl Into<String>) -> Self {
        Self::new(-32001, message)
    }
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("MCP server error: {0}")]
    McpServer(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Invalid token")]
    InvalidToken,

    #[error("Device offline: {0}")]
    DeviceOffline(String),

    #[error("Request timeout")]
    RequestTimeout,

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Internal error: {0}")]
    Internal(String),
}
