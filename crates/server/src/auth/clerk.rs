use anyhow::Result;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClerkClaims {
    pub sub: String,
    pub email: Option<String>,
    pub exp: usize,
    pub iat: usize,
    pub iss: String,
    pub azp: Option<String>,
}

pub struct ClerkAuth {
    domain: String,
    jwks: Arc<RwLock<Option<JwkSet>>>,
}

impl ClerkAuth {
    pub fn new(domain: &str) -> Self {
        Self {
            domain: domain.to_string(),
            jwks: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn fetch_jwks(&self) -> Result<()> {
        let url = format!("https://{}/.well-known/jwks.json", self.domain);
        let response = reqwest::get(&url).await?;
        let jwks: JwkSet = response.json().await?;

        let mut cache = self.jwks.write().await;
        *cache = Some(jwks);

        tracing::info!("Fetched JWKS from Clerk");
        Ok(())
    }

    pub async fn validate_token(&self, token: &str) -> Result<ClerkClaims> {
        let jwks = {
            let cache = self.jwks.read().await;
            cache.clone().ok_or_else(|| anyhow::anyhow!("JWKS not loaded"))?
        };

        let header = decode_header(token)?;
        let kid = header.kid.ok_or_else(|| anyhow::anyhow!("No kid in token header"))?;

        let jwk = jwks
            .find(&kid)
            .ok_or_else(|| anyhow::anyhow!("Key not found in JWKS"))?;

        let decoding_key = DecodingKey::from_jwk(jwk)?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[format!("https://{}", self.domain)]);
        validation.validate_exp = true;

        let token_data = decode::<ClerkClaims>(token, &decoding_key, &validation)?;
        Ok(token_data.claims)
    }
}

impl Clone for ClerkAuth {
    fn clone(&self) -> Self {
        Self {
            domain: self.domain.clone(),
            jwks: self.jwks.clone(),
        }
    }
}
