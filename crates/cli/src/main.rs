use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tunnel", about = "MCP Tunnel CLI")]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new API token
    GenerateToken {
        /// Output format
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Hash a token (for server-side storage)
    HashToken {
        /// The token to hash
        token: String,
    },
    /// Check connection status
    Status {
        /// Server URL
        #[arg(short, long)]
        server: String,
        /// API token
        #[arg(short, long, env = "TUNNEL_TOKEN")]
        token: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::GenerateToken { format } => {
            let token = generate_token();
            match format.as_str() {
                "json" => println!(r#"{{"token": "{}"}}"#, token),
                _ => println!("{}", token),
            }
        }
        Commands::HashToken { token } => {
            let hash = hash_token(&token);
            println!("{}", hash);
        }
        Commands::Status { server, token } => {
            check_status(&server, &token).await?;
        }
    }

    Ok(())
}

fn generate_token() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    format!("tmcp_{}", hex::encode(bytes))
}

fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(token.as_bytes());
    hex::encode(hash)
}

async fn check_status(server: &str, token: &str) -> Result<()> {
    let url = format!("{}/v1/{}/status", server.trim_end_matches('/'), token);

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;

    let status: serde_json::Value = response.json().await?;
    println!("{}", serde_json::to_string_pretty(&status)?);

    Ok(())
}
