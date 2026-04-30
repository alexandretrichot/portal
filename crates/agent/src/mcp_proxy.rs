use std::collections::HashMap;
use std::sync::Arc;

use rmcp::{
    Peer, RoleClient, ServiceExt,
    model::CallToolRequestParams,
    transport::{
        TokioChildProcess,
        StreamableHttpClientTransport,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tokio::sync::Mutex;
use common::{McpRequestMessage, McpResponseMessage, TunnelError};
use common::commands::{McpServerConfig, McpServerStatus};

use crate::native_tools::NativeTools;

struct McpServer {
    peer: Arc<Peer<RoleClient>>,
    pid: u32,
    tools_count: usize,
}

pub struct McpProxyManager {
    configs: HashMap<String, McpServerConfig>,
    servers: Arc<Mutex<HashMap<String, McpServer>>>,
    tool_to_server: Mutex<HashMap<String, String>>,
    errors: Arc<Mutex<HashMap<String, String>>>,
    native_tools: NativeTools,
}

impl McpProxyManager {
    pub fn new(configs: HashMap<String, McpServerConfig>) -> Self {
        Self {
            configs,
            servers: Arc::new(Mutex::new(HashMap::new())),
            tool_to_server: Mutex::new(HashMap::new()),
            errors: Arc::new(Mutex::new(HashMap::new())),
            native_tools: NativeTools::new(),
        }
    }

    pub async fn get_status(&self) -> HashMap<String, McpServerStatus> {
        let servers = self.servers.lock().await;
        let errors = self.errors.lock().await;

        let mut result = HashMap::new();

        for name in self.configs.keys() {
            let status = if let Some(server) = servers.get(name) {
                McpServerStatus {
                    running: true,
                    error: None,
                    pid: Some(server.pid),
                    tools_count: server.tools_count,
                    logs: vec![],
                }
            } else {
                McpServerStatus {
                    running: false,
                    error: errors.get(name).cloned(),
                    pid: None,
                    tools_count: 0,
                    logs: vec![],
                }
            };
            result.insert(name.clone(), status);
        }

        result
    }

    pub async fn restart_server(&self, name: &str) {
        let mut servers = self.servers.lock().await;
        servers.remove(name);

        let mut errors = self.errors.lock().await;
        errors.remove(name);

        let mut tool_map = self.tool_to_server.lock().await;
        tool_map.retain(|_, v| v != name);

        tracing::info!(server = %name, "MCP server stopped, will restart on next request");
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

        // Add native tools first
        let native_tools = self.native_tools.get_tools();
        for tool in &native_tools {
            tool_map.insert(tool.name.to_string(), "__native__".to_string());
        }
        all_tools.extend(native_tools);

        // Add tools from external MCP servers
        for (name, config) in &self.configs {
            let peer = match self.get_or_create_server(name, config).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(server = %name, error = %e, "Failed to connect to MCP server");
                    continue;
                }
            };

            match peer.list_tools(Default::default()).await {
                Ok(result) => {
                    let tools_count = result.tools.len();
                    let tool_names: Vec<_> = result.tools.iter().map(|t| &*t.name).collect();
                    tracing::info!(server = %name, count = tools_count, tools = ?tool_names, "Listed tools from MCP server");

                    for tool in &result.tools {
                        tool_map.insert(tool.name.to_string(), name.clone());
                    }
                    all_tools.extend(result.tools);

                    // Update tools count
                    let mut servers = self.servers.lock().await;
                    if let Some(server) = servers.get_mut(name) {
                        server.tools_count = tools_count;
                    }
                }
                Err(e) => {
                    tracing::error!(server = %name, error = %e, "Failed to list tools");
                }
            }
        }

        tracing::info!(total_tools = all_tools.len(), servers = self.configs.len(), "Aggregated tools from all MCP servers");

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

        // Handle native tools
        if server_name == "__native__" {
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

            match self.native_tools.handle_tool_call(tool_params).await {
                Ok(result) => {
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    });
                    return McpResponseMessage::success(req.correlation_id, response);
                }
                Err(e) => {
                    return McpResponseMessage::error(
                        req.correlation_id,
                        TunnelError::new(-32603, format!("Native tool failed: {:?}", e)),
                    );
                }
            }
        }

        // Handle external MCP server tools
        let config = match self.configs.get(&server_name) {
            Some(c) => c,
            None => {
                return McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32603, format!("Server '{}' not found", server_name)),
                );
            }
        };

        let peer = match self.get_or_create_server(&server_name, config).await {
            Ok(p) => p,
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
            let peer = match self.get_or_create_server(name, config).await {
                Ok(p) => p,
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
            let peer = match self.get_or_create_server(name, config).await {
                Ok(p) => p,
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
        let (target_name, config) = match self.configs.iter().next() {
            Some((name, cfg)) => (name.clone(), cfg.clone()),
            None => {
                return McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32600, "No MCP server configured"),
                );
            }
        };

        let peer = match self.get_or_create_server(&target_name, &config).await {
            Ok(p) => p,
            Err(e) => {
                return McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32603, e),
                );
            }
        };

        // Forward the raw request - this is a fallback for unknown methods
        McpResponseMessage::error(
            req.correlation_id,
            TunnelError::new(-32601, "Method not supported for forwarding"),
        )
    }

    async fn get_or_create_server(
        &self,
        name: &str,
        config: &McpServerConfig,
    ) -> Result<Arc<Peer<RoleClient>>, String> {
        // Check if already connected
        {
            let servers = self.servers.lock().await;
            if let Some(server) = servers.get(name) {
                return Ok(server.peer.clone());
            }
        }

        match config {
            McpServerConfig::Stdio { command, args, env, .. } => {
                self.create_stdio_server(name, command, args, env).await
            }
            McpServerConfig::Http { url, headers, .. } => {
                self.create_http_server(name, url, headers).await
            }
        }
    }

    async fn create_stdio_server(
        &self,
        name: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Arc<Peer<RoleClient>>, String> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        for (key, value) in env {
            cmd.env(key, value);
        }

        tracing::info!(server = %name, command = %command, args = ?args, "Starting stdio MCP server");

        let transport = TokioChildProcess::new(cmd)
            .map_err(|e| format!("Failed to spawn process: {}", e))?;

        let running = ().serve(transport).await
            .map_err(|e| {
                let err = format!("Failed to initialize MCP client: {}", e);
                tracing::error!(server = %name, "{}", err);
                err
            })?;

        let peer = Arc::new(running.peer().clone());
        let pid = std::process::id();

        {
            let mut servers = self.servers.lock().await;
            servers.insert(name.to_string(), McpServer {
                peer: peer.clone(),
                pid,
                tools_count: 0,
            });
        }

        // Keep the running service alive and handle cleanup
        let name_clone = name.to_string();
        let servers_ref = self.servers.clone();
        let errors_ref = self.errors.clone();
        tokio::spawn(async move {
            let result = running.waiting().await;
            tracing::info!(server = %name_clone, "MCP server process exited");
            servers_ref.lock().await.remove(&name_clone);
            if let Err(e) = result {
                errors_ref.lock().await.insert(name_clone, e.to_string());
            }
        });

        tracing::info!(server = %name, "Stdio MCP server started successfully");
        Ok(peer)
    }

    async fn create_http_server(
        &self,
        name: &str,
        url: &str,
        headers: &HashMap<String, String>,
    ) -> Result<Arc<Peer<RoleClient>>, String> {
        tracing::info!(server = %name, url = %url, "Connecting to HTTP MCP server");

        let mut config = StreamableHttpClientTransportConfig::with_uri(url);

        // Add Authorization header if present
        if let Some(auth) = headers.get("Authorization") {
            // Strip "Bearer " prefix if present
            let token = auth.strip_prefix("Bearer ").unwrap_or(auth);
            config = config.auth_header(token);
        }

        let transport = StreamableHttpClientTransport::<reqwest::Client>::from_config(config);

        let running = ().serve(transport).await
            .map_err(|e| {
                let err = format!("Failed to connect to HTTP MCP server: {}", e);
                tracing::error!(server = %name, "{}", err);
                err
            })?;

        let peer = Arc::new(running.peer().clone());

        {
            let mut servers = self.servers.lock().await;
            servers.insert(name.to_string(), McpServer {
                peer: peer.clone(),
                pid: 0, // No PID for HTTP servers
                tools_count: 0,
            });
        }

        // Keep the running service alive
        let name_clone = name.to_string();
        let servers_ref = self.servers.clone();
        let errors_ref = self.errors.clone();
        tokio::spawn(async move {
            let result = running.waiting().await;
            tracing::info!(server = %name_clone, "HTTP MCP server connection closed");
            servers_ref.lock().await.remove(&name_clone);
            if let Err(e) = result {
                errors_ref.lock().await.insert(name_clone, e.to_string());
            }
        });

        tracing::info!(server = %name, "HTTP MCP server connected successfully");
        Ok(peer)
    }
}
