//! dynodoc HTTP API (library crate).
//!
//! The binary ([`main`](../main.rs)) is a thin wrapper that reads configuration from
//! the environment, connects to Postgres, and serves [`app`]. Exposing the app as a
//! library lets integration tests in `tests/` drive the real router and handlers, and
//! lets `engine-cli` emit the OpenAPI spec from the same [`ApiDoc`].
//!
//! Stage 18 lands the full REST + Server-Sent-Events surface on top of the stage-17
//! identity routes: documents (read + create), the write path through the stage-16
//! governance gate (ops, proposals, deploy), a per-document SSE stream, and the public
//! audit endpoints (verify / snapshot / merkle-proof). Cross-cutting: a single
//! [`error::ApiError`], opaque cursor [`pagination`], Postgres-backed [`idempotency`],
//! and a generated [`ApiDoc`] OpenAPI document.

pub mod admin;
mod admin_controls;
pub mod audit;
pub mod auth;
pub mod conflicts;
mod discussions;
pub mod documents;
pub mod error;
pub mod idempotency;
pub mod ops;
pub mod pagination;
pub mod product;
pub mod stream;
pub mod workspace;

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;

pub use auth::AuthConfig;
pub use stream::Subscriptions;

/// Shared application state handed to every handler via `axum::extract::State`.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: AuthConfig,
    /// Per-document SSE broadcast registry (live event fan-out).
    pub subscriptions: Subscriptions,
    pub admin_emails: String,
}

impl AppState {
    /// Construct state with a fresh, empty subscription registry.
    pub fn new(pool: PgPool, auth: AuthConfig) -> Self {
        Self {
            pool,
            auth,
            subscriptions: Subscriptions::new(),
            admin_emails: std::env::var("ADMIN_EMAILS").unwrap_or_default(),
        }
    }
}

/// Build the application router with all routes, cross-cutting middleware, and state.
///
/// Middleware (docs/18 §1): gzip compression, a CORS layer (permissive for the
/// local-only PoC — tighten origins for a deployment), and an OpenTelemetry-compatible
/// `TraceLayer`. Rate limiting is intentionally minimal in the PoC (see the module-level
/// note in [`idempotency`]); idempotency is implemented fully.
pub fn app(state: AppState) -> Router {
    // PoC: permissive CORS (local-only). A deployment restricts `allow_origin` to the
    // known frontend origins (docs/18 "restrict to known origins").
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any);

    Router::new()
        .route("/healthz", get(healthz))
        .route("/openapi.json", get(openapi_json))
        .merge(auth::router())
        .merge(admin::router())
        .merge(product::router())
        .merge(documents::router())
        .merge(ops::router())
        .merge(conflicts::router())
        .merge(stream::router())
        .merge(audit::router())
        .merge(workspace::router())
        .merge(discussions::router())
        .with_state(state)
        .layer(cors)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}

/// Liveness probe. Returns 200 with a small JSON body.
async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "engine-api" }))
}

/// Serve the generated OpenAPI document as JSON (the committed `openapi.yaml` is the
/// source of truth for codegen; this endpoint is a convenience for tooling).
async fn openapi_json() -> Json<Value> {
    Json(serde_json::to_value(ApiDoc::openapi()).unwrap_or_else(|_| json!({})))
}

/// The OpenAPI document, derived from the annotated handlers (docs/18 §3). `engine-cli
/// openapi` emits this to `openapi.yaml`; CI fails if the committed file drifts.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "dynodoc engine API",
        description = "Document version control, collaborative review and authorized administration. Canonical content history is append-only except explicit audited whole-document erasure.",
        version = "0.1.0"
    ),
    modifiers(&WorkspaceDocs),
    paths(
        documents::routes::list_documents,
        documents::routes::get_document,
        documents::routes::get_node,
        documents::routes::list_events,
        documents::routes::create_document,
        ops::routes::op_node_create,
        ops::routes::op_field_edit,
        ops::routes::op_comment,
        ops::routes::op_suggest,
        ops::routes::accept,
        ops::routes::reject,
        ops::routes::deploy,
        conflicts::routes::list_conflicts,
        conflicts::routes::resolve,
        audit::verify,
        audit::get_snapshot,
        audit::merkle_proof,
    ),
    components(schemas(
        documents::routes::CreateDocumentRequest,
        ops::routes::NodeCreateRequest,
        ops::routes::FieldEditRequest,
        ops::routes::CommentRequest,
        ops::routes::SuggestRequest,
        ops::routes::RejectRequest,
        conflicts::routes::ResolveRequest,
        conflicts::derive::ConflictView,
        conflicts::derive::ConflictOption,
    )),
    tags(
        (name = "documents", description = "Document read + create"),
        (name = "operations", description = "The write path (governance-gated)"),
        (name = "conflicts", description = "Four-card conflict resolution"),
        (name = "audit", description = "Member-only hash-chain audit endpoints"),
    )
)]
pub struct ApiDoc;

struct WorkspaceDocs;
impl utoipa::Modify for WorkspaceDocs {
    fn modify(&self, api: &mut utoipa::openapi::OpenApi) {
        api.merge(workspace::WorkspaceApi::openapi());
        api.merge(product::ProductApi::openapi());
        api.merge(admin::AdminApi::openapi());
        api.merge(admin_controls::AdminControlsApi::openapi());
        api.merge(discussions::DiscussionApi::openapi());
        let components = api.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "paseto",
            utoipa::openapi::security::SecurityScheme::Http(
                utoipa::openapi::security::HttpBuilder::new()
                    .scheme(utoipa::openapi::security::HttpAuthScheme::Bearer)
                    .bearer_format("PASETO v4.local")
                    .build(),
            ),
        );
    }
}
