use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

fn create_offline_response(request_id: Option<serde_json::Value>) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {
            "code": -32001,
            "message": "No devices are currently online for this gateway",
            "data": {
                "error_type": "device_offline",
                "retry_after_seconds": 30,
                "help": "The device may be disconnected. Please try again later."
            }
        }
    })
}

pub struct OfflineResponse {
    pub body: serde_json::Value,
    pub retry_after: u32,
}

impl OfflineResponse {
    pub fn new(request_id: Option<serde_json::Value>, retry_after: u32) -> Self {
        Self {
            body: create_offline_response(request_id),
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
