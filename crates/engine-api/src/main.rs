//! dynodoc API server (binary entry point).
//!
//! Reads configuration from the environment, connects to Postgres, applies pending
//! migrations, and serves [`engine_api::app`]. The server exposes the full REST + SSE
//! surface (stage 18) on top of the `/auth/*` identity routes (stage 17): documents,
//! the governance-gated write path, the per-document event stream, and the public
//! audit endpoints.

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

    let config = load_auth_config()?;
    if std::env::var("APP_ENV").as_deref() == Ok("production")
        && config.service_key.as_ref().is_none_or(|s| s.len() < 32)
    {
        anyhow::bail!("Production requires WEB_SERVICE_KEY of at least 32 characters");
    }
    let state = AppState::new(pool, config);

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
    if paseto_key == [0u8; 32] && std::env::var("APP_ENV").as_deref() == Ok("production") {
        anyhow::bail!("Production requires a random PASETO_LOCAL_KEY");
    }

    let token_ttl_seconds = std::env::var("TOKEN_TTL_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(86_400);
    let public_base_url = std::env::var("PUBLIC_API_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

    Ok(AuthConfig {
        paseto_key,
        service_key: std::env::var("WEB_SERVICE_KEY").ok(),
        token_ttl_seconds,
        public_base_url,
    })
}
