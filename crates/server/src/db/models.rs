use std::collections::HashMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub clerk_user_id: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gateway {
    pub id: String,
    pub user_id: String,
    pub key: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub gateway_id: String,
    pub key: String,
    pub name: String,
    pub alias: String,
    pub mcp_config: McpConfig,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "MCP servers configuration for this device")]
pub struct McpConfig {
    /// Map of server name to configuration
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

/// Configuration for a single MCP server (stdio or http)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
#[schemars(description = "MCP server configuration - either stdio (command) or http (url)")]
pub enum McpServerConfig {
    /// Stdio-based MCP server (spawns a process)
    Stdio {
        /// Command to execute (e.g., "npx", "uvx", "/path/to/binary")
        command: String,
        /// Arguments to pass to the command
        #[serde(default)]
        args: Vec<String>,
        /// Environment variables (e.g., API keys)
        #[serde(default)]
        env: HashMap<String, String>,
        /// Whether this server is enabled
        #[serde(default = "default_true")]
        enabled: bool,
    },
    /// HTTP-based MCP server (connects to URL)
    Http {
        /// URL of the MCP server
        url: String,
        /// HTTP headers (e.g., Authorization)
        #[serde(default)]
        headers: HashMap<String, String>,
        /// Whether this server is enabled
        #[serde(default = "default_true")]
        enabled: bool,
    },
}

impl McpServerConfig {
    pub fn is_enabled(&self) -> bool {
        match self {
            McpServerConfig::Stdio { enabled, .. } => *enabled,
            McpServerConfig::Http { enabled, .. } => *enabled,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceWithGateway {
    pub device: Device,
    pub gateway_key: String,
}
