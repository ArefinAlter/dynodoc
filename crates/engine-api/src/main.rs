//! dynodoc API server (binary entry point).
//!
//! Reads configuration from the environment, connects to Postgres, applies pending
//! migrations, and serves [`engine_api::app`]. Stage 17 adds the `/auth/*` identity
//! routes (email magic link → Paseto v4 access token); the full REST + SSE surface
//! lands in stage 18.

use std::net::SocketAddr;

use anyhow::Context as _;
use engine_api::{app, AppState, AuthConfig};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,engine_api=debug".into()),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL is not set (see .env / SETUP.md)")?;
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&database_url)
        .await
        .context("could not connect to Postgres")?;

    // Apply any pending migrations on boot so a fresh database is usable immediately.
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .context("running database migrations failed")?;

    let state = AppState {
        pool,
        auth: load_auth_config()?,
    };

    let host = std::env::var("API_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("API_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    tracing::info!(%addr, "engine-api listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app(state)).await?;
    Ok(())
}

/// Load the auth configuration from the environment (Paseto key + token lifetimes).
fn load_auth_config() -> anyhow::Result<AuthConfig> {
    let key_hex = std::env::var("PASETO_LOCAL_KEY")
        .context("PASETO_LOCAL_KEY is not set (32 bytes, hex-encoded)")?;
    let key_bytes = hex::decode(key_hex.trim()).context("PASETO_LOCAL_KEY must be hex-encoded")?;
    let paseto_key: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("PASETO_LOCAL_KEY must be 32 bytes (64 hex chars)"))?;
    if paseto_key == [0u8; 32] {
        tracing::warn!(
            "PASETO_LOCAL_KEY is the all-zero placeholder — fine for local dev, never for a deployment"
        );
    }

    let token_ttl_seconds = std::env::var("TOKEN_TTL_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(86_400);
    let public_base_url = std::env::var("PUBLIC_API_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

    Ok(AuthConfig {
        paseto_key,
        token_ttl_seconds,
        public_base_url,
    })
}
