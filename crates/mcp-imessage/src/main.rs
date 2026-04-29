mod db;
mod tools;

use anyhow::Result;
use rmcp::{ServerHandler, ServiceExt, model::*, service::{RequestContext, RoleServer}};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::tools::{get_tools, handle_tool_call};

#[derive(Clone)]
pub struct ImessageServer {
    db_path: String,
}

impl ImessageServer {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        let db_path = home.join("Library/Messages/chat.db");

        if !db_path.exists() {
            anyhow::bail!("iMessage database not found at {:?}", db_path);
        }

        Ok(Self {
            db_path: db_path.to_string_lossy().to_string(),
        })
    }
}

impl ServerHandler for ImessageServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
            .with_server_info(Implementation::new("mcp-imessage", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        async move {
            Ok(ListToolsResult::with_all_items(get_tools()))
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, ErrorData>> + Send + '_ {
        let db_path = self.db_path.clone();
        async move {
            handle_tool_call(&db_path, request).await
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    tracing::info!("Starting mcp-imessage server");

    let server = ImessageServer::new()?;
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
