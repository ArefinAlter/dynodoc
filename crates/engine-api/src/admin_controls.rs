//! Explicit administrator operations. No arbitrary SQL, shell or trigger bypass.
use crate::{
    admin::require_admin,
    auth::{sha256, AuthContext},
    error::ApiError,
    AppState,
};
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/impact", get(impact))
        .route("/admin/erase", post(erase))
        .route("/admin/control", post(control))
}
fn bad(message: &str) -> ApiError {
    ApiError::BadRequest {
        reason: message.into(),
    }
}

#[derive(Deserialize)]
struct Target {
    kind: String,
    id: Uuid,
}

async fn plan(
    db: &mut PgConnection,
    target: &Target,
    admin_emails: &str,
    actor: Uuid,
) -> Result<Value, ApiError> {
    let mut blockers = Vec::<String>::new();
    let (label, counts) = match target.kind.as_str() {
        "documents" => {
            let row = sqlx::query(
                "select title, deleted_at is not null as removed from document where id=$1",
            )
            .bind(target.id)
            .fetch_optional(&mut *db)
            .await?
            .ok_or(ApiError::NotFound)?;
            if !row.get::<bool, _>("removed") {
                blockers.push("Move this document to Trash first.".into());
            }
            let counts: Value = sqlx::query_scalar("select jsonb_build_object('events',(select count(*) from event where document_id=$1),'last_event',(select coalesce(max(seq),0) from event where document_id=$1),'blocks',(select count(*) from node where document_id=$1),'versions',(select count(*) from document_version where document_id=$1),'snapshots',(select count(*) from snapshot where document_id=$1),'drafts',(select count(*) from workspace_draft where document_id=$1),'draft_edits',(select count(*) from draft_edit e join workspace_draft d on d.id=e.draft_id where d.document_id=$1),'files',(select count(*) from document_upload where document_id=$1),'file_bytes',(select coalesce(sum(octet_length(content)),0) from document_upload where document_id=$1),'public_links',(select count(*) from public_document_view where document_id=$1))")
                .bind(target.id).fetch_one(&mut *db).await?;
            (row.get::<String, _>("title"), counts)
        }
        "users" => {
            let row = sqlx::query("select email, disabled_at is not null as removed, erased_at is not null as erased from identity where id=$1")
                .bind(target.id).fetch_optional(&mut *db).await?.ok_or(ApiError::NotFound)?;
            let email: String = row.get("email");
            if row.get::<bool, _>("erased") {
                return Err(bad("This account has already been permanently removed."));
            }
            if target.id == actor || crate::admin::is_admin(&email, admin_emails) {
                blockers.push(
                    "Configured administrators cannot be erased through the dashboard.".into(),
                );
            }
            if !row.get::<bool, _>("removed") {
                blockers.push("Disable the account first.".into());
            }
            let counts: Value = sqlx::query_scalar("select jsonb_build_object('owned_documents',(select count(*) from document where created_by=$1),'owned_workspaces',(select count(*) from space_member where identity_id=$1 and role='owner'),'sessions',(select count(*) from refresh_token where identity_id=$1),'contributions_retained',(select count(*) from event where actor_id=$1),'drafts',(select count(*) from workspace_draft where created_by=$1))")
                .bind(target.id).fetch_one(&mut *db).await?;
            if counts["owned_documents"].as_i64().unwrap_or(0) > 0 {
                blockers.push("Transfer or permanently delete owned documents first, including documents in Trash.".into());
            }
            if counts["owned_workspaces"].as_i64().unwrap_or(0) > 0 {
                blockers.push("Transfer workspace ownership first.".into());
            }
            (email, counts)
        }
        _ => return Err(bad("Choose users or documents.")),
    };
    let mut result = json!({"kind":target.kind,"id":target.id,"label":label,"counts":counts,"blockers":blockers});
    let revision = hex::encode(sha256(result.to_string().as_bytes()));
    result["revision"] = json!(revision);
    Ok(result)
}

#[utoipa::path(get, path="/admin/impact", tag="administration", security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn impact(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(target): Query<Target>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &auth).await?;
    let mut db = state.pool.acquire().await?;
    Ok(Json(
        plan(&mut db, &target, &state.admin_emails, auth.identity_id).await?,
    ))
}

#[derive(Deserialize)]
struct Erase {
    kind: String,
    id: Uuid,
    confirmation: String,
    revision: String,
    reason: String,
    acknowledge_backups: bool,
}
#[utoipa::path(post, path="/admin/erase", tag="administration", request_body=Value, security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn erase(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(input): Json<Erase>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &auth).await?;
    if !input.acknowledge_backups
        || !["owner_request", "duplicate", "policy", "other"].contains(&input.reason.as_str())
    {
        return Err(bad(
            "Choose a reason and acknowledge the backup retention policy.",
        ));
    }
    let target = Target {
        kind: input.kind,
        id: input.id,
    };
    let mut tx = state.pool.begin().await?;
    let lock = match target.kind.as_str() {
        "documents" => "select id from document where id=$1 for update",
        "users" => "select id from identity where id=$1 for update",
        _ => return Err(bad("Choose users or documents.")),
    };
    sqlx::query(lock)
        .bind(target.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    if target.kind == "documents" {
        sqlx::query("select id from workspace_draft where document_id=$1 order by id for update")
            .bind(target.id)
            .fetch_all(&mut *tx)
            .await?;
    }
    let current = plan(&mut tx, &target, &state.admin_emails, auth.identity_id).await?;
    if !current["blockers"].as_array().is_some_and(Vec::is_empty) {
        return Err(bad(
            "Resolve the listed dependencies before permanently deleting this resource.",
        ));
    }
    if current["revision"] != input.revision || current["label"] != input.confirmation {
        return Err(bad(
            "The resource changed or confirmation does not match. Review a fresh deletion preview.",
        ));
    }
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,$2,$3,$4,$5)")
        .bind(target.id).bind(&target.kind).bind(auth.identity_id).bind(&input.reason).bind(&current["counts"]).execute(&mut *tx).await?;
    if target.kind == "documents" {
        sqlx::query("delete from document where id=$1")
            .bind(target.id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("delete from public_document_view where created_by=$1")
            .bind(target.id)
            .execute(&mut *tx)
            .await?;
        for table in [
            "refresh_token",
            "magic_link",
            "idempotency_key",
            "document_access",
            "space_member",
        ] {
            sqlx::query(&format!("delete from {table} where identity_id=$1"))
                .bind(target.id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("update identity set email=$2,display_name='Deleted account',erased_at=now(),disabled_at=now(),session_generation=session_generation+1 where id=$1")
            .bind(target.id).bind(format!("deleted-{}@removed.invalid",target.id)).execute(&mut *tx).await?;
    }
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        &format!("{}.erased", target.kind),
        target.id,
        json!({"reason":input.reason,"counts":current["counts"]}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true,"receipt_id":target.id})))
}

#[derive(Deserialize)]
struct Control {
    action: String,
    id: Uuid,
    confirmation: String,
    recipient: Option<Uuid>,
}
#[utoipa::path(post, path="/admin/control", tag="administration", request_body=Value, security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn control(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(input): Json<Control>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &auth).await?;
    let mut tx = state.pool.begin().await?;
    match input.action.as_str() {
        "revoke_sessions" => {
            let email: Option<String> = sqlx::query_scalar(
                "select email from identity where id=$1 and erased_at is null for update",
            )
            .bind(input.id)
            .fetch_optional(&mut *tx)
            .await?;
            if email.ok_or(ApiError::NotFound)? != input.confirmation {
                return Err(bad("Type the account email to confirm."));
            }
            sqlx::query("update identity set session_generation=session_generation+1 where id=$1")
                .bind(input.id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("delete from refresh_token where identity_id=$1")
                .bind(input.id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("delete from magic_link where identity_id=$1")
                .bind(input.id)
                .execute(&mut *tx)
                .await?;
        }
        "revoke_public_links" | "transfer_document" => {
            let title: Option<String> =
                sqlx::query_scalar("select title from document where id=$1 for update")
                    .bind(input.id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if title.ok_or(ApiError::NotFound)? != input.confirmation {
                return Err(bad("Type the document title to confirm."));
            }
            if input.action == "revoke_public_links" {
                sqlx::query("update public_document_view set revoked_at=now() where document_id=$1 and revoked_at is null").bind(input.id).execute(&mut *tx).await?;
            } else {
                let recipient = input
                    .recipient
                    .ok_or(bad("Choose an active recipient account."))?;
                sqlx::query("select id from identity where id=$1 and disabled_at is null and erased_at is null for share")
                    .bind(recipient).fetch_optional(&mut *tx).await?.ok_or(bad("Choose an active recipient account."))?;
                sqlx::query("update document set created_by=$2,updated_at=now() where id=$1")
                    .bind(input.id)
                    .bind(recipient)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("insert into document_access(document_id,identity_id,role) values($1,$2,'author') on conflict(document_id,identity_id) do update set role='author'")
                    .bind(input.id).bind(recipient).execute(&mut *tx).await?;
            }
        }
        "transfer_workspace" => {
            let name: Option<String> =
                sqlx::query_scalar("select name from access_space where id=$1 for update")
                    .bind(input.id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if name.ok_or(ApiError::NotFound)? != input.confirmation {
                return Err(bad("Type the workspace name to confirm."));
            }
            let recipient = input
                .recipient
                .ok_or(bad("Choose an active recipient account."))?;
            sqlx::query("select id from identity where id=$1 and disabled_at is null and erased_at is null for share")
                .bind(recipient).fetch_optional(&mut *tx).await?.ok_or(bad("Choose an active recipient account."))?;
            sqlx::query("update space_member set role='manager' where space_id=$1 and role='owner' and identity_id<>$2").bind(input.id).bind(recipient).execute(&mut *tx).await?;
            sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,'owner') on conflict(space_id,identity_id) do update set role='owner'")
                .bind(input.id).bind(recipient).execute(&mut *tx).await?;
        }
        _ => return Err(bad("Unknown administrator control.")),
    }
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        &format!("admin.{}", input.action),
        input.id,
        json!({"recipient":input.recipient}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(impact, erase, control))]
pub struct AdminControlsApi;
