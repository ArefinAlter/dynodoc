//! Change requests: reviewable personal drafts, like pull requests for documents.
//!
//! A contributor pushes a locally edited file (or submits an existing draft). The
//! draft starts from the revision the file was downloaded at, so a three-way merge
//! compares that ancestor, the current team version and the contributor's work.
//! Editors review, choose overlaps and merge; merging appends ordinary canonical
//! events through the same governance gate as every other write.
use crate::{
    auth::AuthContext,
    error::ApiError,
    ops::apply,
    workspace::{append_to_team, carry_into_draft, load_draft},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use engine_core::{
    governance::{self, Role},
    materializer::{apply_payload, DocumentState, Materializer},
};
use engine_shared::{DocumentId, Event, EventPayload, IdentityId, NodeId, Snapshot};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents/:id/history-points", get(history_points))
        .route("/documents/:id/state", get(state_at))
        .route(
            "/documents/:id/change-requests",
            get(list)
                .post(create)
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route("/documents/:id/change-requests/:draft_id", get(detail))
        .route(
            "/documents/:id/change-requests/:draft_id/merge",
            post(merge),
        )
        .route(
            "/documents/:id/change-requests/:draft_id/decline",
            post(decline),
        )
        .route("/documents/:id/drafts/:draft_id/submit", post(submit))
        .route("/documents/:id/drafts/:draft_id/withdraw", post(withdraw))
}

fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}
fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(e.to_string())
}
async fn latest_seq(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<i64, ApiError> {
    Ok(
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?,
    )
}

/// Canonical state after `through_seq`, from the nearest earlier snapshot plus its tail.
async fn state_through(
    executor: &mut sqlx::PgConnection,
    id: Uuid,
    through_seq: i64,
) -> Result<DocumentState, ApiError> {
    let snapshot = sqlx::query_as::<_, Snapshot>(
        "select * from snapshot where document_id=$1 and through_seq<=$2 order by through_seq desc limit 1",
    )
    .bind(id)
    .bind(through_seq)
    .fetch_optional(&mut *executor)
    .await?;
    let tail = sqlx::query_as::<_, Event>(
        "select * from event where document_id=$1 and seq>$2 and seq<=$3 order by seq",
    )
    .bind(id)
    .bind(snapshot.as_ref().map_or(0, |s| s.through_seq))
    .bind(through_seq)
    .fetch_all(&mut *executor)
    .await?;
    match snapshot {
        Some(s) => Materializer::from_snapshot(&s, &tail).map_err(internal),
        None => Materializer::fold(&tail).map_err(internal),
    }
}

/// Revisions a pushed file may have started from: the latest team version, the first
/// saved state (normally the original upload) and named versions.
#[utoipa::path(get, path="/documents/{id}/history-points", tag="reviews", params(("id" = Uuid, Path, description = "Document identifier")), responses((status=200, description="Latest, initial and named-version revisions", body=Value),(status=403, description="Document membership required")))]
async fn history_points(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let latest: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    // One batch shares its transaction timestamp, so this is the end of the first save.
    let initial: i64 = sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1 and created_at=(select created_at from event where document_id=$1 and seq=1)")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    let versions:Vec<Value>=sqlx::query_scalar("select jsonb_build_object('id',v.id,'name',v.name,'through_seq',s.through_seq,'created_at',v.created_at) from document_version v join snapshot s on s.id=v.snapshot_id where v.document_id=$1 order by v.created_at desc limit 50")
        .bind(id)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(
        json!({"latest_seq":latest,"initial_seq":initial,"versions":versions}),
    ))
}

#[derive(Deserialize)]
struct StateQuery {
    through_seq: i64,
}
#[utoipa::path(get, path="/documents/{id}/state", tag="reviews", params(("id" = Uuid, Path, description = "Document identifier"), ("through_seq" = i64, Query, description = "Revision to materialize")), responses((status=200, description="Canonical state at an earlier revision", body=Value),(status=400, description="Unknown revision"),(status=403, description="Document membership required")))]
async fn state_at(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<StateQuery>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let mut tx = state.pool.begin().await?;
    let latest = latest_seq(&mut tx, id).await?;
    if query.through_seq < 0 || query.through_seq > latest {
        return Err(bad("That revision does not exist in this document"));
    }
    let at = state_through(&mut tx, id, query.through_seq).await?;
    Ok(Json(
        json!({"through_seq":query.through_seq,"latest_seq":latest,"state":at}),
    ))
}

#[derive(Deserialize)]
struct CreateRequest {
    name: String,
    #[serde(default)]
    note: String,
    base_seq: i64,
    ops: Vec<EventPayload>,
    #[serde(default)]
    source: Value,
    #[serde(default)]
    submit: bool,
}
/// Create a draft from an earlier revision plus the pushed file's changes, atomically.
#[utoipa::path(post, path="/documents/{id}/change-requests", tag="reviews", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier")), responses((status=200, description="Draft created and optionally submitted for review", body=Value),(status=400, description="Invalid starting revision or changes"),(status=403, description="Viewers cannot propose changes")))]
async fn create(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<CreateRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.name.trim().is_empty() || req.name.len() > 200 {
        return Err(bad("Name this change in 1–200 characters"));
    }
    if req.note.len() > 4000 {
        return Err(bad("Keep the description under 4,000 characters"));
    }
    if req.ops.is_empty() || req.ops.len() > 20_000 {
        return Err(bad("A pushed file must change 1–20,000 blocks"));
    }
    let mut source = if req.source.is_object() {
        req.source
    } else {
        json!({})
    };
    // Merging moves the draft's base forward; keep the revision the file started from.
    source["start_seq"] = json!(req.base_seq);
    if source.to_string().len() > 8192 {
        return Err(bad("The file description is too large"));
    }
    let doc = DocumentId(id);
    let (mut tx, role, _) = apply::begin_write(&state, doc, IdentityId(auth.identity_id)).await?;
    if role == Role::Auditor {
        return Err(ApiError::Forbidden);
    }
    let latest = latest_seq(&mut tx, id).await?;
    if req.base_seq < 0 || req.base_seq > latest {
        return Err(bad(
            "The file's starting revision does not exist in this document",
        ));
    }
    let base = state_through(&mut tx, id, req.base_seq).await?;
    let draft:Uuid=sqlx::query_scalar("insert into workspace_draft(document_id,name,base_seq,base_state,created_by,source,submitted_at,submission_note,revision) values($1,$2,$3,$4,$5,$6,case when $7 then now() end,$8,1) returning id")
        .bind(id).bind(req.name.trim()).bind(req.base_seq).bind(json!(base)).bind(auth.identity_id).bind(&source).bind(req.submit).bind(req.note.trim()).fetch_one(&mut *tx).await?;
    let mut current = base;
    let mut edits = 0;
    for op in req.ops {
        if !matches!(
            op,
            EventPayload::NodeCreated { .. }
                | EventPayload::FieldEdited { .. }
                | EventPayload::RichTextPatched { .. }
                | EventPayload::NodeMoved { .. }
                | EventPayload::NodeDeleted { .. }
                | EventPayload::NodeRestored { .. }
        ) {
            return Err(bad("A pushed file contains content changes only"));
        }
        governance::authorize(Role::Author, &op, &current)?;
        if engine_core::richtext::redundant_label(&current, &op) {
            continue;
        }
        let op = engine_core::richtext::compact_event(&current, &op);
        apply_payload(&mut current, &op).map_err(internal)?;
        sqlx::query("insert into draft_edit(draft_id,payload,actor_id) values($1,$2,$3)")
            .bind(draft)
            .bind(json!(op))
            .bind(auth.identity_id)
            .execute(&mut *tx)
            .await?;
        edits += 1;
    }
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        if req.submit {
            "document.change_request_submitted"
        } else {
            "document.draft_created_from_file"
        },
        id,
        json!({"draft_id":draft,"base_seq":req.base_seq,"edits":edits,"source":source}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":draft,"revision":1,"edits":edits,"submitted":req.submit}),
    ))
}

const REQUEST_JSON: &str = "jsonb_build_object('id',d.id,'name',d.name,'note',d.submission_note,'source',d.source,'base_seq',d.base_seq,'created_at',d.created_at,'submitted_at',d.submitted_at,'merged_at',d.merged_at,'review_outcome',d.review_outcome,'reviewed_at',d.reviewed_at,'review_note',d.review_note,'revision',d.revision,'author',jsonb_build_object('id',a.id,'name',a.display_name,'email',a.email),'reviewer',case when r.id is null then null else jsonb_build_object('id',r.id,'name',r.display_name,'email',r.email) end,'edits',(select count(*) from draft_edit e where e.draft_id=d.id),'status',case when d.merged_at is not null then 'merged' when d.review_outcome is not null then d.review_outcome else 'open' end)";

/// Submitted change requests are visible to every member; unsubmitted drafts stay private.
#[utoipa::path(get, path="/documents/{id}/change-requests", tag="reviews", params(("id" = Uuid, Path, description = "Document identifier")), responses((status=200, description="Open and recently closed change requests", body=Value),(status=403, description="Document membership required")))]
async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let items: Vec<Value> = sqlx::query_scalar(&format!("select {REQUEST_JSON}||jsonb_build_object('mine',d.created_by=$2) from workspace_draft d join identity a on a.id=d.created_by left join identity r on r.id=d.reviewed_by where d.document_id=$1 and d.submitted_at is not null order by (d.merged_at is null and d.review_outcome is null) desc, d.submitted_at desc limit 200"))
        .bind(id)
        .bind(auth.identity_id)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(json!({"items":items})))
}

async fn request_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    draft: Uuid,
    viewer: Uuid,
) -> Result<Value, ApiError> {
    sqlx::query_scalar(&format!("select {REQUEST_JSON}||jsonb_build_object('mine',d.created_by=$3) from workspace_draft d join identity a on a.id=d.created_by left join identity r on r.id=d.reviewed_by where d.id=$1 and d.document_id=$2 and (d.submitted_at is not null or d.created_by=$3)"))
        .bind(draft)
        .bind(id)
        .bind(viewer)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(ApiError::NotFound)
}

/// The request, the contributor's own changes and what merging would change now.
#[utoipa::path(get, path="/documents/{id}/change-requests/{draft_id}", tag="reviews", params(("id" = Uuid, Path, description = "Document identifier"), ("draft_id" = Uuid, Path, description = "Change request identifier")), responses((status=200, description="Review comparison against the current team version", body=Value),(status=403, description="Document membership required"),(status=404, description="Not submitted or not visible")))]
async fn detail(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let doc = DocumentId(id);
    let (mut tx, _, team) = apply::begin_write(&state, doc, IdentityId(auth.identity_id)).await?;
    let request = request_row(&mut tx, id, draft, auth.identity_id).await?;
    let seq = latest_seq(&mut tx, id).await?;
    // Closed requests keep their record; only open ones have a live comparison.
    if request["status"] != "open" {
        return Ok(Json(json!({"request":request,"team_seq":seq})));
    }
    let ctx = load_draft(&mut tx, id, draft, None).await?;
    let plan = engine_core::workspace_merge::merge(&ctx.base, &team, &ctx.local, &BTreeMap::new());
    let own = engine_core::workspace_merge::diff(&ctx.base, &ctx.local);
    // Before/after of every block the contributor touched, for side-by-side review.
    let touched: BTreeSet<NodeId> = own
        .iter()
        .chain(plan.ops.iter())
        .filter_map(|op| op.target_node_id().cloned())
        .chain(plan.conflicts.iter().map(|c| c.node_id.clone()))
        .collect();
    let nodes: BTreeMap<&str, Value> = touched
        .iter()
        .map(|n| {
            (
                n.0.as_str(),
                json!({"ancestor":ctx.base.nodes.get(n),"draft":ctx.local.nodes.get(n),"team":team.nodes.get(n)}),
            )
        })
        .collect();
    Ok(Json(
        json!({"request":request,"team_seq":seq,"revision":ctx.revision,"ops":plan.ops,"conflicts":plan.conflicts,"own":own,"nodes":nodes,"behind":seq-ctx.row["base_seq"].as_i64().unwrap_or(0)}),
    ))
}

#[derive(Deserialize)]
struct MergeRequest {
    team_seq: i64,
    revision: i64,
    #[serde(default)]
    resolutions: BTreeMap<String, String>,
    included_nodes: Option<Vec<NodeId>>,
    #[serde(default)]
    note: String,
}
/// An editor merges selected changes. Unselected changes stay open in the request.
#[utoipa::path(post, path="/documents/{id}/change-requests/{draft_id}/merge", tag="reviews", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier"), ("draft_id" = Uuid, Path, description = "Change request identifier")), responses((status=200, description="Selected changes appended to the team version", body=Value),(status=403, description="Editors merge change requests"),(status=409, description="Resolve overlaps or review newer changes first")))]
async fn merge(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<MergeRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.note.len() > 4000 {
        return Err(bad("Keep the review note under 4,000 characters"));
    }
    let doc = DocumentId(id);
    let actor = IdentityId(auth.identity_id);
    let (mut tx, role, mut team) = apply::begin_write(&state, doc, actor).await?;
    if role != Role::Author {
        return Err(ApiError::Forbidden);
    }
    let request = request_row(&mut tx, id, draft, auth.identity_id).await?;
    if request["status"] != "open" || request["submitted_at"].is_null() {
        return Err(ApiError::Conflict {
            reason: "This change request is no longer open.".into(),
        });
    }
    let ctx = load_draft(&mut tx, id, draft, None).await?;
    let seq = latest_seq(&mut tx, id).await?;
    if req.team_seq != seq || req.revision != ctx.revision {
        return Err(ApiError::Conflict {
            reason: "The team version or this change request changed. Review the updated comparison before merging.".into(),
        });
    }
    let plan = engine_core::workspace_merge::merge(&ctx.base, &team, &ctx.local, &req.resolutions);
    if !plan.conflicts.is_empty() {
        return Err(ApiError::Conflict {
            reason: "Choose which value to keep for each overlapping change.".into(),
        });
    }
    let included = req
        .included_nodes
        .as_ref()
        .map(|ids| ids.iter().map(|n| n.0.as_str()).collect::<BTreeSet<_>>());
    let chosen: Vec<EventPayload> = plan
        .ops
        .iter()
        .filter(|op| {
            included.as_ref().is_none_or(|ids| {
                op.target_node_id()
                    .is_some_and(|n| ids.contains(n.0.as_str()))
            })
        })
        .cloned()
        .collect();
    if chosen.is_empty() {
        return Err(bad("Select at least one change to merge"));
    }
    let events = append_to_team(&mut tx, doc, actor, role, &mut team, &chosen).await?;
    let mut local = ctx.local;
    carry_into_draft(&mut tx, draft, auth.identity_id, &mut local, &plan.state).await?;
    let completed = engine_core::workspace_merge::diff(&team, &local).is_empty();
    let next_seq = events.last().map_or(seq, |e| e.seq);
    sqlx::query("update workspace_draft set merge_base_state=$2,base_seq=$3,revision=revision+1,merged_at=case when $4 then now() end,review_outcome=case when $4 then 'merged' end,reviewed_by=$5,reviewed_at=now(),review_note=$6 where id=$1")
        .bind(draft).bind(json!(team)).bind(next_seq).bind(completed).bind(auth.identity_id).bind(req.note.trim()).execute(&mut *tx).await?;
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "document.change_request_merged",
        id,
        json!({"draft_id":draft,"contributor":request["author"]["id"],"events":events.len(),"from_seq":events.first().map(|e|e.seq),"through_seq":events.last().map(|e|e.seq),"completed":completed}),
    )
    .await?;
    tx.commit().await?;
    engine_core::log::post_commit_snapshot(&state.pool, doc, &chosen)
        .await
        .map_err(internal)?;
    for event in &events {
        state.subscriptions.publish(doc, event.clone());
    }
    Ok(Json(
        json!({"events":events,"completed":completed,"team_seq":next_seq,"remaining":engine_core::workspace_merge::diff(&team, &local).len()}),
    ))
}

#[derive(Deserialize)]
struct NoteRequest {
    #[serde(default)]
    note: String,
}
async fn close_request(
    state: &AppState,
    id: Uuid,
    draft: Uuid,
    auth: &AuthContext,
    outcome: &str,
    note: &str,
) -> Result<Json<Value>, ApiError> {
    if note.len() > 4000 {
        return Err(bad("Keep the note under 4,000 characters"));
    }
    let (mut tx, role, _) =
        apply::begin_write(state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let request = request_row(&mut tx, id, draft, auth.identity_id).await?;
    let mine = request["mine"] == true;
    if (outcome == "declined" && role != Role::Author) || (outcome == "withdrawn" && !mine) {
        return Err(ApiError::Forbidden);
    }
    if request["status"] != "open" || request["submitted_at"].is_null() {
        return Err(ApiError::Conflict {
            reason: "This change request is no longer open.".into(),
        });
    }
    sqlx::query("update workspace_draft set review_outcome=$2,reviewed_by=$3,reviewed_at=now(),review_note=$4 where id=$1")
        .bind(draft).bind(outcome).bind(auth.identity_id).bind(note.trim()).execute(&mut *tx).await?;
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        &format!("document.change_request_{outcome}"),
        id,
        json!({"draft_id":draft,"contributor":request["author"]["id"]}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"status":outcome})))
}
#[utoipa::path(post, path="/documents/{id}/change-requests/{draft_id}/decline", tag="reviews", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier"), ("draft_id" = Uuid, Path, description = "Change request identifier")), responses((status=200, description="Declined; the contributor keeps their draft", body=Value),(status=403, description="Editors decline change requests")))]
async fn decline(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<NoteRequest>,
) -> Result<Json<Value>, ApiError> {
    close_request(&state, id, draft, &auth, "declined", &req.note).await
}
#[utoipa::path(post, path="/documents/{id}/drafts/{draft_id}/withdraw", tag="reviews", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier"), ("draft_id" = Uuid, Path, description = "Draft identifier")), responses((status=200, description="The contributor withdrew their request", body=Value),(status=403, description="Only the contributor can withdraw")))]
async fn withdraw(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<NoteRequest>,
) -> Result<Json<Value>, ApiError> {
    close_request(&state, id, draft, &auth, "withdrawn", &req.note).await
}

/// Ask the document's editors to review an existing personal draft.
#[utoipa::path(post, path="/documents/{id}/drafts/{draft_id}/submit", tag="reviews", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier"), ("draft_id" = Uuid, Path, description = "Draft identifier")), responses((status=200, description="Submitted for review", body=Value),(status=403, description="Viewers cannot propose changes"),(status=404, description="Not your open draft")))]
async fn submit(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<NoteRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.note.len() > 4000 {
        return Err(bad("Keep the description under 4,000 characters"));
    }
    let (mut tx, role, _) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if role == Role::Auditor {
        return Err(ApiError::Forbidden);
    }
    let updated = sqlx::query("update workspace_draft set submitted_at=case when submitted_at is null or review_outcome is not null then now() else submitted_at end,source=case when source ? 'start_seq' then source else source||jsonb_build_object('start_seq',base_seq) end,submission_note=$4,review_outcome=null,reviewed_by=null,reviewed_at=null,review_note='' where id=$1 and document_id=$2 and created_by=$3 and merged_at is null")
        .bind(draft).bind(id).bind(auth.identity_id).bind(req.note.trim()).execute(&mut *tx).await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "document.change_request_submitted",
        id,
        json!({"draft_id":draft}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"status":"open"})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    history_points,
    state_at,
    create,
    list,
    detail,
    merge,
    decline,
    withdraw,
    submit
))]
pub struct ReviewApi;
