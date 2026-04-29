use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    #[serde(default)]
    pub device_name: Option<String>,

    #[serde(default)]
    pub server: Option<ServerConfig>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            server: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub url: String,
    pub key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl McpServerConfig {
    pub fn is_stdio(&self) -> bool {
        self.command.is_some()
    }
}

pub fn load_config(path: &Path) -> Result<AgentConfig> {
    if !path.exists() {
        return Ok(AgentConfig::default());
    }
    let content = std::fs::read_to_string(path)?;
    let config: AgentConfig = serde_json::from_str(&content)?;
    Ok(config)
}
