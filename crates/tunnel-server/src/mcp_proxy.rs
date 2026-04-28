use std::sync::Arc;

use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    model::*,
    service::{RequestContext, RoleServer},
};
use uuid::Uuid;

use crate::pending::PendingRequests;
use crate::registry::{ConnectionRegistry, ConnectionHandle};

#[derive(Clone)]
pub struct TunnelProxyServer {
    pub token: String,
    pub registry: Arc<ConnectionRegistry>,
    pub pending: Arc<PendingRequests>,
}

impl TunnelProxyServer {
    pub fn new(token: String, registry: Arc<ConnectionRegistry>, pending: Arc<PendingRequests>) -> Self {
        Self { token, registry, pending }
    }

    fn get_connection(&self) -> Option<ConnectionHandle> {
        self.registry.get(&self.token)
    }

    async fn forward_request<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: impl serde::Serialize,
    ) -> Result<T, McpError> {
        let handle = self.get_connection().ok_or_else(|| {
            McpError::internal_error("Device is offline", None)
        })?;

        let correlation_id = Uuid::new_v4();
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": correlation_id.to_string(),
            "method": method,
            "params": params,
        });

        let mcp_request = tunnel_common::TunnelMessage::McpRequest(tunnel_common::McpRequestMessage {
            correlation_id,
            payload,
            target_server: None,
            timeout_ms: Some(30000),
        });

        let rx = self.pending.register(correlation_id);

        handle.sender.send(mcp_request).await.map_err(|_| {
            McpError::internal_error("Failed to send request", None)
        })?;

        let response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            rx,
        ).await.map_err(|_| {
            self.pending.cancel(&correlation_id);
            McpError::internal_error("Request timeout", None)
        })?.map_err(|_| {
            McpError::internal_error("Response channel closed", None)
        })?;

        if let Some(error) = response.error {
            return Err(McpError::new(
                ErrorCode::INTERNAL_ERROR,
                error.message,
                None,
            ));
        }

        let payload = response.payload.ok_or_else(|| {
            McpError::internal_error("Empty response", None)
        })?;

        let result = payload.get("result").ok_or_else(|| {
            McpError::internal_error("No result in response", None)
        })?;

        serde_json::from_value(result.clone()).map_err(|e| {
            McpError::internal_error(format!("Failed to parse result: {}", e), None)
        })
    }
}

impl ServerHandler for TunnelProxyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
            .with_server_info(Implementation::new("tunnel-proxy", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            self.forward_request::<ListToolsResult>("tools/list", serde_json::json!({})).await
        }
    }

    fn call_tool(
        &self,
        params: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            self.forward_request::<CallToolResult>("tools/call", params).await
        }
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        async move {
            self.forward_request::<ListResourcesResult>("resources/list", serde_json::json!({})).await
        }
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>> + Send + '_ {
        async move {
            self.forward_request::<ListPromptsResult>("prompts/list", serde_json::json!({})).await
        }
    }
}
