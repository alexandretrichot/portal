use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{any, get};
use axum::Router;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use serde_json::json;
use tower::Service as TowerService;

use crate::mcp_proxy::TunnelProxyServer;
use crate::offline::OfflineResponse;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/{token}/mcp", any(handle_mcp_streamable))
        .route("/v1/{token}/status", get(check_device_status))
        .route("/health", get(health_check))
}

async fn handle_mcp_streamable(
    State(state): State<AppState>,
    Path(token): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if !state.registry.is_online(&token) {
        let device_info = state.registry.get_device_info(&token);
        return OfflineResponse::new(
            None,
            device_info.as_ref(),
            state.config.offline_response.retry_after_secs,
        )
        .into_response();
    }

    let registry = state.registry.clone();
    let pending = state.pending.clone();
    let token_clone = token.clone();

    let config = StreamableHttpServerConfig::default()
        .with_stateful_mode(false)
        .with_json_response(true)
        .disable_allowed_hosts();

    let session_manager = Arc::new(LocalSessionManager::default());
    let mut service = StreamableHttpService::new(
        move || -> Result<TunnelProxyServer, std::io::Error> {
            Ok(TunnelProxyServer::new(
                token_clone.clone(),
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
