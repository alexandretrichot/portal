use std::collections::HashMap;
use std::sync::Arc;

use rmcp::{
    Peer, RoleClient, ServiceExt,
    model::CallToolRequestParams,
    transport::TokioChildProcess,
};
use tokio::sync::Mutex;
use tunnel_common::{McpRequestMessage, McpResponseMessage, TunnelError};

use crate::config::McpServerConfig;

pub struct McpProxyManager {
    configs: HashMap<String, McpServerConfig>,
    clients: Mutex<HashMap<String, Arc<Peer<RoleClient>>>>,
}

impl McpProxyManager {
    pub fn new(configs: HashMap<String, McpServerConfig>) -> Self {
        Self {
            configs,
            clients: Mutex::new(HashMap::new()),
        }
    }

    pub async fn forward_request(&self, req: McpRequestMessage) -> McpResponseMessage {
        let target_name = req
            .target_server
            .as_ref()
            .or_else(|| self.configs.keys().next())
            .cloned();

        let Some(target_name) = target_name else {
            return McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32600, "No MCP server configured"),
            );
        };

        let Some(config) = self.configs.get(&target_name) else {
            return McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32600, format!("MCP server '{}' not found", target_name)),
            );
        };

        if config.is_stdio() {
            self.forward_stdio(req, &target_name, config).await
        } else if config.is_http() {
            self.forward_http(req, config).await
        } else {
            McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32600, "Invalid MCP server config: need command or url"),
            )
        }
    }

    async fn get_or_create_client(
        &self,
        name: &str,
        config: &McpServerConfig,
    ) -> Result<Arc<Peer<RoleClient>>, String> {
        let mut clients = self.clients.lock().await;

        if let Some(client) = clients.get(name) {
            return Ok(client.clone());
        }

        let command = config.command.as_ref().ok_or("No command specified")?;

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(&config.args);
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let transport = TokioChildProcess::new(cmd)
            .map_err(|e| format!("Failed to spawn process: {}", e))?;

        let running = ().serve(transport).await
            .map_err(|e| format!("Failed to initialize MCP client: {}", e))?;

        let peer = Arc::new(running.peer().clone());
        clients.insert(name.to_string(), peer.clone());

        // Keep the running service alive in a background task
        tokio::spawn(async move {
            let _ = running.waiting().await;
        });

        Ok(peer)
    }

    async fn forward_stdio(
        &self,
        req: McpRequestMessage,
        name: &str,
        config: &McpServerConfig,
    ) -> McpResponseMessage {
        let peer = match self.get_or_create_client(name, config).await {
            Ok(c) => c,
            Err(e) => {
                return McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32603, e),
                );
            }
        };

        let method = req.payload.get("method").and_then(|m| m.as_str());
        let params = req.payload.get("params");
        let id = req.payload.get("id").cloned();

        match method {
            Some("tools/list") => {
                match peer.list_tools(Default::default()).await {
                    Ok(result) => {
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result
                        });
                        McpResponseMessage::success(req.correlation_id, response)
                    }
                    Err(e) => McpResponseMessage::error(
                        req.correlation_id,
                        TunnelError::new(-32603, format!("tools/list failed: {}", e)),
                    ),
                }
            }
            Some("tools/call") => {
                let tool_params: CallToolRequestParams = match params {
                    Some(p) => match serde_json::from_value(p.clone()) {
                        Ok(p) => p,
                        Err(e) => {
                            return McpResponseMessage::error(
                                req.correlation_id,
                                TunnelError::new(-32602, format!("Invalid params: {}", e)),
                            );
                        }
                    },
                    None => {
                        return McpResponseMessage::error(
                            req.correlation_id,
                            TunnelError::new(-32602, "Missing params for tools/call"),
                        );
                    }
                };

                match peer.call_tool(tool_params).await {
                    Ok(result) => {
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result
                        });
                        McpResponseMessage::success(req.correlation_id, response)
                    }
                    Err(e) => McpResponseMessage::error(
                        req.correlation_id,
                        TunnelError::new(-32603, format!("tools/call failed: {}", e)),
                    ),
                }
            }
            Some("resources/list") => {
                match peer.list_resources(Default::default()).await {
                    Ok(result) => {
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result
                        });
                        McpResponseMessage::success(req.correlation_id, response)
                    }
                    Err(e) => McpResponseMessage::error(
                        req.correlation_id,
                        TunnelError::new(-32603, format!("resources/list failed: {}", e)),
                    ),
                }
            }
            Some("prompts/list") => {
                match peer.list_prompts(Default::default()).await {
                    Ok(result) => {
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result
                        });
                        McpResponseMessage::success(req.correlation_id, response)
                    }
                    Err(e) => McpResponseMessage::error(
                        req.correlation_id,
                        TunnelError::new(-32603, format!("prompts/list failed: {}", e)),
                    ),
                }
            }
            Some(other) => McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32601, format!("Method not supported: {}", other)),
            ),
            None => McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32600, "Missing method in request"),
            ),
        }
    }

    async fn forward_http(
        &self,
        req: McpRequestMessage,
        _config: &McpServerConfig,
    ) -> McpResponseMessage {
        McpResponseMessage::error(
            req.correlation_id,
            TunnelError::new(-32600, "HTTP transport not yet implemented"),
        )
    }
}
