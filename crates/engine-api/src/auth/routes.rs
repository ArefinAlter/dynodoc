//! Auth HTTP surface: the `/auth/*` routes and the `Authorization: Bearer` extractor.
//!
//! Issuance (`magic-link`, `exchange`) requires a trusted service key in every environment.
//! `verify`, `refresh` and `logout` require their respective secret; `me` requires a
//! valid access token via the [`AuthContext`] extractor. The document/event REST + SSE
//! surface (stage 18) reuses this [`AuthContext`] and wires [`store::role_for`] into the
//! governance gate on the write path (see [`crate::ops::apply`]).

use axum::extract::{FromRequestParts, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    random_token, sha256, store, token, AuthError, MAGIC_LINK_TTL_SECONDS, REFRESH_TTL_SECONDS,
};
use crate::AppState;

/// The authenticated caller, extracted from a validated bearer token. A handler that
/// takes this as an argument is automatically gated: a missing/invalid token short-
/// circuits with `401` before the handler body runs.
pub struct AuthContext {
    pub identity_id: Uuid,
    pub session_generation: i64,
    pub session_id: Option<Uuid>,
}

#[axum::async_trait]
impl FromRequestParts<AppState> for AuthContext {
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AuthError> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError::TokenInvalid)?;
        let raw = header
            .strip_prefix("Bearer ")
            .ok_or(AuthError::TokenInvalid)?
            .trim();
        let (identity_id, session_generation, session_id) =
            token::verify_bound_session(&state.auth.paseto_key, raw)?;
        store::ensure_bound_session(&state.pool, identity_id, session_generation, session_id)
            .await?;
        Ok(AuthContext {
            identity_id,
            session_generation,
            session_id,
        })
    }
}

/// The `/auth/*` routes, parameterized over the shared [`AppState`].
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/magic-link", post(magic_link_request))
        .route("/auth/verify", post(verify))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/exchange", post(exchange))
}

// --- magic link request ---------------------------------------------------------

#[derive(Deserialize)]
struct MagicLinkRequest {
    email: String,
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Serialize)]
struct MagicLinkResponse {
    /// Returned only to the authenticated web server, which delivers it by email.
    magic_link_token: String,
    /// Convenience URL a human can use to complete login.
    verify_url: String,
    expires_at: DateTime<Utc>,
}

/// Create the identity if new and issue a one-time magic link.
async fn magic_link_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<MagicLinkRequest>,
) -> Result<Json<MagicLinkResponse>, AuthError> {
    require_service(&state, &headers)?;
    if req.email.len() > 254 || !req.email.contains('@') {
        return Err(AuthError::Forbidden);
    }
    let rate_key = sha256(req.email.trim().to_lowercase().as_bytes());
    let attempts: i32 = sqlx::query_scalar("insert into auth_request_window(key_hash) values($1) on conflict(key_hash) do update set attempts=case when auth_request_window.started_at < now()-interval '15 minutes' then 1 else auth_request_window.attempts+1 end, started_at=case when auth_request_window.started_at < now()-interval '15 minutes' then now() else auth_request_window.started_at end returning attempts")
        .bind(rate_key).fetch_one(&state.pool).await?;
    if attempts > 5 {
        return Err(AuthError::Forbidden);
    }
    let identity =
        store::upsert_identity(&state.pool, &req.email, req.display_name.as_deref()).await?;

    let (plaintext, hash) = random_token()?;
    let expires_at = Utc::now() + Duration::seconds(MAGIC_LINK_TTL_SECONDS);
    store::issue_magic_link(&state.pool, identity.id, &hash, expires_at).await?;

    tracing::info!("issued magic link to authenticated web service");
    let verify_url = format!(
        "{}/auth/verify?token={}",
        state.auth.public_base_url, plaintext
    );

    Ok(Json(MagicLinkResponse {
        magic_link_token: plaintext,
        verify_url,
        expires_at,
    }))
}

// --- token responses (verify + refresh) -----------------------------------------

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    /// Access-token lifetime in seconds.
    expires_in: i64,
    /// Opaque refresh token (rotates on every use).
    refresh_token: String,
}

async fn mint_session(state: &AppState, identity_id: Uuid) -> Result<TokenResponse, AuthError> {
    let generation = store::session_generation(&state.pool, identity_id).await?;
    let session_id = Uuid::new_v4();
    let access_token = token::mint_bound_session(
        &state.auth.paseto_key,
        identity_id,
        state.auth.token_ttl_seconds,
        generation,
        Some(session_id),
    )?;
    let (refresh_plain, refresh_hash) = random_token()?;
    let refresh_expires = Utc::now() + Duration::seconds(REFRESH_TTL_SECONDS);
    store::issue_bound_refresh(
        &state.pool,
        identity_id,
        &refresh_hash,
        refresh_expires,
        generation,
        Some(session_id),
    )
    .await?;
    Ok(TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in: state.auth.token_ttl_seconds,
        refresh_token: refresh_plain,
    })
}

#[derive(Deserialize)]
struct VerifyRequest {
    token: String,
}

/// Exchange a magic-link token for an access token + refresh token.
async fn verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<TokenResponse>, AuthError> {
    let identity_id = store::consume_magic_link(&state.pool, &sha256(req.token.as_bytes())).await?;
    Ok(Json(mint_session(&state, identity_id).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RefreshRequest {
    refresh_token: String,
}

/// Rotate a refresh token and mint a fresh access token.
async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>, AuthError> {
    let presented = sha256(req.refresh_token.as_bytes());
    let (new_plain, new_hash) = random_token()?;
    let refresh_expires = Utc::now() + Duration::seconds(REFRESH_TTL_SECONDS);
    let (identity_id, generation, session_id) =
        store::rotate_bound_refresh(&state.pool, &presented, &new_hash, refresh_expires).await?;

    // The successor keeps the session generation of the presented credential.
    let access_token = token::mint_bound_session(
        &state.auth.paseto_key,
        identity_id,
        state.auth.token_ttl_seconds,
        generation,
        session_id,
    )?;
    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in: state.auth.token_ttl_seconds,
        refresh_token: new_plain,
    }))
}

// --- me -------------------------------------------------------------------------

/// Revoke the login identified by a refresh secret, including rotated ancestors.
/// Invalid/already revoked credentials have the same idempotent response.
#[utoipa::path(post,path="/auth/logout",request_body=RefreshRequest,responses((status=200,description="Login revoked; legacy sessions revoke the account generation",body=serde_json::Value)))]
pub async fn logout(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    if req.refresh_token.len() != 64 || !req.refresh_token.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Ok(Json(serde_json::json!({"ok":true})));
    }
    store::revoke_login(&state.pool, &sha256(req.refresh_token.as_bytes())).await?;
    Ok(Json(serde_json::json!({"ok":true})))
}

#[derive(Serialize)]
struct MeResponse {
    identity_id: Uuid,
    email: String,
    display_name: Option<String>,
}

/// Return the authenticated identity (requires a valid bearer token).
async fn me(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<MeResponse>, AuthError> {
    let identity = store::get_identity(&state.pool, auth.identity_id).await?;
    Ok(Json(MeResponse {
        identity_id: identity.id.0,
        email: identity.email,
        display_name: identity.display_name,
    }))
}

// Only the web server may exchange a provider-verified identity. This route is
// unavailable without a configured service credential, including local development.
#[derive(Deserialize)]
struct ExchangeRequest {
    email: String,
    display_name: Option<String>,
}
async fn exchange(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ExchangeRequest>,
) -> Result<Json<TokenResponse>, AuthError> {
    require_service(&state, &headers)?;
    if req.email.len() > 254 || !req.email.contains('@') {
        return Err(AuthError::Forbidden);
    }
    let identity = store::upsert_identity(
        &state.pool,
        &req.email.trim().to_lowercase(),
        req.display_name.as_deref(),
    )
    .await?;
    Ok(Json(mint_session(&state, identity.id.0).await?))
}
fn require_service(state: &AppState, headers: &HeaderMap) -> Result<(), AuthError> {
    match &state.auth.service_key {
        Some(expected) if expected.len() >= 32 && !expected.trim().is_empty() => {
            let presented = headers
                .get("x-dynodoc-service-key")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            // HMAC verification uses ring's constant-time comparison.
            let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, expected.as_bytes());
            let tag = ring::hmac::sign(&key, b"dynodoc-service-auth");
            let candidate = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, presented.as_bytes());
            ring::hmac::verify(&candidate, b"dynodoc-service-auth", tag.as_ref())
                .map_err(|_| AuthError::Forbidden)
        }
        _ => Err(AuthError::Forbidden),
    }
}
