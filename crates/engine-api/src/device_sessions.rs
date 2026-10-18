//! Project-scoped device sessions: a signed, one-use challenge answered with an
//! enrolled device key buys a one-hour token for one project and a few scopes.
//! See docs/PROJECT-SCOPED-DEVICE-SESSIONS.md. Nothing renewable is stored; the
//! `device_session` row is a replay ledger and issuance window, and project access
//! is read from the live membership tables again on every request. A key that is
//! not in `all` mode additionally needs a per-project grant from its account
//! holder (docs/DEVICE-KEY-GRANTS-API.md), checked at issuance and on every request.
use crate::{
    auth::{
        device::key_reaches,
        random_token,
        token::{self, DeviceClaims, DEVICE_SESSION_TTL_SECONDS},
    },
    error::ApiError,
    product::audit,
    projects, AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::header,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use engine_core::repository::{
    device::validate_public_key,
    device_session::{
        is_agent_scope, normalize_scopes, SessionChallenge, SessionProof, CHALLENGE_TTL_MS,
        MAX_SESSION_BYTES,
    },
};
use ring::hmac;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

/// Persistent per-key issuance budget; the permanent record is the audit row.
const HOURLY: i64 = 30;
const DAILY: i64 = 240;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{id}/device-sessions/challenge", post(challenge))
        .route("/projects/{id}/device-sessions", post(issue))
        .layer(DefaultBodyLimit::max(MAX_SESSION_BYTES * 2))
}

/// Every pre-database refusal is the same response: the caller learns nothing
/// about keys, accounts or projects from the shape of a failure.
fn refused() -> ApiError {
    ApiError::Unauthorized
}

/// A key only this service can compute, domain separated from Paseto's own use.
fn mac_key(state: &AppState) -> hmac::Key {
    let root = hmac::Key::new(hmac::HMAC_SHA256, &state.auth.paseto_key);
    let derived = hmac::sign(&root, b"dynodoc.device-session.challenge-mac.v1");
    hmac::Key::new(hmac::HMAC_SHA256, derived.as_ref())
}
fn challenge_mac(state: &AppState, challenge: &SessionChallenge) -> Result<String, ApiError> {
    let mut input = b"dynodoc.device-session.mac\0v1\0".to_vec();
    input.extend(
        challenge
            .canonical()
            .map_err(|e| ApiError::Internal(e.to_string()))?,
    );
    Ok(hex::encode(hmac::sign(&mac_key(state), &input).as_ref()))
}
fn verify_mac(state: &AppState, proof: &SessionProof) -> Result<(), ApiError> {
    let tag = hex::decode(&proof.mac).map_err(|_| refused())?;
    let mut input = b"dynodoc.device-session.mac\0v1\0".to_vec();
    input.extend(proof.challenge.canonical().map_err(|_| refused())?);
    // ring compares in constant time.
    hmac::verify(&mac_key(state), &input, &tag).map_err(|_| refused())
}

/// Clock disagreement tolerated between the instance that issued a challenge and
/// the one that answers it. Challenges are valid for two minutes.
const SKEW_MS: i64 = 5_000;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// The installation ID is immutable, so one read per process is enough and an
/// unauthenticated challenge request needs no database work at all after it.
async fn authority(state: &AppState) -> Result<Uuid, ApiError> {
    state
        .device_authority
        .get_or_try_init(|| async {
            sqlx::query_scalar::<_, Uuid>("select id from device_authority where singleton")
                .fetch_one(&state.pool)
                .await
        })
        .await
        .copied()
        .map_err(ApiError::from)
}

/// Answerable now, by this installation: not expired and not from the future.
fn current(challenge: &SessionChallenge, authority: Uuid, now_ms: i64) -> bool {
    challenge.authority == authority
        && now_ms < challenge.expires_at_ms
        && challenge.issued_at_ms <= now_ms + SKEW_MS
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct ChallengeRequest {
    /// Lowercase hexadecimal Ed25519 public key of an enrolled device key.
    public_key: String,
    /// One or more of `history:read`, `history:review`, `history:write`,
    /// `project:read` (a person's key) or `content:read` (a key enrolled for an AI
    /// agent; the two sets never mix). A key that is not in `all` mode also needs an
    /// active grant for this project that lists every scope, and an agent scope always
    /// does.
    scopes: Vec<String>,
}

/// Documentation shapes. The wire types are the engine-core session proof.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct ChallengeResponse {
    /// `{version,id,authority,public_key,project,scopes,nonce,issued_at_ms,expires_at_ms,token_ttl_seconds}`
    #[schema(value_type = Object)]
    challenge: Value,
    /// Service MAC over the challenge; echo it unchanged.
    mac: String,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct SessionAnswer {
    /// The challenge exactly as issued.
    #[schema(value_type = Object)]
    challenge: Value,
    mac: String,
    /// Ed25519 signature (128 hex characters) over `dynodoc.device-session\0v1\0`
    /// followed by the canonical JSON of the challenge.
    signature: String,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct DeviceToken {
    access_token: String,
    token_type: String,
    expires_in: i64,
    expires_at: chrono::DateTime<chrono::Utc>,
    project: Uuid,
    scopes: Vec<String>,
}

#[utoipa::path(operation_id="device_session_challenge", post, path="/projects/{id}/device-sessions/challenge", tag="device sessions", params(("id"=Uuid,Path,description="Project")), request_body=ChallengeRequest, responses((status=200, description="Stateless two-minute challenge with the service MAC. Written nowhere; identical for enrolled, unknown and revoked keys and for any project ID", body=ChallengeResponse),(status=400, description="Invalid key or scopes")))]
async fn challenge(
    State(state): State<AppState>,
    Path(project): Path<Uuid>,
    Json(input): Json<ChallengeRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_public_key(&input.public_key).map_err(|_| ApiError::BadRequest {
        reason: "Use a lowercase hexadecimal Ed25519 public key".into(),
    })?;
    let scopes = normalize_scopes(&input.scopes).map_err(|_| ApiError::BadRequest {
        reason: "Choose one or more of history:read, history:review, history:write, project:read, or content:read for an agent key"
            .into(),
    })?;
    let now_ms = now_ms();
    let challenge = SessionChallenge {
        version: 1,
        id: Uuid::new_v4(),
        authority: authority(&state).await?,
        public_key: input.public_key,
        project,
        scopes,
        nonce: random_token()?.0,
        issued_at_ms: now_ms,
        expires_at_ms: now_ms + CHALLENGE_TTL_MS,
        token_ttl_seconds: DEVICE_SESSION_TTL_SECONDS,
    };
    let mac = challenge_mac(&state, &challenge)?;
    Ok(Json(json!({"challenge":challenge,"mac":mac})))
}

#[utoipa::path(operation_id="device_session_issue", post, path="/projects/{id}/device-sessions", tag="device sessions", params(("id"=Uuid,Path,description="Project")), request_body=SessionAnswer, responses((status=200, description="One-hour bearer token for this project and the signed scopes; renew by signing a new challenge. Never cacheable", body=DeviceToken),(status=401, description="Challenge, signature, key or account is not current (uniform)"),(status=403, description="Current project role, or the key's per-project grant, does not allow the scopes (uniform; the reason is not disclosed)"),(status=429, description="Issuance budget reached")))]
async fn issue(
    State(state): State<AppState>,
    Path(project): Path<Uuid>,
    Json(proof): Json<SessionProof>,
) -> Result<impl IntoResponse, ApiError> {
    // Cheap cryptographic checks first: an unauthenticated caller can reach no
    // database work without a service MAC and a valid device signature.
    let c = &proof.challenge;
    c.validate().map_err(|_| refused())?;
    if c.project != project {
        return Err(refused());
    }
    verify_mac(&state, &proof)?;
    proof.verify_signature().map_err(|_| refused())?;
    if !current(c, authority(&state).await?, now_ms()) {
        return Err(refused());
    }
    // One pooled read settles the refusals that must cost nothing: an unknown or
    // revoked key and an already redeemed challenge (a captured proof replayed for
    // two minutes) spend no transaction, lock or admission budget. The ledger
    // insert below remains the authority for one use.
    let known: Option<(Uuid, Uuid, bool, bool)> = sqlx::query_as("select k.id,k.identity_id,exists(select 1 from device_key_revocation r where r.key_id=k.id),exists(select 1 from device_session s where s.id=$2) from device_signing_key k where k.public_key=$1")
        .bind(&c.public_key)
        .bind(c.id)
        .fetch_optional(&state.pool)
        .await?;
    let Some((key_id, identity, revoked, redeemed)) = known else {
        return Err(refused());
    };
    if revoked || redeemed {
        return Err(refused());
    }
    // Admission is keyed by the key's account, reached only with a valid signature
    // of an enrolled, unrevoked key and an unused challenge: invented projects or
    // replays cannot fill the registry or burn the account's budget.
    let _permit = state.workloads.enter(identity, None)?;

    let mut tx = state.pool.begin().await?;
    sqlx::query("set local statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("set local lock_timeout='3s'")
        .execute(&mut *tx)
        .await?;
    // SHARE serializes with revocation, logout, disable and erasure (they take
    // the row FOR UPDATE) but not with other sessions of the same account.
    let generation: Option<i64> = sqlx::query_scalar("select session_generation from identity where id=$1 and disabled_at is null and erased_at is null for share")
        .bind(identity).fetch_optional(&mut *tx).await?;
    let revoked: bool =
        sqlx::query_scalar("select exists(select 1 from device_key_revocation where key_id=$1)")
            .bind(key_id)
            .fetch_one(&mut *tx)
            .await?;
    let Some(generation) = generation.filter(|_| !revoked) else {
        return Err(refused());
    };
    // The role is only checked here to avoid issuing a useless token. Every
    // request re-reads it. All refusals are the same 403: the key holder is not
    // told whether the role, the review rank or the key's grant was missing.
    let role = projects::require(&mut tx, project, identity).await?;
    if c.scopes.iter().any(|s| s == "history:write")
        && !matches!(
            role.as_str(),
            "owner" | "manager" | "editor" | "reviewer" | "approver"
        )
    {
        return Err(ApiError::Forbidden);
    }
    if c.scopes.iter().any(|s| s == "history:review")
        && !matches!(role.as_str(), "owner" | "manager" | "editor" | "approver")
    {
        return Err(ApiError::Forbidden);
    }
    // The account holder's per-project grant (or the key's `all` mode). The
    // account row is held FOR SHARE, so a concurrent grant change (FOR UPDATE)
    // waits for this issuance and applies to the token's next request.
    let (reaches, agent_key): (bool, bool) = sqlx::query_as(&format!(
        "select {},exists(select 1 from device_key_class c where c.key_id=k.id and c.class='agent') from device_signing_key k where k.id=$1",
        key_reaches("$2", "$3::text[]")
    ))
    .bind(key_id)
    .bind(project)
    .bind(&c.scopes)
    .fetch_one(&mut *tx)
    .await?;
    // An agent key obtains agent scopes only, and only an agent key obtains them: the
    // same uniform refusal as a missing grant.
    let agent_scopes = c.scopes.iter().any(|s| is_agent_scope(s));
    let other_scopes = c.scopes.iter().any(|s| !is_agent_scope(s));
    if !reaches || (agent_key && other_scopes) || (!agent_key && agent_scopes) {
        return Err(ApiError::Forbidden);
    }
    // Serialize this key's issuance so the budget is exact under concurrency.
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text,381))")
        .bind(key_id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from device_session where key_id=$1 and issued_at<now()-interval '1 day'")
        .bind(key_id)
        .execute(&mut *tx)
        .await?;
    let (hour, day): (i64, i64) = sqlx::query_as("select count(*) filter(where issued_at>now()-interval '1 hour'),count(*) from device_session where key_id=$1")
        .bind(key_id).fetch_one(&mut *tx).await?;
    if hour >= HOURLY || day >= DAILY {
        // Nothing to roll back; commit so the (idempotent) cleanup is kept.
        tx.commit().await?;
        return Err(ApiError::RateLimited { retry_after: 300 });
    }
    // One use: the challenge ID is the primary key.
    let inserted = sqlx::query("insert into device_session(id,key_id,identity_id,project_id,scopes,expires_at) values($1,$2,$3,$4,$5,now()+make_interval(secs=>$6)) on conflict do nothing")
        .bind(c.id).bind(key_id).bind(identity).bind(project).bind(&c.scopes).bind(DEVICE_SESSION_TTL_SECONDS as f64)
        .execute(&mut *tx).await?;
    if inserted.rows_affected() != 1 {
        return Err(refused());
    }
    let expires_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("select expires_at from device_session where id=$1")
            .bind(c.id)
            .fetch_one(&mut *tx)
            .await?;
    // `key_id`, not `key`: the administrator audit view redacts a field named key.
    audit(
        &mut tx,
        identity,
        "device.session_issued",
        project,
        json!({"session":c.id,"key_id":key_id,"scopes":c.scopes,"expires_at":expires_at}),
    )
    .await?;
    let access_token = token::mint_project_device(
        &state.auth.paseto_key,
        &DeviceClaims {
            identity,
            key: key_id,
            project,
            session: c.id,
            scopes: c.scopes.clone(),
            generation,
        },
        DEVICE_SESSION_TTL_SECONDS,
    )?;
    tx.commit().await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "access_token":access_token,
            "token_type":"Bearer",
            "expires_in":DEVICE_SESSION_TTL_SECONDS,
            "expires_at":expires_at,
            "project":project,
            "scopes":c.scopes,
        })),
    ))
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(challenge, issue),
    components(schemas(ChallengeRequest, ChallengeResponse, SessionAnswer, DeviceToken))
)]
pub struct DeviceSessionsApi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_is_current_only_before_expiry_for_its_own_installation() {
        let authority = Uuid::from_u128(1);
        let challenge = SessionChallenge {
            version: 1,
            id: Uuid::from_u128(2),
            authority,
            public_key: "00".repeat(32),
            project: Uuid::from_u128(3),
            scopes: vec!["history:read".into()],
            nonce: "00".repeat(32),
            issued_at_ms: 1_000_000,
            expires_at_ms: 1_000_000 + CHALLENGE_TTL_MS,
            token_ttl_seconds: DEVICE_SESSION_TTL_SECONDS,
        };
        assert!(current(&challenge, authority, 1_000_000));
        assert!(current(&challenge, authority, challenge.expires_at_ms - 1));
        assert!(!current(&challenge, authority, challenge.expires_at_ms));
        assert!(!current(
            &challenge,
            authority,
            challenge.expires_at_ms + 60_000
        ));
        assert!(!current(&challenge, Uuid::from_u128(9), 1_000_000));
        // Small clock disagreement between instances is tolerated; a lot is not.
        assert!(current(&challenge, authority, 1_000_000 - SKEW_MS));
        assert!(!current(&challenge, authority, 1_000_000 - SKEW_MS - 1));
    }
}
