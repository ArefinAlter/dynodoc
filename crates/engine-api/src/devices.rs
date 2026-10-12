//! Account-bound signing keys. Enrollment is not a project grant or publication.
use crate::{
    auth::{random_token, sha256, AuthContext},
    error::ApiError,
    AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    routing::{get, post},
    Json, Router,
};
use engine_core::repository::{
    device::{validate_public_key, Challenge, Proof, MAX_PROOF_BYTES},
    SignedCommit,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const MAX_ACTIVE: i64 = 20;
const MAX_TOTAL: i64 = 128;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/devices", get(list).post(register))
        .route("/devices/challenge", post(challenge))
        .route("/devices/:id/revoke", post(revoke))
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

#[utoipa::path(post, path="/devices", tag="devices", request_body=Value, security(("paseto"=[])), responses((status=200, description="Enrolled device or exact active retry", body=Value),(status=400, description="Invalid proof"),(status=401, description="Sign in required"),(status=403, description="Wrong account"),(status=409, description="Challenge expired, used, key revoked or limit reached")))]
async fn register(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(proof): Json<Proof>,
) -> Result<Json<Value>, ApiError> {
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
    let existing: Option<Value> = sqlx::query_scalar("select k.proof from device_signing_key k where k.id=$1 and k.identity_id=$2 and not exists(select 1 from device_key_revocation r where r.key_id=k.id)")
        .bind(proof.challenge.id).bind(auth.identity_id).fetch_optional(&mut *tx).await?;
    if existing.as_ref() == Some(&encoded) {
        tx.commit().await?;
        return Ok(Json(
            json!({"id":proof.challenge.id,"public_key":proof.challenge.public_key}),
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
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "device.enrolled",
        proof.challenge.id,
        json!({"public_key":proof.challenge.public_key}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":proof.challenge.id,"public_key":proof.challenge.public_key}),
    ))
}

#[utoipa::path(get, path="/devices", tag="devices", security(("paseto"=[])), responses((status=200, description="Own public signing keys, at most 128; never a directory of other members", body=Value),(status=401, description="Sign in required")))]
async fn list(State(state): State<AppState>, auth: AuthContext) -> Result<Json<Value>, ApiError> {
    let mut tx = state.pool.begin().await?;
    account_lock(&mut tx, &auth).await?;
    let authority: Uuid = sqlx::query_scalar("select id from device_authority where singleton")
        .fetch_one(&mut *tx)
        .await?;
    let keys: Vec<Value> = sqlx::query_scalar("select jsonb_build_object('id',k.id,'public_key',k.public_key,'registered_at',k.registered_at,'revoked_at',r.revoked_at) from device_signing_key k left join device_key_revocation r on r.key_id=k.id where k.identity_id=$1 order by k.registered_at desc,k.id limit 128")
        .bind(auth.identity_id).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"authority":authority,"identity":auth.identity_id,"keys":keys,"active_limit":MAX_ACTIVE,"total_limit":MAX_TOTAL}),
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
#[openapi(paths(challenge, register, list, revoke, verify_commit))]
pub struct DevicesApi;
