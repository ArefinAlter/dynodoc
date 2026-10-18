//! Account-bound signing keys. Enrollment is not a project grant or publication.
use crate::{
    auth::{device::key_reaches, random_token, sha256, AuthContext},
    error::ApiError,
    projects, AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    routing::{get, patch, post, put},
    Json, Router,
};
use engine_core::repository::{
    device::{validate_public_key, Challenge, Proof, MAX_PROOF_BYTES},
    device_session::{normalize_scopes, SCOPES},
    SignedCommit,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const MAX_ACTIVE: i64 = 20;
const MAX_TOTAL: i64 = 128;
/// Active per-project grants one key may hold.
const MAX_GRANTS: i64 = 50;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/devices", get(list).post(register))
        .route("/devices/challenge", post(challenge))
        .route("/devices/{id}/revoke", post(revoke))
        .route("/devices/{id}", patch(set_access))
        .route(
            "/devices/{id}/projects/{project}",
            put(set_grant).delete(clear_grant),
        )
        .layer(DefaultBodyLimit::max(MAX_PROOF_BYTES))
        .merge(
            Router::new()
                .route("/devices/verify-commit", post(verify_commit))
                .layer(DefaultBodyLimit::max(
                    engine_core::repository::MAX_COMMIT_BYTES + 1024,
                )),
        )
}

fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}
fn conflict(reason: &str) -> ApiError {
    ApiError::Conflict {
        reason: reason.into(),
    }
}

/// Serialize with logout, account removal, enrollment and device revocation;
/// recheck the exact login after the lock, not only in the HTTP extractor.
pub(crate) async fn account_lock(
    tx: &mut Transaction<'_, Postgres>,
    auth: &AuthContext,
) -> Result<(), ApiError> {
    sqlx::query("set local statement_timeout='10s'")
        .execute(&mut **tx)
        .await?;
    sqlx::query("set local lock_timeout='3s'")
        .execute(&mut **tx)
        .await?;
    sqlx::query("select id from identity where id=$1 for update")
        .bind(auth.identity_id)
        .fetch_optional(&mut **tx)
        .await?;
    let active: bool = sqlx::query_scalar("select exists(select 1 from identity i where i.id=$1 and i.disabled_at is null and i.erased_at is null and i.session_generation=$2 and ($3::uuid is null or exists(select 1 from refresh_token r where r.identity_id=i.id and r.session_generation=$2 and r.session_id=$3 and r.rotated_at is null and r.revoked_at is null and r.expires_at>now())))")
        .bind(auth.identity_id).bind(auth.session_generation).bind(auth.session_id).fetch_one(&mut **tx).await?;
    if !active {
        return Err(ApiError::Unauthorized);
    }
    // A project device session is also only as good as its key and the key's
    // current grant, rechecked here so revocation and grant changes serialize with
    // this request through the same account row lock (management takes it too).
    if let Some(device) = &auth.device {
        let reaches: Option<bool> = sqlx::query_scalar(&format!(
            "select {} from device_signing_key k where k.id=$1 and k.identity_id=$2 and not exists(select 1 from device_key_revocation r where r.key_id=k.id)",
            key_reaches("$3", "$4::text[]")
        ))
        .bind(device.key_id)
        .bind(auth.identity_id)
        .bind(device.project)
        .bind(vec![device.scope])
        .fetch_optional(&mut **tx)
        .await?;
        match reaches {
            None => return Err(ApiError::Unauthorized),
            Some(false) => return Err(ApiError::Forbidden),
            Some(true) => {}
        }
    }
    Ok(())
}

/// The caller holds account_lock. Current account/session and project/file roles
/// remain separate checks; a valid old signature alone is never a write grant.
pub(crate) async fn current_key(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    key: &str,
) -> Result<Uuid, ApiError> {
    sqlx::query_scalar("select k.id from device_signing_key k where k.identity_id=$1 and k.public_key=$2 and not exists(select 1 from device_key_revocation r where r.key_id=k.id)")
        .bind(actor).bind(key).fetch_optional(&mut **tx).await?.ok_or(ApiError::Forbidden)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enroll {
    public_key: String,
}

#[utoipa::path(post, path="/devices/challenge", tag="devices", request_body=Value, security(("paseto"=[])), responses((status=200, description="Five-minute account and installation bound enrollment challenge", body=Value),(status=400, description="Invalid key"),(status=401, description="Sign in required"),(status=409, description="Key unavailable or device limit"),(status=429, description="Too many enrollment attempts")))]
async fn challenge(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(input): Json<Enroll>,
) -> Result<Json<Value>, ApiError> {
    validate_public_key(&input.public_key)
        .map_err(|_| bad("Use a lowercase hexadecimal Ed25519 public key"))?;
    let mut tx = state.pool.begin().await?;
    account_lock(&mut tx, &auth).await?;
    let window = sha256(format!("device-enrollment-v1:{}", auth.identity_id).as_bytes());
    let attempts: i32 = sqlx::query_scalar("insert into auth_request_window(key_hash) values($1) on conflict(key_hash) do update set attempts=case when auth_request_window.started_at < now()-interval '15 minutes' then 1 else least(auth_request_window.attempts+1,100) end, started_at=case when auth_request_window.started_at < now()-interval '15 minutes' then now() else auth_request_window.started_at end returning attempts")
        .bind(window).fetch_one(&mut *tx).await?;
    if attempts > 10 {
        tx.commit().await?;
        return Err(ApiError::RateLimited { retry_after: 900 });
    }
    sqlx::query(
        "delete from device_enrollment_challenge where identity_id=$1 and expires_at<=now()",
    )
    .bind(auth.identity_id)
    .execute(&mut *tx)
    .await?;
    let pending: i64 =
        sqlx::query_scalar("select count(*) from device_enrollment_challenge where identity_id=$1")
            .bind(auth.identity_id)
            .fetch_one(&mut *tx)
            .await?;
    let (total, active): (i64,i64) = sqlx::query_as("select count(*),count(*) filter(where not exists(select 1 from device_key_revocation r where r.key_id=k.id)) from device_signing_key k where identity_id=$1")
        .bind(auth.identity_id).fetch_one(&mut *tx).await?;
    let used: bool =
        sqlx::query_scalar("select exists(select 1 from device_signing_key where public_key=$1)")
            .bind(&input.public_key)
            .fetch_one(&mut *tx)
            .await?;
    let denied = if pending >= 5 {
        Some(ApiError::RateLimited { retry_after: 300 })
    } else if total >= MAX_TOTAL || active >= MAX_ACTIVE || used {
        Some(conflict(
            "Key cannot be enrolled; use a fresh key and check your device limit",
        ))
    } else {
        None
    };
    if let Some(error) = denied {
        tx.commit().await?;
        return Err(error);
    }
    let authority: Uuid = sqlx::query_scalar("select id from device_authority where singleton")
        .fetch_one(&mut *tx)
        .await?;
    let expires: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "select date_trunc('milliseconds',clock_timestamp())+interval '5 minutes'",
    )
    .fetch_one(&mut *tx)
    .await?;
    let value = Challenge {
        version: 1,
        id: Uuid::new_v4(),
        authority,
        identity: auth.identity_id,
        public_key: input.public_key,
        nonce: random_token()?.0,
        expires_at_ms: expires.timestamp_millis(),
    };
    let value_json = serde_json::to_value(&value).map_err(|e| ApiError::Internal(e.to_string()))?;
    sqlx::query("insert into device_enrollment_challenge(id,identity_id,expires_at,challenge) values($1,$2,$3,$4)")
        .bind(value.id).bind(auth.identity_id).bind(expires).bind(&value_json).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(value_json))
}

/// The signed enrollment proof plus the person's choice of project access. The
/// mode is not part of the signed bytes: the proof only binds the key to the account.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Register {
    challenge: Challenge,
    signature: String,
    /// `granted` (default) or `all`.
    project_access: Option<String>,
}

fn access_mode(value: &str) -> Result<&'static str, ApiError> {
    match value {
        "granted" => Ok("granted"),
        "all" => Ok("all"),
        _ => Err(bad("project_access must be granted or all")),
    }
}

#[utoipa::path(post, path="/devices", tag="devices", request_body=Value, security(("paseto"=[])), responses((status=200, description="Enrolled device or exact active retry. Body: the signed challenge and signature, plus optional project_access (granted by default; all is the account-wide mode). Returns id, public_key and the key's project_access", body=Value),(status=400, description="Invalid proof or project_access"),(status=401, description="Sign in required"),(status=403, description="Wrong account"),(status=409, description="Challenge expired, used, key revoked or limit reached")))]
async fn register(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(input): Json<Register>,
) -> Result<Json<Value>, ApiError> {
    let mode = access_mode(input.project_access.as_deref().unwrap_or("granted"))?;
    let proof = Proof {
        challenge: input.challenge,
        signature: input.signature,
    };
    let _permit = state.workloads.enter(auth.identity_id, None)?;
    proof
        .verify()
        .map_err(|_| bad("Invalid device enrollment proof"))?;
    if proof.challenge.identity != auth.identity_id {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.pool.begin().await?;
    account_lock(&mut tx, &auth).await?;
    let encoded = serde_json::to_value(&proof).map_err(|e| ApiError::Internal(e.to_string()))?;
    // A lost response may be retried, but cannot reactivate or rebind a key.
    let existing: Option<(Value, String)> = sqlx::query_as("select k.proof,coalesce((select a.project_access from device_key_access a where a.key_id=k.id),'granted') from device_signing_key k where k.id=$1 and k.identity_id=$2 and not exists(select 1 from device_key_revocation r where r.key_id=k.id)")
        .bind(proof.challenge.id).bind(auth.identity_id).fetch_optional(&mut *tx).await?;
    if let Some((_, current)) = existing.as_ref().filter(|(stored, _)| stored == &encoded) {
        // An exact retry never changes the mode the key already has.
        let current = current.clone();
        tx.commit().await?;
        return Ok(Json(
            json!({"id":proof.challenge.id,"public_key":proof.challenge.public_key,"project_access":current}),
        ));
    }
    let challenge: Option<Value> = sqlx::query_scalar("delete from device_enrollment_challenge where id=$1 and identity_id=$2 and expires_at>clock_timestamp() returning challenge")
        .bind(proof.challenge.id).bind(auth.identity_id).fetch_optional(&mut *tx).await?;
    if challenge != Some(json!(proof.challenge)) {
        return Err(conflict(
            "Challenge expired or unavailable; request a new challenge",
        ));
    }
    let (total, active): (i64,i64) = sqlx::query_as("select count(*),count(*) filter(where not exists(select 1 from device_key_revocation r where r.key_id=k.id)) from device_signing_key k where identity_id=$1")
        .bind(auth.identity_id).fetch_one(&mut *tx).await?;
    if total >= MAX_TOTAL || active >= MAX_ACTIVE {
        return Err(conflict("Device limit reached"));
    }
    let inserted = sqlx::query("insert into device_signing_key(id,identity_id,public_key,proof) values($1,$2,$3,$4) on conflict do nothing")
        .bind(proof.challenge.id).bind(auth.identity_id).bind(&proof.challenge.public_key).bind(encoded).execute(&mut *tx).await?;
    if inserted.rows_affected() != 1 {
        return Err(conflict("Key cannot be enrolled; use a fresh key"));
    }
    // Same transaction as the key, so a key never exists without an explicit mode.
    sqlx::query("insert into device_key_access(key_id,project_access) values($1,$2)")
        .bind(proof.challenge.id)
        .bind(mode)
        .execute(&mut *tx)
        .await?;
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "device.enrolled",
        proof.challenge.id,
        json!({"public_key":proof.challenge.public_key,"project_access":mode}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":proof.challenge.id,"public_key":proof.challenge.public_key,"project_access":mode}),
    ))
}

/// One key: binding, revocation, access mode and its active grants (a revoked key
/// lists none). `$1` is the account; the project name and role are only exposed
/// while the account is still a member.
const KEY_ROW: &str = "jsonb_build_object('id',k.id,'public_key',k.public_key,'registered_at',k.registered_at,'revoked_at',r.revoked_at,'project_access',coalesce(a.project_access,'granted'),'grants',case when r.key_id is not null then '[]'::jsonb else coalesce((select jsonb_agg(jsonb_build_object('project_id',x.project_id,'project_name',case when x.role is not null then x.name end,'role',x.role,'scopes',to_jsonb(x.scopes),'created_at',x.created_at) order by x.created_at desc,x.id) from (select g.id,g.project_id,g.scopes,g.created_at,s.name,inherited_space_role(g.project_id,$1) as role from device_key_project_grant g join access_space s on s.id=g.project_id where g.key_id=k.id and g.revoked_at is null order by g.created_at desc,g.id limit 50) x),'[]'::jsonb) end)";

#[utoipa::path(get, path="/devices", tag="devices", security(("paseto"=[])), responses((status=200, description="Own public signing keys, at most 128, each with its project_access mode (granted or all) and active per-project grants (at most 50, scopes, current role); never a directory of other members", body=Value),(status=401, description="Sign in required")))]
async fn list(State(state): State<AppState>, auth: AuthContext) -> Result<Json<Value>, ApiError> {
    let mut tx = state.pool.begin().await?;
    account_lock(&mut tx, &auth).await?;
    let authority: Uuid = sqlx::query_scalar("select id from device_authority where singleton")
        .fetch_one(&mut *tx)
        .await?;
    let keys: Vec<Value> = sqlx::query_scalar(&format!("select {KEY_ROW} from device_signing_key k left join device_key_revocation r on r.key_id=k.id left join device_key_access a on a.key_id=k.id where k.identity_id=$1 order by k.registered_at desc,k.id limit 128"))
        .bind(auth.identity_id).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"authority":authority,"identity":auth.identity_id,"keys":keys,"active_limit":MAX_ACTIVE,"total_limit":MAX_TOTAL,"grant_limit":MAX_GRANTS,"scopes":SCOPES}),
    ))
}

#[utoipa::path(post, path="/devices/{id}/revoke", tag="devices", params(("id"=Uuid,Path,description="Own device ID")), security(("paseto"=[])), responses((status=200, description="Permanently revoked, idempotent", body=Value),(status=401, description="Sign in required"),(status=404, description="No such own device")))]
async fn revoke(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = state.pool.begin().await?;
    account_lock(&mut tx, &auth).await?;
    let own: bool = sqlx::query_scalar(
        "select exists(select 1 from device_signing_key where id=$1 and identity_id=$2)",
    )
    .bind(id)
    .bind(auth.identity_id)
    .fetch_one(&mut *tx)
    .await?;
    if !own {
        return Err(ApiError::NotFound);
    }
    let inserted =
        sqlx::query("insert into device_key_revocation(key_id) values($1) on conflict do nothing")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    if inserted.rows_affected() == 1 {
        crate::product::audit(&mut tx, auth.identity_id, "device.revoked", id, json!({})).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"id":id,"revoked":true})))
}

/// The caller's own key: whether it is revoked, or 404 for any other id.
async fn own_key(
    tx: &mut Transaction<'_, Postgres>,
    identity: Uuid,
    key: Uuid,
) -> Result<bool, ApiError> {
    sqlx::query_scalar::<_, bool>("select exists(select 1 from device_key_revocation r where r.key_id=k.id) from device_signing_key k where k.id=$1 and k.identity_id=$2")
        .bind(key)
        .bind(identity)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(ApiError::NotFound)
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct GrantRequest {
    /// One or more of `project:read`, `history:read`, `history:write`, `history:review`.
    scopes: Vec<String>,
}

#[utoipa::path(operation_id="device_grant_set", put, path="/devices/{key_id}/projects/{project_id}", tag="devices", params(("key_id"=Uuid,Path,description="Own device key"),("project_id"=Uuid,Path,description="Project the account is a member of")), request_body=GrantRequest, security(("paseto"=[])), responses((status=200, description="The key's active grant for this project (idempotent); changed is false when the scopes were already these", body=Value),(status=400, description="Empty or unknown scopes"),(status=401, description="Account sign-in required; a device token is refused"),(status=403, description="Not a current member of the project"),(status=404, description="No such own key"),(status=409, description="Key revoked or 50 active grants reached")))]
async fn set_grant(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((key, project)): Path<(Uuid, Uuid)>,
    Json(input): Json<GrantRequest>,
) -> Result<Json<Value>, ApiError> {
    let scopes = normalize_scopes(&input.scopes).map_err(|_| {
        bad("Choose one or more of project:read, history:read, history:write, history:review")
    })?;
    let _permit = state.workloads.enter(auth.identity_id, None)?;
    let mut tx = state.pool.begin().await?;
    account_lock(&mut tx, &auth).await?;
    if own_key(&mut tx, auth.identity_id, key).await? {
        return Err(conflict("The key is revoked"));
    }
    // A grant is only meaningful for a project the account can reach today.
    projects::require(&mut tx, project, auth.identity_id).await?;
    let active: Option<(Uuid, Vec<String>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as("select id,scopes,created_at from device_key_project_grant where key_id=$1 and project_id=$2 and revoked_at is null for update")
        .bind(key).bind(project).fetch_optional(&mut *tx).await?;
    if let Some((_, current, created_at)) = &active {
        if current == &scopes {
            tx.commit().await?;
            return Ok(Json(
                json!({"key_id":key,"project_id":project,"scopes":scopes,"created_at":created_at,"changed":false}),
            ));
        }
    }
    if active.is_none() {
        let count: i64 = sqlx::query_scalar(
            "select count(*) from device_key_project_grant where key_id=$1 and revoked_at is null",
        )
        .bind(key)
        .fetch_one(&mut *tx)
        .await?;
        if count >= MAX_GRANTS {
            return Err(conflict("A key can have at most 50 project grants"));
        }
    }
    // Changing scopes is a revocation plus a new row: a grant row never changes.
    if let Some((id, _, _)) = &active {
        sqlx::query("update device_key_project_grant set revoked_at=clock_timestamp() where id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let created_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("insert into device_key_project_grant(key_id,project_id,scopes,created_at,created_by) values($1,$2,$3,clock_timestamp(),$4) returning created_at")
        .bind(key).bind(project).bind(&scopes).bind(auth.identity_id).fetch_one(&mut *tx).await?;
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "device.grant.set",
        key,
        json!({"key_id":key,"project_id":project,"scopes":scopes,"previous_scopes":active.as_ref().map(|(_, previous, _)| previous)}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"key_id":key,"project_id":project,"scopes":scopes,"created_at":created_at,"changed":true}),
    ))
}

#[utoipa::path(operation_id="device_grant_revoke", delete, path="/devices/{key_id}/projects/{project_id}", tag="devices", params(("key_id"=Uuid,Path,description="Own device key"),("project_id"=Uuid,Path,description="Project")), security(("paseto"=[])), responses((status=200, description="The active grant is revoked; applies to the key's next request. changed is false when there was none", body=Value),(status=401, description="Account sign-in required; a device token is refused"),(status=404, description="No such own key")))]
async fn clear_grant(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((key, project)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    let _permit = state.workloads.enter(auth.identity_id, None)?;
    let mut tx = state.pool.begin().await?;
    account_lock(&mut tx, &auth).await?;
    own_key(&mut tx, auth.identity_id, key).await?;
    let revoked: Option<Vec<String>> = sqlx::query_scalar("update device_key_project_grant set revoked_at=clock_timestamp() where key_id=$1 and project_id=$2 and revoked_at is null returning scopes")
        .bind(key).bind(project).fetch_optional(&mut *tx).await?;
    if let Some(scopes) = &revoked {
        crate::product::audit(
            &mut tx,
            auth.identity_id,
            "device.grant.revoked",
            key,
            json!({"key_id":key,"project_id":project,"scopes":scopes}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(
        json!({"key_id":key,"project_id":project,"revoked":true,"changed":revoked.is_some()}),
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct AccessRequest {
    /// `granted` (only projects with an active grant) or `all` (every project the
    /// account can access; requires `confirm_all_projects`).
    project_access: String,
    /// Must be `true` to choose `all`; ignored for `granted`.
    #[serde(default)]
    confirm_all_projects: bool,
}

#[utoipa::path(operation_id="device_access_set", patch, path="/devices/{key_id}", tag="devices", params(("key_id"=Uuid,Path,description="Own device key")), request_body=AccessRequest, security(("paseto"=[])), responses((status=200, description="The key's mode; changed is false when it already had this mode. Existing grants are kept", body=Value),(status=400, description="Unknown mode, or all without confirm_all_projects"),(status=401, description="Account sign-in required; a device token is refused"),(status=404, description="No such own key"),(status=409, description="Key revoked")))]
async fn set_access(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(key): Path<Uuid>,
    Json(input): Json<AccessRequest>,
) -> Result<Json<Value>, ApiError> {
    let mode = access_mode(&input.project_access)?;
    if mode == "all" && !input.confirm_all_projects {
        return Err(bad(
            "Confirm with confirm_all_projects: this key could then open every project the account can access",
        ));
    }
    let _permit = state.workloads.enter(auth.identity_id, None)?;
    let mut tx = state.pool.begin().await?;
    account_lock(&mut tx, &auth).await?;
    if own_key(&mut tx, auth.identity_id, key).await? {
        return Err(conflict("The key is revoked"));
    }
    let current: String = sqlx::query_scalar(
        "select coalesce((select project_access from device_key_access where key_id=$1),'granted')",
    )
    .bind(key)
    .fetch_one(&mut *tx)
    .await?;
    let changed = current != mode;
    if changed {
        sqlx::query("insert into device_key_access(key_id,project_access) values($1,$2) on conflict(key_id) do update set project_access=excluded.project_access,updated_at=clock_timestamp()")
            .bind(key).bind(mode).execute(&mut *tx).await?;
        crate::product::audit(
            &mut tx,
            auth.identity_id,
            "device.access.changed",
            key,
            json!({"key_id":key,"from":current,"to":mode}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(
        json!({"key_id":key,"project_access":mode,"changed":changed}),
    ))
}

#[utoipa::path(post, path="/devices/verify-commit", tag="devices", request_body=Value, security(("paseto"=[])), responses((status=200, description="Signature matches an active key of this account; does not publish, store or authorize project changes", body=Value),(status=400, description="Invalid signed commit"),(status=401, description="Sign in required"),(status=403, description="Key is not active for this account")))]
async fn verify_commit(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(commit): Json<SignedCommit>,
) -> Result<Json<Value>, ApiError> {
    let _permit = state.workloads.enter(auth.identity_id, None)?;
    let address = commit.verify().map_err(|_| bad("Invalid signed commit"))?;
    let mut tx = state.pool.begin().await?;
    account_lock(&mut tx, &auth).await?;
    let key = current_key(&mut tx, auth.identity_id, &commit.commit.author_key).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"address":address,"identity":auth.identity_id,"device":key,"published":false}),
    ))
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        challenge,
        register,
        list,
        revoke,
        set_grant,
        clear_grant,
        set_access,
        verify_commit
    ),
    components(schemas(GrantRequest, AccessRequest))
)]
pub struct DevicesApi;
