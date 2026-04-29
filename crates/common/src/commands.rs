//! Portal commands and events
//!
//! Commands are sync request/response pairs.
//! Events are fire-and-forget notifications.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::transport::{Command, Event};

// =============================================================================
// Server → Agent Commands
// =============================================================================

/// Get agent diagnostics (permissions, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDiagnostics;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsResponse {
    pub os: String,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub name: String,
    pub status: PermissionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Denied,
    Unknown,
}

impl Command for GetDiagnostics {
    type Response = DiagnosticsResponse;
    const NAME: &'static str = "get_diagnostics";
}

/// Open system settings for a permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSettings {
    pub permission: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSettingsResponse {
    pub success: bool,
}

impl Command for OpenSettings {
    type Response = OpenSettingsResponse;
    const NAME: &'static str = "open_settings";
}

/// Get MCP servers status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMcpStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStatusResponse {
    pub servers: HashMap<String, McpServerStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default)]
    pub tools_count: usize,
    #[serde(default)]
    pub logs: Vec<String>,
}

impl Command for GetMcpStatus {
    type Response = McpStatusResponse;
    const NAME: &'static str = "get_mcp_status";
}

/// Update MCP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMcpConfig {
    pub servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMcpConfigResponse {
    pub success: bool,
}

impl Command for UpdateMcpConfig {
    type Response = UpdateMcpConfigResponse;
    const NAME: &'static str = "update_mcp_config";
}

/// Restart the agent process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartAgent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartAgentResponse {
    pub success: bool,
}

impl Command for RestartAgent {
    type Response = RestartAgentResponse;
    const NAME: &'static str = "restart_agent";
}

/// Restart a specific MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartMcpServer {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartMcpServerResponse {
    pub success: bool,
}

impl Command for RestartMcpServer {
    type Response = RestartMcpServerResponse;
    const NAME: &'static str = "restart_mcp_server";
}

// =============================================================================
// Agent → Server Commands (MCP proxying)
// =============================================================================

/// Forward an MCP request to a specific server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_server: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub payload: serde_json::Value,
}

impl Command for McpRequest {
    type Response = McpResponse;
    const NAME: &'static str = "mcp_request";
}

// =============================================================================
// Agent → Server Events
// =============================================================================

/// Log event from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub level: LogLevel,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl Event for LogEvent {
    const NAME: &'static str = "log";
}

/// MCP server status changed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatusChanged {
    pub name: String,
    pub status: McpServerStatus,
}

impl Event for McpServerStatusChanged {
    const NAME: &'static str = "mcp_server_status_changed";
}

/// Agent ready (sent after successful auth and config applied)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReady {
    pub mcp_servers: Vec<String>,
}

impl Event for AgentReady {
    const NAME: &'static str = "agent_ready";
}
