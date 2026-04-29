mod auth;
mod config;
mod db;
mod mcp_handler;
mod mcp_proxy;
mod offline;
mod pending;
mod registry;
mod device_handler;
mod web;

use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::auth::ClerkAuth;
use crate::config::ServerConfig;
use crate::db::Repository;
use crate::pending::PendingRequests;
use crate::registry::ConnectionRegistry;

#[derive(Parser)]
#[command(name = "portal-server", about = "Portal MCP Gateway Server")]
struct Args {
    #[arg(short, long, default_value = "config/server.json")]
    config: String,

    #[arg(long, env = "PORT", default_value = "8080")]
    port: u16,

    #[arg(long, env = "HOST", default_value = "0.0.0.0")]
    host: String,
}

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<ConnectionRegistry>,
    pub pending: Arc<PendingRequests>,
    pub config: Arc<ServerConfig>,
    pub db: Arc<Repository>,
    pub clerk: Option<Arc<ClerkAuth>>,
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

    let bind_addr = format!("{}:{}", args.host, args.port);

    let conn = db::init_db(&config.database.url)?;
    let repository = Repository::new(conn);

    let clerk = if config.clerk.enabled {
        let clerk_auth = ClerkAuth::new(&config.clerk.domain);
        if let Err(e) = clerk_auth.fetch_jwks().await {
            tracing::warn!("Failed to fetch Clerk JWKS: {}, auth will not work", e);
        }
        Some(Arc::new(clerk_auth))
    } else {
        tracing::info!("Clerk auth disabled");
        None
    };

    let state = AppState {
        registry: Arc::new(ConnectionRegistry::new()),
        pending: Arc::new(PendingRequests::new()),
        config: Arc::new(config),
        db: Arc::new(repository),
        clerk,
    };

    let app = Router::new()
        .merge(device_handler::router())
        .merge(mcp_handler::router())
        .merge(web::router())
        .with_state(state);

    tracing::info!("Starting Portal server on {}", bind_addr);

    let listener = TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
