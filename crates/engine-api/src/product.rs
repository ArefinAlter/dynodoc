//! Workspace hierarchy, recoverable lifecycle and explicitly published public copies.
use crate::{
    auth::{random_token, sha256, AuthContext},
    error::ApiError,
    ops::apply,
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use engine_core::snapshot::merkle_root;
use engine_shared::{DocumentId, IdentityId};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/spaces", get(spaces).post(create_space))
        .route(
            "/spaces/:id/members",
            get(space_members).post(set_space_member),
        )
        .route("/workspace/index", get(index))
        .route("/documents/:id/access", get(document_access))
        .route("/documents/:id/location", post(move_document))
        .route("/documents/:id/trash", post(trash_document))
        .route(
            "/documents/:id/public-links",
            get(public_links).post(publish),
        )
        .route("/documents/:id/public-links/:link/revoke", post(revoke))
        .route("/public/documents/:token", get(public_document))
}
fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}
pub async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    action: &str,
    resource: Uuid,
    detail: Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "insert into operation_audit(actor_id,action,resource_id,detail) values($1,$2,$3,$4)",
    )
    .bind(actor)
    .bind(action)
    .bind(resource)
    .bind(detail)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
async fn manage(tx: &mut Transaction<'_, Postgres>, id: Uuid, actor: Uuid) -> Result<(), ApiError> {
    let exists: Option<Uuid> = sqlx::query_scalar("select id from document where id=$1 for update")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    if exists.is_none() {
        return Err(ApiError::NotFound);
    }
    if !sqlx::query_scalar::<_, bool>("select can_manage_document($1,$2)")
        .bind(id)
        .bind(actor)
        .fetch_one(&mut **tx)
        .await?
    {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}
#[utoipa::path(get, path="/spaces", tag="product", security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn spaces(State(state): State<AppState>, auth: AuthContext) -> Result<Json<Value>, ApiError> {
    let items:Vec<Value> = sqlx::query_scalar("select to_jsonb(s)||jsonb_build_object('role',inherited_space_role(s.id,$1)) from access_space s where inherited_space_role(s.id,$1) is not null order by s.created_at limit 1000").bind(auth.identity_id).fetch_all(&state.pool).await?;
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
struct NewSpace {
    name: String,
    kind: String,
    parent_id: Option<Uuid>,
}
#[utoipa::path(post, path="/spaces", tag="product", request_body=Value, security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn create_space(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(input): Json<NewSpace>,
) -> Result<Json<Value>, ApiError> {
    let name = input.name.trim();
    if name.is_empty()
        || name.len() > 160
        || !["organization", "team", "folder"].contains(&input.kind.as_str())
    {
        return Err(bad("Enter a name and choose organization, team or folder"));
    }
    let mut tx = state.pool.begin().await?;
    if input.kind == "organization" {
        if input.parent_id.is_some() {
            return Err(bad("Organizations cannot have parents"));
        }
    } else {
        let parent = input
            .parent_id
            .ok_or_else(|| bad("Choose a parent workspace"))?;
        let parent_kind: Option<String> =
            sqlx::query_scalar("select kind from access_space where id=$1 for update")
                .bind(parent)
                .fetch_optional(&mut *tx)
                .await?;
        if (input.kind == "team" && parent_kind.as_deref() != Some("organization"))
            || (input.kind == "folder"
                && !matches!(parent_kind.as_deref(), Some("team" | "folder")))
        {
            return Err(bad(
                "Teams belong to organizations; folders belong to teams or folders",
            ));
        }
        let rank: i32 = sqlx::query_scalar("select access_role_rank(inherited_space_role($1,$2))")
            .bind(parent)
            .bind(auth.identity_id)
            .fetch_one(&mut *tx)
            .await?;
        if rank < 4 {
            return Err(ApiError::Forbidden);
        }
        let depth:i64=sqlx::query_scalar("with recursive a as (select id,parent_id from access_space where id=$1 union select s.id,s.parent_id from access_space s join a on a.parent_id=s.id) select count(*) from a").bind(parent).fetch_one(&mut *tx).await?;
        if depth >= 12 {
            return Err(bad("Workspace nesting is limited to twelve levels"));
        }
    }
    let id: Uuid = sqlx::query_scalar(
        "insert into access_space(name,kind,parent_id,created_by) values($1,$2,$3,$4) returning id",
    )
    .bind(name)
    .bind(&input.kind)
    .bind(input.parent_id)
    .bind(auth.identity_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,'owner')")
        .bind(id)
        .bind(auth.identity_id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        auth.identity_id,
        "space.created",
        id,
        json!({"kind":input.kind,"parent_id":input.parent_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
#[utoipa::path(get, path="/spaces/{id}/members", tag="product", params(("id" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn space_members(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let role: Option<String> = sqlx::query_scalar("select inherited_space_role($1,$2)")
        .bind(id)
        .bind(auth.identity_id)
        .fetch_one(&state.pool)
        .await?;
    if role.is_none() {
        return Err(ApiError::Forbidden);
    }
    let items:Vec<Value>=sqlx::query_scalar("with recursive a as (select id,parent_id,name from access_space where id=$1 union select s.id,s.parent_id,s.name from access_space s join a on a.parent_id=s.id) select jsonb_build_object('id',i.id,'email',i.email,'name',i.display_name,'role',m.role,'space_id',a.id,'space_name',a.name,'inherited',a.id<>$1) from a join space_member m on m.space_id=a.id join identity i on i.id=m.identity_id where i.disabled_at is null order by i.email").bind(id).fetch_all(&state.pool).await?;
    Ok(Json(json!({"role":role,"items":items})))
}
#[derive(Deserialize)]
struct MemberChange {
    email: String,
    role: String,
}
#[utoipa::path(post, path="/spaces/{id}/members", tag="product", request_body=Value, params(("id" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn set_space_member(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(input): Json<MemberChange>,
) -> Result<Json<Value>, ApiError> {
    if !["owner", "manager", "editor", "reviewer", "viewer", "remove"]
        .contains(&input.role.as_str())
        || input.email.len() > 254
        || !input.email.contains('@')
    {
        return Err(bad("Choose a valid role and email address"));
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query("select id from access_space where id=$1 for update")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    let rank: i32 = sqlx::query_scalar("select access_role_rank(inherited_space_role($1,$2))")
        .bind(id)
        .bind(auth.identity_id)
        .fetch_one(&mut *tx)
        .await?;
    if rank < 4 || (input.role == "owner" && rank < 5) {
        return Err(ApiError::Forbidden);
    }
    let identity =
        crate::auth::store::upsert_identity(&state.pool, &input.email.trim().to_lowercase(), None)
            .await?;
    let previous: Option<String> =
        sqlx::query_scalar("select role from space_member where space_id=$1 and identity_id=$2")
            .bind(id)
            .bind(identity.id.0)
            .fetch_optional(&mut *tx)
            .await?;
    if previous.as_deref() == Some("owner") && input.role != "owner" {
        let owners: i64 = sqlx::query_scalar(
            "select count(*) from space_member where space_id=$1 and role='owner'",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if rank < 5 || owners <= 1 {
            return Err(bad("Keep at least one direct owner of this workspace"));
        }
    }
    if input.role == "remove" {
        sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
            .bind(id)
            .bind(identity.id.0)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,$3) on conflict(space_id,identity_id) do update set role=excluded.role").bind(id).bind(identity.id.0).bind(&input.role).execute(&mut *tx).await?;
    }
    audit(
        &mut tx,
        auth.identity_id,
        "space.access_changed",
        id,
        json!({"identity_id":identity.id,"role":input.role}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct IndexQuery {
    #[serde(default)]
    offset: i64,
    space_id: Option<Uuid>,
    #[serde(default)]
    trash: bool,
}
#[utoipa::path(get, path="/workspace/index", tag="product", security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn index(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<IndexQuery>,
) -> Result<Json<Value>, ApiError> {
    let items:Vec<Value>=sqlx::query_scalar("select to_jsonb(d)||jsonb_build_object('role',effective_document_role(d.id,$1,true),'can_manage',can_manage_document(d.id,$1)) from document d where effective_document_role(d.id,$1,true) is not null and (d.deleted_at is not null)=$2 and ($3::uuid is null or d.space_id=$3) order by d.updated_at desc,d.id limit 101 offset $4").bind(auth.identity_id).bind(q.trash).bind(q.space_id).bind(q.offset.clamp(0,100000)).fetch_all(&state.pool).await?;
    let more = items.len() > 100;
    Ok(Json(
        json!({"items":items.into_iter().take(100).collect::<Vec<_>>(),"more":more}),
    ))
}
#[utoipa::path(get, path="/documents/{id}/access", tag="product", params(("id" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn document_access(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    apply::require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let value:Value=sqlx::query_scalar("select jsonb_build_object('can_manage',can_manage_document(d.id,$2),'space_id',d.space_id,'inherited_role',inherited_space_role(d.space_id,$2)) from document d where d.id=$1").bind(id).bind(auth.identity_id).fetch_one(&state.pool).await?;
    Ok(Json(value))
}
#[derive(Deserialize)]
struct Location {
    space_id: Option<Uuid>,
}
#[utoipa::path(post, path="/documents/{id}/location", tag="product", request_body=Value, params(("id" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn move_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(input): Json<Location>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = state.pool.begin().await?;
    manage(&mut tx, id, auth.identity_id).await?;
    if let Some(space) = input.space_id {
        let role: i32 = sqlx::query_scalar("select access_role_rank(inherited_space_role($1,$2))")
            .bind(space)
            .bind(auth.identity_id)
            .fetch_one(&mut *tx)
            .await?;
        if role < 3 {
            return Err(ApiError::Forbidden);
        }
    }
    sqlx::query("update document set space_id=$2,updated_at=now() where id=$1")
        .bind(id)
        .bind(input.space_id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        auth.identity_id,
        "document.moved",
        id,
        json!({"space_id":input.space_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct Trash {
    deleted: bool,
}
#[utoipa::path(post, path="/documents/{id}/trash", tag="product", request_body=Value, params(("id" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn trash_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(input): Json<Trash>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = state.pool.begin().await?;
    manage(&mut tx, id, auth.identity_id).await?;
    sqlx::query("update document set deleted_at=case when $2 then now() else null end,updated_at=now() where id=$1").bind(id).bind(input.deleted).execute(&mut *tx).await?;
    if input.deleted {
        sqlx::query("update public_document_view set revoked_at=now() where document_id=$1 and revoked_at is null").bind(id).execute(&mut *tx).await?;
    }
    audit(
        &mut tx,
        auth.identity_id,
        if input.deleted {
            "document.trashed"
        } else {
            "document.restored"
        },
        id,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[utoipa::path(get, path="/documents/{id}/public-links", tag="product", params(("id" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn public_links(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut tx = state.pool.begin().await?;
    manage(&mut tx, id, auth.identity_id).await?;
    let items:Vec<Value>=sqlx::query_scalar("select jsonb_build_object('id',id,'created_at',created_at,'expires_at',expires_at,'revoked_at',revoked_at) from public_document_view where document_id=$1 order by created_at desc limit 100").bind(id).fetch_all(&mut *tx).await?;
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
struct Publication {
    base_seq: i64,
    expires_days: Option<i32>,
}
#[utoipa::path(post, path="/documents/{id}/public-links", tag="product", request_body=Value, params(("id" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn publish(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(input): Json<Publication>,
) -> Result<Json<Value>, ApiError> {
    if input.expires_days.is_some_and(|d| !(1..=365).contains(&d)) {
        return Err(bad("Expiry must be between 1 and 365 days"));
    }
    let (mut tx, _, current) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if !sqlx::query_scalar::<_, bool>("select can_manage_document($1,$2)")
        .bind(id)
        .bind(auth.identity_id)
        .fetch_one(&mut *tx)
        .await?
    {
        return Err(ApiError::Forbidden);
    }
    let last: Option<(i64, Vec<u8>)> = sqlx::query_as(
        "select seq,chain_hash from event where document_id=$1 order by seq desc limit 1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (seq, hash) = last.unwrap_or((0, vec![0u8; 32]));
    if seq != input.base_seq {
        return Err(bad(
            "Refresh and save your changes before publishing this copy",
        ));
    }
    if current.nodes.values().any(|node| {
        !node.deleted
            && node
                .current_fields
                .get("content")
                .is_some_and(has_text_revisions)
    }) {
        return Err(bad(
            "Accept or reject tracked text changes before publishing a public copy",
        ));
    }
    let root = merkle_root(&current)?;
    let snapshot:Uuid=sqlx::query_scalar("insert into snapshot(document_id,through_seq,state,merkle_root,event_chain_hash) values($1,$2,$3,$4,$5) on conflict(document_id,through_seq) do update set through_seq=excluded.through_seq returning id").bind(id).bind(seq).bind(json!(current)).bind(root.as_slice()).bind(hash).fetch_one(&mut *tx).await?;
    let (token, hash) = random_token()?;
    let link:Uuid=sqlx::query_scalar("insert into public_document_view(document_id,token_hash,snapshot_id,created_by,expires_at,published_title,published_kind) select id,$2,$3,$4,case when $5::integer is null then null else now()+make_interval(days=>$5) end,title,coalesce(settings->>'kind','questionnaire') from document where id=$1 returning id").bind(id).bind(hash).bind(snapshot).bind(auth.identity_id).bind(input.expires_days).fetch_one(&mut *tx).await?;
    audit(
        &mut tx,
        auth.identity_id,
        "document.published",
        id,
        json!({"link_id":link,"through_seq":input.base_seq}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":link,"token":token})))
}
#[utoipa::path(post, path="/documents/{id}/public-links/{link}/revoke", tag="product", request_body=Value, params(("id" = String, Path, description = "Resource identifier"),("link" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn revoke(
    State(state): State<AppState>,
    Path((id, link)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut tx = state.pool.begin().await?;
    manage(&mut tx, id, auth.identity_id).await?;
    sqlx::query("update public_document_view set revoked_at=now() where document_id=$1 and id=$2")
        .bind(id)
        .bind(link)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        auth.identity_id,
        "document.public_link_revoked",
        id,
        json!({"link_id":link}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[utoipa::path(get, path="/public/documents/{token}", tag="product", params(("token" = String, Path, description = "Resource identifier")), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn public_document(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::NotFound);
    }
    let row:Option<(String,String,Value)>=sqlx::query_as("select p.published_title,p.published_kind,s.state from public_document_view p join document d on d.id=p.document_id join snapshot s on s.id=p.snapshot_id join identity i on i.id=p.created_by where p.token_hash=$1 and p.revoked_at is null and (p.expires_at is null or p.expires_at>now()) and d.deleted_at is null and i.disabled_at is null").bind(sha256(token.as_bytes())).fetch_optional(&state.pool).await?;
    let (title, kind, mut content) = row.ok_or(ApiError::NotFound)?;
    // The public contract is content only, never suggestions, comments, events or uploads.
    content["comments"] = json!([]);
    content["suggestions"] = json!({});
    let mut hidden: std::collections::HashSet<String> = content
        .get("removed_choices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|id| id.as_str().map(str::to_owned))
        .collect();
    content["removed_choices"] = json!([]);
    if let Some(nodes) = content.get_mut("nodes").and_then(Value::as_object_mut) {
        for (id, node) in nodes.iter() {
            if node.get("deleted").and_then(Value::as_bool) == Some(true) {
                hidden.insert(id.clone());
            }
        }
        // A removed group must not expose its still-materialized descendants.
        loop {
            let before = hidden.len();
            for (id, node) in nodes.iter() {
                if node
                    .get("parent_id")
                    .and_then(Value::as_str)
                    .is_some_and(|parent| hidden.contains(parent))
                {
                    hidden.insert(id.clone());
                }
            }
            if hidden.len() == before {
                break;
            }
        }
        nodes.retain(|id, _| !hidden.contains(id));
        for node in nodes.values_mut() {
            if let Some(fields) = node
                .get_mut("current_fields")
                .and_then(Value::as_object_mut)
            {
                fields.remove("import_report");
                fields.remove("research_source");
                fields.remove("fonts");
                if let Some(rich) = fields.get_mut("content") {
                    strip_private_review(rich);
                    let label = engine_core::richtext::plain_text(rich);
                    fields.insert("label".into(), json!(label));
                }
                for (key, value) in fields.iter_mut() {
                    if key.starts_with("format_") {
                        if let Some(format) = value.as_object_mut() {
                            format.remove("note");
                        }
                    }
                }
            }
        }
    }
    Ok(Json(json!({"title":title,"kind":kind,"state":content})))
}

fn has_text_revisions(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.get("type").and_then(Value::as_str) == Some("trackedChange")
                || object.values().any(has_text_revisions)
        }
        Value::Array(values) => values.iter().any(has_text_revisions),
        _ => false,
    }
}

// Native DOCX comments are embedded marks, unlike event-log comments. The public
// projection must remove both, including their author/body attributes. Never mutate
// the stored snapshot: this is a read-only projection of an explicitly published copy.
fn strip_private_review(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        if let Some(marks) = object.get_mut("marks").and_then(Value::as_array_mut) {
            marks.retain(|mark| {
                !matches!(
                    mark.get("type").and_then(Value::as_str),
                    Some("sourceComment" | "trackedChange")
                )
            });
        }
        if let Some(children) = object.get_mut("content").and_then(Value::as_array_mut) {
            children.retain(|node| {
                !node
                    .get("marks")
                    .and_then(Value::as_array)
                    .is_some_and(|marks| {
                        marks.iter().any(|mark| {
                            mark.get("type").and_then(Value::as_str) == Some("trackedChange")
                                && mark.pointer("/attrs/kind").and_then(Value::as_str)
                                    == Some("delete")
                        })
                    })
            });
            for child in children {
                strip_private_review(child);
            }
        }
    }
}

#[cfg(test)]
mod office_public_tests {
    use super::*;

    #[test]
    fn embedded_review_metadata_stays_out_of_public_copies() {
        let mut content = json!({"type":"paragraph","content":[
            {"type":"text","text":"Visible","marks":[{"type":"bold"},{"type":"sourceComment","attrs":{"author":"Private reviewer","body":"Private note"}}]},
            {"type":"text","text":"Deleted secret","marks":[{"type":"trackedChange","attrs":{"kind":"delete","author":"Private reviewer"}}]},
            {"type":"text","text":" Added","marks":[{"type":"trackedChange","attrs":{"kind":"insert","author":"Private reviewer"}}]}
        ]});
        assert!(has_text_revisions(&content));
        strip_private_review(&mut content);
        assert_eq!(engine_core::richtext::plain_text(&content), "Visible Added");
        assert!(!content.to_string().contains("Private"));
        assert!(!has_text_revisions(&content));
        assert_eq!(content["content"][0]["marks"][0]["type"], "bold");
    }
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    spaces,
    create_space,
    space_members,
    set_space_member,
    index,
    document_access,
    move_document,
    trash_document,
    public_links,
    publish,
    revoke,
    public_document
))]
pub struct ProductApi;
