use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::TunnelError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TunnelMessage {
    // Auth
    Auth(AuthMessage),
    AuthResult(AuthResultMessage),

    // MCP proxying
    McpRequest(McpRequestMessage),
    McpResponse(McpResponseMessage),

    // Keepalive
    Ping(PingMessage),
    Pong(PongMessage),

    // Lifecycle
    Disconnect(DisconnectMessage),

    // Server → Agent: configuration & commands
    Config(ConfigMessage),
    Command(CommandMessage),

    // Agent → Server: status & logs
    Status(StatusMessage),
    Log(LogMessage),
}

/// Server sends this after successful auth with MCP server configs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigMessage {
    pub mcp_servers: HashMap<String, crate::commands::McpServerConfig>,
}

/// Server sends commands to agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMessage {
    pub id: Uuid,
    pub command: AgentCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentCommand {
    /// Open system settings for a permission
    OpenSettings { permission: String },
    /// Restart the agent process
    Restart,
    /// Reload MCP servers with new config
    ReloadConfig,
    /// Restart a specific MCP server
    RestartMcpServer { name: String },
}

/// Agent sends status updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMessage {
    pub diagnostics: Option<crate::commands::DiagnosticsResponse>,
    pub mcp_servers: HashMap<String, crate::commands::McpServerStatus>,
}

/// Agent streams logs to server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMessage {
    pub token: String,
    pub client_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResultMessage {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl AuthResultMessage {
    pub fn success(session_id: String) -> Self {
        Self {
            success: true,
            error: None,
            session_id: Some(session_id),
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(error.into()),
            session_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequestMessage {
    pub correlation_id: Uuid,
    pub payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponseMessage {
    pub correlation_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TunnelError>,
}

impl McpResponseMessage {
    pub fn success(correlation_id: Uuid, payload: serde_json::Value) -> Self {
        Self {
            correlation_id,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn error(correlation_id: Uuid, error: TunnelError) -> Self {
        Self {
            correlation_id,
            payload: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingMessage {
    pub timestamp: u64,
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongMessage {
    pub timestamp: u64,
    pub sequence: u64,
    pub server_timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectMessage {
    pub reason: DisconnectReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisconnectReason {
    AuthFailed,
    TokenRevoked,
    ServerShutdown,
    ProtocolError,
    Timeout,
    ClientRequest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_message_serialization() {
        let msg = TunnelMessage::Auth(AuthMessage {
            token: "test_token".into(),
            client_version: "0.1.0".into(),
            device_name: Some("MacBook".into()),
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"auth\""));

        let parsed: TunnelMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            TunnelMessage::Auth(auth) => {
                assert_eq!(auth.token, "test_token");
                assert_eq!(auth.device_name, Some("MacBook".into()));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_mcp_request_serialization() {
        let msg = TunnelMessage::McpRequest(McpRequestMessage {
            correlation_id: Uuid::nil(),
            payload: serde_json::json!({"jsonrpc": "2.0", "method": "tools/list"}),
            target_server: None,
            timeout_ms: Some(30000),
        });

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: TunnelMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            TunnelMessage::McpRequest(req) => {
                assert_eq!(req.timeout_ms, Some(30000));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_disconnect_reason_serialization() {
        let msg = TunnelMessage::Disconnect(DisconnectMessage {
            reason: DisconnectReason::TokenRevoked,
            message: Some("Token has been revoked".into()),
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"reason\":\"token_revoked\""));
    }
}
