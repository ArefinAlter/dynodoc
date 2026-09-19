//! The write path HTTP surface (docs/18 §2 "Operations").
//!
//! Each endpoint maps a typed request body to an [`EventPayload`], then drives the
//! shared [`apply`] funnel (governance gate → materialize → append → snapshot →
//! publish). Every write accepts an optional `Idempotency-Key` header: a replayed key
//! returns the first response without re-executing (so a retried POST never duplicates
//! an event), via [`crate::idempotency`].
//!
//! Tier annotations (docs/18) are enforced by the governance capability matrix:
//! `node-create`/`field-edit`/`comment`/`suggest` and the proposal/deploy acts each
//! resolve the actor's [`engine_core::governance::Role`] from the access list and pass
//! it to the gate. A `vote` op is **out of PoC scope** — the PoC `EventPayload`
//! vocabulary has no `VoteCast` (consultation-layer, docs/16) — so no `/ops/vote`
//! route is mounted here.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use engine_shared::{DocumentId, EventPayload, IdentityId, NodeId, NodeType};
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa::ToSchema;
use uuid::Uuid;

use super::apply;
use crate::auth::AuthContext;
use crate::documents::store as doc_store;
use crate::error::ApiError;
use crate::{idempotency, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents/:id/ops/node-create", post(op_node_create))
        .route("/documents/:id/ops/field-edit", post(op_field_edit))
        .route("/documents/:id/ops/comment", post(op_comment))
        .route("/documents/:id/ops/suggest", post(op_suggest))
        .route("/documents/:id/proposals/:event_id/accept", post(accept))
        .route("/documents/:id/proposals/:event_id/reject", post(reject))
        .route("/documents/:id/deploy", post(deploy))
}

// --- request bodies -------------------------------------------------------------

/// `POST /ops/node-create`. `node_id` is optional; absent, the server mints a ULID.
#[derive(Debug, Deserialize, ToSchema)]
pub struct NodeCreateRequest {
    #[serde(default)]
    pub node_id: Option<String>,
    #[schema(value_type = String)]
    pub node_type: NodeType,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub pos: String,
    #[serde(default)]
    pub fields: Value,
    #[serde(default)]
    pub var_name: Option<String>,
}

/// `POST /ops/field-edit`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct FieldEditRequest {
    #[serde(default)]
    pub base_seq: Option<i64>,
    pub node_id: String,
    pub field: String,
    pub value: Value,
}

/// `POST /ops/comment`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CommentRequest {
    #[serde(default)]
    pub node_id: Option<String>,
    pub body: String,
}

/// `POST /ops/suggest`. `detail` is the wrapped content op (internally-tagged JSON).
#[derive(Debug, Deserialize, ToSchema)]
pub struct SuggestRequest {
    #[serde(default)]
    pub suggestion_id: Option<String>,
    #[serde(default)]
    pub target_node_id: Option<String>,
    pub detail: Value,
}

/// `POST /proposals/:event_id/reject`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct RejectRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

// --- handlers -------------------------------------------------------------------

/// Create a node. `[T2+]` (Author/Reviewer-via-suggest in the PoC role model).
#[utoipa::path(
    post, path = "/documents/{id}/ops/node-create",
    request_body = NodeCreateRequest,
    params(("id" = Uuid, Path, description = "Document id")),
    responses((status = 200, description = "The appended event", body = serde_json::Value)),
    security(("bearer" = [])), tag = "operations"
)]
pub async fn op_node_create(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    auth: AuthContext,
    Json(req): Json<NodeCreateRequest>,
) -> Result<Json<Value>, ApiError> {
    let document_id = DocumentId(id);
    let node_id = NodeId(req.node_id.unwrap_or_else(|| ulid::Ulid::new().to_string()));
    let op = EventPayload::NodeCreated {
        node_id,
        node_type: req.node_type,
        parent_id: req.parent_id.map(NodeId),
        pos: req.pos,
        fields: req.fields,
        var_name: req.var_name.filter(|v| !v.is_empty()),
    };
    write(&state, &headers, document_id, auth, "ops/node-create", op).await
}

/// Edit a single field of a node. `[T2+]`.
#[utoipa::path(
    post, path = "/documents/{id}/ops/field-edit",
    request_body = FieldEditRequest,
    params(("id" = Uuid, Path, description = "Document id")),
    responses((status = 200, description = "The appended event", body = serde_json::Value)),
    security(("bearer" = [])), tag = "operations"
)]
pub async fn op_field_edit(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    auth: AuthContext,
    Json(req): Json<FieldEditRequest>,
) -> Result<Json<Value>, ApiError> {
    let op = EventPayload::FieldEdited {
        node_id: NodeId(req.node_id),
        field: req.field,
        value: req.value,
    };
    let actor = IdentityId(auth.identity_id);
    with_idempotency(
        &state,
        &headers,
        actor,
        "ops/field-edit",
        DocumentId(id),
        || async {
            let event =
                apply::apply_checked_op(&state, DocumentId(id), actor, op, req.base_seq).await?;
            Ok(json!({ "event": event }))
        },
    )
    .await
}

/// Add a comment, optionally anchored to a node. `[T2+]`.
#[utoipa::path(
    post, path = "/documents/{id}/ops/comment",
    request_body = CommentRequest,
    params(("id" = Uuid, Path, description = "Document id")),
    responses((status = 200, description = "The appended event", body = serde_json::Value)),
    security(("bearer" = [])), tag = "operations"
)]
pub async fn op_comment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    auth: AuthContext,
    Json(req): Json<CommentRequest>,
) -> Result<Json<Value>, ApiError> {
    let op = EventPayload::CommentAdded {
        node_id: req.node_id.map(NodeId),
        body: req.body,
    };
    write(&state, &headers, DocumentId(id), auth, "ops/comment", op).await
}

/// Propose a wrapped change without applying it (the suggesting workflow). `[T2+]`.
#[utoipa::path(
    post, path = "/documents/{id}/ops/suggest",
    request_body = SuggestRequest,
    params(("id" = Uuid, Path, description = "Document id")),
    responses((status = 200, description = "The appended proposal event", body = serde_json::Value)),
    security(("bearer" = [])), tag = "operations"
)]
pub async fn op_suggest(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    auth: AuthContext,
    Json(req): Json<SuggestRequest>,
) -> Result<Json<Value>, ApiError> {
    let op = EventPayload::SuggestionProposed {
        suggestion_id: req
            .suggestion_id
            .unwrap_or_else(|| ulid::Ulid::new().to_string()),
        target_node_id: req.target_node_id.map(NodeId),
        detail: req.detail,
    };
    write(&state, &headers, DocumentId(id), auth, "ops/suggest", op).await
}

/// Accept a pending proposal (atomic: applies the wrapped op + records acceptance).
/// `[Sponsor]` (Author in the PoC role model). The `:event_id` path segment is the
/// suggestion id.
#[utoipa::path(
    post, path = "/documents/{id}/proposals/{event_id}/accept",
    params(
        ("id" = Uuid, Path, description = "Document id"),
        ("event_id" = String, Path, description = "Suggestion id")
    ),
    responses((status = 200, description = "The canonical wrapped event", body = serde_json::Value)),
    security(("bearer" = [])), tag = "operations"
)]
pub async fn accept(
    State(state): State<AppState>,
    Path((id, suggestion_id)): Path<(Uuid, String)>,
    headers: HeaderMap,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let document_id = DocumentId(id);
    let actor = IdentityId(auth.identity_id);
    with_idempotency(
        &state,
        &headers,
        actor,
        "proposals/accept",
        document_id,
        || async {
            let event = apply::accept_proposal(&state, document_id, actor, &suggestion_id).await?;
            Ok(json!({ "event": event }))
        },
    )
    .await
}

/// Reject a pending proposal (recorded with its reason, never applied). `[Sponsor]`.
#[utoipa::path(
    post, path = "/documents/{id}/proposals/{event_id}/reject",
    request_body = RejectRequest,
    params(
        ("id" = Uuid, Path, description = "Document id"),
        ("event_id" = String, Path, description = "Suggestion id")
    ),
    responses((status = 200, description = "The rejection event", body = serde_json::Value)),
    security(("bearer" = [])), tag = "operations"
)]
pub async fn reject(
    State(state): State<AppState>,
    Path((id, suggestion_id)): Path<(Uuid, String)>,
    headers: HeaderMap,
    auth: AuthContext,
    Json(req): Json<RejectRequest>,
) -> Result<Json<Value>, ApiError> {
    let document_id = DocumentId(id);
    let actor = IdentityId(auth.identity_id);
    with_idempotency(
        &state,
        &headers,
        actor,
        "proposals/reject",
        document_id,
        || async {
            let event = apply::reject_proposal(
                &state,
                document_id,
                actor,
                &suggestion_id,
                req.reason.clone(),
            )
            .await?;
            Ok(json!({ "event": event }))
        },
    )
    .await
}

/// Mark a deployed checkpoint (runs the referential-integrity gate, pins a snapshot).
/// `[Sponsor]`.
#[utoipa::path(
    post, path = "/documents/{id}/deploy",
    params(("id" = Uuid, Path, description = "Document id")),
    responses(
        (status = 200, description = "The Deployed event", body = serde_json::Value),
        (status = 409, description = "Deploy refused: referential-integrity violations")
    ),
    security(("bearer" = [])), tag = "operations"
)]
pub async fn deploy(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let op = EventPayload::Deployed { snapshot_id: None };
    write(&state, &headers, DocumentId(id), auth, "deploy", op).await
}

// --- shared write plumbing ------------------------------------------------------

/// Run a content/governance op through the apply funnel, wrapped in idempotency. The
/// document must exist (else 404 before any gate).
async fn write(
    state: &AppState,
    headers: &HeaderMap,
    document_id: DocumentId,
    auth: AuthContext,
    path: &str,
    op: EventPayload,
) -> Result<Json<Value>, ApiError> {
    let actor = IdentityId(auth.identity_id);
    with_idempotency(state, headers, actor, path, document_id, || async {
        let event = apply::apply_op(state, document_id, actor, op.clone()).await?;
        Ok(json!({ "event": event }))
    })
    .await
}

/// Wrap a write in the idempotency-key protocol: replay a cached response for a seen
/// key, otherwise reserve → run → cache. Without a key header the body just runs.
async fn with_idempotency<F, Fut>(
    state: &AppState,
    headers: &HeaderMap,
    actor: IdentityId,
    path: &str,
    document_id: DocumentId,
    run: F,
) -> Result<Json<Value>, ApiError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Value, ApiError>>,
{
    // A write to a missing document is a 404 regardless of idempotency.
    if !doc_store::document_exists(&state.pool, document_id).await? {
        return Err(ApiError::NotFound);
    }

    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let Some(key) = key else {
        // No key: just run the write.
        return run().await.map(Json);
    };

    apply::require_role(&state.pool, document_id, actor).await?;
    let path = format!("/documents/{}/{path}", document_id.0);
    match idempotency::reserve(&state.pool, actor.0, &key, &path).await? {
        idempotency::Reservation::Replay { body, .. } => Ok(Json(body)),
        idempotency::Reservation::InFlight => Err(ApiError::Conflict {
            reason: "a request with this idempotency key is in progress".into(),
        }),
        idempotency::Reservation::Fresh => match run().await {
            Ok(body) => {
                // 200 is the only success status the write handlers return.
                idempotency::complete(&state.pool, actor.0, &key, 200, &body).await?;
                Ok(Json(body))
            }
            Err(err) => {
                // Release the reservation so a retry can run rather than stall.
                idempotency::release(&state.pool, actor.0, &key).await;
                Err(err)
            }
        },
    }
}
