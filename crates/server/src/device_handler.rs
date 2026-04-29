use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use common::{AuthResultMessage, PingMessage, TunnelMessage};
use uuid::Uuid;

use crate::registry::ConnectionHandle;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/device", get(device_websocket))
}

#[derive(Deserialize)]
struct DeviceQuery {
    key: String,
}

async fn device_websocket(
    ws: WebSocketUpgrade,
    Query(query): Query<DeviceQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_device_connection(socket, query.key, state))
}

async fn handle_device_connection(socket: WebSocket, device_key: String, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    let device_info = match state.db.get_device_by_key(&device_key).await {
        Ok(Some(info)) => info,
        Ok(None) => {
            tracing::warn!("Invalid device key attempted");
            let fail_msg = TunnelMessage::AuthResult(AuthResultMessage::failure("Invalid device key"));
            let _ = sender
                .send(Message::Text(serde_json::to_string(&fail_msg).unwrap().into()))
                .await;
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "Database error during device auth");
            let fail_msg = TunnelMessage::AuthResult(AuthResultMessage::failure("Internal error"));
            let _ = sender
                .send(Message::Text(serde_json::to_string(&fail_msg).unwrap().into()))
                .await;
            return;
        }
    };

    let auth_timeout = Duration::from_secs(10);
    let auth_result = tokio::time::timeout(auth_timeout, async {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(TunnelMessage::Auth(auth)) = serde_json::from_str(&text) {
                        return Some(auth);
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

    let auth = match auth_result {
        Ok(Some(auth)) => auth,
        _ => {
            let fail_msg = TunnelMessage::AuthResult(AuthResultMessage::failure("Authentication timeout"));
            let _ = sender
                .send(Message::Text(serde_json::to_string(&fail_msg).unwrap().into()))
                .await;
            return;
        }
    };

    let (tx, mut rx) = mpsc::channel::<TunnelMessage>(100);
    let session_id = Uuid::new_v4().to_string();

    let handle = ConnectionHandle {
        session_id: session_id.clone(),
        device_id: device_info.device.id.clone(),
        gateway_id: device_info.device.gateway_id.clone(),
        device_alias: device_info.device.alias.clone(),
        connected_at: Instant::now(),
        connected_at_utc: Utc::now(),
        device_name: auth.device_name.clone(),
        sender: tx,
        last_seen: Arc::new(AtomicU64::new(Utc::now().timestamp() as u64)),
    };

    state
        .registry
        .register(
            &device_key,
            &device_info.device.id,
            &device_info.device.gateway_id,
            &device_info.device.alias,
            handle,
        )
        .await;

    tracing::info!(
        session_id = %session_id,
        device_alias = %device_info.device.alias,
        device_name = ?auth.device_name,
        "Device connected"
    );

    let success_msg = TunnelMessage::AuthResult(AuthResultMessage::success(session_id.clone()));
    if sender
        .send(Message::Text(serde_json::to_string(&success_msg).unwrap().into()))
        .await
        .is_err()
    {
        state.registry.unregister(&device_key);
        return;
    }

    let heartbeat_interval = Duration::from_secs(state.config.heartbeat.interval_secs);
    let mut heartbeat_timer = tokio::time::interval(heartbeat_interval);
    let mut ping_sequence: u64 = 0;

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        state.registry.update_last_seen(&device_key);
                        if let Ok(tunnel_msg) = serde_json::from_str::<TunnelMessage>(&text) {
                            handle_client_message(tunnel_msg, &state).await;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        state.registry.update_last_seen(&device_key);
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!(session_id = %session_id, "Device disconnected");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!(session_id = %session_id, error = %e, "WebSocket error");
                        break;
                    }
                    _ => {}
                }
            }
            msg = rx.recv() => {
                match msg {
                    Some(tunnel_msg) => {
                        let text = serde_json::to_string(&tunnel_msg).unwrap();
                        if sender.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = heartbeat_timer.tick() => {
                ping_sequence += 1;
                let ping = TunnelMessage::Ping(PingMessage {
                    timestamp: Utc::now().timestamp() as u64,
                    sequence: ping_sequence,
                });
                let text = serde_json::to_string(&ping).unwrap();
                if sender.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }

    state.registry.unregister(&device_key);
    tracing::info!(session_id = %session_id, "Connection closed");
}

async fn handle_client_message(msg: TunnelMessage, state: &AppState) {
    match msg {
        TunnelMessage::McpResponse(response) => {
            let correlation_id = response.correlation_id;
            if state.pending.complete(response) {
                tracing::debug!(correlation_id = %correlation_id, "MCP response delivered");
            } else {
                tracing::warn!(correlation_id = %correlation_id, "No pending request for MCP response");
            }
        }
        TunnelMessage::Pong(pong) => {
            tracing::trace!(sequence = pong.sequence, "Received pong");
        }
        TunnelMessage::Disconnect(disconnect) => {
            tracing::info!(reason = ?disconnect.reason, "Client requested disconnect");
        }
        _ => {}
    }
}
