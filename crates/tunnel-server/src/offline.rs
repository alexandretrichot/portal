use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

use crate::registry::DeviceInfo;

pub fn create_offline_response(
    request_id: Option<serde_json::Value>,
    device_info: Option<&DeviceInfo>,
) -> serde_json::Value {
    let device_desc = device_info
        .and_then(|d| d.name.as_ref())
        .map(|name| format!("Device '{}' ", name))
        .unwrap_or_else(|| "The requested device ".to_string());

    let last_seen_iso = device_info.map(|d| d.last_seen.to_rfc3339());

    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {
            "code": -32001,
            "message": format!("{}is currently offline or disconnected", device_desc),
            "data": {
                "error_type": "device_offline",
                "retry_after_seconds": 30,
                "help": "The device may be powered off, disconnected from the internet, or the tunnel client may not be running. Please try again later or contact the device owner.",
                "last_seen": last_seen_iso
            }
        }
    })
}

pub struct OfflineResponse {
    pub body: serde_json::Value,
    pub retry_after: u32,
}

impl OfflineResponse {
    pub fn new(
        request_id: Option<serde_json::Value>,
        device_info: Option<&DeviceInfo>,
        retry_after: u32,
    ) -> Self {
        Self {
            body: create_offline_response(request_id, device_info),
            retry_after,
        }
    }
}

impl IntoResponse for OfflineResponse {
    fn into_response(self) -> axum::response::Response {
        let body = serde_json::to_string(&self.body).unwrap_or_else(|_| "{}".to_string());

        (
            StatusCode::SERVICE_UNAVAILABLE,
            [
                ("Content-Type", "application/json"),
                ("Retry-After", &self.retry_after.to_string()),
                ("X-Device-Status", "offline"),
            ],
            body,
        )
            .into_response()
    }
}

pub fn create_timeout_response(
    request_id: Option<serde_json::Value>,
    device_info: Option<&DeviceInfo>,
) -> serde_json::Value {
    let device_desc = device_info
        .and_then(|d| d.name.as_ref())
        .map(|name| format!("Device '{}' ", name))
        .unwrap_or_else(|| "The device ".to_string());

    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {
            "code": -32000,
            "message": format!("{}did not respond in time", device_desc),
            "data": {
                "error_type": "request_timeout",
                "retry_after_seconds": 10,
                "help": "The device is connected but did not respond to the request. This could be due to high load or a slow MCP server."
            }
        }
    })
}
