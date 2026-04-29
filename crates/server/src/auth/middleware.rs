use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::clerk::{ClerkAuth, ClerkClaims};

#[derive(Clone)]
pub struct AuthLayer {
    pub clerk: ClerkAuth,
}

impl AuthLayer {
    pub fn new(clerk: ClerkAuth) -> Self {
        Self { clerk }
    }
}

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub clerk_user_id: String,
    pub email: Option<String>,
}

pub async fn auth_middleware(
    auth_layer: AuthLayer,
    mut request: Request,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => &header[7..],
        _ => {
            return (StatusCode::UNAUTHORIZED, "Missing or invalid Authorization header")
                .into_response();
        }
    };

    match auth_layer.clerk.validate_token(token).await {
        Ok(claims) => {
            let user = AuthUser {
                clerk_user_id: claims.sub,
                email: claims.email,
            };
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Err(e) => {
            tracing::debug!(error = %e, "Token validation failed");
            (StatusCode::UNAUTHORIZED, "Invalid token").into_response()
        }
    }
}

pub async fn optional_auth_middleware(
    auth_layer: AuthLayer,
    mut request: Request,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    if let Some(header) = auth_header {
        if header.starts_with("Bearer ") {
            let token = &header[7..];
            if let Ok(claims) = auth_layer.clerk.validate_token(token).await {
                let user = AuthUser {
                    clerk_user_id: claims.sub,
                    email: claims.email,
                };
                request.extensions_mut().insert(user);
            }
        }
    }

    next.run(request).await
}
