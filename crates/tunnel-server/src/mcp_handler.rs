use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde_json::json;
use tokio::time::timeout;
use tunnel_common::{McpRequestMessage, TunnelMessage};
use uuid::Uuid;

use crate::offline::OfflineResponse;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/{token}/mcp", post(handle_mcp_request))
        .route("/v1/{token}/status", get(check_device_status))
        .route("/health", get(health_check))
}

async fn handle_mcp_request(
    State(state): State<AppState>,
    Path(token): Path<String>,
    body: String,
) -> impl IntoResponse {
    let payload: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                })
                .to_string(),
            )
                .into_response();
        }
    };

    let request_id = payload.get("id").cloned();
    let correlation_id = Uuid::new_v4();

    let handle = match state.registry.get(&token) {
        Some(h) => h,
        None => {
            let device_info = state.registry.get_device_info(&token);
            return OfflineResponse::new(
                request_id,
                device_info.as_ref(),
                state.config.offline_response.retry_after_secs,
            )
            .into_response();
        }
    };

    let mcp_request = TunnelMessage::McpRequest(McpRequestMessage {
        correlation_id,
        payload,
        target_server: None,
        timeout_ms: Some(state.config.limits.request_timeout_secs * 1000),
    });

    if handle.sender.send(mcp_request).await.is_err() {
        let device_info = state.registry.get_device_info(&token);
        return OfflineResponse::new(
            request_id,
            device_info.as_ref(),
            state.config.offline_response.retry_after_secs,
        )
        .into_response();
    }

    let timeout_duration = Duration::from_secs(state.config.limits.request_timeout_secs);

    let response_rx = state.pending.register(correlation_id);

    match timeout(timeout_duration, response_rx).await {
        Ok(Ok(response)) => {
            if let Some(error) = response.error {
                (
                    StatusCode::OK,
                    json!({
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "error": {
                            "code": error.code,
                            "message": error.message
                        }
                    })
                    .to_string(),
                )
                    .into_response()
            } else if let Some(payload) = response.payload {
                (StatusCode::OK, payload.to_string()).into_response()
            } else {
                (
                    StatusCode::OK,
                    json!({
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "result": null
                    })
                    .to_string(),
                )
                    .into_response()
            }
        }
        Ok(Err(_)) => {
            let device_info = state.registry.get_device_info(&token);
            (
                StatusCode::GATEWAY_TIMEOUT,
                crate::offline::create_timeout_response(request_id, device_info.as_ref())
                    .to_string(),
            )
                .into_response()
        }
        Err(_) => {
            let device_info = state.registry.get_device_info(&token);
            (
                StatusCode::GATEWAY_TIMEOUT,
                crate::offline::create_timeout_response(request_id, device_info.as_ref())
                    .to_string(),
            )
                .into_response()
        }
    }
}

async fn check_device_status(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let is_online = state.registry.is_online(&token);
    let device_info = state.registry.get_device_info(&token);

    let status = if is_online { "online" } else { "offline" };

    (
        StatusCode::OK,
        json!({
            "status": status,
            "device_name": device_info.as_ref().and_then(|d| d.name.clone()),
            "last_seen": device_info.as_ref().map(|d| d.last_seen.to_rfc3339())
        })
        .to_string(),
    )
        .into_response()
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, json!({"status": "ok"}).to_string())
}
