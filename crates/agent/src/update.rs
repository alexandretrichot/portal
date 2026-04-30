use semver::Version;
use serde::Deserialize;
use std::time::Duration;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

pub fn start_update_loop(github_repo: &str) {
    let repo = github_repo.to_string();
    tokio::spawn(async move {
        loop {
            check_and_update(&repo).await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

async fn check_and_update(github_repo: &str) {
    match fetch_latest_release(github_repo).await {
        Ok(release) => {
            let latest = release.tag_name.trim_start_matches('v');
            if is_newer(latest, CURRENT_VERSION) {
                tracing::info!(
                    current = CURRENT_VERSION,
                    latest = latest,
                    "New version available, updating..."
                );
                if let Err(e) = perform_update(&release).await {
                    tracing::error!(error = %e, "Failed to update");
                }
            } else {
                tracing::debug!(version = CURRENT_VERSION, "Agent is up to date");
            }
        }
        Err(e) => {
            tracing::debug!(error = %e, "Failed to check for updates");
        }
    }
}

async fn fetch_latest_release(github_repo: &str) -> anyhow::Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", github_repo);

    let client = reqwest::Client::builder()
        .user_agent("portal-agent")
        .timeout(Duration::from_secs(10))
        .build()?;

    let release: GitHubRelease = client.get(&url).send().await?.json().await?;
    Ok(release)
}

async fn perform_update(release: &GitHubRelease) -> anyhow::Result<()> {
    let asset_name = get_asset_name();

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| anyhow::anyhow!("No asset found for {}", asset_name))?;

    tracing::info!(asset = %asset.name, "Downloading update...");

    let client = reqwest::Client::builder()
        .user_agent("portal-agent")
        .timeout(Duration::from_secs(120))
        .build()?;

    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await?
        .bytes()
        .await?;

    let current_exe = std::env::current_exe()?;
    let temp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("old");

    // Write new binary
    std::fs::write(&temp_path, &bytes)?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // Backup current -> .old, new -> current
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)?;
    }
    std::fs::rename(&current_exe, &backup_path)?;
    std::fs::rename(&temp_path, &current_exe)?;

    tracing::info!("Update downloaded, restarting...");

    restart_daemon()?;

    Ok(())
}

fn get_asset_name() -> String {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "unknown"
    };

    format!("portal-agent-{}-{}", os, arch)
}

fn restart_daemon() -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = get_launchd_plist_path()?;
        std::process::Command::new("launchctl")
            .args(["unload", &plist_path])
            .output()?;
        std::process::Command::new("launchctl")
            .args(["load", &plist_path])
            .output()?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("systemctl")
            .args(["--user", "restart", "portal-agent"])
            .output()?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn get_launchd_plist_path() -> anyhow::Result<String> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir"))?;
    Ok(home
        .join("Library/LaunchAgents/com.portal.agent.plist")
        .to_string_lossy()
        .to_string())
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (Version::parse(latest), Version::parse(current)) {
        (Ok(l), Ok(c)) => l > c,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        // Basic semver
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("1.1.0", "1.0.0"));
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.1"));

        // Prereleases: 1.0.0-alpha < 1.0.0-beta < 1.0.0
        assert!(is_newer("1.0.0-beta", "1.0.0-alpha"));
        assert!(is_newer("1.0.0", "1.0.0-beta"));
        assert!(is_newer("1.0.0-alpha.2", "1.0.0-alpha.1"));
        assert!(!is_newer("1.0.0-alpha", "1.0.0"));
    }

    #[test]
    fn test_asset_name() {
        let name = get_asset_name();
        assert!(name.starts_with("portal-agent-"));
    }
}
