//! dynodoc API server (binary entry point).
//!
//! Reads configuration, verifies the schema and restricted Postgres login, and
//! serves [`engine_api::app`]. The server exposes the full REST + SSE
//! surface (stage 18) on top of the `/auth/*` identity routes (stage 17): documents,
//! the governance-gated write path, the per-document event stream, and the public
//! audit endpoints.

use std::net::SocketAddr;

use anyhow::Context as _;
use engine_api::{app, config::StartupConfig, AppState};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,engine_api=debug".into()),
        )
        .init();

    // Fail before connecting, listing every configuration problem at once: missing
    // configuration is never a dev bypass.
    let StartupConfig {
        auth: config,
        database_url,
        retention,
    } = StartupConfig::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|connection, _| {
            Box::pin(async move {
                for statement in [
                    "set search_path = pg_catalog, public, pg_temp",
                    "set statement_timeout = '15s'",
                    "set lock_timeout = '5s'",
                    "set idle_in_transaction_session_timeout = '30s'",
                ] {
                    sqlx::query(statement).execute(&mut *connection).await?;
                }
                Ok(())
            })
        })
        .connect(database_url.expose())
        .await
        .context("could not connect to Postgres")?;

    engine_core::database::check_runtime(&pool)
        .await
        .context("runtime database boundary check failed")?;

    let mut state = AppState::new(pool, config);
    if let Some(path) = std::env::var_os("DYNODOC_REF_SIGNING_KEY_FILE") {
        use engine_core::repository::server_authority;
        let key = server_authority::load_key(std::path::Path::new(&path))?;
        let mut authority_connection = state.pool.acquire().await?;
        // Fail closed: the mounted key must be the newest authority epoch's key.
        // A revoked epoch still starts (everything else must keep working) but
        // issues no receipt until the operator installs a recovery epoch.
        let active = server_authority::check_signer(&mut authority_connection, &key).await?;
        if let Some(marker) = &active.revocation {
            tracing::warn!(epoch = active.epoch, reason = %marker.reason, "receipt authority epoch is revoked; shared branches cannot advance");
        }
        state.ref_signer = Some(std::sync::Arc::new(key));
    }
    // Copy detection runs beside the edit path, never inside it.
    engine_api::copies::spawn(state.clone());
    // Expired sign-in rows and old metrics are pruned on one replica at a time.
    let _ = engine_api::retention::spawn(state.clone(), retention);

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
