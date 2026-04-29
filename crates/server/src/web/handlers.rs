use askama::Template;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;

use crate::auth::middleware::AuthUser;
use crate::AppState;

fn extract_base_url(headers: &HeaderMap) -> String {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("http");

    format!("{}://{}", proto, host)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/dashboard", get(dashboard))
        .route("/devices", post(create_device))
        .route("/devices/{id}/delete", post(delete_device))
        .route("/devices/{id}/regenerate", post(regenerate_device))
        .route("/gateway/regenerate", post(regenerate_gateway))
        .route("/install.sh", get(install_script))
        .route("/install/{device_key}", get(device_install_script))
}

async fn index() -> Redirect {
    Redirect::to("/dashboard")
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    email: String,
    base_url: String,
    gateway_key: String,
    devices: Vec<DeviceView>,
}

struct DeviceView {
    id: String,
    name: String,
    alias: String,
    key: String,
    is_online: bool,
}

async fn dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: Option<axum::Extension<AuthUser>>,
) -> impl IntoResponse {
    let user = match user {
        Some(axum::Extension(u)) => u,
        None => {
            if state.clerk.is_none() {
                AuthUser {
                    clerk_user_id: "dev-user".to_string(),
                    email: Some("dev@example.com".to_string()),
                }
            } else {
                return Redirect::to("/login").into_response();
            }
        }
    };

    let db_user = match state
        .db
        .get_or_create_user(&user.clerk_user_id, user.email.as_deref().unwrap_or("unknown"))
        .await
    {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get/create user");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let gateway = match state.db.get_gateway_by_user_id(&db_user.id).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            tracing::error!("No gateway found for user");
            return (StatusCode::INTERNAL_SERVER_ERROR, "No gateway found").into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get gateway");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let devices = match state.db.get_devices_by_gateway_id(&gateway.id).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get devices");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let device_views: Vec<DeviceView> = devices
        .into_iter()
        .map(|d| {
            let is_online = state.registry.is_online(&d.key);
            DeviceView {
                id: d.id,
                name: d.name,
                alias: d.alias,
                key: d.key,
                is_online,
            }
        })
        .collect();

    let base_url = extract_base_url(&headers);

    let template = DashboardTemplate {
        email: user.email.unwrap_or_else(|| "Unknown".to_string()),
        base_url,
        gateway_key: gateway.key,
        devices: device_views,
    };

    Html(template.render().unwrap_or_else(|e| format!("Template error: {}", e))).into_response()
}

#[derive(Deserialize)]
struct CreateDeviceForm {
    name: String,
    alias: String,
}

async fn create_device(
    State(state): State<AppState>,
    user: Option<axum::Extension<AuthUser>>,
    Form(form): Form<CreateDeviceForm>,
) -> impl IntoResponse {
    let user = match user {
        Some(axum::Extension(u)) => u,
        None => {
            if state.clerk.is_none() {
                AuthUser {
                    clerk_user_id: "dev-user".to_string(),
                    email: Some("dev@example.com".to_string()),
                }
            } else {
                return Redirect::to("/login").into_response();
            }
        }
    };

    let db_user = match state
        .db
        .get_or_create_user(&user.clerk_user_id, user.email.as_deref().unwrap_or("unknown"))
        .await
    {
        Ok(u) => u,
        Err(_) => return Redirect::to("/dashboard").into_response(),
    };

    let gateway = match state.db.get_gateway_by_user_id(&db_user.id).await {
        Ok(Some(g)) => g,
        _ => return Redirect::to("/dashboard").into_response(),
    };

    let alias = form.alias.to_lowercase();
    if alias.len() < 2 || alias.len() > 8 || !alias.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Redirect::to("/dashboard").into_response();
    }

    let _ = state.db.create_device(&gateway.id, &form.name, &alias).await;

    Redirect::to("/dashboard").into_response()
}

async fn delete_device(
    State(state): State<AppState>,
    user: Option<axum::Extension<AuthUser>>,
    Path(device_id): Path<String>,
) -> impl IntoResponse {
    let user = match user {
        Some(axum::Extension(u)) => u,
        None => {
            if state.clerk.is_none() {
                AuthUser {
                    clerk_user_id: "dev-user".to_string(),
                    email: Some("dev@example.com".to_string()),
                }
            } else {
                return Redirect::to("/login");
            }
        }
    };

    let db_user = match state
        .db
        .get_or_create_user(&user.clerk_user_id, user.email.as_deref().unwrap_or("unknown"))
        .await
    {
        Ok(u) => u,
        Err(_) => return Redirect::to("/dashboard"),
    };

    let gateway = match state.db.get_gateway_by_user_id(&db_user.id).await {
        Ok(Some(g)) => g,
        _ => return Redirect::to("/dashboard"),
    };

    let _ = state.db.delete_device(&device_id, &gateway.id).await;

    Redirect::to("/dashboard")
}

async fn regenerate_device(
    State(state): State<AppState>,
    user: Option<axum::Extension<AuthUser>>,
    Path(device_id): Path<String>,
) -> impl IntoResponse {
    let user = match user {
        Some(axum::Extension(u)) => u,
        None => {
            if state.clerk.is_none() {
                AuthUser {
                    clerk_user_id: "dev-user".to_string(),
                    email: Some("dev@example.com".to_string()),
                }
            } else {
                return Redirect::to("/login");
            }
        }
    };

    let db_user = match state
        .db
        .get_or_create_user(&user.clerk_user_id, user.email.as_deref().unwrap_or("unknown"))
        .await
    {
        Ok(u) => u,
        Err(_) => return Redirect::to("/dashboard"),
    };

    let gateway = match state.db.get_gateway_by_user_id(&db_user.id).await {
        Ok(Some(g)) => g,
        _ => return Redirect::to("/dashboard"),
    };

    let _ = state.db.regenerate_device_key(&device_id, &gateway.id).await;

    Redirect::to("/dashboard")
}

async fn regenerate_gateway(
    State(state): State<AppState>,
    user: Option<axum::Extension<AuthUser>>,
) -> impl IntoResponse {
    let user = match user {
        Some(axum::Extension(u)) => u,
        None => {
            if state.clerk.is_none() {
                AuthUser {
                    clerk_user_id: "dev-user".to_string(),
                    email: Some("dev@example.com".to_string()),
                }
            } else {
                return Redirect::to("/login");
            }
        }
    };

    let db_user = match state
        .db
        .get_or_create_user(&user.clerk_user_id, user.email.as_deref().unwrap_or("unknown"))
        .await
    {
        Ok(u) => u,
        Err(_) => return Redirect::to("/dashboard"),
    };

    let gateway = match state.db.get_gateway_by_user_id(&db_user.id).await {
        Ok(Some(g)) => g,
        _ => return Redirect::to("/dashboard"),
    };

    let _ = state.db.regenerate_gateway_key(&gateway.id).await;

    Redirect::to("/dashboard")
}

async fn install_script(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let base_url = extract_base_url(&headers);

    let github_repo = match &state.config.github.repo {
        Some(repo) => repo.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Install script not available: github.repo not configured",
            )
                .into_response();
        }
    };

    let script = format!(
        r#"#!/bin/bash
set -e

# Portal Agent Installer
# Downloads from GitHub releases: {github_repo}

REPO="{github_repo}"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
    x86_64|amd64)
        ARCH="amd64"
        ;;
    aarch64|arm64)
        ARCH="arm64"
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

case "$OS" in
    linux|darwin)
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

ASSET_NAME="portal-agent-${{OS}}-${{ARCH}}"
INSTALL_DIR="/usr/local/bin"

echo "Fetching latest release from $REPO..."

# Get latest release download URL
RELEASE_URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep "browser_download_url.*$ASSET_NAME\"" \
    | cut -d '"' -f 4)

if [ -z "$RELEASE_URL" ]; then
    echo "Error: Could not find release asset for $ASSET_NAME"
    echo "Check releases at: https://github.com/$REPO/releases"
    exit 1
fi

echo "Downloading Portal agent..."
echo "  URL: $RELEASE_URL"

if command -v curl &> /dev/null; then
    curl -fsSL "$RELEASE_URL" -o /tmp/portal-agent
elif command -v wget &> /dev/null; then
    wget -q "$RELEASE_URL" -O /tmp/portal-agent
else
    echo "Error: curl or wget required"
    exit 1
fi

chmod +x /tmp/portal-agent

if [ -w "$INSTALL_DIR" ]; then
    mv /tmp/portal-agent "$INSTALL_DIR/portal-agent"
else
    echo "Installing to $INSTALL_DIR (requires sudo)..."
    sudo mv /tmp/portal-agent "$INSTALL_DIR/portal-agent"
fi

echo ""
echo "Portal agent installed successfully!"
echo ""
echo "Usage:"
echo "  portal-agent --key YOUR_DEVICE_KEY --server {base_url}"
echo ""
"#
    );

    (
        [("Content-Type", "text/plain; charset=utf-8")],
        script,
    )
        .into_response()
}

async fn device_install_script(
    State(state): State<AppState>,
    Path(device_key): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let base_url = extract_base_url(&headers);

    let github_repo = match &state.config.github.repo {
        Some(repo) => repo.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Install script not available: github.repo not configured",
            )
                .into_response();
        }
    };

    let script = format!(
        r#"#!/bin/bash
set -e

# Portal Agent Installer & Setup
# Server: {base_url}

REPO="{github_repo}"
SERVER_URL="{base_url}"
DEVICE_KEY="{device_key}"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
    x86_64|amd64) ARCH="amd64" ;;
    aarch64|arm64) ARCH="arm64" ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

case "$OS" in
    linux|darwin) ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

ASSET_NAME="portal-agent-${{OS}}-${{ARCH}}"
INSTALL_DIR="/usr/local/bin"

echo "==> Fetching latest release from $REPO..."

RELEASE_URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep "browser_download_url.*$ASSET_NAME\"" \
    | cut -d '"' -f 4)

if [ -z "$RELEASE_URL" ]; then
    echo "Error: Could not find release asset for $ASSET_NAME"
    echo "Check releases at: https://github.com/$REPO/releases"
    exit 1
fi

echo "==> Downloading Portal agent..."
curl -fsSL "$RELEASE_URL" -o /tmp/portal-agent
chmod +x /tmp/portal-agent

if [ -w "$INSTALL_DIR" ]; then
    mv /tmp/portal-agent "$INSTALL_DIR/portal-agent"
else
    echo "==> Installing to $INSTALL_DIR (requires sudo)..."
    sudo mv /tmp/portal-agent "$INSTALL_DIR/portal-agent"
fi

echo "==> Setting up daemon..."

if [ "$OS" = "darwin" ]; then
    # macOS: launchd
    PLIST_PATH="$HOME/Library/LaunchAgents/com.portal.agent.plist"
    mkdir -p "$HOME/Library/LaunchAgents"

    cat > "$PLIST_PATH" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.portal.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/portal-agent</string>
        <string>--server</string>
        <string>SERVER_URL_PLACEHOLDER</string>
        <string>--key</string>
        <string>DEVICE_KEY_PLACEHOLDER</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/portal-agent.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/portal-agent.log</string>
</dict>
</plist>
PLIST

    sed -i '' "s|SERVER_URL_PLACEHOLDER|$SERVER_URL|g" "$PLIST_PATH"
    sed -i '' "s|DEVICE_KEY_PLACEHOLDER|$DEVICE_KEY|g" "$PLIST_PATH"

    launchctl unload "$PLIST_PATH" 2>/dev/null || true
    launchctl load "$PLIST_PATH"

    echo "==> Daemon installed and started (launchd)"
    echo "    Logs: /tmp/portal-agent.log"
    echo "    Stop: launchctl unload $PLIST_PATH"
    echo ""
    echo "Portal agent is now running!"

elif [ "$OS" = "linux" ]; then
    if command -v systemctl &> /dev/null && systemctl --user status &> /dev/null; then
        # Linux with systemd
        SERVICE_DIR="$HOME/.config/systemd/user"
        SERVICE_PATH="$SERVICE_DIR/portal-agent.service"
        mkdir -p "$SERVICE_DIR"

        cat > "$SERVICE_PATH" << SYSTEMD
[Unit]
Description=Portal Agent
After=network.target

[Service]
ExecStart=/usr/local/bin/portal-agent --server $SERVER_URL --key $DEVICE_KEY
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
SYSTEMD

        systemctl --user daemon-reload
        systemctl --user enable portal-agent
        systemctl --user restart portal-agent

        echo "==> Daemon installed and started (systemd)"
        echo "    Status: systemctl --user status portal-agent"
        echo "    Logs: journalctl --user -u portal-agent -f"
        echo "    Stop: systemctl --user stop portal-agent"
        echo ""
        echo "Portal agent is now running!"
    else
        # Linux without systemd
        echo ""
        echo "==> systemd not available"
        echo ""
        echo "To run manually:"
        echo "  portal-agent --server $SERVER_URL --key $DEVICE_KEY"
        echo ""
        echo "Or install systemd and re-run this script:"
        echo "  apt install systemd"
    fi
fi
echo ""
"#
    );

    (
        [("Content-Type", "text/plain; charset=utf-8")],
        script,
    )
        .into_response()
}
