use std::collections::HashMap;
use std::sync::Arc;

use rmcp::{
    Peer, RoleClient, ServiceExt,
    model::CallToolRequestParams,
    transport::TokioChildProcess,
};
use tokio::sync::Mutex;
use common::{McpRequestMessage, McpResponseMessage, TunnelError};
use common::commands::McpServerStatus;

use crate::config::McpServerConfig;

pub struct McpProxyManager {
    configs: HashMap<String, McpServerConfig>,
    clients: Mutex<HashMap<String, Arc<Peer<RoleClient>>>>,
    tool_to_server: Mutex<HashMap<String, String>>,
    errors: Mutex<HashMap<String, String>>,
}

impl McpProxyManager {
    pub fn new(configs: HashMap<String, McpServerConfig>) -> Self {
        Self {
            configs,
            clients: Mutex::new(HashMap::new()),
            tool_to_server: Mutex::new(HashMap::new()),
            errors: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get_status(&self) -> HashMap<String, McpServerStatus> {
        let clients = self.clients.lock().await;
        let errors = self.errors.lock().await;

        self.configs
            .keys()
            .map(|name| {
                let running = clients.contains_key(name);
                let error = errors.get(name).cloned();

                (
                    name.clone(),
                    McpServerStatus {
                        running,
                        error,
                        pid: None,
                    },
                )
            })
            .collect()
    }

    pub async fn restart_server(&self, name: &str) {
        let mut clients = self.clients.lock().await;
        clients.remove(name);

        let mut errors = self.errors.lock().await;
        errors.remove(name);

        tracing::info!(server = %name, "MCP server will restart on next request");
    }

    pub async fn forward_request(&self, req: McpRequestMessage) -> McpResponseMessage {
        let method = req.payload.get("method").and_then(|m| m.as_str());

        match method {
            Some("tools/list") => self.aggregate_tools_list(req).await,
            Some("tools/call") => self.route_tool_call(req).await,
            Some("resources/list") => self.aggregate_resources_list(req).await,
            Some("prompts/list") => self.aggregate_prompts_list(req).await,
            _ => self.forward_to_first(req).await,
        }
    }

    async fn aggregate_tools_list(&self, req: McpRequestMessage) -> McpResponseMessage {
        let id = req.payload.get("id").cloned();
        let mut all_tools = Vec::new();
        let mut tool_map = self.tool_to_server.lock().await;

        for (name, config) in &self.configs {
            if !config.is_stdio() {
                continue;
            }

            let peer = match self.get_or_create_client(name, config).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Failed to connect to MCP server '{}': {}", name, e);
                    continue;
                }
            };

            match peer.list_tools(Default::default()).await {
                Ok(result) => {
                    for tool in &result.tools {
                        tool_map.insert(tool.name.to_string(), name.clone());
                    }
                    all_tools.extend(result.tools);
                }
                Err(e) => {
                    tracing::warn!("Failed to list tools from '{}': {}", name, e);
                }
            }
        }

        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": all_tools }
        });
        McpResponseMessage::success(req.correlation_id, response)
    }

    async fn route_tool_call(&self, req: McpRequestMessage) -> McpResponseMessage {
        let id = req.payload.get("id").cloned();
        let params = req.payload.get("params");

        let tool_name = params
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str());

        let Some(tool_name) = tool_name else {
            return McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32602, "Missing tool name"),
            );
        };

        let server_name = {
            let tool_map = self.tool_to_server.lock().await;
            tool_map.get(tool_name).cloned()
        };

        let Some(server_name) = server_name else {
            return McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32601, format!("Unknown tool: {}", tool_name)),
            );
        };

        let config = match self.configs.get(&server_name) {
            Some(c) => c,
            None => {
                return McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32603, format!("Server '{}' not found", server_name)),
                );
            }
        };

        let peer = match self.get_or_create_client(&server_name, config).await {
            Ok(c) => c,
            Err(e) => {
                return McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32603, e),
                );
            }
        };

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
                    TunnelError::new(-32602, "Missing params"),
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

    async fn aggregate_resources_list(&self, req: McpRequestMessage) -> McpResponseMessage {
        let id = req.payload.get("id").cloned();
        let mut all_resources = Vec::new();

        for (name, config) in &self.configs {
            if !config.is_stdio() {
                continue;
            }

            let peer = match self.get_or_create_client(name, config).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            if let Ok(result) = peer.list_resources(Default::default()).await {
                all_resources.extend(result.resources);
            }
        }

        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "resources": all_resources }
        });
        McpResponseMessage::success(req.correlation_id, response)
    }

    async fn aggregate_prompts_list(&self, req: McpRequestMessage) -> McpResponseMessage {
        let id = req.payload.get("id").cloned();
        let mut all_prompts = Vec::new();

        for (name, config) in &self.configs {
            if !config.is_stdio() {
                continue;
            }

            let peer = match self.get_or_create_client(name, config).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            if let Ok(result) = peer.list_prompts(Default::default()).await {
                all_prompts.extend(result.prompts);
            }
        }

        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "prompts": all_prompts }
        });
        McpResponseMessage::success(req.correlation_id, response)
    }

    async fn forward_to_first(&self, req: McpRequestMessage) -> McpResponseMessage {
        let target_name = self.configs.keys().next().cloned();

        let Some(target_name) = target_name else {
            return McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32600, "No MCP server configured"),
            );
        };

        let config = self.configs.get(&target_name).unwrap();

        if config.is_stdio() {
            self.forward_stdio(req, &target_name, config).await
        } else {
            McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32600, "Only stdio servers supported"),
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
        _name: &str,
        _config: &McpServerConfig,
    ) -> McpResponseMessage {
        McpResponseMessage::error(
            req.correlation_id,
            TunnelError::new(-32601, "Method not supported"),
        )
    }
}
