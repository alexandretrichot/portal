use std::collections::HashMap;
use std::sync::Arc;

use common::commands::*;
use common::{AgentCommands, CommandResult, Sender};
use tokio::sync::RwLock;

use crate::diagnostics::Diagnostics;
use crate::mcp_proxy::McpProxyManager;

pub struct AgentHandler {
    mcp_manager: Arc<RwLock<Option<McpProxyManager>>>,
    sender: Option<Sender>,
}

impl AgentHandler {
    pub fn new() -> Self {
        Self {
            mcp_manager: Arc::new(RwLock::new(None)),
            sender: None,
        }
    }

    pub fn with_sender(mut self, sender: Sender) -> Self {
        self.sender = Some(sender);
        self
    }

    pub async fn apply_config(&self, config: HashMap<String, McpServerConfig>) {
        let agent_config: HashMap<String, crate::config::McpServerConfig> = config
            .into_iter()
            .filter(|(_, cfg)| cfg.enabled)
            .map(|(name, cfg)| {
                (
                    name,
                    crate::config::McpServerConfig {
                        command: Some(cfg.command),
                        args: cfg.args,
                        env: cfg.env,
                    },
                )
            })
            .collect();

        let manager = McpProxyManager::new(agent_config);
        *self.mcp_manager.write().await = Some(manager);

        tracing::info!("MCP config applied");
    }

    pub async fn get_mcp_manager(&self) -> Option<Arc<RwLock<Option<McpProxyManager>>>> {
        Some(self.mcp_manager.clone())
    }
}

impl AgentCommands for AgentHandler {
    async fn get_diagnostics(&self) -> CommandResult<DiagnosticsResponse> {
        let diag = Diagnostics::check();

        Ok(DiagnosticsResponse {
            os: diag.os,
            permissions: diag
                .permissions
                .into_iter()
                .map(|p| Permission {
                    name: p.name,
                    status: match p.status {
                        crate::diagnostics::PermissionStatus::Granted => PermissionStatus::Granted,
                        crate::diagnostics::PermissionStatus::Denied => PermissionStatus::Denied,
                        crate::diagnostics::PermissionStatus::Unknown => PermissionStatus::Unknown,
                        crate::diagnostics::PermissionStatus::NotApplicable => PermissionStatus::Unknown,
                    },
                    settings_url: p.settings_url,
                })
                .collect(),
        })
    }

    async fn open_settings(&self, cmd: OpenSettings) -> CommandResult<OpenSettingsResponse> {
        let diag = Diagnostics::check();
        let success = diag.open_settings(&cmd.permission);
        Ok(OpenSettingsResponse { success })
    }

    async fn get_mcp_status(&self) -> CommandResult<McpStatusResponse> {
        let manager = self.mcp_manager.read().await;

        let servers = if let Some(mgr) = manager.as_ref() {
            mgr.get_status().await
        } else {
            HashMap::new()
        };

        Ok(McpStatusResponse { servers })
    }

    async fn update_mcp_config(&self, cmd: UpdateMcpConfig) -> CommandResult<UpdateMcpConfigResponse> {
        self.apply_config(cmd.servers).await;
        Ok(UpdateMcpConfigResponse { success: true })
    }

    async fn restart_agent(&self) -> CommandResult<RestartAgentResponse> {
        tracing::info!("Restart requested, exiting...");

        // Exit with success - launchd/systemd will restart us
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            std::process::exit(0);
        });

        Ok(RestartAgentResponse { success: true })
    }

    async fn restart_mcp_server(&self, cmd: RestartMcpServer) -> CommandResult<RestartMcpServerResponse> {
        let manager = self.mcp_manager.read().await;

        if let Some(mgr) = manager.as_ref() {
            mgr.restart_server(&cmd.name).await;
            Ok(RestartMcpServerResponse { success: true })
        } else {
            Ok(RestartMcpServerResponse { success: false })
        }
    }
}
