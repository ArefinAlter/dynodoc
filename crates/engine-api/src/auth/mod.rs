//! Authentication and identity (stage 17, PoC scope).
//!
//! PoC auth is **email + magic link only** (PRD §6.8 FR-25, NFR-9): no passwords, no
//! tiers, no phone/OAuth, no Sybil signals. The flow is:
//!
//! 1. `POST /auth/magic-link` with an email creates the `identity` (if new) and issues
//!    a one-time link. Only the SHA-256 of the link token is stored.
//! 2. `POST /auth/verify` exchanges the link token for a short-lived **Paseto v4
//!    local** access token (NFR-8 — not JWT) plus a rotating refresh token.
//! 3. `POST /auth/refresh` rotates the refresh token and mints a new access token.
//! 4. Protected handlers take the [`AuthContext`] extractor, which validates the
//!    `Authorization: Bearer <paseto>` header.
//!
//! A document's capability role (Author/Reviewer/Auditor) is **not** in the token; it
//! is per-document state read from `document_access` at request time ([`store::role_for`])
//! and handed to the stage-16 governance gate.

pub mod routes;
pub mod store;
pub mod token;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ring::digest::{digest, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::json;

pub use routes::{router, AuthContext};

/// Process-wide auth configuration, derived from environment at boot.
#[derive(Clone)]
pub struct AuthConfig {
    /// The 32-byte Paseto v4 local symmetric key (`PASETO_LOCAL_KEY`, hex-decoded).
    pub paseto_key: [u8; 32],
    pub service_key: Option<String>,
    /// Access-token lifetime in seconds (`TOKEN_TTL_SECONDS`, default 24h).
    pub token_ttl_seconds: i64,
    /// Base URL used to render magic links (`PUBLIC_API_BASE_URL`).
    pub public_base_url: String,
}

/// Refresh tokens outlive access tokens so a session continues without re-emailing a
/// link. 30 days for the PoC.
pub const REFRESH_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;

/// Magic links are short-lived: 15 minutes (PRD review flow is interactive).
pub const MAGIC_LINK_TTL_SECONDS: i64 = 15 * 60;

/// Everything that can go wrong in the auth layer, mapped to an HTTP status by
/// [`IntoResponse`].
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// `PASETO_LOCAL_KEY` is missing, not hex, or not 32 bytes.
    #[error("paseto key is invalid (need 32 bytes hex-encoded)")]
    InvalidKey,
    /// Minting a token failed (should not happen with a valid key).
    #[error("token mint failed: {0}")]
    TokenMint(String),
    /// The bearer token is missing, malformed, expired, or tampered.
    #[error("invalid or expired token")]
    TokenInvalid,
    /// The magic-link token is unknown, expired, or already consumed.
    #[error("invalid or expired magic link")]
    MagicLinkInvalid,
    /// The refresh token is unknown, expired, rotated, or revoked.
    #[error("invalid or expired refresh token")]
    RefreshInvalid,
    /// Authenticated, but not permitted to act on this resource.
    #[error("forbidden")]
    Forbidden,
    /// A database error.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match self {
            AuthError::TokenInvalid => StatusCode::UNAUTHORIZED,
            AuthError::MagicLinkInvalid | AuthError::RefreshInvalid => StatusCode::UNAUTHORIZED,
            AuthError::Forbidden => StatusCode::FORBIDDEN,
            AuthError::InvalidKey | AuthError::TokenMint(_) | AuthError::Db(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        // Don't leak internal detail to clients; log the full error server-side.
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "auth internal error");
        }
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            "Authentication service unavailable".to_string()
        } else {
            self.to_string()
        };
        (status, axum::Json(json!({ "error": message }))).into_response()
    }
}

/// SHA-256 of `bytes` (the at-rest form of every opaque token).
pub fn sha256(bytes: &[u8]) -> Vec<u8> {
    digest(&SHA256, bytes).as_ref().to_vec()
}

/// Generate a fresh opaque token: 32 CSPRNG bytes, hex-encoded for the URL/body, with
/// its SHA-256 for storage. The plaintext is returned once and never persisted.
pub fn random_token() -> Result<(String, Vec<u8>), AuthError> {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| AuthError::TokenMint("csprng failure".into()))?;
    let token = hex::encode(bytes);
    let hash = sha256(token.as_bytes());
    Ok((token, hash))
}
