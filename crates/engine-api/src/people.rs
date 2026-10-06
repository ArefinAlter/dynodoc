//! Who can do what with a document: the owner, members with their roles (direct or
//! through a workspace), review rules, and ownership transfer. See [`crate::access`].
use crate::{
    access::{self, MemberRole},
    auth::AuthContext,
    error::ApiError,
    notifications, AppState,
};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents/:id/people", get(people))
        .route("/documents/:id/policy", post(set_policy))
        .route("/documents/:id/policy/inherit", post(inherit_policy))
        .route("/documents/:id/owner", post(transfer))
}

fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}

/// The owner, everyone with access and how, the review rules, and what the caller
/// may do. Any member can see this, as in a shared Google Doc.
#[utoipa::path(get, path="/documents/{id}/people", tag="people", params(("id" = Uuid, Path, description = "Document identifier")), security(("paseto" = [])), responses((status=200, description="Owner, members, review rules and the caller's capabilities", body=Value),(status=403, description="Document membership required")))]
async fn people(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut db = state.pool.acquire().await?;
    let role = access::require_member(&mut db, id, auth.identity_id).await?;
    let (rules, policy_source) = access::policy_with_source(&mut db, id).await?;
    let owner: Value = sqlx::query_scalar("select jsonb_build_object('id',i.id,'email',case when i.erased_at is null then i.email end,'name',i.display_name) from document d join identity i on i.id=d.created_by where d.id=$1")
        .bind(id)
        .fetch_one(&mut *db)
        .await?;
    // Direct grants, then people who reach the document through its workspace.
    let members: Vec<Value> = sqlx::query_scalar(
        "with direct as (
           select i.id,i.email,i.display_name,document_member_role($1,i.id) as role,'direct' as via,null::text as space_name
           from document_access a join identity i on i.id=a.identity_id and i.disabled_at is null
           where a.document_id=$1),
         spaced as (
           select distinct on (i.id) i.id,i.email,i.display_name,document_member_role($1,i.id) as role,'workspace' as via,s.name as space_name
           from document d
           join lateral (with recursive up as (select id,parent_id,name from access_space where id=d.space_id
                 union select p.id,p.parent_id,p.name from access_space p join up on up.parent_id=p.id) select * from up) s on true
           join space_member m on m.space_id=s.id join identity i on i.id=m.identity_id and i.disabled_at is null
           where d.id=$1 and not exists(select 1 from direct where direct.id=i.id)
           order by i.id,member_role_order(m.role) desc)
         select jsonb_build_object('id',id,'email',email,'name',display_name,'role',role,'via',via,'space',space_name)
         from (select * from direct union all select * from spaced) people
         where role is not null order by member_role_order(case role when 'contributor' then 'reviewer' when 'reviewer' then 'approver' else role end) desc, email",
    )
    .bind(id)
    .fetch_all(&mut *db)
    .await?;
    Ok(Json(json!({
        "owner": owner,
        "you": {"id": auth.identity_id, "role": role.as_str(), "can": access::capabilities(role, &rules)},
        "members": members,
        "policy": rules,
        "policy_source": policy_source,
    })))
}

#[derive(Deserialize)]
struct PolicyRequest {
    protect_team_version: bool,
    required_approvals: i16,
    merge_roles: String,
}
/// Change the review rules. The owner and managers only.
#[utoipa::path(post, path="/documents/{id}/policy", tag="people", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier")), security(("paseto" = [])), responses((status=200, description="Saved review rules", body=Value),(status=400, description="Invalid rules"),(status=403, description="The owner or a manager")))]
async fn set_policy(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<PolicyRequest>,
) -> Result<Json<Value>, ApiError> {
    if !(0..=5).contains(&req.required_approvals)
        || !["editors", "owners"].contains(&req.merge_roles.as_str())
    {
        return Err(bad(
            "Require 0–5 approvals and choose who merges: editors or owners",
        ));
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query("select id from document where id=$1 for update")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !access::require_member(&mut tx, id, auth.identity_id)
        .await?
        .can_manage()
    {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("insert into document_policy(document_id,protect_team_version,required_approvals,merge_roles,updated_by,updated_at) values($1,$2,$3,$4,$5,now()) on conflict(document_id) do update set protect_team_version=excluded.protect_team_version,required_approvals=excluded.required_approvals,merge_roles=excluded.merge_roles,updated_by=excluded.updated_by,updated_at=now()")
        .bind(id)
        .bind(req.protect_team_version)
        .bind(req.required_approvals)
        .bind(&req.merge_roles)
        .bind(auth.identity_id)
        .execute(&mut *tx)
        .await?;
    let rules = access::policy(&mut tx, id).await?;
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "document.review_rules_changed",
        id,
        json!(rules),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!(rules)))
}

#[utoipa::path(post,path="/documents/{id}/policy/inherit",params(("id"=Uuid,Path)),security(("paseto"=[])),responses((status=200,description="File uses project or default rules again",body=Value)))]
async fn inherit_policy(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut tx = s.pool.begin().await?;
    sqlx::query("select id from document where id=$1 for update")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !access::require_member(&mut tx, id, a.identity_id)
        .await?
        .can_manage()
    {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("delete from document_policy where document_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let (rules, source) = access::policy_with_source(&mut tx, id).await?;
    crate::product::audit(
        &mut tx,
        a.identity_id,
        "document.review_rules_inherited",
        id,
        json!({"source":source}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"policy":rules,"policy_source":source})))
}

#[derive(Deserialize)]
struct TransferRequest {
    email: String,
}
/// Make another person the owner. The previous owner stays an editor.
#[utoipa::path(post, path="/documents/{id}/owner", tag="people", request_body=Value, params(("id" = Uuid, Path, description = "Document identifier")), security(("paseto" = [])), responses((status=200, description="Ownership transferred", body=Value),(status=400, description="Unknown or inactive account"),(status=403, description="The owner or a manager")))]
async fn transfer(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<TransferRequest>,
) -> Result<Json<Value>, ApiError> {
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 254 {
        return Err(bad("Enter a valid email address"));
    }
    let mut tx = state.pool.begin().await?;
    let previous: Uuid =
        sqlx::query_scalar("select created_by from document where id=$1 for update")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(ApiError::NotFound)?;
    if !access::require_member(&mut tx, id, auth.identity_id)
        .await?
        .can_manage()
    {
        return Err(ApiError::Forbidden);
    }
    let recipient: Uuid = sqlx::query_scalar("select id from identity where lower(email)=$1")
        .bind(&email)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| bad("Share the document with this person before making them the owner"))?;
    let active: bool = sqlx::query_scalar(
        "select disabled_at is null and erased_at is null from identity where id=$1",
    )
    .bind(recipient)
    .fetch_one(&mut *tx)
    .await?;
    if !active || access::member_role(&mut tx, id, recipient).await?.is_none() {
        return Err(bad(
            "Share the document with this person before making them the owner",
        ));
    }
    if recipient == previous {
        return Ok(Json(json!({"owner":recipient})));
    }
    sqlx::query("update document set created_by=$2,updated_at=now() where id=$1")
        .bind(id)
        .bind(recipient)
        .execute(&mut *tx)
        .await?;
    for person in [recipient, previous] {
        sqlx::query("insert into document_access(document_id,identity_id,role) values($1,$2,'author') on conflict(document_id,identity_id) do update set role='author'")
            .bind(id)
            .bind(person)
            .execute(&mut *tx)
            .await?;
    }
    notifications::notify(
        &mut tx,
        recipient,
        "ownership_transferred",
        Some(id),
        Some(auth.identity_id),
        None,
        json!({"previous_owner":previous}),
    )
    .await?;
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "document.ownership_transferred",
        id,
        json!({"from":previous,"to":recipient}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"owner":recipient,"previous_owner_role":MemberRole::Editor.as_str()}),
    ))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(people, set_policy, inherit_policy, transfer))]
pub struct PeopleApi;
