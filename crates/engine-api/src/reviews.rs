//! Change requests: reviewable personal drafts, like pull requests for documents.
//!
//! A contributor pushes a locally edited file (or submits an existing draft). The
//! draft starts from the revision the file was downloaded at, so a three-way merge
//! compares that ancestor, the current team version and the contributor's work.
//! Editors review, choose overlaps and merge; merging appends ordinary canonical
//! events through the same governance gate as every other write.
use crate::{
    access::{self, MemberRole, Policy},
    agent::identity::owned_by,
    auth::AuthContext,
    error::ApiError,
    notifications,
    ops::apply,
    workspace::{append_to_team, carry_into_draft, load_draft},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use engine_core::{
    governance::{self, Role},
    materializer::{apply_payload, DocumentState},
    similarity,
    workspace_merge::MergePlan,
};
use engine_shared::{DocumentId, EventPayload, IdentityId, NodeId};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents/{id}/history-points", get(history_points))
        .route("/documents/{id}/state", get(state_at))
        .route(
            "/documents/{id}/change-requests",
            get(list)
                .post(create)
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route("/documents/{id}/change-requests/{draft_id}", get(detail))
        .route(
            "/documents/{id}/change-requests/{draft_id}/merge",
            post(merge),
        )
        .route(
            "/documents/{id}/change-requests/{draft_id}/decline",
            post(decline),
        )
        .route(
            "/documents/{id}/change-requests/{draft_id}/reviews",
            post(review),
        )
        .route("/documents/{id}/drafts/{draft_id}/submit", post(submit))
        .route("/documents/{id}/drafts/{draft_id}/withdraw", post(withdraw))
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
    engine_core::snapshot::read_state_at(executor, DocumentId(id), through_seq)
        .await
        .map(|at| at.state)
        .map_err(internal)
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

/// SQL boolean: the review `r`, joined to its draft `w`, still counts. A review that names a
/// revision (every one written since migration 0045) counts while the request's content has
/// not changed after that revision (`content_revision`); a merge of part of the request moves
/// `revision` and leaves the content, so it does not lapse. An older review, which names none,
/// keeps the rule it was written under: written after the last change.
const FRESH: &str = "case when r.draft_revision is null then r.created_at>coalesce(w.content_changed_at,'-infinity'::timestamptz) else r.draft_revision>=w.content_revision end";

/// SQL: the verdict that counts for each reviewer of draft `draft` in document `document`
/// (expressions): the latest approval or request for changes that is still [`FRESH`], from a
/// person who still holds a reviewer role or above and is not an agent identity (I4). The one
/// definition of "counts", used by the review state, the lapse audit and the proposal lists.
/// Columns: reviewer, verdict, review id, the revision it names. Which of one reviewer's
/// decisions is the latest is the insert order (`seq`, drawn under the document lock, so it is
/// commit order) for every row written since migration 0045, and `created_at` (the
/// transaction's start, which can disagree with commit order) only among the older rows that
/// name no revision, which all count as older than the new ones. The caller's expressions
/// must not use the aliases `r`, `w`, `ai` or `l`.
pub(crate) fn latest_counted(draft: &str, document: &str) -> String {
    format!("select distinct on (r.reviewer_id) r.reviewer_id,r.verdict,r.id as review_id,r.draft_revision from change_request_review r join workspace_draft w on w.id=r.draft_id where r.draft_id={draft} and r.verdict in ('approved','changes_requested') and {FRESH} and document_member_role({document},r.reviewer_id) in ('reviewer','editor','manager','owner') and not exists(select 1 from agent_identity ai where ai.identity_id=r.reviewer_id) order by r.reviewer_id,(r.draft_revision is not null) desc,case when r.draft_revision is not null then r.seq end desc nulls last,r.created_at desc")
}

/// SQL: the state word of the draft row `draft` (a table alias), as the proposal lists and the
/// agent's status say it: `draft`, `open`, `changes_requested` (an open request one counted
/// reviewer has asked to change), `merged`, `declined` or `withdrawn`.
pub(crate) fn state_sql(draft: &str) -> String {
    format!("case when {draft}.merged_at is not null or {draft}.review_outcome='merged' then 'merged' when {draft}.review_outcome='declined' then 'declined' when {draft}.review_outcome='withdrawn' then 'withdrawn' when {draft}.submitted_at is null then 'draft' when exists(select 1 from ({}) l where l.verdict='changes_requested') then 'changes_requested' else 'open' end", latest_counted(&format!("{draft}.id"), &format!("{draft}.document_id")))
}

/// Reviews on a request, and whether the document's rules allow merging it now.
/// Approvals count while the request's content is as the reviewer saw it, and
/// only from people who still hold a reviewer role or above.
pub(crate) async fn review_state(
    db: &mut sqlx::PgConnection,
    id: Uuid,
    draft: Uuid,
    request: &Value,
    role: MemberRole,
    rules: &Policy,
) -> Result<Value, ApiError> {
    let reviews: Vec<Value> = sqlx::query_scalar(&format!("select jsonb_build_object('id',r.id,'verdict',r.verdict,'note',r.note,'node_id',r.node_id,'created_at',r.created_at,'revision',r.draft_revision,'stale',not ({FRESH}),'reviewer',jsonb_build_object('id',i.id,'name',i.display_name,'email',case when i.erased_at is null then i.email end)) from change_request_review r join identity i on i.id=r.reviewer_id join workspace_draft w on w.id=r.draft_id where r.draft_id=$1 order by r.created_at limit 500"))
        .bind(draft)
        .fetch_all(&mut *db)
        .await?;
    let contributor = request["author"]["id"].as_str().unwrap_or_default();
    // I5 (docs/AI-AGENT-PROTOCOL-SPEC.md 9.5): for a request an AI agent wrote, the person
    // it acts for is the contributor too, so cannot be one of the approvers it needs.
    let principal = request["agent"]["principal"]["id"]
        .as_str()
        .unwrap_or_default();
    // I4: an agent identity never counts, whatever a row says (it holds no membership, so
    // the role test already excludes it, and the database refuses its review rows).
    let latest: Vec<(Uuid, String, Uuid, Option<i64>)> =
        sqlx::query_as(&latest_counted("$1", "$2"))
            .bind(draft)
            .bind(id)
            .fetch_all(&mut *db)
            .await?;
    let approved: Vec<String> = latest
        .iter()
        .filter(|(who, verdict, _, _)| {
            verdict == "approved" && who.to_string() != contributor && who.to_string() != principal
        })
        .map(|(who, ..)| who.to_string())
        .collect();
    let blocked: Vec<String> = latest
        .iter()
        .filter(|(_, verdict, ..)| verdict == "changes_requested")
        .map(|(who, ..)| who.to_string())
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
        "can_override": role.can_manage() && !satisfied && !own_agent_request(request),
        "can_approve": role.can_approve() && request["mine"] != true,
    }))
}

/// Call in the transaction that is about to change the content of `draft` (its revision moves
/// and `content_revision` follows), before the update. The approvals and requests for changes
/// that count today stop counting with it; they stay in the append-only table, marked stale by
/// the revision they name, and this writes the one audit row that says what lapsed and when,
/// so that the trail reads: approved at revision N, changed, revision N+1. What lapses is
/// exactly what counted: each eligible reviewer's latest decision ([`latest_counted`]), not the
/// rows it superseded and not a person who no longer holds a reviewer role. Nothing is written
/// when no review counted. `cause` says why the content moved: `edited` (a save, pull, rebase
/// or an agent's addition) or `reopened` (a declined or withdrawn request sent for review
/// again).
pub(crate) async fn audit_lapsed_reviews(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    document: Uuid,
    draft: Uuid,
    actor: Uuid,
    cause: &str,
) -> Result<(), ApiError> {
    let lapsing: Vec<Value> = sqlx::query_scalar(&format!("select jsonb_build_object('review',l.review_id,'verdict',l.verdict,'revision',l.draft_revision,'reviewer',l.reviewer_id) from ({}) l limit 50", latest_counted("$1", "$2")))
        .bind(draft)
        .bind(document)
        .fetch_all(&mut **tx)
        .await?;
    if lapsing.is_empty() {
        return Ok(());
    }
    let revision: i64 = sqlx::query_scalar("select revision from workspace_draft where id=$1")
        .bind(draft)
        .fetch_one(&mut **tx)
        .await?;
    crate::product::audit(
        tx,
        actor,
        "document.change_request_changed",
        document,
        json!({"draft_id":draft,"cause":cause,"from_revision":revision,"to_revision":revision+1,"lapsed":lapsing}),
    )
    .await
}

/// Is this the viewer's own agent's request? (I5: the person an AI agent acts for is its
/// contributor, so what a contributor may not do to their own request they may not do to this
/// one either; here, bypass its approvals.)
fn own_agent_request(request: &Value) -> bool {
    request["agent"].is_object() && request["mine"] == true
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
    if req.source["kind"] == "history_recovery" {
        return Err(bad("Historical recovery must use the history-recovery endpoint so its source is checked by the server"));
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
    let stored =
        crate::draft_storage::encode(&mut tx, DocumentId(id), &base, state.shared_draft_bases)
            .await?;
    let draft:Uuid=sqlx::query_scalar("insert into workspace_draft(document_id,name,base_seq,base_state,created_by,source,submitted_at,submission_note,revision,content_changed_at) values($1,$2,$3,$4,$5,$6,case when $7 then now() end,$8,1,now()) returning id")
        .bind(id).bind(req.name.trim()).bind(req.base_seq).bind(stored).bind(auth.identity_id).bind(&source).bind(req.submit).bind(req.note.trim()).fetch_one(&mut *tx).await?;
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

/// The JSON of one request row (aliases `d` the draft, `a` its author, `r` its reviewer).
/// A request an AI agent wrote gains an `agent` object (its identity, client product, display
/// name and the person it acts for, docs/AI-AGENT-CONNECTORS.md 6.2) and an agent identity's
/// email is null wherever it appears as author or as the reviewer who closed the request (the
/// address is the placeholder of the reserved domain). Every other request is
/// exactly what it was. The `agent` part is `identity::agent_json("d")`, written out because
/// this is a constant; a test keeps the two equal.
pub(crate) const REQUEST_JSON: &str = "(jsonb_build_object('id',d.id,'name',d.name,'note',d.submission_note,'source',d.source,'base_seq',d.base_seq,'created_at',d.created_at,'submitted_at',d.submitted_at,'merged_at',d.merged_at,'review_outcome',d.review_outcome,'reviewed_at',d.reviewed_at,'review_note',d.review_note,'revision',d.revision,'author',jsonb_build_object('id',a.id,'name',a.display_name,'email',case when lower(a.email) like '%@agents.invalid' then null else a.email end),'reviewer',case when r.id is null then null else jsonb_build_object('id',r.id,'name',r.display_name,'email',case when lower(r.email) like '%@agents.invalid' then null else r.email end) end,'edits',(select count(*) from draft_edit e where e.draft_id=d.id),'status',case when d.merged_at is not null then 'merged' when d.review_outcome is not null then d.review_outcome else 'open' end)||coalesce((select jsonb_build_object('agent',jsonb_build_object('id',ai.identity_id,'client_kind',ai.client_kind,'display_name',ag.display_name,'principal',jsonb_build_object('id',pr.id,'name',pr.display_name),'revoked',ai.revoked_at is not null)) from agent_identity ai join identity ag on ag.id=ai.identity_id join identity pr on pr.id=ai.principal_id where ai.identity_id=d.created_by),'{}'::jsonb))";

/// Submitted change requests are visible to every member; unsubmitted drafts stay private.
#[utoipa::path(get, path="/documents/{id}/change-requests", tag="reviews", params(("id" = Uuid, Path, description = "Document identifier")), responses((status=200, description="Open and recently closed change requests", body=Value),(status=403, description="Document membership required")))]
async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    // `mine`: yours, or written by an AI agent acting for you (so that you cannot approve it).
    let owned = owned_by("d", "$2");
    let items: Vec<Value> = sqlx::query_scalar(&format!("select {REQUEST_JSON}||jsonb_build_object('mine',{owned}) from workspace_draft d join identity a on a.id=d.created_by left join identity r on r.id=d.reviewed_by where d.document_id=$1 and d.submitted_at is not null order by (d.merged_at is null and d.review_outcome is null) desc, d.submitted_at desc limit 200"))
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
    let owned = owned_by("d", "$3");
    sqlx::query_scalar(&format!("select {REQUEST_JSON}||jsonb_build_object('mine',{owned}) from workspace_draft d join identity a on a.id=d.created_by left join identity r on r.id=d.reviewed_by where d.id=$1 and d.document_id=$2 and (d.submitted_at is not null or {owned})"))
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
    let role = access::governance_role(&mut tx, id, auth.identity_id).await?;
    let rules = access::policy(&mut tx, id).await?;
    let review = review_state(&mut tx, id, draft, &request, role, &rules).await?;
    // What an AI agent said about its proposal (its words: untrusted text), for the reviewer.
    let statement = if request["agent"].is_object() {
        Some(crate::agent::inbox::statement(&mut tx, draft).await?)
    } else {
        None
    };
    // Closed requests keep their record; only open ones have a live comparison.
    if request["status"] != "open" {
        let mut closed = json!({"request":request,"team_seq":seq,"review":review});
        if let Some(statement) = statement {
            closed["agent_statement"] = statement;
        }
        return Ok(Json(closed));
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
    let mut open = json!({"request":request,"team_seq":seq,"revision":ctx.revision,"ops":plan.ops,"conflicts":plan.conflicts,"own":own,"nodes":nodes,"behind":seq-ctx.row["base_seq"].as_i64().unwrap_or(0),"review":review});
    if let Some(statement) = statement {
        open["agent_statement"] = statement;
    }
    Ok(Json(open))
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
    refuse_agent(&mut tx, auth.identity_id).await?;
    let request = request_row(&mut tx, id, draft, auth.identity_id).await?;
    if request["status"] != "open" || request["submitted_at"].is_null() {
        return Err(ApiError::Conflict {
            reason: "This change request is no longer open.".into(),
        });
    }
    let member = access::governance_role(&mut tx, id, auth.identity_id).await?;
    let rules = access::policy(&mut tx, id).await?;
    if !rules.can_merge(member) {
        return Err(ApiError::Denied {
            reason: "Only the owner and managers merge change requests in this document.".into(),
        });
    }
    let review = review_state(&mut tx, id, draft, &request, member, &rules).await?;
    let overridden = review["satisfied"] != true;
    // I5 (docs/AI-AGENT-PROTOCOL-SPEC.md 9.5): the bypass of the approvals is not for the
    // person whose agent wrote the request. Another manager's bypass is as for any request.
    if overridden && req.override_rules && own_agent_request(&request) {
        return Err(ApiError::Denied {
            reason: "Your AI agent wrote this change request, so you cannot approve it or bypass its approvals. Ask another reviewer to approve it, or another manager to merge it.".into(),
        });
    }
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
    let included = req.included_nodes.as_ref().map(|ids| {
        engine_core::workspace_merge::with_dependencies(
            &plan.ops,
            &team,
            ids.iter().map(|n| n.0.as_str()).collect(),
        )
    });
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
    let stored =
        crate::draft_storage::encode(&mut tx, doc, &team, state.shared_draft_bases).await?;
    sqlx::query("update workspace_draft set merge_base_state=$2,base_seq=$3,revision=revision+1,merged_at=case when $4 then now() end,review_outcome=case when $4 then 'merged' end,reviewed_by=$5,reviewed_at=now(),review_note=$6 where id=$1")
        .bind(draft).bind(stored).bind(next_seq).bind(completed).bind(auth.identity_id).bind(req.note.trim()).execute(&mut *tx).await?;
    let mut detail = json!({"draft_id":draft,"contributor":request["author"]["id"],"events":events.len(),"from_seq":events.first().map(|e|e.seq),"through_seq":events.last().map(|e|e.seq),"completed":completed,"rules_overridden":overridden,"approvals":review["approvals"]});
    // Attribution layer B (docs/AI-AGENT-PROTOCOL-SPEC.md 9.5): the contributor of an agent's
    // request is the agent identity (as above); say so, and for whom it acted. The events
    // themselves keep the merger as `actor_id`, unchanged.
    name_agent(&mut detail, &request);
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "document.change_request_merged",
        id,
        detail,
    )
    .await?;
    if let Some(contributor) = contributor_of(&request) {
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
        refuse_agent(&mut tx, auth.identity_id).await?;
        let member = access::governance_role(&mut tx, id, auth.identity_id).await?;
        if !access::policy(&mut tx, id).await?.can_merge(member) {
            return Err(ApiError::Denied {
                reason: "Only the owner and managers decide on change requests in this document."
                    .into(),
            });
        }
    }
    // The person an AI agent acts for may discard its draft before sending it for review:
    // it is closed as withdrawn and kept (the edits, the receipts and this row stay as history),
    // exactly what the agent's own withdraw does. Nobody else can see the draft at all.
    let discarding = outcome == "withdrawn"
        && mine
        && request["agent"].is_object()
        && request["submitted_at"].is_null();
    if request["status"] != "open" || (request["submitted_at"].is_null() && !discarding) {
        return Err(ApiError::Conflict {
            reason: "This change request is no longer open.".into(),
        });
    }
    sqlx::query("update workspace_draft set review_outcome=$2,reviewed_by=$3,reviewed_at=now(),review_note=$4 where id=$1")
        .bind(draft).bind(outcome).bind(auth.identity_id).bind(note.trim()).execute(&mut *tx).await?;
    let mut detail = json!({"draft_id":draft,"contributor":request["author"]["id"]});
    name_agent(&mut detail, &request);
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        &if discarding {
            "document.draft_withdrawn".to_owned()
        } else {
            format!("document.change_request_{outcome}")
        },
        id,
        detail,
    )
    .await?;
    if outcome == "declined" {
        if let Some(contributor) = contributor_of(&request) {
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
#[utoipa::path(post, path="/documents/{id}/drafts/{draft_id}/withdraw", tag="reviews", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier"), ("draft_id" = Uuid, Path, description = "Draft identifier")), responses((status=200, description="The contributor withdrew their request, or the person an AI agent acts for discarded its draft before sending it for review (closed as withdrawn and kept as history)", body=Value),(status=403, description="Only the contributor can withdraw"),(status=409, description="Already closed, or an unsent draft that is not an AI agent's")))]
async fn withdraw(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<NoteRequest>,
) -> Result<Json<Value>, ApiError> {
    close_request(&state, id, draft, &auth, "withdrawn", &req.note).await
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SubmitRequest {
    #[serde(default)]
    note: String,
    /// The `revision` of the draft the person was looking at. **Required to adopt a draft an
    /// AI agent wrote** (an agent can add to its own draft until it is submitted, so the
    /// person must send what they read); when given for a draft of the person's own it must
    /// match as well. A draft that moved is `409` with the code `request_changed`.
    revision: Option<i64>,
}

/// Ask the document's editors to review an existing personal draft. A person may also submit
/// the draft an AI agent wrote for them (adopt it): the request stays the agent's work, the
/// audit row names the person who submitted it and the agent that proposed it, and the draft's
/// `revision` must be sent, so what is submitted is what was read.
#[utoipa::path(post, path="/documents/{id}/drafts/{draft_id}/submit", tag="reviews", request_body=SubmitRequest, params(("id" = Uuid, Path, description = "Document identifier"), ("draft_id" = Uuid, Path, description = "Draft identifier")), responses((status=200, description="Submitted for review", body=Value),(status=400, description="Invalid note; code revision_required when adopting an AI agent's draft without naming its revision"),(status=403, description="Viewers cannot propose changes"),(status=404, description="Not your open draft"),(status=409, description="The draft changed after the revision named (code request_changed, details.current_revision)")))]
async fn submit(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<SubmitRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.note.len() > 4000 {
        return Err(bad("Keep the description under 4,000 characters"));
    }
    if req.revision.is_some_and(|r| r < 0) {
        return Err(bad("The revision of a draft is not negative"));
    }
    let (mut tx, role, _) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if role == Role::Auditor {
        return Err(ApiError::Forbidden);
    }
    // The document lock is held: the revision read here is the one that is submitted.
    let owned = owned_by("workspace_draft", "$3");
    let found: Option<(i64, bool)> = sqlx::query_as(&format!("select revision,created_by<>$3 from workspace_draft where id=$1 and document_id=$2 and {owned}"))
        .bind(draft)
        .bind(id)
        .bind(auth.identity_id)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some((revision, adopting)) = found {
        if adopting && req.revision.is_none() {
            return Err(revision_required());
        }
        if req.revision.is_some_and(|named| named != revision) {
            return Err(request_changed(revision, req.revision));
        }
    }
    // Your draft, or one an AI agent made for you.
    if !mark_submitted(&mut tx, id, draft, auth.identity_id, req.note.trim(), true).await? {
        return Err(ApiError::NotFound);
    }
    tx.commit().await?;
    Ok(Json(json!({"status":"open"})))
}

fn revision_required() -> ApiError {
    ApiError::Coded {
        status: StatusCode::BAD_REQUEST,
        code: "revision_required",
        reason: "Say which revision you read (the revision of the page you were looking at). Reload the page to read the current one.".into(),
        details: json!({}),
    }
}

/// `409 request_changed`: the request or draft is not at the revision the caller read.
fn request_changed(current: i64, named: Option<i64>) -> ApiError {
    ApiError::Coded {
        status: StatusCode::CONFLICT,
        code: "request_changed",
        reason: "This change request changed while you were reading it. Read the new version, then decide again.".into(),
        details: json!({"current_revision":current,"revision":named}),
    }
}

/// Submit the unmerged draft `draft` of `actor` (or of an agent identity acting for
/// `actor`) for review: stamp it, audit it and tell the reviewers. `false` when there is no
/// such draft. `reopen` lets a closed (declined or withdrawn) request be submitted again, as a
/// person may; an agent's own route passes `false`. The caller holds the document lock and
/// has checked the role. This is the one submit path of people and agents.
pub(crate) async fn mark_submitted(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    draft: Uuid,
    actor: Uuid,
    note: &str,
    reopen: bool,
) -> Result<bool, ApiError> {
    let owned = owned_by("workspace_draft", "$3");
    let closed = if reopen {
        ""
    } else {
        " and review_outcome is null"
    };
    // Sending a declined or withdrawn request for review again is a new round: the reviews
    // given before it must not count for it, or an approval that predates a decline would
    // satisfy the rules again (and, for an AI agent's request, its principal could revive it).
    // Both counters move, so every counting path (they all go through [`FRESH`]) ignores those
    // reviews, whatever the clock says, and the older rule's timestamp moves too.
    let reopening: Option<bool> = sqlx::query_scalar(&format!("select review_outcome is not null from workspace_draft where id=$1 and document_id=$2 and {owned} and merged_at is null"))
        .bind(draft).bind(id).bind(actor).fetch_optional(&mut **tx).await?;
    if reopen && reopening == Some(true) {
        audit_lapsed_reviews(tx, id, draft, actor, "reopened").await?;
    }
    let updated = sqlx::query(&format!("update workspace_draft set submitted_at=case when submitted_at is null or review_outcome is not null then now() else submitted_at end,source=case when source ? 'start_seq' then source else source||jsonb_build_object('start_seq',base_seq) end,submission_note=$4,revision=case when review_outcome is not null then revision+1 else revision end,content_revision=case when review_outcome is not null then revision+1 else content_revision end,content_changed_at=case when review_outcome is not null then now() else content_changed_at end,review_outcome=null,reviewed_by=null,reviewed_at=null,review_note='' where id=$1 and document_id=$2 and {owned} and merged_at is null{closed}"))
        .bind(draft).bind(id).bind(actor).bind(note).execute(&mut **tx).await?;
    if updated.rows_affected() == 0 {
        return Ok(false);
    }
    // A person submitting the draft of their AI agent is named as such, like the agent itself.
    let agent: Option<(Uuid, String, Uuid)> = sqlx::query_as("select ai.identity_id,ai.client_kind,ai.principal_id from workspace_draft w join agent_identity ai on ai.identity_id=w.created_by where w.id=$1")
        .bind(draft)
        .fetch_optional(&mut **tx)
        .await?;
    let mut detail = json!({"draft_id":draft});
    if let Some((identity, client_kind, principal)) = agent {
        detail["contributor"] = json!(identity);
        detail["contributor_kind"] = json!("agent");
        detail["agent"] = json!({"client_kind":client_kind,"principal":principal});
        // The row's actor is the person; the agent that proposed it is `contributor`.
        if actor == principal {
            detail["adopted"] = json!(true);
        }
    }
    crate::product::audit(tx, actor, "document.change_request_submitted", id, detail).await?;
    notifications::notify_reviewers(
        tx,
        id,
        actor,
        "change_request_submitted",
        Some(draft),
        json!({}),
    )
    .await?;
    Ok(true)
}

/// For a request an AI agent wrote, say so in an audit detail (an object): the contributor is
/// an agent, of which client product, acting for whom. Other requests add nothing.
fn name_agent(detail: &mut Value, request: &Value) {
    if request["agent"].is_object() {
        detail["contributor_kind"] = json!("agent");
        detail["agent"] = json!({"client_kind":request["agent"]["client_kind"],"principal":request["agent"]["principal"]["id"]});
    }
}

/// Who hears about what happens to a request: its author, or the person its AI agent acts
/// for (the agent identity receives nothing).
fn contributor_of(request: &Value) -> Option<Uuid> {
    request["agent"]["principal"]["id"]
        .as_str()
        .or_else(|| request["author"]["id"].as_str())
        .and_then(|v| v.parse().ok())
}

/// Invariant I4, as an explicit check on the paths that decide: an agent identity never
/// approves, comments on, merges or declines a request. It cannot sign in, so none of those
/// routes can be reached as one; this is the second lock on the door.
async fn refuse_agent(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: Uuid,
) -> Result<(), ApiError> {
    if crate::agent::identity::is_agent(tx, actor).await? {
        return Err(ApiError::Denied {
            reason: "AI agents propose changes; people review them.".into(),
        });
    }
    Ok(())
}

#[derive(Deserialize, utoipa::ToSchema)]
struct ReviewRequest {
    /// `approved`, `changes_requested` or `commented`.
    verdict: String,
    #[serde(default)]
    note: String,
    /// Comment on one changed block.
    node_id: Option<String>,
    /// The `revision` of the change request the reviewer was looking at (the one its detail
    /// returned). **Required for `approved` and `changes_requested`**: the review is given for
    /// that revision and for no other. If the request has moved since, the answer is `409`
    /// with the code `request_changed`; read the request again. A comment may leave it out.
    revision: Option<i64>,
}
/// Approve, request changes or comment. Reviewers and above approve or request
/// changes on other people's requests; any contributor can comment. An approval or a
/// request for changes names the revision of the request it was given for (`revision`), and
/// counts only while the request's content is as it was at that revision.
#[utoipa::path(post, path="/documents/{id}/change-requests/{draft_id}/reviews", tag="reviews", request_body=ReviewRequest, params(("id" = Uuid, Path, description = "Document identifier"), ("draft_id" = Uuid, Path, description = "Change request identifier")), responses((status=200, description="Review recorded", body=Value),(status=400, description="Invalid review; code revision_required when an approval or a request for changes names no revision"),(status=403, description="Role cannot give this review"),(status=409, description="The request is closed (code conflict), or it changed after the revision the review names (code request_changed, details.current_revision)")))]
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
    // A decision is given for one revision of the request, the one the reviewer read.
    let decides = req.verdict != "commented";
    if req.revision.is_some_and(|r| r < 0) {
        return Err(bad("The revision of a change request is not negative"));
    }
    if decides && req.revision.is_none() {
        return Err(revision_required());
    }
    let (mut tx, _, _) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    refuse_agent(&mut tx, auth.identity_id).await?;
    let request = request_row(&mut tx, id, draft, auth.identity_id).await?;
    if request["status"] != "open" || request["submitted_at"].is_null() {
        return Err(ApiError::Conflict {
            reason: "This change request is no longer open.".into(),
        });
    }
    let member = access::governance_role(&mut tx, id, auth.identity_id).await?;
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
    // The document lock is held, so the revision cannot move between this read and the insert
    // (and the database refuses a deciding review for any other revision).
    let current = request["revision"].as_i64().unwrap_or(0);
    if decides && req.revision != Some(current) {
        return Err(request_changed(current, req.revision));
    }
    let named = req.revision.filter(|r| (0..=current).contains(r));
    let review: Uuid = sqlx::query_scalar("insert into change_request_review(document_id,draft_id,reviewer_id,verdict,node_id,note,draft_revision) values($1,$2,$3,$4,$5,$6,$7) returning id")
        .bind(id)
        .bind(draft)
        .bind(auth.identity_id)
        .bind(&req.verdict)
        .bind(&req.node_id)
        .bind(req.note.trim())
        .bind(named)
        .fetch_one(&mut *tx)
        .await?;
    if let Some(contributor) = contributor_of(&request) {
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
        json!({"draft_id":draft,"review_id":review,"verdict":req.verdict,"revision":named}),
    )
    .await?;
    let rules = access::policy(&mut tx, id).await?;
    let summary = review_state(&mut tx, id, draft, &request, member, &rules).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":review,"revision":current,"review":summary}),
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_json_carries_the_shared_agent_badge_and_hides_the_placeholder_address() {
        // The constant is written out; it must stay the shared definition.
        assert!(REQUEST_JSON.contains(&crate::agent::identity::agent_json("d")));
        assert!(REQUEST_JSON.contains("like '%@agents.invalid' then null"));
        assert!(REQUEST_JSON.starts_with('(') && REQUEST_JSON.ends_with(')'));
    }

    #[test]
    fn only_the_viewers_own_agents_request_is_closed_to_their_override() {
        let agent_theirs = json!({"agent":{"principal":{"id":"p"}},"mine":true});
        let agent_others = json!({"agent":{"principal":{"id":"p"}},"mine":false});
        let ordinary_theirs = json!({"mine":true});
        let ordinary_others = json!({"mine":false});
        assert!(own_agent_request(&agent_theirs));
        assert!(!own_agent_request(&agent_others));
        assert!(!own_agent_request(&ordinary_theirs));
        assert!(!own_agent_request(&ordinary_others));
        assert!(!own_agent_request(&json!({})));
    }

    #[test]
    fn an_audit_detail_names_the_agent_only_for_its_request() {
        let mut ordinary = json!({"draft_id":"d"});
        name_agent(&mut ordinary, &json!({"author":{"id":"a"}}));
        assert_eq!(ordinary, json!({"draft_id":"d"}));
        let mut by_agent = json!({"draft_id":"d"});
        name_agent(
            &mut by_agent,
            &json!({"agent":{"client_kind":"claude","principal":{"id":"p","name":"P"}}}),
        );
        assert_eq!(
            by_agent,
            json!({"draft_id":"d","contributor_kind":"agent","agent":{"client_kind":"claude","principal":"p"}})
        );
    }

    #[test]
    fn the_closing_reviewer_hides_the_placeholder_address_too() {
        assert!(REQUEST_JSON.contains("'reviewer',case when r.id is null then null else jsonb_build_object('id',r.id,'name',r.display_name,'email',case when lower(r.email) like '%@agents.invalid' then null else r.email end) end"));
    }

    #[test]
    fn the_contributor_to_tell_is_the_person_an_agent_acts_for() {
        let person = Uuid::from_u128(1);
        let agent = Uuid::from_u128(2);
        let by_person = json!({"author":{"id":person}});
        assert_eq!(contributor_of(&by_person), Some(person));
        let by_agent = json!({"author":{"id":agent},"agent":{"principal":{"id":person}}});
        assert_eq!(contributor_of(&by_agent), Some(person));
        assert_eq!(contributor_of(&json!({})), None);
    }
}
