mod config;
mod mcp_handler;
mod offline;
mod pending;
mod registry;
mod tunnel_handler;

use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::ServerConfig;
use crate::pending::PendingRequests;
use crate::registry::ConnectionRegistry;

#[derive(Parser)]
#[command(name = "tunnel-server", about = "MCP Tunnel Server")]
struct Args {
    #[arg(short, long, default_value = "config/server.json")]
    config: String,

    #[arg(long)]
    bind: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<ConnectionRegistry>,
    pub pending: Arc<PendingRequests>,
    pub config: Arc<ServerConfig>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    let config = config::load_config(&args.config).unwrap_or_else(|e| {
        tracing::warn!("Failed to load config from {}: {}, using defaults", args.config, e);
        ServerConfig::default()
    });

    let bind_addr = args.bind.unwrap_or_else(|| config.server.bind.clone());

    let state = AppState {
        registry: Arc::new(ConnectionRegistry::new()),
        pending: Arc::new(PendingRequests::new()),
        config: Arc::new(config),
    };

    let app = Router::new()
        .merge(tunnel_handler::router())
        .merge(mcp_handler::router())
        .with_state(state);

    tracing::info!("Starting tunnel server on {}", bind_addr);

    let listener = TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
