use axum::{
    body::Body,
    extract::Request,
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

async fn serve_frontend(req: Request) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');

    // Try exact path first
    if let Some(content) = Assets::get(path) {
        return response_for_asset(path, &content.data);
    }

    // For SPA: serve index.html for non-asset routes
    if let Some(content) = Assets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(content.data.to_vec()))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not found"))
        .unwrap()
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
