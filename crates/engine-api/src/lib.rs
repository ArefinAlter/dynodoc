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

pub mod access;
pub mod admin;
mod admin_controls;
pub mod agent;
pub mod audit;
pub mod auth;
pub mod conflicts;
pub mod copies;
pub mod counters;
mod device_sessions;
mod devices;
mod discussions;
pub mod documents;
mod draft_storage;
pub mod error;
mod history;
pub mod idempotency;
pub mod notifications;
pub mod ops;
pub mod pagination;
pub mod people;
pub mod product;
mod profiles;
mod project_activity;
mod project_history;
mod project_issues;
mod project_staging;
pub mod projects;
pub mod provenance;
pub mod reviews;
pub mod stream;
pub mod workloads;
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
    /// Wakes the background copy-detection indexer ([`copies::spawn`]).
    pub copy_index: copies::CopyIndex,
    pub workloads: workloads::Workloads,
    /// Error responses counted by code since start (admin overview).
    pub counters: counters::Counters,
    /// Explicitly loaded secret outside the database; required for shared ref writes.
    pub ref_signer: Option<std::sync::Arc<ring::signature::Ed25519KeyPair>>,
    /// Explicit opt-in; new draft/rebase bases use typed shared objects.
    pub shared_draft_bases: bool,
    /// Explicit opt-in; named versions, public pins and deploys share typed state.
    pub named_snapshot_storage: engine_core::snapshot::NamedStorage,
    /// `AGENT_GATEWAY=on` mounts the agent data plane ([`agent`]); anything else leaves
    /// its routes absent. Off by default.
    pub agent_gateway: bool,
    /// Bounded cache of verified document states for the agent routes.
    pub agent_cache: agent::StateCache,
    /// The immutable installation ID, read once for device-session challenges.
    device_authority: std::sync::Arc<tokio::sync::OnceCell<uuid::Uuid>>,
}

impl AppState {
    /// Construct state with a fresh, empty subscription registry.
    pub fn new(pool: PgPool, auth: AuthConfig) -> Self {
        Self {
            pool,
            auth,
            subscriptions: Subscriptions::new(),
            admin_emails: std::env::var("ADMIN_EMAILS").unwrap_or_default(),
            copy_index: copies::CopyIndex::default(),
            workloads: workloads::Workloads::default(),
            counters: Default::default(),
            ref_signer: None,
            shared_draft_bases: std::env::var("DYNODOC_SHARED_DRAFT_BASES").as_deref()
                == Ok("shared-v2"),
            named_snapshot_storage: if std::env::var("DYNODOC_NAMED_SNAPSHOT_STORAGE").as_deref()
                == Ok("shared-v2")
            {
                engine_core::snapshot::NamedStorage::SharedV2
            } else {
                engine_core::snapshot::NamedStorage::Legacy
            },
            agent_gateway: std::env::var("AGENT_GATEWAY").as_deref() == Ok("on"),
            agent_cache: Default::default(),
            device_authority: Default::default(),
        }
    }
}

/// Build the application router with all routes, cross-cutting middleware, and state.
///
/// Middleware (docs/18 §1): gzip compression, a CORS layer (permissive for the
/// local-only PoC — tighten origins for a deployment), and an OpenTelemetry-compatible
/// `TraceLayer`. Process-local admission/deadline controls are in [`workloads`];
/// these do not replace tenant quotas at a multi-instance gateway.
pub fn app(state: AppState) -> Router {
    // PoC: permissive CORS (local-only). A deployment restricts `allow_origin` to the
    // known frontend origins (docs/18 "restrict to known origins").
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any);

    let routes = Router::new()
        .route("/healthz", get(healthz))
        .route("/openapi.json", get(openapi_json))
        .merge(auth::router())
        .merge(admin::router())
        .merge(product::router())
        .merge(projects::router())
        .merge(project_activity::router())
        .merge(project_staging::router())
        .merge(project_history::router())
        .merge(project_issues::router())
        .merge(profiles::router())
        .merge(devices::router())
        .merge(device_sessions::router())
        .merge(provenance::router())
        .merge(documents::router())
        .merge(ops::router())
        .merge(conflicts::router())
        .merge(stream::router())
        .merge(audit::router())
        .merge(workspace::router())
        .merge(reviews::router())
        .merge(history::router())
        .merge(discussions::router())
        .merge(notifications::router())
        .merge(copies::router())
        .merge(people::router());
    // Off (the default): the agent routes are not mounted at all, so they answer exactly
    // as a path that never existed.
    let routes = if state.agent_gateway {
        routes.merge(agent::router())
    } else {
        routes
    };
    routes
        .layer(axum::middleware::from_fn_with_state(
            state.workloads.clone(),
            workloads::admission,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.counters.clone(),
            counters::count,
        ))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            16 * 1024 * 1024,
        ))
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
        auth::routes::logout,
        profiles::own,
        profiles::read,
        profiles::save,
        profiles::storage,
        profiles::profile_activity,
        profiles::starred,
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
        auth::routes::RefreshRequest,
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
        api.merge(reviews::ReviewApi::openapi());
        api.merge(history::HistoryApi::openapi());
        api.merge(product::ProductApi::openapi());
        api.merge(projects::ProjectsApi::openapi());
        api.merge(project_activity::ProjectActivityApi::openapi());
        api.merge(provenance::ProvenanceApi::openapi());
        api.merge(admin::AdminApi::openapi());
        api.merge(admin_controls::AdminControlsApi::openapi());
        api.merge(discussions::DiscussionApi::openapi());
        api.merge(notifications::NotificationApi::openapi());
        api.merge(copies::CopiesApi::openapi());
        api.merge(people::PeopleApi::openapi());
        api.merge(devices::DevicesApi::openapi());
        api.merge(device_sessions::DeviceSessionsApi::openapi());
        api.merge(project_staging::ProjectStagingApi::openapi());
        api.merge(project_history::ProjectHistoryApi::openapi());
        api.merge(project_issues::ProjectIssuesApi::openapi());
        api.merge(agent::AgentApi::openapi());
        // A workspace that declares no licence (`license-file` only) leaves the derived name
        // empty; an empty licence object is invalid OpenAPI, so drop it.
        if api.info.license.as_ref().is_some_and(|l| l.name.is_empty()) {
            api.info.license = None;
        }
        let components = api.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "connector_key",
            utoipa::openapi::security::SecurityScheme::Http(
                utoipa::openapi::security::HttpBuilder::new()
                    .scheme(utoipa::openapi::security::HttpAuthScheme::Bearer)
                    .bearer_format("64 hexadecimal characters; file-scoped editor key")
                    .build(),
            ),
        );
        // Scopes are role names (OpenAPI 3.1): `project:read`, `history:read`,
        // `history:write` and `history:review`. Only the allow-listed project
        // operations list it.
        components.add_security_scheme(
            "device_session",
            utoipa::openapi::security::SecurityScheme::Http(
                utoipa::openapi::security::HttpBuilder::new()
                    .scheme(utoipa::openapi::security::HttpAuthScheme::Bearer)
                    .bearer_format("PASETO v4.local project device session; one project, 1 hour")
                    .build(),
            ),
        );
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
