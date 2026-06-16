//! dynodoc API server.
//!
//! Stage 12 scaffolding: boots Axum and serves a single `/healthz` endpoint.
//! The real REST + SSE surface lands in stage 18.

use std::net::SocketAddr;

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,engine_api=debug".into()),
        )
        .init();

    let app = Router::new().route("/healthz", get(healthz));

    let host = std::env::var("API_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("API_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    tracing::info!(%addr, "engine-api listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Liveness probe. Returns 200 with a small JSON body.
async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "engine-api" }))
}
