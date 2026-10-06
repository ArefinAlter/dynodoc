//! Change requests: reviewable personal drafts, like pull requests for documents.
//!
//! A contributor pushes a locally edited file (or submits an existing draft). The
//! draft starts from the revision the file was downloaded at, so a three-way merge
//! compares that ancestor, the current team version and the contributor's work.
//! Editors review, choose overlaps and merge; merging appends ordinary canonical
//! events through the same governance gate as every other write.
use crate::{
    access::{self, MemberRole, Policy},
    auth::AuthContext,
    error::ApiError,
    notifications,
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
    similarity,
    workspace_merge::MergePlan,
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
        .route(
            "/documents/:id/change-requests/:draft_id/reviews",
            post(review),
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
pub(crate) async fn state_through(
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

/// How a pushed file relates to the revision it claims to start from: content
/// similarity, and the share of that revision's blocks the file kept. A file that
/// keeps little of either is unrelated content, such as a different file uploaded
/// under the same name, and every change it makes to existing blocks needs a choice.
fn relatedness(base: &DocumentState, local: &DocumentState) -> Value {
    let before = similarity::fingerprint_state(base);
    let after = similarity::fingerprint_state(local);
    let comparison = similarity::compare(&after, &before, false);
    let blocks: Vec<_> = base
        .nodes
        .values()
        .filter(|n| !n.deleted && n.parent_id.is_some())
        .collect();
    let kept = blocks
        .iter()
        .filter(|n| local.nodes.get(&n.id).is_some_and(|l| !l.deleted))
        .count();
    let retention = if blocks.is_empty() {
        1.0
    } else {
        kept as f32 / blocks.len() as f32
    };
    let unrelated = !before.is_trivial() && comparison.score < 0.3 && retention < 0.5;
    json!({"relation":comparison.relation.as_str(),"similarity":comparison.score,"retention":retention,"unrelated":unrelated})
}

/// The three-way merge for a request; unrelated content uses the strict variant.
fn plan(
    request: &Value,
    base: &DocumentState,
    team: &DocumentState,
    local: &DocumentState,
    resolutions: &BTreeMap<String, String>,
) -> MergePlan {
    if request["source"]["unrelated"] == true {
        engine_core::workspace_merge::merge_strict(base, team, local, resolutions)
    } else {
        engine_core::workspace_merge::merge(base, team, local, resolutions)
    }
}

/// Reviews on a request, and whether the document's rules allow merging it now.
/// Approvals count while the contributor has not changed the request since, and
/// only from people who still hold a reviewer role or above.
async fn review_state(
    db: &mut sqlx::PgConnection,
    id: Uuid,
    draft: Uuid,
    request: &Value,
    role: MemberRole,
    rules: &Policy,
) -> Result<Value, ApiError> {
    let reviews: Vec<Value> = sqlx::query_scalar("select jsonb_build_object('id',r.id,'verdict',r.verdict,'note',r.note,'node_id',r.node_id,'created_at',r.created_at,'stale',r.created_at<=coalesce(w.content_changed_at,'-infinity'::timestamptz),'reviewer',jsonb_build_object('id',i.id,'name',i.display_name,'email',case when i.erased_at is null then i.email end)) from change_request_review r join identity i on i.id=r.reviewer_id join workspace_draft w on w.id=r.draft_id where r.draft_id=$1 order by r.created_at limit 500")
        .bind(draft)
        .fetch_all(&mut *db)
        .await?;
    let contributor = request["author"]["id"].as_str().unwrap_or_default();
    let latest: Vec<(Uuid, String)> = sqlx::query_as("select distinct on (r.reviewer_id) r.reviewer_id,r.verdict from change_request_review r join workspace_draft w on w.id=r.draft_id where r.draft_id=$1 and r.verdict in ('approved','changes_requested') and r.created_at>coalesce(w.content_changed_at,'-infinity'::timestamptz) and document_member_role($2,r.reviewer_id) in ('reviewer','editor','manager','owner') order by r.reviewer_id,r.created_at desc")
        .bind(draft)
        .bind(id)
        .fetch_all(&mut *db)
        .await?;
    let approved: Vec<String> = latest
        .iter()
        .filter(|(who, verdict)| verdict == "approved" && who.to_string() != contributor)
        .map(|(who, _)| who.to_string())
        .collect();
    let blocked: Vec<String> = latest
        .iter()
        .filter(|(_, verdict)| verdict == "changes_requested")
        .map(|(who, _)| who.to_string())
        .collect();
    let required = i64::from(rules.required_approvals);
    let satisfied = approved.len() as i64 >= required && (required == 0 || blocked.is_empty());
    Ok(json!({
        "reviews": reviews,
        "rules": rules,
        "approvals": approved.len(),
        "approved_by": approved,
        "changes_requested_by": blocked,
        "required_approvals": required,
        "satisfied": satisfied,
        "can_merge": rules.can_merge(role),
        "can_override": role.can_manage() && !satisfied,
        "can_approve": role.can_approve() && request["mine"] != true,
    }))
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
pub(crate) struct CreateRequest {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) note: String,
    pub(crate) base_seq: i64,
    pub(crate) ops: Vec<EventPayload>,
    #[serde(default)]
    pub(crate) source: Value,
    #[serde(default)]
    pub(crate) submit: bool,
    #[serde(skip)]
    pub(crate) provenance: Option<crate::provenance::Push>,
    #[serde(skip)]
    pub(crate) connector_grant: Option<Uuid>,
}
/// Create a draft from an earlier revision plus the pushed file's changes, atomically.
#[utoipa::path(post, path="/documents/{id}/change-requests", tag="reviews", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier")), responses((status=200, description="Draft created and optionally submitted for review", body=Value),(status=400, description="Invalid starting revision or changes"),(status=403, description="Viewers cannot propose changes")))]
pub(crate) async fn create(
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
    let (mut tx, role, team) =
        apply::begin_write(&state, doc, IdentityId(auth.identity_id)).await?;
    if role == Role::Auditor {
        return Err(ApiError::Forbidden);
    }
    if let Some(grant) = req.connector_grant {
        crate::provenance::check_grant(&mut tx, grant, id, auth.identity_id).await?;
    }
    let digest = if let Some(input) = &req.provenance {
        let (digest, receipt) =
            crate::provenance::prepare(&mut tx, id, auth.identity_id, input).await?;
        if let Some(receipt) = receipt {
            return Ok(Json(receipt));
        }
        Some(digest)
    } else {
        None
    };
    let latest = latest_seq(&mut tx, id).await?;
    if req.base_seq < 0 || req.base_seq > latest {
        return Err(bad(
            "The file's starting revision does not exist in this document",
        ));
    }
    let base = state_through(&mut tx, id, req.base_seq).await?;
    let draft:Uuid=sqlx::query_scalar("insert into workspace_draft(document_id,name,base_seq,base_state,created_by,source,submitted_at,submission_note,revision,content_changed_at) values($1,$2,$3,$4,$5,$6,case when $7 then now() end,$8,1,now()) returning id")
        .bind(id).bind(req.name.trim()).bind(req.base_seq).bind(json!(base)).bind(auth.identity_id).bind(&source).bind(req.submit).bind(req.note.trim()).fetch_one(&mut *tx).await?;
    let mut current = base.clone();
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
    // The server decides whether the file shares history with the document; the
    // client cannot mark unrelated content as related.
    let mut related = relatedness(&base, &current);
    // A file pushed onto an almost empty starting point is also unrelated when it
    // shares nothing with what the team has written since.
    let team_fp = similarity::fingerprint_state(&team);
    if related["unrelated"] == false
        && similarity::fingerprint_state(&base).is_trivial()
        && !team_fp.is_trivial()
        && similarity::compare(&similarity::fingerprint_state(&current), &team_fp, false).score
            < 0.3
    {
        related["unrelated"] = json!(true);
    }
    for key in ["relation", "similarity", "retention", "unrelated"] {
        source[key] = related[key].clone();
    }
    sqlx::query("update workspace_draft set source=$2 where id=$1")
        .bind(draft)
        .bind(&source)
        .execute(&mut *tx)
        .await?;
    if req.submit {
        notifications::notify_reviewers(
            &mut tx,
            id,
            auth.identity_id,
            "change_request_submitted",
            Some(draft),
            json!({"unrelated":related["unrelated"]}),
        )
        .await?;
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
    let mut receipt =
        json!({"id":draft,"revision":1,"edits":edits,"submitted":req.submit,"relation":related});
    if let (Some(input), Some(digest)) = (&req.provenance, digest) {
        receipt["bundle_id"] = json!(input.bundle.bundle_id);
        receipt["digest"] = json!(hex::encode(&digest));
        receipt["uploaded_by"] = json!(auth.identity_id);
        crate::provenance::record(
            &mut tx,
            id,
            auth.identity_id,
            draft,
            input,
            &digest,
            &receipt,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(receipt))
}

pub(crate) const REQUEST_JSON: &str = "jsonb_build_object('id',d.id,'name',d.name,'note',d.submission_note,'source',d.source,'base_seq',d.base_seq,'created_at',d.created_at,'submitted_at',d.submitted_at,'merged_at',d.merged_at,'review_outcome',d.review_outcome,'reviewed_at',d.reviewed_at,'review_note',d.review_note,'revision',d.revision,'author',jsonb_build_object('id',a.id,'name',a.display_name,'email',a.email),'reviewer',case when r.id is null then null else jsonb_build_object('id',r.id,'name',r.display_name,'email',r.email) end,'edits',(select count(*) from draft_edit e where e.draft_id=d.id),'status',case when d.merged_at is not null then 'merged' when d.review_outcome is not null then d.review_outcome else 'open' end)";

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
    let role = access::require_member(&mut tx, id, auth.identity_id).await?;
    let rules = access::policy(&mut tx, id).await?;
    let review = review_state(&mut tx, id, draft, &request, role, &rules).await?;
    // Closed requests keep their record; only open ones have a live comparison.
    if request["status"] != "open" {
        return Ok(Json(
            json!({"request":request,"team_seq":seq,"review":review}),
        ));
    }
    let ctx = load_draft(&mut tx, id, draft, None).await?;
    let plan = plan(&request, &ctx.base, &team, &ctx.local, &BTreeMap::new());
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
        json!({"request":request,"team_seq":seq,"revision":ctx.revision,"ops":plan.ops,"conflicts":plan.conflicts,"own":own,"nodes":nodes,"behind":seq-ctx.row["base_seq"].as_i64().unwrap_or(0),"review":review}),
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
    /// The owner or a manager merges without the required approvals.
    #[serde(default)]
    override_rules: bool,
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
    let member = access::require_member(&mut tx, id, auth.identity_id).await?;
    let rules = access::policy(&mut tx, id).await?;
    if !rules.can_merge(member) {
        return Err(ApiError::Denied {
            reason: "Only the owner and managers merge change requests in this document.".into(),
        });
    }
    let review = review_state(&mut tx, id, draft, &request, member, &rules).await?;
    let overridden = review["satisfied"] != true;
    if overridden && !(req.override_rules && member.can_manage()) {
        return Err(ApiError::Denied {
            reason: format!(
                "This document needs {} approval(s) with no outstanding change requests before merging.",
                rules.required_approvals
            ),
        });
    }
    let ctx = load_draft(&mut tx, id, draft, None).await?;
    let seq = latest_seq(&mut tx, id).await?;
    if req.team_seq != seq || req.revision != ctx.revision {
        return Err(ApiError::Conflict {
            reason: "The team version or this change request changed. Review the updated comparison before merging.".into(),
        });
    }
    let plan = plan(&request, &ctx.base, &team, &ctx.local, &req.resolutions);
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
        json!({"draft_id":draft,"contributor":request["author"]["id"],"events":events.len(),"from_seq":events.first().map(|e|e.seq),"through_seq":events.last().map(|e|e.seq),"completed":completed,"rules_overridden":overridden,"approvals":review["approvals"]}),
    )
    .await?;
    if let Some(contributor) = request["author"]["id"]
        .as_str()
        .and_then(|v| v.parse().ok())
    {
        notifications::notify(
            &mut tx,
            contributor,
            "change_request_merged",
            Some(id),
            Some(auth.identity_id),
            Some(draft),
            json!({"completed":completed,"events":events.len()}),
        )
        .await?;
    }
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
    if outcome == "declined" {
        let member = access::require_member(&mut tx, id, auth.identity_id).await?;
        if !access::policy(&mut tx, id).await?.can_merge(member) {
            return Err(ApiError::Denied {
                reason: "Only the owner and managers decide on change requests in this document."
                    .into(),
            });
        }
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
    if outcome == "declined" {
        if let Some(contributor) = request["author"]["id"]
            .as_str()
            .and_then(|v| v.parse().ok())
        {
            notifications::notify(
                &mut tx,
                contributor,
                "change_request_declined",
                Some(id),
                Some(auth.identity_id),
                Some(draft),
                json!({}),
            )
            .await?;
        }
    }
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
    notifications::notify_reviewers(
        &mut tx,
        id,
        auth.identity_id,
        "change_request_submitted",
        Some(draft),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"status":"open"})))
}

#[derive(Deserialize)]
struct ReviewRequest {
    /// `approved`, `changes_requested` or `commented`.
    verdict: String,
    #[serde(default)]
    note: String,
    /// Comment on one changed block.
    node_id: Option<String>,
}
/// Approve, request changes or comment. Reviewers and above approve or request
/// changes on other people's requests; any contributor can comment.
#[utoipa::path(post, path="/documents/{id}/change-requests/{draft_id}/reviews", tag="reviews", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier"), ("draft_id" = Uuid, Path, description = "Change request identifier")), responses((status=200, description="Review recorded", body=Value),(status=403, description="Role cannot give this review"),(status=409, description="The request is closed")))]
async fn review(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<ReviewRequest>,
) -> Result<Json<Value>, ApiError> {
    if !["approved", "changes_requested", "commented"].contains(&req.verdict.as_str())
        || req.note.len() > 4000
        || req.node_id.as_ref().is_some_and(|n| n.len() > 64)
    {
        return Err(bad(
            "Choose approve, request changes or comment, with a note under 4,000 characters",
        ));
    }
    if req.verdict != "approved" && req.note.trim().is_empty() {
        return Err(bad("Add a note explaining what should change"));
    }
    let (mut tx, _, _) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let request = request_row(&mut tx, id, draft, auth.identity_id).await?;
    if request["status"] != "open" || request["submitted_at"].is_null() {
        return Err(ApiError::Conflict {
            reason: "This change request is no longer open.".into(),
        });
    }
    let member = access::require_member(&mut tx, id, auth.identity_id).await?;
    let verdict_given = req.verdict != "commented";
    if member < MemberRole::Contributor
        || (verdict_given && (!member.can_approve() || request["mine"] == true))
    {
        return Err(ApiError::Denied {
            reason: if request["mine"] == true {
                "You can't approve your own change request.".into()
            } else {
                "Reviewers, editors and the owner approve change requests.".into()
            },
        });
    }
    let review: Uuid = sqlx::query_scalar("insert into change_request_review(document_id,draft_id,reviewer_id,verdict,node_id,note) values($1,$2,$3,$4,$5,$6) returning id")
        .bind(id)
        .bind(draft)
        .bind(auth.identity_id)
        .bind(&req.verdict)
        .bind(&req.node_id)
        .bind(req.note.trim())
        .fetch_one(&mut *tx)
        .await?;
    if let Some(contributor) = request["author"]["id"]
        .as_str()
        .and_then(|v| v.parse().ok())
    {
        notifications::notify(
            &mut tx,
            contributor,
            "change_request_reviewed",
            Some(id),
            Some(auth.identity_id),
            Some(draft),
            json!({"verdict":req.verdict}),
        )
        .await?;
    }
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "document.change_request_reviewed",
        id,
        json!({"draft_id":draft,"review_id":review,"verdict":req.verdict}),
    )
    .await?;
    let rules = access::policy(&mut tx, id).await?;
    let summary = review_state(&mut tx, id, draft, &request, member, &rules).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":review,"review":summary})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    review,
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
