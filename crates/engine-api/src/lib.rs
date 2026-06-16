//! dynodoc HTTP API (library crate).
//!
//! The binary ([`main`](../main.rs)) is a thin wrapper that reads configuration from
//! the environment, connects to Postgres, and serves [`app`]. Exposing the app as a
//! library lets integration tests in `tests/` drive the real router and handlers.
//!
//! Stage 17 ships the identity surface (`/auth/*`). The document/event REST + SSE
//! surface lands in stage 18.

pub mod auth;

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use sqlx::PgPool;

pub use auth::AuthConfig;

/// Shared application state handed to every handler via `axum::extract::State`.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: AuthConfig,
}

/// Build the application router with all routes mounted and state attached.
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .merge(auth::router())
        .with_state(state)
}

/// Liveness probe. Returns 200 with a small JSON body.
async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "engine-api" }))
}
