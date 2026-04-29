use std::collections::HashMap;
use std::sync::Arc;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, Mutex};
use uuid::Uuid;

use crate::commands::*;

/// Wire format for all messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// Sync command request (expects response)
    Request {
        id: Uuid,
        name: String,
        payload: serde_json::Value,
    },
    /// Response to a request
    Response {
        id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Fire-and-forget event
    Event {
        name: String,
        payload: serde_json::Value,
    },
}

/// Trait for sync commands (request/response)
pub trait Command: Serialize + DeserializeOwned + Send + 'static {
    type Response: Serialize + DeserializeOwned + Send + 'static;
    const NAME: &'static str;
}

/// Trait for fire-and-forget events
pub trait Event: Serialize + DeserializeOwned + Send + 'static {
    const NAME: &'static str;
}

/// Error type for command handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandError {
    pub message: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

/// Result type for command handlers
pub type CommandResult<T> = Result<T, CommandError>;

// =============================================================================
// Handler Traits (compile-time safety)
// =============================================================================

/// Commands handled by the Agent (sent by Server)
#[allow(async_fn_in_trait)]
pub trait AgentCommands: Send + Sync + 'static {
    async fn get_diagnostics(&self) -> CommandResult<DiagnosticsResponse>;
    async fn open_settings(&self, cmd: OpenSettings) -> CommandResult<OpenSettingsResponse>;
    async fn get_mcp_status(&self) -> CommandResult<McpStatusResponse>;
    async fn update_mcp_config(&self, cmd: UpdateMcpConfig) -> CommandResult<UpdateMcpConfigResponse>;
    async fn restart_agent(&self) -> CommandResult<RestartAgentResponse>;
    async fn restart_mcp_server(&self, cmd: RestartMcpServer) -> CommandResult<RestartMcpServerResponse>;
}

/// Commands handled by the Server (sent by Agent)
#[allow(async_fn_in_trait)]
pub trait ServerCommands: Send + Sync + 'static {
    async fn mcp_request(&self, cmd: McpRequest) -> CommandResult<McpResponse>;
}

/// Events handled by the Server (sent by Agent)
#[allow(async_fn_in_trait)]
pub trait ServerEvents: Send + Sync + 'static {
    async fn on_log(&self, event: LogEvent);
    async fn on_mcp_server_status_changed(&self, event: McpServerStatusChanged);
    async fn on_agent_ready(&self, event: AgentReady);
}

// =============================================================================
// Dispatcher
// =============================================================================

/// Dispatches incoming messages to the appropriate handler
pub struct Dispatcher<H> {
    handler: Arc<H>,
}

impl<H> Clone for Dispatcher<H> {
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
        }
    }
}

impl<H: AgentCommands> Dispatcher<H> {
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
        }
    }

    pub async fn dispatch_agent_command(&self, name: &str, payload: serde_json::Value) -> Result<serde_json::Value, CommandError> {
        match name {
            GetDiagnostics::NAME => {
                let response = self.handler.get_diagnostics().await?;
                Ok(serde_json::to_value(response).unwrap())
            }
            OpenSettings::NAME => {
                let cmd: OpenSettings = serde_json::from_value(payload)?;
                let response = self.handler.open_settings(cmd).await?;
                Ok(serde_json::to_value(response).unwrap())
            }
            GetMcpStatus::NAME => {
                let response = self.handler.get_mcp_status().await?;
                Ok(serde_json::to_value(response).unwrap())
            }
            UpdateMcpConfig::NAME => {
                let cmd: UpdateMcpConfig = serde_json::from_value(payload)?;
                let response = self.handler.update_mcp_config(cmd).await?;
                Ok(serde_json::to_value(response).unwrap())
            }
            RestartAgent::NAME => {
                let response = self.handler.restart_agent().await?;
                Ok(serde_json::to_value(response).unwrap())
            }
            RestartMcpServer::NAME => {
                let cmd: RestartMcpServer = serde_json::from_value(payload)?;
                let response = self.handler.restart_mcp_server(cmd).await?;
                Ok(serde_json::to_value(response).unwrap())
            }
            _ => Err(CommandError::from(format!("unknown command: {}", name))),
        }
    }

    pub async fn handle(&self, msg: Message) -> Option<Message> {
        match msg {
            Message::Request { id, name, payload } => {
                let result = self.dispatch_agent_command(&name, payload).await;
                Some(match result {
                    Ok(payload) => Message::Response {
                        id,
                        payload: Some(payload),
                        error: None,
                    },
                    Err(e) => Message::Response {
                        id,
                        payload: None,
                        error: Some(e.message),
                    },
                })
            }
            Message::Event { .. } => None,
            Message::Response { .. } => None,
        }
    }
}

/// Server-side dispatcher
pub struct ServerDispatcher<H> {
    handler: Arc<H>,
}

impl<H> Clone for ServerDispatcher<H> {
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
        }
    }
}

impl<H: ServerCommands + ServerEvents> ServerDispatcher<H> {
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
        }
    }

    pub async fn dispatch_command(&self, name: &str, payload: serde_json::Value) -> Result<serde_json::Value, CommandError> {
        match name {
            McpRequest::NAME => {
                let cmd: McpRequest = serde_json::from_value(payload)?;
                let response = self.handler.mcp_request(cmd).await?;
                Ok(serde_json::to_value(response).unwrap())
            }
            _ => Err(CommandError::from(format!("unknown command: {}", name))),
        }
    }

    pub async fn dispatch_event(&self, name: &str, payload: serde_json::Value) {
        match name {
            LogEvent::NAME => {
                if let Ok(event) = serde_json::from_value::<LogEvent>(payload) {
                    self.handler.on_log(event).await;
                }
            }
            McpServerStatusChanged::NAME => {
                if let Ok(event) = serde_json::from_value::<McpServerStatusChanged>(payload) {
                    self.handler.on_mcp_server_status_changed(event).await;
                }
            }
            AgentReady::NAME => {
                if let Ok(event) = serde_json::from_value::<AgentReady>(payload) {
                    self.handler.on_agent_ready(event).await;
                }
            }
            _ => {}
        }
    }

    pub async fn handle(&self, msg: Message) -> Option<Message> {
        match msg {
            Message::Request { id, name, payload } => {
                let result = self.dispatch_command(&name, payload).await;
                Some(match result {
                    Ok(payload) => Message::Response {
                        id,
                        payload: Some(payload),
                        error: None,
                    },
                    Err(e) => Message::Response {
                        id,
                        payload: None,
                        error: Some(e.message),
                    },
                })
            }
            Message::Event { name, payload } => {
                self.dispatch_event(&name, payload).await;
                None
            }
            Message::Response { .. } => None,
        }
    }
}

// =============================================================================
// Pending Requests (for sync commands)
// =============================================================================

/// Tracks pending requests waiting for responses
pub struct PendingRequests {
    pending: Mutex<HashMap<Uuid, oneshot::Sender<Result<serde_json::Value, CommandError>>>>,
}

impl Default for PendingRequests {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingRequests {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub async fn register(&self, id: Uuid) -> oneshot::Receiver<Result<serde_json::Value, CommandError>> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        rx
    }

    pub async fn complete(&self, msg: &Message) -> bool {
        if let Message::Response { id, payload, error } = msg {
            if let Some(tx) = self.pending.lock().await.remove(id) {
                let result = match (payload, error) {
                    (Some(p), _) => Ok(p.clone()),
                    (None, Some(e)) => Err(CommandError::from(e.clone())),
                    (None, None) => Ok(serde_json::Value::Null),
                };
                let _ = tx.send(result);
                return true;
            }
        }
        false
    }
}

// =============================================================================
// Sender (for sending commands/events)
// =============================================================================

/// Connection handle for sending commands and events
#[derive(Clone)]
pub struct Sender {
    tx: mpsc::Sender<Message>,
    pending: Arc<PendingRequests>,
}

impl Sender {
    pub fn new(tx: mpsc::Sender<Message>, pending: Arc<PendingRequests>) -> Self {
        Self { tx, pending }
    }

    /// Send a command and wait for response
    pub async fn command<C: Command>(&self, cmd: C) -> CommandResult<C::Response> {
        let id = Uuid::new_v4();
        let payload = serde_json::to_value(&cmd)?;

        let rx = self.pending.register(id).await;

        self.tx
            .send(Message::Request {
                id,
                name: C::NAME.to_string(),
                payload,
            })
            .await
            .map_err(|_| CommandError::from("send failed"))?;

        let result = rx.await.map_err(|_| CommandError::from("response channel closed"))??;

        Ok(serde_json::from_value(result)?)
    }

    /// Send an event (fire-and-forget)
    pub async fn event<E: Event>(&self, event: E) -> Result<(), CommandError> {
        let payload = serde_json::to_value(&event)?;

        self.tx
            .send(Message::Event {
                name: E::NAME.to_string(),
                payload,
            })
            .await
            .map_err(|_| CommandError::from("send failed"))
    }

    /// Send raw message
    pub async fn send(&self, msg: Message) -> Result<(), CommandError> {
        self.tx
            .send(msg)
            .await
            .map_err(|_| CommandError::from("send failed"))
    }
}

// =============================================================================
// Serde error conversion
// =============================================================================

impl From<serde_json::Error> for CommandError {
    fn from(e: serde_json::Error) -> Self {
        Self {
            message: format!("json error: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAgent;

    impl AgentCommands for TestAgent {
        async fn get_diagnostics(&self) -> CommandResult<DiagnosticsResponse> {
            Ok(DiagnosticsResponse {
                os: "test".into(),
                permissions: vec![],
            })
        }

        async fn open_settings(&self, _cmd: OpenSettings) -> CommandResult<OpenSettingsResponse> {
            Ok(OpenSettingsResponse { success: true })
        }

        async fn get_mcp_status(&self) -> CommandResult<McpStatusResponse> {
            Ok(McpStatusResponse {
                servers: HashMap::new(),
            })
        }

        async fn update_mcp_config(&self, _cmd: UpdateMcpConfig) -> CommandResult<UpdateMcpConfigResponse> {
            Ok(UpdateMcpConfigResponse { success: true })
        }

        async fn restart_agent(&self) -> CommandResult<RestartAgentResponse> {
            Ok(RestartAgentResponse { success: true })
        }

        async fn restart_mcp_server(&self, _cmd: RestartMcpServer) -> CommandResult<RestartMcpServerResponse> {
            Ok(RestartMcpServerResponse { success: true })
        }
    }

    #[tokio::test]
    async fn test_dispatcher() {
        let dispatcher = Dispatcher::new(TestAgent);

        let request = Message::Request {
            id: Uuid::new_v4(),
            name: "get_diagnostics".to_string(),
            payload: serde_json::json!({}),
        };

        let response = dispatcher.handle(request).await.unwrap();
        if let Message::Response { payload, error, .. } = response {
            assert!(error.is_none());
            let diag: DiagnosticsResponse = serde_json::from_value(payload.unwrap()).unwrap();
            assert_eq!(diag.os, "test");
        } else {
            panic!("expected response");
        }
    }
}
