mod config;
mod native_tools;
pub mod diagnostics;
mod handler;
mod mcp_proxy;
mod tunnel;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::{AgentConfig, ServerConfig};
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
    /// Check system permissions and requirements
    Doctor {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Run the agent (default if no subcommand)
    Run,
    /// Uninstall the agent completely
    Uninstall {
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_path = args.config.clone().unwrap_or_else(default_config_path);

    match args.command {
        Some(Command::Configure { server, key }) => {
            configure(&config_path, &server, &key)?;
        }
        Some(Command::Doctor { json }) => {
            doctor(json);
        }
        Some(Command::Uninstall { yes }) => {
            uninstall(yes, &config_path)?;
        }
        Some(Command::Run) | None => {
            run_agent(args, &config_path).await?;
        }
    }

    Ok(())
}

fn doctor(json: bool) {
    use diagnostics::{Diagnostics, PermissionStatus};

    let diag = Diagnostics::check();

    if json {
        println!("{}", serde_json::to_string_pretty(&diag).unwrap());
        return;
    }

    println!("Portal Agent Diagnostics");
    println!("========================");
    println!("OS: {}", diag.os);
    println!();

    if diag.permissions.is_empty() {
        println!("No special permissions required on this platform.");
        return;
    }

    println!("Permissions:");
    for perm in &diag.permissions {
        let (icon, status_text) = match perm.status {
            PermissionStatus::Granted => ("✓", "Granted"),
            PermissionStatus::Denied => ("✗", "Denied"),
            PermissionStatus::Unknown => ("?", "Unknown"),
            PermissionStatus::NotApplicable => ("-", "N/A"),
        };

        println!("  {} {}: {}", icon, perm.name, status_text);

        if perm.status == PermissionStatus::Denied {
            if let Some(url) = &perm.settings_url {
                println!("    Fix: open \"{}\"", url);
            }
        }
    }

    println!();
    if diag.all_granted() {
        println!("All permissions granted!");
    } else {
        println!("Some permissions are missing. Run the commands above to fix.");
    }
}

fn uninstall(skip_confirm: bool, config_path: &std::path::Path) -> Result<()> {
    use std::io::{self, Write};
    use std::process::Command as ProcessCommand;

    if !skip_confirm {
        print!("This will completely remove Portal Agent. Continue? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    println!("Uninstalling Portal Agent...");

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().expect("No home directory");
        let plist_path = home.join("Library/LaunchAgents/com.portal.agent.plist");
        let app_path = home.join("Applications/Portal Agent.app");

        // Stop and unload launchd service
        println!("  Stopping daemon...");
        let _ = ProcessCommand::new("launchctl")
            .args(["unload", plist_path.to_str().unwrap()])
            .output();

        // Remove launchd plist
        if plist_path.exists() {
            std::fs::remove_file(&plist_path)?;
            println!("  Removed {}", plist_path.display());
        }

        // Remove app bundle
        if app_path.exists() {
            std::fs::remove_dir_all(&app_path)?;
            println!("  Removed {}", app_path.display());
        }

        // Remove symlink from /usr/local/bin
        let symlink_path = std::path::Path::new("/usr/local/bin/portal-agent");
        if symlink_path.exists() || symlink_path.is_symlink() {
            let _ = std::fs::remove_file(symlink_path);
            println!("  Removed {}", symlink_path.display());
        }
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().expect("No home directory");
        let service_path = home.join(".config/systemd/user/portal-agent.service");

        // Stop and disable systemd service
        println!("  Stopping daemon...");
        let _ = ProcessCommand::new("systemctl")
            .args(["--user", "stop", "portal-agent"])
            .output();
        let _ = ProcessCommand::new("systemctl")
            .args(["--user", "disable", "portal-agent"])
            .output();

        // Remove service file
        if service_path.exists() {
            std::fs::remove_file(&service_path)?;
            println!("  Removed {}", service_path.display());
            let _ = ProcessCommand::new("systemctl")
                .args(["--user", "daemon-reload"])
                .output();
        }

        // Note: we don't remove /usr/local/bin/portal-agent as it might need sudo
        let bin_path = std::path::Path::new("/usr/local/bin/portal-agent");
        if bin_path.exists() {
            println!("  Note: Run 'sudo rm {}' to remove the binary", bin_path.display());
        }
    }

    // Remove config directory
    if let Some(config_dir) = config_path.parent() {
        if config_dir.exists() {
            std::fs::remove_dir_all(config_dir)?;
            println!("  Removed {}", config_dir.display());
        }
    }

    println!();
    println!("Portal Agent uninstalled successfully.");

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
        "Starting Portal agent"
    );

    let tunnel = TunnelConnection::new(server_config, config.device_name);
    tunnel.run().await?;

    Ok(())
}
