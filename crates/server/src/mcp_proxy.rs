use std::sync::Arc;

use rmcp::{
    model::*,
    service::{RequestContext, RoleServer},
    ErrorData as McpError, ServerHandler,
};
use uuid::Uuid;

use crate::pending::PendingRequests;
use crate::registry::{ConnectionHandle, ConnectionRegistry};

#[derive(Clone)]
pub struct TunnelProxyServer {
    pub gateway_id: String,
    pub registry: Arc<ConnectionRegistry>,
    pub pending: Arc<PendingRequests>,
}

impl TunnelProxyServer {
    pub fn new_for_gateway(
        gateway_id: String,
        registry: Arc<ConnectionRegistry>,
        pending: Arc<PendingRequests>,
    ) -> Self {
        Self {
            gateway_id,
            registry,
            pending,
        }
    }

    fn get_connections(&self) -> Vec<ConnectionHandle> {
        self.registry.get_gateway_connections(&self.gateway_id)
    }

    fn get_any_connection(&self) -> Option<ConnectionHandle> {
        self.get_connections().into_iter().next()
    }

    async fn forward_request<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: impl serde::Serialize,
    ) -> Result<T, McpError> {
        let handle = self.get_any_connection().ok_or_else(|| {
            McpError::internal_error("No devices online for this gateway", None)
        })?;

        let correlation_id = Uuid::new_v4();
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": correlation_id.to_string(),
            "method": method,
            "params": params,
        });

        let mcp_request = common::TunnelMessage::McpRequest(common::McpRequestMessage {
            correlation_id,
            payload,
            target_server: None,
            timeout_ms: Some(30000),
        });

        let rx = self.pending.register(correlation_id);

        handle
            .sender
            .send(mcp_request)
            .await
            .map_err(|_| McpError::internal_error("Failed to send request", None))?;

        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| {
                self.pending.cancel(&correlation_id);
                McpError::internal_error("Request timeout", None)
            })?
            .map_err(|_| McpError::internal_error("Response channel closed", None))?;

        if let Some(error) = response.error {
            return Err(McpError::new(ErrorCode::INTERNAL_ERROR, error.message, None));
        }

        let payload = response
            .payload
            .ok_or_else(|| McpError::internal_error("Empty response", None))?;

        let result = payload
            .get("result")
            .ok_or_else(|| McpError::internal_error("No result in response", None))?;

        serde_json::from_value(result.clone())
            .map_err(|e| McpError::internal_error(format!("Failed to parse result: {}", e), None))
    }

    async fn aggregate_tools(&self) -> Result<ListToolsResult, McpError> {
        let connections = self.get_connections();
        if connections.is_empty() {
            return Ok(ListToolsResult {
                tools: vec![],
                next_cursor: None,
                meta: None,
            });
        }

        let single_device = connections.len() == 1;
        let mut all_tools = Vec::new();

        for handle in connections {
            let correlation_id = Uuid::new_v4();
            let payload = serde_json::json!({
                "jsonrpc": "2.0",
                "id": correlation_id.to_string(),
                "method": "tools/list",
                "params": {},
            });

            let mcp_request = common::TunnelMessage::McpRequest(common::McpRequestMessage {
                correlation_id,
                payload,
                target_server: None,
                timeout_ms: Some(30000),
            });

            let rx = self.pending.register(correlation_id);

            if handle.sender.send(mcp_request).await.is_err() {
                continue;
            }

            if let Ok(Ok(response)) =
                tokio::time::timeout(std::time::Duration::from_secs(30), rx).await
            {
                if response.error.is_none() {
                    if let Some(payload) = response.payload {
                        if let Some(result) = payload.get("result") {
                            if let Ok(tools_result) =
                                serde_json::from_value::<ListToolsResult>(result.clone())
                            {
                                for mut tool in tools_result.tools {
                                    let prefix = if single_device {
                                        String::new()
                                    } else {
                                        format!("{}:", handle.device_alias)
                                    };

                                    tool.name = format!("{}{}", prefix, tool.name).into();

                                    if let Some(ref mut desc) = tool.description {
                                        if !single_device {
                                            *desc = format!("[{}] {}", handle.device_alias, desc).into();
                                        }
                                    }

                                    all_tools.push(tool);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(ListToolsResult {
            tools: all_tools,
            next_cursor: None,
            meta: None,
        })
    }

    async fn route_tool_call(&self, params: CallToolRequestParams) -> Result<CallToolResult, McpError> {
        let connections = self.get_connections();
        if connections.is_empty() {
            return Err(McpError::invalid_params(
                format!("Tool '{}' not found (no devices connected)", params.name),
                None,
            ));
        }

        let single_device = connections.len() == 1;
        let tool_name = params.name.as_ref();

        let (target_alias, actual_tool_name): (Option<String>, String) = if single_device {
            (None, tool_name.to_string())
        } else if let Some((alias, name)) = tool_name.split_once(':') {
            (Some(alias.to_string()), name.to_string())
        } else {
            return Err(McpError::invalid_params(
                "Tool name must be prefixed with device alias (e.g., 'mac:tool_name')",
                None,
            ));
        };

        let handle = if let Some(ref alias) = target_alias {
            connections
                .into_iter()
                .find(|c| c.device_alias == *alias)
                .ok_or_else(|| {
                    McpError::invalid_params(format!("Device '{}' not found or offline", alias), None)
                })?
        } else {
            connections.into_iter().next().unwrap()
        };

        let modified_params = serde_json::json!({
            "name": actual_tool_name,
            "arguments": params.arguments,
        });

        let correlation_id = Uuid::new_v4();
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": correlation_id.to_string(),
            "method": "tools/call",
            "params": modified_params,
        });

        let mcp_request = common::TunnelMessage::McpRequest(common::McpRequestMessage {
            correlation_id,
            payload,
            target_server: None,
            timeout_ms: Some(30000),
        });

        let rx = self.pending.register(correlation_id);

        handle
            .sender
            .send(mcp_request)
            .await
            .map_err(|_| McpError::internal_error("Failed to send request", None))?;

        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| {
                self.pending.cancel(&correlation_id);
                McpError::internal_error("Request timeout", None)
            })?
            .map_err(|_| McpError::internal_error("Response channel closed", None))?;

        if let Some(error) = response.error {
            return Err(McpError::new(ErrorCode::INTERNAL_ERROR, error.message, None));
        }

        let payload = response
            .payload
            .ok_or_else(|| McpError::internal_error("Empty response", None))?;

        let result = payload
            .get("result")
            .ok_or_else(|| McpError::internal_error("No result in response", None))?;

        serde_json::from_value(result.clone())
            .map_err(|e| McpError::internal_error(format!("Failed to parse result: {}", e), None))
    }
}

impl ServerHandler for TunnelProxyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
            .with_server_info(Implementation::new("portal-gateway", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move { self.aggregate_tools().await }
    }

    fn call_tool(
        &self,
        params: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move { self.route_tool_call(params).await }
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        async move {
            if self.get_connections().is_empty() {
                return Ok(ListResourcesResult {
                    resources: vec![],
                    next_cursor: None,
                    meta: None,
                });
            }
            self.forward_request::<ListResourcesResult>("resources/list", serde_json::json!({}))
                .await
        }
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>> + Send + '_ {
        async move {
            if self.get_connections().is_empty() {
                return Ok(ListPromptsResult {
                    prompts: vec![],
                    next_cursor: None,
                    meta: None,
                });
            }
            self.forward_request::<ListPromptsResult>("prompts/list", serde_json::json!({}))
                .await
        }
    }
}
