mod config;
mod mcp_proxy;
mod tunnel;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::{AgentConfig, ServerConfig};
use crate::mcp_proxy::McpProxyManager;
use crate::tunnel::TunnelConnection;

#[derive(Parser)]
#[command(name = "portal-agent", about = "Portal Agent - Connect your device to Portal gateway")]
struct Args {
    #[arg(short, long, default_value = "config/agent.json")]
    config: String,

    #[arg(long, env = "PORTAL_KEY")]
    key: Option<String>,

    #[arg(long, env = "PORTAL_SERVER")]
    server: Option<String>,

    #[arg(long)]
    device_name: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    let mut config = config::load_config(&args.config).unwrap_or_else(|e| {
        tracing::warn!("Failed to load config from {}: {}, using defaults", args.config, e);
        AgentConfig::default()
    });

    if args.device_name.is_some() {
        config.device_name = args.device_name;
    }

    let server_config = match (args.server, args.key, config.server) {
        (Some(url), Some(key), _) => ServerConfig { url, key },
        (Some(url), None, Some(cfg)) => ServerConfig { url, key: cfg.key },
        (None, Some(key), Some(cfg)) => ServerConfig { url: cfg.url, key },
        (None, None, Some(cfg)) => cfg,
        _ => {
            return Err(anyhow::anyhow!(
                "Server config required. Use --server and --key, or set in config file"
            ));
        }
    };

    tracing::info!(
        server = %server_config.url,
        device_name = ?config.device_name,
        mcp_servers = config.mcp_servers.len(),
        "Starting Portal agent"
    );

    let mcp_manager = Arc::new(McpProxyManager::new(config.mcp_servers));

    let tunnel = TunnelConnection::new(server_config, config.device_name, mcp_manager);

    tunnel.run().await?;

    Ok(())
}
