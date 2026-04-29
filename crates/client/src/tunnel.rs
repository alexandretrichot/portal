use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use common::{AuthMessage, PongMessage, TunnelMessage};

use crate::config::TunnelServerConfig;
use crate::mcp_proxy::McpProxyManager;

const BASE_DELAY_SECS: u64 = 1;
const MAX_DELAY_SECS: u64 = 60;

pub struct TunnelConnection {
    server_config: TunnelServerConfig,
    device_name: Option<String>,
    mcp_manager: Arc<McpProxyManager>,
}

impl TunnelConnection {
    pub fn new(
        server_config: TunnelServerConfig,
        device_name: Option<String>,
        mcp_manager: Arc<McpProxyManager>,
    ) -> Self {
        Self {
            server_config,
            device_name,
            mcp_manager,
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
        let url = format!("{}/tunnel", self.server_config.url);
        tracing::info!(url = %url, "Connecting to tunnel server");

        let (ws_stream, _) = connect_async(&url).await?;
        let (mut write, mut read) = ws_stream.split();

        let auth_msg = TunnelMessage::Auth(AuthMessage {
            token: self.server_config.token.clone(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            device_name: self.device_name.clone(),
        });

        write
            .send(Message::Text(serde_json::to_string(&auth_msg)?.into()))
            .await?;

        let auth_result = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(TunnelMessage::AuthResult(result)) = serde_json::from_str(&text) {
                            return Some(result);
                        }
                    }
                    Ok(Message::Close(_)) => return None,
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

        let (tx, mut rx) = mpsc::channel::<TunnelMessage>(100);

        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(tunnel_msg) = serde_json::from_str::<TunnelMessage>(&text) {
                                self.handle_message(tunnel_msg, &tx).await;
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            write.send(Message::Pong(data)).await?;
                        }
                        Some(Ok(Message::Close(_))) | None => {
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
                    if let Some(tunnel_msg) = msg {
                        let text = serde_json::to_string(&tunnel_msg)?;
                        write.send(Message::Text(text.into())).await?;
                    }
                }
            }
        }
    }

    async fn handle_message(&self, msg: TunnelMessage, tx: &mpsc::Sender<TunnelMessage>) {
        match msg {
            TunnelMessage::McpRequest(req) => {
                let tx = tx.clone();
                let mcp_manager = self.mcp_manager.clone();

                tokio::spawn(async move {
                    let response = mcp_manager.forward_request(req).await;
                    let _ = tx.send(TunnelMessage::McpResponse(response)).await;
                });
            }
            TunnelMessage::Ping(ping) => {
                let pong = TunnelMessage::Pong(PongMessage {
                    timestamp: ping.timestamp,
                    sequence: ping.sequence,
                    server_timestamp: Utc::now().timestamp() as u64,
                });
                let _ = tx.send(pong).await;
            }
            TunnelMessage::Disconnect(disconnect) => {
                tracing::warn!(
                    reason = ?disconnect.reason,
                    message = ?disconnect.message,
                    "Server requested disconnect"
                );
            }
            _ => {}
        }
    }
}
