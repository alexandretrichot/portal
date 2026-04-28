use std::collections::HashMap;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tunnel_common::{McpRequestMessage, McpResponseMessage, TunnelError};

use crate::config::McpServerConfig;

pub struct McpProxyManager {
    configs: HashMap<String, McpServerConfig>,
    processes: Mutex<HashMap<String, McpProcess>>,
}

struct McpProcess {
    #[allow(dead_code)]
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
}

impl McpProxyManager {
    pub fn new(configs: HashMap<String, McpServerConfig>) -> Self {
        Self {
            configs,
            processes: Mutex::new(HashMap::new()),
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

    async fn forward_stdio(
        &self,
        req: McpRequestMessage,
        name: &str,
        config: &McpServerConfig,
    ) -> McpResponseMessage {
        let mut processes = self.processes.lock().await;

        if !processes.contains_key(name) {
            match self.spawn_process(config).await {
                Ok(process) => {
                    processes.insert(name.to_string(), process);
                }
                Err(e) => {
                    return McpResponseMessage::error(
                        req.correlation_id,
                        TunnelError::new(-32603, format!("Failed to spawn MCP server: {}", e)),
                    );
                }
            }
        }

        let process = processes.get_mut(name).unwrap();

        let request_line = match serde_json::to_string(&req.payload) {
            Ok(s) => s,
            Err(e) => {
                return McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32700, format!("Failed to serialize request: {}", e)),
                );
            }
        };

        if let Err(e) = process.stdin.write_all(request_line.as_bytes()).await {
            processes.remove(name);
            return McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32603, format!("Failed to write to MCP server: {}", e)),
            );
        }

        if let Err(e) = process.stdin.write_all(b"\n").await {
            processes.remove(name);
            return McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32603, format!("Failed to write to MCP server: {}", e)),
            );
        }

        if let Err(e) = process.stdin.flush().await {
            processes.remove(name);
            return McpResponseMessage::error(
                req.correlation_id,
                TunnelError::new(-32603, format!("Failed to flush to MCP server: {}", e)),
            );
        }

        let mut response_line = String::new();
        match process.stdout.read_line(&mut response_line).await {
            Ok(0) => {
                processes.remove(name);
                McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32603, "MCP server closed connection"),
                )
            }
            Ok(_) => match serde_json::from_str(&response_line) {
                Ok(payload) => McpResponseMessage::success(req.correlation_id, payload),
                Err(e) => McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32700, format!("Invalid response from MCP server: {}", e)),
                ),
            },
            Err(e) => {
                processes.remove(name);
                McpResponseMessage::error(
                    req.correlation_id,
                    TunnelError::new(-32603, format!("Failed to read from MCP server: {}", e)),
                )
            }
        }
    }

    async fn spawn_process(&self, config: &McpServerConfig) -> anyhow::Result<McpProcess> {
        let command = config
            .command
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No command specified"))?;

        let mut cmd = Command::new(command);
        cmd.args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("No stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("No stdout"))?;

        Ok(McpProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
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
