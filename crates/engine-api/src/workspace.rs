//! Researcher workspace: atomic batches, sharing and named versions.
use crate::{
    auth::{store, AuthContext},
    error::ApiError,
    ops::apply,
    AppState,
};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use engine_core::{
    governance::{self, Role},
    log,
    materializer::{apply_payload, DocumentState},
    snapshot::merkle_root,
};
use engine_shared::{DocumentId, Event, EventPayload, IdentityId};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/search", get(search))
        .route(
            "/documents/:id/uploads",
            get(uploads)
                .post(upload)
                .layer(axum::extract::DefaultBodyLimit::max(30 * 1024 * 1024)),
        )
        .route("/documents/:id/uploads/:upload_id", get(download_upload))
        .route("/documents/:id/batch", post(batch))
        .route("/documents/:id/structure", get(structure))
        .route("/documents/:id/drafts/:draft_id/sync", post(sync_draft))
        .route(
            "/documents/:id/versions/:version_id/restore",
            post(restore_version),
        )
        .route("/documents/:id/members", get(members).post(share))
        .route("/documents/:id/versions", get(versions).post(save_version))
        .route("/documents/:id/metadata", post(metadata))
        .route("/documents/:id/drafts", get(drafts).post(create_draft))
        .route(
            "/documents/:id/drafts/:draft_id",
            get(get_draft).post(edit_draft),
        )
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024))
}

#[derive(Deserialize)]
pub struct BatchRequest {
    pub ops: Vec<EventPayload>,
    pub base_seq: Option<i64>,
    pub revision: Option<i64>,
}

#[utoipa::path(post, path="/documents/{id}/batch", tag="workspace", request_body=Value, params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn batch(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(mut req): Json<BatchRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.ops.is_empty() || req.ops.len() > 20_000 {
        return Err(bad(
            "A batch must contain 1–5000 changes, or up to 20,000 initial import nodes",
        ));
    }
    let doc = DocumentId(id);
    let actor = IdentityId(auth.identity_id);
    let (mut tx, role, mut current) = apply::begin_write(&state, doc, actor).await?;
    let latest: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    // Large documents can exceed 5,000 structural blocks. Permit one atomic
    // initial import within the existing 16 MiB request bound, without raising
    // the general editing limit or permitting partial multi-request imports.
    if req.ops.len() > 5000
        && (latest != 0
            || req
                .ops
                .iter()
                .any(|op| !matches!(op, EventPayload::NodeCreated { .. })))
    {
        return Err(bad("More than 5,000 changes are allowed only when importing new nodes into an empty document"));
    }
    if let Some(base) = req.base_seq.filter(|base| *base != latest) {
        if base < 0 || base > latest {
            return Err(bad("Invalid starting version"));
        }
        let prior = sqlx::query_as::<_, Event>(
            "select * from event where document_id=$1 and seq<=$2 order by seq",
        )
        .bind(id)
        .bind(base)
        .fetch_all(&mut *tx)
        .await?;
        let ancestor = engine_core::materializer::Materializer::fold(&prior)
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let mut local = ancestor.clone();
        for op in &req.ops {
            governance::authorize(role, op, &local)?;
            apply_payload(&mut local, op).map_err(|e| ApiError::Internal(e.to_string()))?;
        }
        let plan =
            engine_core::workspace_merge::merge(&ancestor, &current, &local, &Default::default());
        if !plan.conflicts.is_empty() {
            return Err(ApiError::Conflict{reason:"Another researcher changed the same block or cell. Your edits are kept on this screen. Save a personal draft to review the overlap.".into()});
        }
        let mut merged = plan.ops;
        // Review events are additive and are not part of the content tree delta.
        merged.extend(req.ops.into_iter().filter(|op| {
            matches!(
                op,
                EventPayload::CommentAdded { .. } | EventPayload::SuggestionProposed { .. }
            )
        }));
        req.ops = merged;
    }
    let mut events = Vec::new();
    for op in &req.ops {
        // Acceptance must use its atomic wrapped-operation endpoint.
        if !matches!(
            op,
            EventPayload::NodeCreated { .. }
                | EventPayload::FieldEdited { .. }
                | EventPayload::RichTextPatched { .. }
                | EventPayload::NodeMoved { .. }
                | EventPayload::NodeDeleted { .. }
                | EventPayload::NodeRestored { .. }
                | EventPayload::ChoiceAdded { .. }
                | EventPayload::ChoiceRemoved { .. }
                | EventPayload::CommentAdded { .. }
                | EventPayload::SuggestionProposed { .. }
        ) {
            return Err(bad("This action uses a separate review endpoint"));
        }
        governance::authorize(role, op, &current)?;
        if engine_core::richtext::redundant_label(&current, op) {
            continue;
        }
        let compact = engine_core::richtext::compact_event(&current, op);
        let op = &compact;
        apply::materialize_node_change(&mut tx, doc, op).await?;
        let event = log::append_in_tx(&mut tx, doc, op, actor).await?;
        apply_payload(&mut current, op).map_err(|e| ApiError::Internal(e.to_string()))?;
        events.push(event);
    }
    sqlx::query("update document set updated_at=now() where id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    log::post_commit_snapshot(&state.pool, doc, &req.ops).await?;
    for event in &events {
        state.subscriptions.publish(doc, event.clone());
    }
    Ok(Json(json!({"events":events})))
}

#[utoipa::path(get, path="/documents/{id}/members", tag="workspace", params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn members(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let rows: Vec<(Uuid,String,Option<String>,String)>=sqlx::query_as("select i.id,i.email,i.display_name,a.role from document_access a join identity i on i.id=a.identity_id where a.document_id=$1 order by i.email").bind(id).fetch_all(&state.pool).await?;
    Ok(Json(
        json!({"items": rows.into_iter().map(|(id,email,name,role)| json!({"id":id,"email":email,"name":name,"role":role})).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
struct ShareRequest {
    email: String,
    role: String,
}
#[utoipa::path(post, path="/documents/{id}/members", tag="workspace", request_body=Value, params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn share(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<ShareRequest>,
) -> Result<Json<Value>, ApiError> {
    let (mut tx, role, _) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if role != Role::Author
        || !sqlx::query_scalar::<_, bool>("select can_manage_document($1,$2)")
            .bind(id)
            .bind(auth.identity_id)
            .fetch_one(&mut *tx)
            .await?
    {
        return Err(ApiError::Forbidden);
    }
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 254 {
        return Err(bad("Enter a valid email address"));
    }
    let owner: Option<Uuid> = sqlx::query_scalar("select created_by from document where id=$1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let identity = store::upsert_identity(&state.pool, &email, None).await?;
    if owner == Some(identity.id.0) && req.role != "author" {
        return Err(bad("The document owner must remain an editor"));
    }
    if req.role == "remove" {
        sqlx::query("delete from document_access where document_id=$1 and identity_id=$2")
            .bind(id)
            .bind(identity.id.0)
            .execute(&mut *tx)
            .await?;
    } else {
        let granted: Role = req
            .role
            .parse()
            .map_err(|_| bad("Choose author, reviewer, auditor or remove"))?;
        sqlx::query("insert into document_access(document_id,identity_id,role) values($1,$2,$3) on conflict(document_id,identity_id) do update set role=excluded.role").bind(id).bind(identity.id.0).bind(granted.as_str()).execute(&mut *tx).await?;
    }
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "document.access_changed",
        id,
        json!({"identity_id":identity.id,"role":req.role}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"email":email,"role":req.role})))
}

#[derive(Deserialize)]
struct VersionRequest {
    name: String,
    #[serde(default)]
    note: String,
    base_seq: Option<i64>,
}
#[utoipa::path(post, path="/documents/{id}/versions", tag="workspace", request_body=Value, params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn save_version(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<VersionRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.name.trim().is_empty() || req.name.len() > 200 {
        return Err(bad("Name this version in 1–200 characters"));
    }
    let (mut tx, role, current) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if role != Role::Author {
        return Err(ApiError::Forbidden);
    }
    let last = sqlx::query_as::<_, Event>(
        "select * from event where document_id=$1 order by seq desc limit 1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let seq = last.as_ref().map_or(0, |e| e.seq);
    let hash = last.map_or_else(|| vec![0u8; 32], |e| e.chain_hash);
    let root = merkle_root(&current)?;
    let snapshot:Uuid=sqlx::query_scalar("insert into snapshot(document_id,through_seq,state,merkle_root,event_chain_hash) values($1,$2,$3,$4,$5) on conflict(document_id,through_seq) do update set through_seq=excluded.through_seq returning id")
        .bind(id).bind(seq).bind(json!(current)).bind(root.as_slice()).bind(hash).fetch_one(&mut *tx).await?;
    let version:Uuid=sqlx::query_scalar("insert into document_version(document_id,snapshot_id,name,note,created_by) values($1,$2,$3,$4,$5) returning id").bind(id).bind(snapshot).bind(req.name.trim()).bind(req.note).bind(auth.identity_id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":version,"snapshot_id":snapshot,"through_seq":seq}),
    ))
}
#[utoipa::path(get, path="/documents/{id}/versions", tag="workspace", params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn versions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let items:Vec<Value>=sqlx::query_scalar("select to_jsonb(v)||jsonb_build_object('through_seq',s.through_seq,'author',i.display_name) from document_version v join snapshot s on s.id=v.snapshot_id join identity i on i.id=v.created_by where v.document_id=$1 order by v.created_at desc").bind(id).fetch_all(&state.pool).await?;
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
struct Metadata {
    title: Option<String>,
    settings: Option<Value>,
    archived: Option<bool>,
}
#[utoipa::path(post, path="/documents/{id}/metadata", tag="workspace", request_body=Value, params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn metadata(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<Metadata>,
) -> Result<Json<Value>, ApiError> {
    let (mut tx, role, _) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if role != Role::Author {
        return Err(ApiError::Forbidden);
    }
    if req
        .title
        .as_ref()
        .is_some_and(|s| s.trim().is_empty() || s.len() > 500)
    {
        return Err(bad("Title must contain 1–500 characters"));
    }
    sqlx::query("update document set title=coalesce($2,title),settings=coalesce($3,settings),status=case when $4::boolean is null then status when $4 then 'archived' when deployed_snapshot_id is not null then 'deployed' else 'draft' end,updated_at=now() where id=$1").bind(id).bind(req.title).bind(req.settings).bind(req.archived).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[utoipa::path(post, path="/documents/{id}/drafts", tag="workspace", request_body=Value, params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn create_draft(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<VersionRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.name.trim().is_empty() || req.name.len() > 200 {
        return Err(bad("Name this draft in 1–200 characters"));
    }
    let (mut tx, role, mut current) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if role == Role::Auditor {
        return Err(ApiError::Forbidden);
    }
    let seq: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    let seq = if let Some(base) = req.base_seq {
        if base < 0 || base > seq {
            return Err(bad("Invalid starting version"));
        }
        let events = sqlx::query_as::<_, Event>(
            "select * from event where document_id=$1 and seq<=$2 order by seq",
        )
        .bind(id)
        .bind(base)
        .fetch_all(&mut *tx)
        .await?;
        current = engine_core::materializer::Materializer::fold(&events)
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        base
    } else {
        seq
    };
    let draft:Uuid=sqlx::query_scalar("insert into workspace_draft(document_id,name,base_seq,base_state,created_by) values($1,$2,$3,$4,$5) returning id").bind(id).bind(req.name).bind(seq).bind(json!(current)).bind(auth.identity_id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":draft})))
}
#[utoipa::path(get, path="/documents/{id}/drafts", tag="workspace", params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn drafts(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let items:Vec<Value>=sqlx::query_scalar("select to_jsonb(d)-'base_state' from workspace_draft d where document_id=$1 and created_by=$2 order by created_at desc").bind(id).bind(auth.identity_id).fetch_all(&state.pool).await?;
    Ok(Json(json!({"items":items})))
}
#[utoipa::path(get, path="/documents/{id}/drafts/{draft_id}", tag="workspace", params(("id" = Uuid, Path, description = "Resource identifier"), ("draft_id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn get_draft(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let row:Value=sqlx::query_scalar("select to_jsonb(d) from workspace_draft d where id=$1 and document_id=$2 and created_by=$3").bind(draft).bind(id).bind(auth.identity_id).fetch_optional(&state.pool).await?.ok_or(ApiError::NotFound)?;
    let mut current: DocumentState = serde_json::from_value(row["base_state"].clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let ops: Vec<Value> =
        sqlx::query_scalar("select payload from draft_edit where draft_id=$1 order by id")
            .bind(draft)
            .fetch_all(&state.pool)
            .await?;
    for op in &ops {
        apply_payload(
            &mut current,
            &serde_json::from_value(op.clone()).map_err(|e| ApiError::Internal(e.to_string()))?,
        )
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    Ok(Json(json!({"draft":row,"state":current,"ops":ops})))
}
#[utoipa::path(post, path="/documents/{id}/drafts/{draft_id}", tag="workspace", request_body=Value, params(("id" = Uuid, Path, description = "Resource identifier"), ("draft_id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn edit_draft(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<BatchRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.ops.is_empty() || req.ops.len() > 5000 {
        return Err(bad("A draft save must contain 1–5000 changes"));
    }
    let (mut tx, role, _) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if role == Role::Auditor {
        return Err(ApiError::Forbidden);
    }
    let revision:i64=sqlx::query_scalar("select revision from workspace_draft where id=$1 and document_id=$2 and created_by=$3 for update").bind(draft).bind(id).bind(auth.identity_id).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?;
    if req.revision != Some(revision) {
        return Err(ApiError::Conflict {
            reason: "This draft changed in another tab. Reopen it before saving.".into(),
        });
    }
    let base:Value=sqlx::query_scalar("select base_state from workspace_draft where id=$1 and document_id=$2 and created_by=$3 and merged_at is null for update").bind(draft).bind(id).bind(auth.identity_id).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?;
    let mut current: DocumentState =
        serde_json::from_value(base).map_err(|e| ApiError::Internal(e.to_string()))?;
    let prior: Vec<Value> =
        sqlx::query_scalar("select payload from draft_edit where draft_id=$1 order by id")
            .bind(draft)
            .fetch_all(&mut *tx)
            .await?;
    for op in prior {
        apply_payload(
            &mut current,
            &serde_json::from_value(op).map_err(|e| ApiError::Internal(e.to_string()))?,
        )
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    for op in req.ops {
        if !matches!(
            op,
            EventPayload::NodeCreated { .. }
                | EventPayload::FieldEdited { .. }
                | EventPayload::RichTextPatched { .. }
                | EventPayload::NodeMoved { .. }
                | EventPayload::NodeDeleted { .. }
                | EventPayload::NodeRestored { .. }
                | EventPayload::ChoiceAdded { .. }
                | EventPayload::ChoiceRemoved { .. }
        ) {
            return Err(bad("Drafts contain content changes only"));
        }
        governance::authorize(Role::Author, &op, &current)?;
        if engine_core::richtext::redundant_label(&current, &op) {
            continue;
        }
        let op = engine_core::richtext::compact_event(&current, &op);
        apply_payload(&mut current, &op).map_err(|e| ApiError::Internal(e.to_string()))?;
        sqlx::query("insert into draft_edit(draft_id,payload,actor_id) values($1,$2,$3)")
            .bind(draft)
            .bind(json!(op))
            .bind(auth.identity_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("update workspace_draft set revision=revision+1 where id=$1")
        .bind(draft)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"state":current,"revision":revision+1})))
}
fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}

/// Derived ontology/provenance view, permissioned exactly like document content.
/// It is not an additional source of truth and does not require a graph database.
#[utoipa::path(get, path="/documents/{id}/structure", tag="workspace", params(("id" = Uuid, Path, description = "Document identifier")), responses((status=200, description="Formatting-run classification and revision provenance", body=Value),(status=403, description="Document membership required")))]
async fn structure(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let doc = DocumentId(id);
    apply::require_role(&state.pool, doc, IdentityId(auth.identity_id)).await?;
    let (current, through_seq) = engine_core::snapshot::SnapshotEngine::new(state.pool.clone())
        .read_current_state_with_seq(doc)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let provenance: Vec<Value> = sqlx::query_scalar("select distinct on (target_node_id) jsonb_build_object('node_id',target_node_id,'activity',id,'agent',actor_id,'seq',seq,'at',created_at) from event where document_id=$1 and seq<=$2 and target_node_id is not null and type in ('NodeCreated','NodeMoved','NodeDeleted','NodeRestored','FieldEdited','RichTextPatched','ChoiceAdded','ChoiceRemoved') order by target_node_id,seq desc")
        .bind(id).bind(through_seq).fetch_all(&state.pool).await?;
    let blocks: Vec<_> = current.nodes.values().filter(|n| !n.deleted && n.current_fields["content"].is_object()).map(|n| {
        let content = &n.current_fields["content"];
        json!({"id":n.id,"parent_id":n.parent_id,"kind":content["type"],"revision_hash":engine_core::richtext::fingerprint(content),"analysis":engine_core::richtext::analyze(content)})
    }).collect();
    Ok(Json(
        json!({"schema":"dynodoc-structure-v1","document_id":id,"through_seq":through_seq,"blocks":blocks,"provenance":provenance,"relations":{"parent_id":"contains (reverse direction)","activity":"wasGeneratedBy","agent":"wasAssociatedWith"}}),
    ))
}

#[derive(Deserialize)]
struct UploadRequest {
    filename: String,
    content_type: String,
    content: String,
    report: Value,
}
#[utoipa::path(post, path="/documents/{id}/uploads", tag="workspace", request_body=Value, params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn upload(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<UploadRequest>,
) -> Result<Json<Value>, ApiError> {
    use base64::Engine;
    let role =
        apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if role != Role::Author {
        return Err(ApiError::Forbidden);
    }
    let content = base64::engine::general_purpose::STANDARD
        .decode(req.content)
        .map_err(|_| bad("Invalid file encoding"))?;
    if content.len() > 10 * 1024 * 1024 || req.filename.len() > 255 {
        return Err(bad("File exceeds the upload limit"));
    }
    let upload:Uuid=sqlx::query_scalar("insert into document_upload(document_id,filename,content_type,content,report,created_by) values($1,$2,$3,$4,$5,$6) returning id")
        .bind(id).bind(req.filename).bind(req.content_type).bind(content).bind(req.report).bind(auth.identity_id).fetch_one(&state.pool).await?;
    Ok(Json(json!({"id":upload})))
}
#[utoipa::path(get, path="/documents/{id}/uploads", tag="workspace", params(("id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn uploads(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let items:Vec<Value>=sqlx::query_scalar("select to_jsonb(u)-'content' from document_upload u where document_id=$1 order by created_at desc").bind(id).fetch_all(&state.pool).await?;
    Ok(Json(json!({"items":items})))
}
#[utoipa::path(get, path="/documents/{id}/uploads/{upload_id}", tag="workspace", params(("id" = Uuid, Path, description = "Resource identifier"), ("upload_id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn download_upload(
    State(state): State<AppState>,
    Path((id, upload)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let row: (String, Vec<u8>) = sqlx::query_as(
        "select filename,content from document_upload where id=$1 and document_id=$2",
    )
    .bind(upload)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let safe: String = row
        .0
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || ".-_".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut response = row.1.into_response();
    response.headers_mut().insert(
        "content-type",
        axum::http::HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        "content-disposition",
        format!("attachment; filename=\"{safe}\"")
            .parse()
            .map_err(|_| bad("Invalid filename"))?,
    );
    response.headers_mut().insert(
        "x-content-type-options",
        axum::http::HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

#[derive(Deserialize)]
struct SyncRequest {
    #[serde(default)]
    action: String,
    team_seq: Option<i64>,
    revision: Option<i64>,
    #[serde(default)]
    resolutions: std::collections::BTreeMap<String, String>,
    included_nodes: Option<Vec<engine_shared::NodeId>>,
    #[serde(default)]
    remember_selection: bool,
}
#[utoipa::path(post, path="/documents/{id}/drafts/{draft_id}/sync", tag="workspace", request_body=Value, params(("id" = Uuid, Path, description = "Resource identifier"), ("draft_id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn sync_draft(
    State(state): State<AppState>,
    Path((id, draft)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<SyncRequest>,
) -> Result<Json<Value>, ApiError> {
    let doc = DocumentId(id);
    let actor = IdentityId(auth.identity_id);
    let (mut tx, role, mut team) = apply::begin_write(&state, doc, actor).await?;
    let row:Value=sqlx::query_scalar("select to_jsonb(d) from workspace_draft d where id=$1 and document_id=$2 and created_by=$3 and merged_at is null for update").bind(draft).bind(id).bind(auth.identity_id).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?;
    let decode = |v: Value| {
        serde_json::from_value::<DocumentState>(v).map_err(|e| ApiError::Internal(e.to_string()))
    };
    let mut local = decode(row["base_state"].clone())?;
    let base = decode(if row["merge_base_state"].is_null() {
        row["base_state"].clone()
    } else {
        row["merge_base_state"].clone()
    })?;
    let edits: Vec<Value> =
        sqlx::query_scalar("select payload from draft_edit where draft_id=$1 order by id")
            .bind(draft)
            .fetch_all(&mut *tx)
            .await?;
    for edit in edits {
        apply_payload(
            &mut local,
            &serde_json::from_value(edit).map_err(|e| ApiError::Internal(e.to_string()))?,
        )
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    let seq: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    let revision = row["revision"].as_i64().unwrap_or(0);
    let plan = engine_core::workspace_merge::merge(&base, &team, &local, &req.resolutions);
    let changes = plan.ops.clone();
    if req.action.is_empty() || req.action == "preview" {
        return Ok(Json(
            json!({"conflicts":plan.conflicts,"ops":changes,"team_seq":seq,"revision":revision,"excluded_nodes":row["excluded_nodes"]}),
        ));
    }
    if req.team_seq != Some(seq) || req.revision != Some(revision) {
        return Err(ApiError::Conflict{reason:"The team version or this draft changed. Review the updated comparison before continuing.".into()});
    }
    if !plan.conflicts.is_empty() {
        return Err(ApiError::Conflict {
            reason: "Choose which value to keep for each overlapping change.".into(),
        });
    }
    if role == Role::Auditor || (req.action == "share" && role != Role::Author) {
        return Err(ApiError::Forbidden);
    }
    if req.action != "share" && req.action != "pull" {
        return Err(bad("Choose preview, pull or share"));
    }
    let mut events = Vec::new();
    if req.action == "share" {
        let excluded: std::collections::BTreeSet<String> =
            serde_json::from_value(row["excluded_nodes"].clone()).unwrap_or_default();
        let included = req.included_nodes.as_ref().map(|ids| {
            ids.iter()
                .map(|id| id.0.as_str())
                .collect::<std::collections::BTreeSet<_>>()
        });
        if req.remember_selection {
            if let Some(included) = &included {
                let held: std::collections::BTreeSet<String> = changes
                    .iter()
                    .filter_map(|op| op.target_node_id())
                    .filter(|node| !included.contains(node.0.as_str()))
                    .map(|node| node.0.clone())
                    .collect();
                // Keep earlier exclusions for unchanged nodes, too.
                let mut keep: std::collections::BTreeSet<String> = excluded
                    .iter()
                    .filter(|id| !included.contains(id.as_str()))
                    .cloned()
                    .collect();
                keep.extend(held);
                if keep.len() > 20_000 {
                    return Err(bad("A draft can keep up to 20,000 block exclusions. Share or separate this draft before adding more."));
                }
                sqlx::query("update workspace_draft set excluded_nodes=$2 where id=$1")
                    .bind(draft)
                    .bind(keep.into_iter().collect::<Vec<_>>())
                    .execute(&mut *tx)
                    .await?;
            }
        }
        for op in changes.iter().filter(|op| {
            included.as_ref().map_or_else(
                || {
                    op.target_node_id()
                        .is_none_or(|id| !excluded.contains(&id.0))
                },
                |ids| {
                    op.target_node_id()
                        .is_some_and(|id| ids.contains(id.0.as_str()))
                },
            )
        }) {
            governance::authorize(role, op, &team)?;
            if engine_core::richtext::redundant_label(&team, op) {
                continue;
            }
            apply::materialize_node_change(&mut tx, doc, op).await?;
            events.push(log::append_in_tx(&mut tx, doc, op, actor).await?);
            apply_payload(&mut team, op).map_err(|e| ApiError::Internal(e.to_string()))?;
        }
        let issues = engine_core::integrity::validate_integrity(&team);
        if !issues.is_empty() {
            return Err(ApiError::Conflict{reason:format!("The combined document has {} validation issue(s). Include the related changes or fix the draft first.",issues.len())});
        }
    }
    // Carry new team work into this personal draft without replaying or rewriting
    // canonical history. The new base makes the next comparison incremental.
    for op in engine_core::workspace_merge::diff(&local, &plan.state) {
        governance::authorize(Role::Author, &op, &local)?;
        apply_payload(&mut local, &op).map_err(|e| ApiError::Internal(e.to_string()))?;
        sqlx::query("insert into draft_edit(draft_id,payload,actor_id) values($1,$2,$3)")
            .bind(draft)
            .bind(json!(op))
            .bind(auth.identity_id)
            .execute(&mut *tx)
            .await?;
    }
    let completed =
        req.action == "share" && engine_core::workspace_merge::diff(&team, &local).is_empty();
    let next_seq = events.last().map_or(seq, |e| e.seq);
    sqlx::query("update workspace_draft set merge_base_state=$2,base_seq=$3,revision=revision+1,merged_at=case when $4 then now() else merged_at end where id=$1").bind(draft).bind(json!(team)).bind(next_seq).bind(completed).execute(&mut *tx).await?;
    tx.commit().await?;
    for event in &events {
        state.subscriptions.publish(doc, event.clone());
    }
    Ok(Json(
        json!({"state":local,"events":events,"revision":revision+1,"completed":completed,"team_seq":next_seq}),
    ))
}
#[derive(Deserialize)]
struct RestoreRequest {
    base_seq: i64,
}
#[utoipa::path(post, path="/documents/{id}/versions/{version_id}/restore", tag="workspace", request_body=Value, params(("id" = Uuid, Path, description = "Resource identifier"), ("version_id" = Uuid, Path, description = "Resource identifier")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn restore_version(
    State(state): State<AppState>,
    Path((id, version)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(req): Json<RestoreRequest>,
) -> Result<Json<Value>, ApiError> {
    let doc = DocumentId(id);
    let actor = IdentityId(auth.identity_id);
    let (mut tx, role, mut current) = apply::begin_write(&state, doc, actor).await?;
    if role != Role::Author {
        return Err(ApiError::Forbidden);
    }
    let seq: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if seq != req.base_seq {
        return Err(ApiError::Conflict {
            reason: "The document changed. Review the latest version before restoring.".into(),
        });
    }
    let saved:Value=sqlx::query_scalar("select s.state from document_version v join snapshot s on s.id=v.snapshot_id where v.id=$1 and v.document_id=$2").bind(version).bind(id).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?;
    let target: DocumentState =
        serde_json::from_value(saved).map_err(|e| ApiError::Internal(e.to_string()))?;
    let ops = engine_core::workspace_merge::diff(&current, &target);
    let mut events = Vec::new();
    for op in &ops {
        governance::authorize(role, op, &current)?;
        if engine_core::richtext::redundant_label(&current, op) {
            continue;
        }
        let compact = engine_core::richtext::compact_event(&current, op);
        let op = &compact;
        apply::materialize_node_change(&mut tx, doc, op).await?;
        events.push(log::append_in_tx(&mut tx, doc, op, actor).await?);
        apply_payload(&mut current, op).map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    tx.commit().await?;
    for event in &events {
        state.subscriptions.publish(doc, event.clone());
    }
    Ok(Json(json!({"events":events})))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}
#[utoipa::path(get, path="/search", tag="workspace", params(("q" = String, Query, description = "Literal search term, 2–200 characters")), responses((status=200, description="Success; requires document membership", body=Value),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Review a concurrent change before retrying")))]
async fn search(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<SearchQuery>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let term = query.q.trim();
    if term.len() < 2 {
        return Ok(Json(json!({"items":[]})));
    }
    if term.len() > 200 {
        return Err(bad("Search in 200 characters or fewer"));
    }
    let items:Vec<Value>=sqlx::query_scalar("select jsonb_build_object('document_id',d.id,'title',d.title,'node_id',n.id,'text',left(n.current_fields::text,4000)) from document d left join node n on n.document_id=d.id and not n.deleted where effective_document_role(d.id,$1) is not null and d.status<>'archived' and (strpos(lower(d.title),lower($2))>0 or strpos(lower(n.current_fields::text),lower($2))>0) order by d.updated_at desc,n.pos limit 50").bind(auth.identity_id).bind(term).fetch_all(&state.pool).await?;
    Ok(Json(json!({"items":items})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    batch,
    members,
    share,
    versions,
    save_version,
    metadata,
    drafts,
    create_draft,
    get_draft,
    edit_draft,
    sync_draft,
    restore_version,
    uploads,
    upload,
    download_upload,
    search,
    structure
))]
pub struct WorkspaceApi;
