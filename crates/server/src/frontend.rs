use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rust_embed::Embed;

use crate::AppState;

#[derive(Embed)]
#[folder = "../../frontend/dist"]
struct Assets;

pub fn router() -> Router<AppState> {
    Router::new().fallback(get(serve_frontend))
}

async fn serve_frontend(State(state): State<AppState>, req: Request) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');

    // Try exact path first (static assets)
    if let Some(content) = Assets::get(path) {
        return response_for_asset(path, &content.data);
    }

    // For SPA: serve index.html with injected config
    if let Some(content) = Assets::get("index.html") {
        let html = String::from_utf8_lossy(&content.data);
        let html = inject_config(&html, &state);

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(html))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not found"))
        .unwrap()
}

fn inject_config(html: &str, state: &AppState) -> String {
    let config = serde_json::json!({
        "clerkPublishableKey": state.config.clerk.publishable_key,
    });

    let script = format!(
        r#"<script>window.__CONFIG__ = {};</script>"#,
        serde_json::to_string(&config).unwrap_or_default()
    );

    html.replace("<head>", &format!("<head>{}", script))
}

fn response_for_asset(path: &str, data: &[u8]) -> Response {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(data.to_vec()))
        .unwrap()
}
