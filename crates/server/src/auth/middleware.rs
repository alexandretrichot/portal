use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use super::clerk::ClerkAuth;

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub clerk_user_id: String,
    pub email: Option<String>,
}

fn extract_token(request: &Request) -> Option<String> {
    // Try Authorization header first
    if let Some(header) = request.headers().get("Authorization").and_then(|h| h.to_str().ok()) {
        if header.starts_with("Bearer ") {
            return Some(header[7..].to_string());
        }
    }

    // Try __clerk_db_jwt query param (used in dev mode redirect)
    if let Some(query) = request.uri().query() {
        for param in query.split('&') {
            if let Some(value) = param.strip_prefix("__clerk_db_jwt=") {
                // URL-decode the token
                return Some(urlencoding::decode(value).unwrap_or(value.into()).into_owned());
            }
        }
    }

    // Try __session cookie
    if let Some(cookie_header) = request.headers().get("Cookie").and_then(|h| h.to_str().ok()) {
        for cookie in cookie_header.split(';') {
            let cookie = cookie.trim();
            if let Some(value) = cookie.strip_prefix("__session=") {
                return Some(value.to_string());
            }
        }
    }

    None
}

pub async fn clerk_auth_middleware(
    State(clerk): State<Option<Arc<ClerkAuth>>>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(clerk) = &clerk {
        if let Some(token) = extract_token(&request) {
            // Validate JWT and extract claims
            match clerk.validate_token(&token).await {
                Ok(claims) => {
                    let user = AuthUser {
                        clerk_user_id: claims.sub,
                        email: claims.email,
                    };
                    request.extensions_mut().insert(user);
                }
                Err(e) => {
                    tracing::debug!(error = %e, "JWT validation failed");
                }
            }
        }
    }

    next.run(request).await
}
