use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    #[serde(default)]
    pub device_name: Option<String>,

    #[serde(default)]
    pub tunnel_server: Option<TunnelServerConfig>,

    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            tunnel_server: None,
            mcp_servers: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelServerConfig {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub url: Option<String>,
}

impl McpServerConfig {
    pub fn is_stdio(&self) -> bool {
        self.command.is_some()
    }

    pub fn is_http(&self) -> bool {
        self.url.is_some()
    }
}

pub fn load_config(path: &str) -> Result<ClientConfig> {
    let path = Path::new(path);
    if !path.exists() {
        return Ok(ClientConfig::default());
    }
    let content = std::fs::read_to_string(path)?;
    let config: ClientConfig = serde_json::from_str(&content)?;
    Ok(config)
}
