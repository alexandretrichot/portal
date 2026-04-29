mod config;
mod mcp_proxy;
mod tunnel;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::{ClientConfig, TunnelServerConfig};
use crate::mcp_proxy::McpProxyManager;
use crate::tunnel::TunnelConnection;

#[derive(Parser)]
#[command(name = "tunnel-client", about = "MCP Tunnel Client (Local Gateway)")]
struct Args {
    #[arg(short, long, default_value = "config/client.json")]
    config: String,

    #[arg(long, env = "TUNNEL_TOKEN")]
    token: Option<String>,

    #[arg(long, env = "TUNNEL_URL")]
    server_url: Option<String>,

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
        ClientConfig::default()
    });

    if args.device_name.is_some() {
        config.device_name = args.device_name;
    }

    let tunnel_config = match (args.server_url, args.token, config.tunnel_server) {
        (Some(url), Some(token), _) => TunnelServerConfig { url, token },
        (Some(url), None, Some(cfg)) => TunnelServerConfig { url, token: cfg.token },
        (None, Some(token), Some(cfg)) => TunnelServerConfig { url: cfg.url, token },
        (None, None, Some(cfg)) => cfg,
        _ => {
            return Err(anyhow::anyhow!(
                "Tunnel server config required. Set tunnelServer in config or use --server-url and --token"
            ));
        }
    };

    tracing::info!(
        server_url = %tunnel_config.url,
        device_name = ?config.device_name,
        mcp_servers = config.mcp_servers.len(),
        "Starting tunnel client"
    );

    let mcp_manager = Arc::new(McpProxyManager::new(config.mcp_servers));

    let tunnel = TunnelConnection::new(tunnel_config, config.device_name, mcp_manager);

    tunnel.run().await?;

    Ok(())
}
