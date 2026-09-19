//! The conflict HTTP surface (docs/21 item 4).
//!
//! - `GET  /documents/{id}/conflicts` `[auth]` — derive and list the live four-card
//!   conflicts (concurrent pending suggestions disagreeing on a field).
//! - `POST /documents/{id}/conflicts/{conflict_id}/resolve` `[Author]` — resolve one,
//!   either by accepting a competing proposal or by writing a custom value.
//!
//! Conflicts are never stored; both endpoints re-derive them from current materialized
//! state, so a `conflict_id` always maps to the *current* conflict for its
//! `(node_id, field)` — or 404s once the field no longer diverges.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use engine_shared::{DocumentId, IdentityId};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::apply::{resolve_conflict, Resolution};
use super::derive::{self, ConflictView};
use crate::auth::AuthContext;
use crate::documents::store as doc_store;
use crate::error::ApiError;
use crate::AppState;
use engine_core::snapshot::SnapshotEngine;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents/:id/conflicts", get(list_conflicts))
        .route(
            "/documents/:id/conflicts/:conflict_id/resolve",
            post(resolve),
        )
}

/// `POST .../resolve` body. Exactly one of the two fields must be set.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ResolveRequest {
    /// Accept this competing proposal's value as canonical.
    #[serde(default)]
    pub accept_suggestion_id: Option<String>,
    /// Override all proposals with this custom value.
    #[serde(default)]
    pub custom_value: Option<Value>,
}

/// List the live four-card conflicts for a document. `[auth]`.
#[utoipa::path(
    get,
    path = "/documents/{id}/conflicts",
    params(("id" = Uuid, Path, description = "Document id")),
    responses(
        (status = 200, description = "Live conflicts, `{ items: [ConflictView...] }`", body = serde_json::Value),
        (status = 404, description = "No such document")
    ),
    security(("bearer" = [])),
    tag = "conflicts"
)]
pub async fn list_conflicts(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let document_id = DocumentId(id);
    crate::ops::apply::require_role(&state.pool, document_id, IdentityId(auth.identity_id)).await?;
    if !doc_store::document_exists(&state.pool, document_id).await? {
        return Err(ApiError::NotFound);
    }

    let current = SnapshotEngine::new(state.pool.clone())
        .read_current_state(document_id)
        .await?;
    let mut conflicts = derive::derive_conflicts(&current);

    // Enrich each option with the actor that proposed it (the materializer does not
    // carry the proposer, so read it from the log's SuggestionProposed events).
    if !conflicts.is_empty() {
        let proposers = suggestion_proposers(&state, document_id).await?;
        attach_actors(&mut conflicts, &proposers);
    }

    Ok(Json(json!({ "items": conflicts })))
}

/// Resolve one conflict. `[Author]`.
#[utoipa::path(
    post,
    path = "/documents/{id}/conflicts/{conflict_id}/resolve",
    request_body = ResolveRequest,
    params(
        ("id" = Uuid, Path, description = "Document id"),
        ("conflict_id" = String, Path, description = "Opaque conflict id from GET /conflicts")
    ),
    responses(
        (status = 200, description = "The appended canonical events, `{ events: [...] }`", body = serde_json::Value),
        (status = 400, description = "Neither/both decision fields set, or an unknown suggestion id"),
        (status = 403, description = "Not the document author"),
        (status = 404, description = "No such document or live conflict")
    ),
    security(("bearer" = [])),
    tag = "conflicts"
)]
pub async fn resolve(
    State(state): State<AppState>,
    Path((id, conflict_id)): Path<(Uuid, String)>,
    _headers: HeaderMap,
    auth: AuthContext,
    Json(req): Json<ResolveRequest>,
) -> Result<Json<Value>, ApiError> {
    let document_id = DocumentId(id);
    crate::ops::apply::require_role(&state.pool, document_id, IdentityId(auth.identity_id)).await?;
    if !doc_store::document_exists(&state.pool, document_id).await? {
        return Err(ApiError::NotFound);
    }
    let actor = IdentityId(auth.identity_id);

    // Exactly one of the two decision fields must be present.
    let resolution = match (req.accept_suggestion_id, req.custom_value) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(ApiError::BadRequest {
                reason: "set exactly one of accept_suggestion_id or custom_value".into(),
            });
        }
        (Some(suggestion_id), None) => Resolution::Accept { suggestion_id },
        (None, Some(value)) => Resolution::Custom { value },
    };

    let events = resolve_conflict(&state, document_id, actor, &conflict_id, resolution).await?;
    Ok(Json(json!({ "events": events })))
}

/// Map each `SuggestionProposed` suggestion id → the actor that proposed it.
async fn suggestion_proposers(
    state: &AppState,
    document_id: DocumentId,
) -> Result<HashMap<String, Uuid>, ApiError> {
    let rows: Vec<(String, Uuid)> = sqlx::query_as(
        "select payload->>'suggestion_id' as suggestion_id, actor_id
         from event
         where document_id = $1 and type = 'SuggestionProposed'",
    )
    .bind(document_id.0)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Fill in each option's `actor` from the proposer map (best-effort: an option with no
/// matching SuggestionProposed row is left actor-less rather than failing the read).
fn attach_actors(conflicts: &mut [ConflictView], proposers: &HashMap<String, Uuid>) {
    for conflict in conflicts {
        for option in &mut conflict.options {
            option.actor = proposers.get(&option.suggestion_id).copied();
        }
    }
}
