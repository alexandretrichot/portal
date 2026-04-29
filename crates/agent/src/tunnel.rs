use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

use common::commands::{AgentReady, LogEvent, LogLevel, UpdateMcpConfig};
use common::{AuthMessage, Dispatcher, Message, PendingRequests, PongMessage, Sender, TunnelMessage};

use crate::config::ServerConfig;
use crate::handler::AgentHandler;

const BASE_DELAY_SECS: u64 = 1;
const MAX_DELAY_SECS: u64 = 60;

pub struct TunnelConnection {
    server_config: ServerConfig,
    device_name: Option<String>,
}

impl TunnelConnection {
    pub fn new(server_config: ServerConfig, device_name: Option<String>) -> Self {
        Self {
            server_config,
            device_name,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let mut attempt: u32 = 0;

        loop {
            match self.connect_and_run().await {
                Ok(()) => {
                    tracing::info!("Connection closed cleanly");
                    break;
                }
                Err(e) => {
                    attempt += 1;
                    let delay = self.calculate_backoff(attempt);
                    tracing::warn!(
                        attempt,
                        delay_secs = delay.as_secs(),
                        error = %e,
                        "Connection failed, reconnecting..."
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }

        Ok(())
    }

    fn calculate_backoff(&self, attempt: u32) -> Duration {
        let exponential = BASE_DELAY_SECS.saturating_mul(2u64.saturating_pow(attempt.min(10)));
        let capped = exponential.min(MAX_DELAY_SECS);
        let jitter = rand::random::<f64>() * 0.3;
        Duration::from_secs_f64(capped as f64 * (1.0 + jitter))
    }

    async fn connect_and_run(&self) -> Result<()> {
        let url = format!(
            "{}/device?key={}",
            self.server_config.url, self.server_config.key
        );
        tracing::info!(server = %self.server_config.url, "Connecting to Portal server");

        let (ws_stream, _) = connect_async(&url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Send auth message (legacy protocol for initial handshake)
        let auth_msg = TunnelMessage::Auth(AuthMessage {
            token: self.server_config.key.clone(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            device_name: self.device_name.clone(),
        });

        write
            .send(WsMessage::Text(serde_json::to_string(&auth_msg)?.into()))
            .await?;

        // Wait for auth result
        let auth_result = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(WsMessage::Text(text)) => {
                        if let Ok(TunnelMessage::AuthResult(result)) = serde_json::from_str(&text) {
                            return Some(result);
                        }
                    }
                    Ok(WsMessage::Close(_)) => return None,
                    Err(_) => return None,
                    _ => continue,
                }
            }
            None
        })
        .await;

        let auth_result = match auth_result {
            Ok(Some(r)) => r,
            Ok(None) => return Err(anyhow::anyhow!("Connection closed during auth")),
            Err(_) => return Err(anyhow::anyhow!("Auth timeout")),
        };

        if !auth_result.success {
            return Err(anyhow::anyhow!(
                "Auth failed: {}",
                auth_result.error.unwrap_or_default()
            ));
        }

        let session_id = auth_result.session_id.unwrap_or_default();
        tracing::info!(session_id = %session_id, "Connected and authenticated");

        // Setup transport
        let (tx, mut rx) = mpsc::channel::<Message>(100);
        let pending = Arc::new(PendingRequests::new());
        let sender = Sender::new(tx, pending.clone());

        // Create handler and dispatcher
        let handler = AgentHandler::new().with_sender(sender.clone());
        let dispatcher = Dispatcher::new(handler);

        // Main event loop
        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(WsMessage::Text(text))) => {
                            // Try new transport protocol first
                            if let Ok(transport_msg) = serde_json::from_str::<Message>(&text) {
                                // Check if it's a response to a pending request
                                if !pending.complete(&transport_msg).await {
                                    // Not a response, dispatch to handler
                                    if let Some(response) = dispatcher.handle(transport_msg).await {
                                        let text = serde_json::to_string(&response)?;
                                        write.send(WsMessage::Text(text.into())).await?;
                                    }
                                }
                            }
                            // Try legacy protocol
                            else if let Ok(tunnel_msg) = serde_json::from_str::<TunnelMessage>(&text) {
                                self.handle_legacy_message(tunnel_msg, &mut write, &dispatcher).await?;
                            }
                        }
                        Some(Ok(WsMessage::Ping(data))) => {
                            write.send(WsMessage::Pong(data)).await?;
                        }
                        Some(Ok(WsMessage::Close(_))) | None => {
                            tracing::info!("Server closed connection");
                            return Ok(());
                        }
                        Some(Err(e)) => {
                            return Err(e.into());
                        }
                        _ => {}
                    }
                }
                msg = rx.recv() => {
                    if let Some(transport_msg) = msg {
                        let text = serde_json::to_string(&transport_msg)?;
                        write.send(WsMessage::Text(text.into())).await?;
                    }
                }
            }
        }
    }

    async fn handle_legacy_message<S>(
        &self,
        msg: TunnelMessage,
        write: &mut S,
        dispatcher: &Dispatcher<AgentHandler>,
    ) -> Result<()>
    where
        S: SinkExt<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        match msg {
            TunnelMessage::Config(config) => {
                tracing::info!(servers = ?config.mcp_servers.keys().collect::<Vec<_>>(), "Received MCP config");

                // Convert to UpdateMcpConfig command and dispatch
                let cmd = UpdateMcpConfig {
                    servers: config.mcp_servers,
                };
                let payload = serde_json::to_value(&cmd)?;
                let _ = dispatcher
                    .dispatch_agent_command("update_mcp_config", payload)
                    .await;

                // Send AgentReady event
                let ready = AgentReady {
                    mcp_servers: vec![],
                };
                let event = Message::Event {
                    name: "agent_ready".to_string(),
                    payload: serde_json::to_value(&ready)?,
                };
                let text = serde_json::to_string(&event)?;
                write.send(WsMessage::Text(text.into())).await?;
            }
            TunnelMessage::Ping(ping) => {
                let pong = TunnelMessage::Pong(PongMessage {
                    timestamp: ping.timestamp,
                    sequence: ping.sequence,
                    server_timestamp: Utc::now().timestamp() as u64,
                });
                let text = serde_json::to_string(&pong)?;
                write.send(WsMessage::Text(text.into())).await?;
            }
            TunnelMessage::Disconnect(disconnect) => {
                tracing::warn!(
                    reason = ?disconnect.reason,
                    message = ?disconnect.message,
                    "Server requested disconnect"
                );
            }
            TunnelMessage::McpRequest(req) => {
                // TODO: Handle MCP requests via the handler
                tracing::debug!(correlation_id = %req.correlation_id, "MCP request received");
            }
            _ => {}
        }
        Ok(())
    }
}
