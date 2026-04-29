use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{any, get};
use axum::Router;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::json;
use tower::Service as TowerService;

use crate::mcp_proxy::TunnelProxyServer;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/gw/{gateway_key}/mcp", any(handle_gateway_mcp))
        .route("/gw/{gateway_key}/status", get(check_gateway_status))
        .route("/health", get(health_check))
}

async fn handle_gateway_mcp(
    State(state): State<AppState>,
    Path(gateway_key): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    let gateway = match state.db.get_gateway_by_key(&gateway_key).await {
        Ok(Some(gw)) => gw,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                json!({"error": "Gateway not found"}).to_string(),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "Database error");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "Internal error"}).to_string(),
            )
                .into_response();
        }
    };

    let registry = state.registry.clone();
    let pending = state.pending.clone();
    let gateway_id = gateway.id.clone();

    let config = StreamableHttpServerConfig::default()
        .with_stateful_mode(false)
        .with_json_response(true)
        .disable_allowed_hosts();

    let session_manager = Arc::new(LocalSessionManager::default());
    let mut service = StreamableHttpService::new(
        move || -> Result<TunnelProxyServer, std::io::Error> {
            Ok(TunnelProxyServer::new_for_gateway(
                gateway_id.clone(),
                registry.clone(),
                pending.clone(),
            ))
        },
        session_manager,
        config,
    );

    match service.call(request).await {
        Ok(response) => response.into_response(),
        Err(e) => {
            tracing::error!("MCP service error: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "Internal server error"}).to_string(),
            )
                .into_response()
        }
    }
}

async fn check_gateway_status(
    State(state): State<AppState>,
    Path(gateway_key): Path<String>,
) -> impl IntoResponse {
    let gateway = match state.db.get_gateway_by_key(&gateway_key).await {
        Ok(Some(gw)) => gw,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, json!({"error": "Gateway not found"}).to_string())
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "Internal error"}).to_string(),
            )
                .into_response();
        }
    };

    let connections = state.registry.get_gateway_connections(&gateway.id);
    let devices: Vec<_> = connections
        .iter()
        .map(|c| {
            json!({
                "alias": c.device_alias,
                "device_name": c.device_name,
                "connected_at": c.connected_at_utc.to_rfc3339()
            })
        })
        .collect();

    (
        StatusCode::OK,
        json!({
            "status": if devices.is_empty() { "offline" } else { "online" },
            "online_devices": devices.len(),
            "devices": devices
        })
        .to_string(),
    )
        .into_response()
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, json!({"status": "ok"}).to_string())
}
