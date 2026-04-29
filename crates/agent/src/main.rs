mod config;
mod mcp_proxy;
mod tunnel;

use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::{AgentConfig, ServerConfig};
use crate::mcp_proxy::McpProxyManager;
use crate::tunnel::TunnelConnection;

fn default_config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .map(|p| p.join("portal").join("agent.json"))
        .unwrap_or_else(|| "config/agent.json".into())
}

#[derive(Parser)]
#[command(name = "portal-agent", about = "Portal Agent - Connect your device to Portal gateway")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short, long, global = true)]
    config: Option<std::path::PathBuf>,

    #[arg(long, env = "PORTAL_KEY")]
    key: Option<String>,

    #[arg(long, env = "PORTAL_SERVER")]
    server: Option<String>,

    #[arg(long)]
    device_name: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Configure the agent with server URL and device key
    Configure {
        /// Portal server URL
        #[arg(long, required = true)]
        server: String,

        /// Device key from Portal dashboard
        #[arg(long, required = true)]
        key: String,
    },
    /// Run the agent (default if no subcommand)
    Run,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_path = args.config.clone().unwrap_or_else(default_config_path);

    match args.command {
        Some(Command::Configure { server, key }) => {
            configure(&config_path, &server, &key)?;
        }
        Some(Command::Run) | None => {
            run_agent(args, &config_path).await?;
        }
    }

    Ok(())
}

fn configure(config_path: &std::path::Path, server: &str, key: &str) -> Result<()> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let config = serde_json::json!({
        "server": {
            "url": server,
            "key": key
        }
    });

    std::fs::write(config_path, serde_json::to_string_pretty(&config)?)?;

    println!("Configuration saved to {}", config_path.display());
    println!();
    println!("You can now run: portal-agent");

    Ok(())
}

async fn run_agent(args: Args, config_path: &std::path::Path) -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut config = config::load_config(config_path).unwrap_or_else(|e| {
        tracing::warn!("Failed to load config from {}: {}, using defaults", config_path.display(), e);
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
                "Server config required. Run: portal-agent configure --server URL --key KEY"
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
