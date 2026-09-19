//! The `/documents` REST surface (docs/18 §2 "Documents").
//!
//! Reads (`list`, `get`, `node`) are `[document member]` and require a valid token; they serve materialized
//! current state via the snapshot engine. The raw event stream is `[auth]`. Creating a
//! document requires a valid identity (the creator becomes its `Author`).

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use engine_core::materializer::{DocumentState, MaterializedNode};
use engine_core::snapshot::SnapshotEngine;
use engine_shared::{Document, DocumentId, Event, NodeId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::store;
use crate::auth::AuthContext;
use crate::error::ApiError;
use crate::ops::apply::require_role;
use crate::pagination::{Page, PageParams};
use crate::AppState;
use engine_shared::IdentityId;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents", get(list_documents).post(create_document))
        .route("/documents/:id", get(get_document))
        .route("/documents/:id/node/:node_id", get(get_node))
        .route("/documents/:id/events", get(list_events))
}

// --- list -----------------------------------------------------------------------

/// `?status=` filter for the public document index.
///
/// `cursor`/`limit` are listed inline (not `#[serde(flatten)]` a [`PageParams`]):
/// `serde_urlencoded` buffers flattened values as strings, so a flattened
/// `limit: Option<i64>` fails to deserialize from `?limit=2`. Top-level fields coerce
/// fine. [`ListQuery::page`] rebuilds the [`PageParams`] for its helper methods.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

impl ListQuery {
    fn page(&self) -> PageParams {
        PageParams {
            cursor: self.cursor.clone(),
            limit: self.limit,
        }
    }
}

/// List accessible (non-archived) documents, cursor-paginated. `[document member]`.
#[utoipa::path(
    get,
    path = "/documents",
    params(
        ("status" = Option<String>, Query, description = "Filter by lifecycle status"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("limit" = Option<i64>, Query, description = "Page size (1..=1000)")
    ),
    responses((status = 200, description = "A page of documents, `{ items, next_cursor }`", body = serde_json::Value)),
    tag = "documents"
)]
pub async fn list_documents(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
    auth: AuthContext,
) -> Result<Json<Page<Document>>, ApiError> {
    let page = query.page();
    let cursor = page.decoded_cursor().map_err(|_| ApiError::BadRequest {
        reason: "invalid cursor".into(),
    })?;
    let limit = page.effective_limit();

    let rows = store::list_documents(
        &state.pool,
        auth.identity_id,
        query.status.as_deref(),
        cursor.as_ref(),
        limit,
    )
    .await?;

    // A full page implies there may be more; hand back a cursor to the last row.
    let next = if rows.len() as i64 == limit {
        rows.last().map(store::document_cursor)
    } else {
        None
    };
    Ok(Json(Page::new(rows, next)))
}

// --- get document ---------------------------------------------------------------

/// Full document metadata plus its current materialized state.
#[derive(Debug, Serialize)]
pub struct DocumentDetail {
    #[serde(flatten)]
    pub document: Document,
    /// The folded current state (nodes + comment/suggestion side-state).
    pub state: DocumentState,
    /// The event `seq` `state` was folded through (0 for an empty log). Clients resume
    /// the SSE stream from here so the backfill carries only newer events — no second
    /// request, no re-fold, no missed window.
    pub latest_seq: i64,
    pub role: String,
}

/// Document metadata + current materialized state. `[document member]`.
#[utoipa::path(
    get,
    path = "/documents/{id}",
    params(("id" = Uuid, Path, description = "Document id")),
    responses(
        (status = 200, description = "Document detail (metadata + materialized state)", body = serde_json::Value),
        (status = 404, description = "No such document")
    ),
    tag = "documents"
)]
pub async fn get_document(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<Json<DocumentDetail>, ApiError> {
    let document_id = DocumentId(id);
    require_role(&state.pool, document_id, IdentityId(auth.identity_id)).await?;
    let document = store::get_document(&state.pool, document_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let (current, latest_seq) = SnapshotEngine::new(state.pool.clone())
        .read_current_state_with_seq(document_id)
        .await?;

    Ok(Json(DocumentDetail {
        document,
        state: current,
        latest_seq,
        role: require_role(&state.pool, document_id, IdentityId(auth.identity_id))
            .await?
            .as_str()
            .into(),
    }))
}

// --- get node -------------------------------------------------------------------

/// A single node's current materialized state. `[document member]`.
#[utoipa::path(
    get,
    path = "/documents/{id}/node/{node_id}",
    params(
        ("id" = Uuid, Path, description = "Document id"),
        ("node_id" = String, Path, description = "Node id (ULID)")
    ),
    responses(
        (status = 200, description = "Node state", body = serde_json::Value),
        (status = 404, description = "No such document or node")
    ),
    tag = "documents"
)]
pub async fn get_node(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((id, node_id)): Path<(Uuid, String)>,
) -> Result<Json<MaterializedNode>, ApiError> {
    let document_id = DocumentId(id);
    require_role(&state.pool, document_id, IdentityId(auth.identity_id)).await?;
    if !store::document_exists(&state.pool, document_id).await? {
        return Err(ApiError::NotFound);
    }

    let current = SnapshotEngine::new(state.pool.clone())
        .read_current_state(document_id)
        .await?;

    current
        .nodes
        .get(&NodeId(node_id))
        .cloned()
        .map(Json)
        .ok_or(ApiError::NotFound)
}

// --- list events ----------------------------------------------------------------

/// `?from_seq=&to_seq=` window for the raw event stream.
#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    pub from_seq: Option<i64>,
    #[serde(default)]
    pub to_seq: Option<i64>,
    // Inline rather than `#[serde(flatten)]`: see [`ListQuery`] — flatten + urlencoded
    // mis-handles the non-string `limit`.
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

impl EventsQuery {
    fn page(&self) -> PageParams {
        PageParams {
            cursor: self.cursor.clone(),
            limit: self.limit,
        }
    }
}

/// Raw, cursor-paginated event log for a document. `[auth]` — the audit stream is for
/// authenticated participants, not the public read path.
#[utoipa::path(
    get,
    path = "/documents/{id}/events",
    params(
        ("id" = Uuid, Path, description = "Document id"),
        ("from_seq" = Option<i64>, Query, description = "Lowest seq to return"),
        ("to_seq" = Option<i64>, Query, description = "Highest seq to return"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("limit" = Option<i64>, Query, description = "Page size (1..=1000)")
    ),
    responses((status = 200, description = "A page of events, `{ items, next_cursor }`", body = serde_json::Value)),
    security(("bearer" = [])),
    tag = "documents"
)]
pub async fn list_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<EventsQuery>,
    auth: AuthContext,
) -> Result<Json<Page<Event>>, ApiError> {
    let document_id = DocumentId(id);
    require_role(&state.pool, document_id, IdentityId(auth.identity_id)).await?;
    if !store::document_exists(&state.pool, document_id).await? {
        return Err(ApiError::NotFound);
    }

    let page = query.page();
    let cursor = page.decoded_cursor().map_err(|_| ApiError::BadRequest {
        reason: "invalid cursor".into(),
    })?;
    let limit = page.effective_limit();

    // The window floor is the max of the explicit from_seq and the cursor position
    // (so a cursor advances strictly past the last returned event).
    let from = query.from_seq.unwrap_or(1).max(1);
    let from = match &cursor {
        Some(c) => from.max(c.last_seq + 1),
        None => from,
    };
    let to = query.to_seq.unwrap_or(i64::MAX);
    if to < from {
        return Ok(Json(Page::new(Vec::new(), None)));
    }

    let mut events = engine_core::log::read_range(&state.pool, document_id, from, to).await?;
    events.truncate(limit as usize);

    let next = if events.len() as i64 == limit {
        events.last().map(|e| crate::pagination::Cursor {
            last_seq: e.seq,
            last_id: e.id.0.clone(),
        })
    } else {
        None
    };
    Ok(Json(Page::new(events, next)))
}

// --- create document ------------------------------------------------------------

/// Body for `POST /documents`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDocumentRequest {
    pub title: String,
    #[serde(default)]
    pub languages: Vec<String>,
}

/// Create a new document; the creator becomes its `Author`. `[Sponsor/creator]`.
#[utoipa::path(
    post,
    path = "/documents",
    request_body = CreateDocumentRequest,
    responses(
        (status = 200, description = "The created document", body = serde_json::Value),
        (status = 400, description = "Invalid body")
    ),
    security(("bearer" = [])),
    tag = "documents"
)]
pub async fn create_document(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<CreateDocumentRequest>,
) -> Result<Json<Document>, ApiError> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest {
            reason: "title must not be empty".into(),
        });
    }
    let doc = store::create_document(&state.pool, title, &req.languages, auth.identity_id).await?;
    Ok(Json(doc))
}
