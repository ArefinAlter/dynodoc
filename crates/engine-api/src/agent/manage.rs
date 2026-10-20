//! The agent management plane (docs/AI-AGENT-CONNECTORS.md section 7.2): how a project
//! owner or manager decides whether AI agents may read the project, and watches what they
//! did. Account sessions only: a device, agent or connector token never decrypts here and
//! is `401`, so no agent can change its own policy (docs/AI-AGENT-SECURITY.md AG3).
//!
//! | Route | Who | What |
//! |---|---|---|
//! | `GET/PUT /projects/{id}/agent-policy` | project owner or manager | the project policy; `PUT` needs `If-Match`, requires `enabled` and keeps the saved value of any other field it leaves out |
//! | `POST /projects/{id}/agent-policy/kill` | project owner or manager | disable now and revoke every agent grant |
//! | `GET /projects/{id}/agent-usage` | project owner or manager | calls, bytes and denials by day and client claim |
//! | `PUT /documents/{id}/agent-policy` | the file's owner or a manager (only a project owner or manager may loosen it) | the file override |
//!
//! These routes are mounted whether or not `AGENT_GATEWAY` is on, so a project can be
//! prepared first; `gateway` in the policy view says whether this server serves agents.
//!
//! **Answers.** A non-member, an unknown project and a member below owner or manager get
//! one `403` (the rule of the key grants); a file the caller cannot open or that does not
//! exist is `404`, a file they can open but not manage is `403`. Bodies are JSON with
//! unknown fields refused; a malformed shape is the framework's `422`, a value problem
//! `400` and a stale `If-Match` `409`.
//!
//! **History.** The policy tables keep the current value; every effective change writes an
//! `operation_audit` row in the same transaction (`agent.policy.changed`,
//! `agent.file_policy.changed`, `agent.killswitch`, and one `device.grant.revoked` per
//! grant the kill switch ended) naming the actor and, for a policy, each changed field with
//! its value before and after. A request that changes nothing writes nothing.

use axum::{
    extract::{rejection::QueryRejection, DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use super::policy::{self, Policy, Stored, CALLS_PER_MINUTE, KINDS, READ_BYTES_PER_DAY, ROLES};
use crate::{auth::AuthContext, error::ApiError, product::audit, projects, AppState};

/// Policy bodies are a few hundred bytes.
const BODY_LIMIT: usize = 16 * 1024;
/// Most file overrides listed in a policy view.
const MAX_OVERRIDES: i64 = 100;
/// Widest usage range, in days, and most rows returned.
const MAX_USAGE_DAYS: i64 = 31;
/// Years the usage route accepts (inclusive): far wider than the ledger can hold, and well
/// inside what PostgreSQL's `date` and chrono's arithmetic both handle.
const USAGE_YEARS: (i32, i32) = (2000, 2200);
const MAX_USAGE_ROWS: usize = 500;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{id}/agent-policy",
            get(get_policy).put(put_policy),
        )
        .route("/projects/{id}/agent-policy/kill", post(kill))
        .route("/projects/{id}/agent-usage", get(usage))
        .route("/documents/{id}/agent-policy", put(put_file_policy))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}

/// A transaction with the lock and statement timeouts of the other management routes.
async fn begin(state: &AppState) -> Result<Transaction<'static, Postgres>, ApiError> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("set local statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("set local lock_timeout='3s'")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// Owner or manager of the project. A non-member, an unknown project and a lower role
/// all get the same `403`.
async fn require_manager(db: &mut PgConnection, project: Uuid, who: Uuid) -> Result<(), ApiError> {
    let role = projects::require(db, project, who).await?;
    if matches!(role.as_str(), "owner" | "manager") {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

/// Serialize with member and role changes of the project (they take the row `FOR UPDATE`),
/// so the role just checked still holds at commit.
async fn lock_project(db: &mut PgConnection, project: Uuid) -> Result<(), ApiError> {
    sqlx::query("select id from access_space where id=$1 and kind='project' for share")
        .bind(project)
        .fetch_optional(db)
        .await?;
    Ok(())
}

/// Charge a management request to a bucket of the caller's own that agent traffic cannot
/// spend, after the caller is known to manage the project. An agent read charges its
/// identity's bucket (`enter(identity, ..)`) and the project's; a compromised agent running
/// as the sole owner could therefore keep the owner's slots full and have the owner's kill
/// switch answered `429`. Management is instead charged to `enter(<derived id>, None)`
/// where the id is a hash of the person and a fixed label: a bucket in the same registry
/// (120 a minute, 2 at once) that no read ever names, one per person, so managers do not
/// contend with each other either. The process-wide limit on expensive requests is still
/// shared.
fn charge(state: &AppState, who: Uuid) -> Result<crate::workloads::ReadPermit, ApiError> {
    let hash = crate::auth::sha256(format!("dynodoc.agent-management:{who}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    state.workloads.enter(Uuid::from_bytes(bytes), None)
}

/// Drop the cached states of every document of `project`. The cache never decides access
/// (every request is authorised first), so this is hygiene for text that was just
/// withdrawn; if the lookup fails every cached state goes.
async fn purge_project(state: &AppState, project: Uuid) {
    let cached = state.agent_cache.documents();
    if cached.is_empty() {
        return;
    }
    let ours: Result<Vec<Uuid>, _> =
        sqlx::query_scalar("select id from unnest($1::uuid[]) id where document_project(id)=$2")
            .bind(&cached)
            .bind(project)
            .fetch_all(&state.pool)
            .await;
    for document in ours.unwrap_or(cached) {
        state.agent_cache.purge_document(document);
    }
}

// --- views --------------------------------------------------------------------------

/// A file whose policy is not `inherit`.
#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct FileOverride {
    document_id: Uuid,
    /// `none` or `read`.
    access: String,
    updated_at: DateTime<Utc>,
}

/// The project's agent policy as saved, or the defaults when none was ever saved.
#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PolicyView {
    project_id: Uuid,
    /// False until an owner or manager first saves a policy. A project with no saved
    /// policy has agents disabled.
    configured: bool,
    /// Master switch. Default false.
    enabled: bool,
    /// Lowest project role whose agents may read: `viewer`, `contributor`, `reviewer`,
    /// `editor`, `manager` or `owner`. Default `viewer` (every member).
    min_role: String,
    /// File kinds agents may read: any of `document`, `spreadsheet`, `presentation`.
    kinds: Vec<String>,
    /// Reads per minute for each agent grant (1 to 600). Default 60.
    calls_per_minute: i32,
    /// Response bytes per UTC day for each agent grant (10,000 to 1,000,000,000).
    /// Default 20,000,000.
    read_bytes_per_day: i64,
    /// Zero until the first save; then increases by one with every change. Send it back
    /// in `If-Match`.
    revision: i64,
    updated_at: Option<DateTime<Utc>>,
    /// When the kill switch was last pulled (null if never). Agent sessions issued before
    /// it are refused, whatever is granted or enabled afterwards.
    killed_at: Option<DateTime<Utc>>,
    /// Whether this server serves the agent routes at all (`AGENT_GATEWAY=on`). The
    /// policy can be saved either way; it has no effect while this is false.
    gateway: bool,
    /// Files with a policy of `none` or `read`, newest first, at most 100.
    file_overrides: Vec<FileOverride>,
    file_overrides_truncated: bool,
    /// On `PUT`: false when the body equalled the saved policy and nothing was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    changed: Option<bool>,
}

async fn view(
    db: &mut PgConnection,
    state: &AppState,
    project: Uuid,
    stored: Option<&Stored>,
    changed: Option<bool>,
) -> Result<PolicyView, ApiError> {
    let rows: Vec<(Uuid, String, DateTime<Utc>)> = sqlx::query_as("select f.document_id,f.access,f.updated_at from file_agent_policy f join document d on d.id=f.document_id where f.access<>'inherit' and d.deleted_at is null and document_project(d.id)=$1 order by f.updated_at desc,f.document_id limit $2")
        .bind(project)
        .bind(MAX_OVERRIDES + 1)
        .fetch_all(db)
        .await?;
    let truncated = rows.len() as i64 > MAX_OVERRIDES;
    let current = stored.map_or_else(Policy::default, |s| s.policy.clone());
    Ok(PolicyView {
        project_id: project,
        configured: stored.is_some(),
        enabled: current.enabled,
        min_role: current.min_role,
        kinds: current.kinds,
        calls_per_minute: current.calls_per_minute,
        read_bytes_per_day: current.read_bytes_per_day,
        revision: stored.map_or(0, |s| s.revision),
        updated_at: stored.map(|s| s.updated_at),
        killed_at: stored.and_then(|s| s.killed_at),
        gateway: state.agent_gateway,
        file_overrides: rows
            .into_iter()
            .take(MAX_OVERRIDES as usize)
            .map(|(document_id, access, updated_at)| FileOverride {
                document_id,
                access,
                updated_at,
            })
            .collect(),
        file_overrides_truncated: truncated,
        changed,
    })
}

/// `200` with the revision as the `ETag`; a policy is never stored by a cache.
fn reply(view: PolicyView) -> impl IntoResponse {
    (
        [
            (header::ETAG, format!("\"{}\"", view.revision)),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        Json(view),
    )
}

// --- GET and PUT /projects/{id}/agent-policy ----------------------------------------

#[utoipa::path(operation_id="agent_policy_get", get, path="/projects/{id}/agent-policy", tag="agent policy", params(("id"=Uuid,Path,description="Project")), security(("paseto"=[])), responses((status=200, description="The project's agent policy (agents are disabled until an owner or manager enables them; configured=false means none was ever saved). Sets ETag to the revision. Works whether or not the agent gateway is switched on; gateway says which", body=PolicyView),(status=401, description="Account sign-in required; a device, agent or connector token is refused"),(status=403, description="Not a project owner or manager (the same answer for a non-member and an unknown project)")))]
async fn get_policy(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let mut db = state.pool.acquire().await?;
    require_manager(&mut db, project, auth.identity_id).await?;
    let stored = policy::load(&mut db, project, false).await?;
    Ok(reply(
        view(&mut db, &state, project, stored.as_ref(), None).await?,
    ))
}

/// The new policy. `enabled` must be said; every other field left out keeps its SAVED value
/// (the defaults when nothing was saved), so a request that only flips `enabled` cannot
/// silently widen the role floor, the kinds or the budgets.
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PolicyBody {
    /// Master switch. Required.
    enabled: bool,
    /// `viewer` (default), `contributor`, `reviewer`, `editor`, `manager` or `owner`.
    min_role: Option<String>,
    /// One or more of `document`, `spreadsheet`, `presentation` (default: all three).
    kinds: Option<Vec<String>>,
    /// 1 to 600 (default 60).
    calls_per_minute: Option<i64>,
    /// 10,000 to 1,000,000,000 (default 20,000,000).
    read_bytes_per_day: Option<i64>,
}

impl PolicyBody {
    /// The policy this body asks for, with `base` (the saved policy) for what it leaves out.
    fn validate(self, base: &Policy) -> Result<Policy, ApiError> {
        let min_role = self.min_role.unwrap_or_else(|| base.min_role.clone());
        if !ROLES.contains(&min_role.as_str()) {
            return Err(bad(
                "min_role must be viewer, contributor, reviewer, editor, manager or owner",
            ));
        }
        let kinds = match self.kinds {
            None => base.kinds.clone(),
            Some(list) => {
                if list.iter().any(|k| !KINDS.contains(&k.as_str())) {
                    return Err(bad(
                        "kinds may hold only document, spreadsheet and presentation",
                    ));
                }
                // Canonical order, no duplicates.
                let kept: Vec<String> = KINDS
                    .iter()
                    .filter(|k| list.iter().any(|l| l == *k))
                    .map(|k| (*k).to_owned())
                    .collect();
                if kept.is_empty() {
                    return Err(bad(
                        "Name at least one kind; to stop all agents set enabled to false",
                    ));
                }
                kept
            }
        };
        let calls = self
            .calls_per_minute
            .unwrap_or(i64::from(base.calls_per_minute));
        if !(CALLS_PER_MINUTE.0..=CALLS_PER_MINUTE.1).contains(&calls) {
            return Err(bad("calls_per_minute must be 1 to 600"));
        }
        let bytes = self.read_bytes_per_day.unwrap_or(base.read_bytes_per_day);
        if !(READ_BYTES_PER_DAY.0..=READ_BYTES_PER_DAY.1).contains(&bytes) {
            return Err(bad("read_bytes_per_day must be 10,000 to 1,000,000,000"));
        }
        Ok(Policy {
            enabled: self.enabled,
            min_role,
            kinds,
            calls_per_minute: calls as i32,
            read_bytes_per_day: bytes,
        })
    }
}

/// `If-Match` as a revision: `"3"`, `W/"3"` or `3`. Required, because two managers saving
/// a security setting must not silently overwrite each other.
fn if_match(headers: &HeaderMap) -> Result<i64, ApiError> {
    let raw = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            bad("Send If-Match with the policy revision you read (0 when none was saved yet)")
        })?;
    raw.trim()
        .trim_start_matches("W/")
        .trim_matches('"')
        .parse::<i64>()
        .ok()
        .filter(|revision| *revision >= 0)
        .ok_or_else(|| bad("If-Match must be the policy revision, for example \"3\""))
}

/// The fields that differ, with their values before and after.
fn diff(before: &Policy, after: &Policy) -> (Vec<&'static str>, Value, Value) {
    let mut names = Vec::new();
    let (mut from, mut to) = (Map::new(), Map::new());
    let mut note = |name: &'static str, old: Value, new: Value| {
        if old != new {
            names.push(name);
            from.insert(name.to_owned(), old);
            to.insert(name.to_owned(), new);
        }
    };
    note("enabled", json!(before.enabled), json!(after.enabled));
    note("min_role", json!(before.min_role), json!(after.min_role));
    note("kinds", json!(before.kinds), json!(after.kinds));
    note(
        "calls_per_minute",
        json!(before.calls_per_minute),
        json!(after.calls_per_minute),
    );
    note(
        "read_bytes_per_day",
        json!(before.read_bytes_per_day),
        json!(after.read_bytes_per_day),
    );
    (names, Value::Object(from), Value::Object(to))
}

#[utoipa::path(operation_id="agent_policy_put", put, path="/projects/{id}/agent-policy", tag="agent policy", params(("id"=Uuid,Path,description="Project"),("If-Match"=String,Header,description="The revision from the last read, for example \"3\"; 0 when no policy was saved. Required")), request_body=PolicyBody, security(("paseto"=[])), responses((status=200, description="The saved policy. enabled is required; a field left out keeps its saved value (the defaults when none was saved), so flipping enabled never widens the role floor, kinds or budgets. changed=false and nothing written when the result equalled the saved policy. Each effective change writes an audit row (agent.policy.changed) with the changed fields before and after. Disabling purges the project's cached document states", body=PolicyView),(status=400, description="If-Match missing or malformed, or a value out of range"),(status=401, description="Account sign-in required; a device, agent or connector token is refused"),(status=403, description="Not a project owner or manager"),(status=409, description="The policy changed since the revision in If-Match"),(status=422, description="Malformed body, a missing enabled or an unknown field"),(status=429, description="The caller's management budget (120 a minute, 2 at once) is used up")))]
async fn put_policy(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<PolicyBody>,
) -> Result<impl IntoResponse, ApiError> {
    let expected = if_match(&headers)?;
    let mut tx = begin(&state).await?;
    lock_project(&mut tx, project).await?;
    require_manager(&mut tx, project, auth.identity_id).await?;
    let _permit = charge(&state, auth.identity_id)?;
    // The row lock serialises concurrent writers; the loser sees the new revision.
    let current = policy::load(&mut tx, project, true).await?;
    let revision = current.as_ref().map_or(0, |c| c.revision);
    let before = current
        .as_ref()
        .map_or_else(Policy::default, |c| c.policy.clone());
    let wanted = body.validate(&before)?;
    if expected != revision {
        return Err(ApiError::Conflict {
            reason: "Agent policy changed. Reload before saving.".into(),
        });
    }
    let (changed, from, to) = diff(&before, &wanted);
    if changed.is_empty() {
        let out = view(&mut tx, &state, project, current.as_ref(), Some(false)).await?;
        tx.commit().await?;
        return Ok(reply(out));
    }
    let saved: Option<(i64, DateTime<Utc>)> = if current.is_some() {
        sqlx::query_as("update project_agent_policy set enabled=$2,min_role=$3,kinds=$4,calls_per_minute=$5,read_bytes_per_day=$6,revision=revision+1,updated_by=$7,updated_at=clock_timestamp() where project_id=$1 returning revision,updated_at")
            .bind(project)
            .bind(wanted.enabled)
            .bind(&wanted.min_role)
            .bind(&wanted.kinds)
            .bind(wanted.calls_per_minute)
            .bind(wanted.read_bytes_per_day)
            .bind(auth.identity_id)
            .fetch_optional(&mut *tx)
            .await?
    } else {
        // Nothing to lock yet: of two first saves, the second inserts nothing.
        sqlx::query_as("insert into project_agent_policy(project_id,enabled,min_role,kinds,calls_per_minute,read_bytes_per_day,updated_by,updated_at) values($1,$2,$3,$4,$5,$6,$7,clock_timestamp()) on conflict(project_id) do nothing returning revision,updated_at")
            .bind(project)
            .bind(wanted.enabled)
            .bind(&wanted.min_role)
            .bind(&wanted.kinds)
            .bind(wanted.calls_per_minute)
            .bind(wanted.read_bytes_per_day)
            .bind(auth.identity_id)
            .fetch_optional(&mut *tx)
            .await?
    };
    let Some((saved_revision, updated_at)) = saved else {
        return Err(ApiError::Conflict {
            reason: "Agent policy changed. Reload before saving.".into(),
        });
    };
    audit(
        &mut tx,
        auth.identity_id,
        "agent.policy.changed",
        project,
        json!({"revision":saved_revision,"previous_revision":revision,"changed":changed,"from":from,"to":to}),
    )
    .await?;
    let saved = Stored {
        policy: wanted.clone(),
        revision: saved_revision,
        updated_at,
        killed_at: current.as_ref().and_then(|c| c.killed_at),
    };
    let out = view(&mut tx, &state, project, Some(&saved), Some(true)).await?;
    tx.commit().await?;
    if before.enabled && !wanted.enabled {
        purge_project(&state, project).await;
    }
    Ok(reply(out))
}

// --- POST /projects/{id}/agent-policy/kill ------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct KillView {
    project_id: Uuid,
    /// Always false after this call.
    enabled: bool,
    /// The policy revision now. Every pull is a change of the policy row, so it moves by
    /// one (the first pull on a project with no saved policy creates the row, revision 1).
    revision: i64,
    /// Whether the policy was enabled when the switch was pulled.
    was_enabled: bool,
    /// Active agent grants of this project ended by this call.
    revoked_grants: i64,
}

#[utoipa::path(operation_id="agent_policy_kill", post, path="/projects/{id}/agent-policy/kill", tag="agent policy", params(("id"=Uuid,Path,description="Project")), security(("paseto"=[])), responses((status=200, description="Agents are disabled for the project and every active grant of an agent key for it is revoked, in one transaction, and the project's cached document states are purged. Takes effect on the very next agent request. Every agent session issued before the call is refused for good (sessions are stateless tokens, so this is recorded as a cut-off time on the policy, killed_at): enabling the policy or granting a key again restores none of them, and the key holders must grant again and open a new session. Each pull moves the policy revision and is audited (agent.killswitch, plus device.grant.revoked per grant); pulling it again is harmless", body=KillView),(status=401, description="Account sign-in required; a device, agent or connector token is refused"),(status=403, description="Not a project owner or manager"),(status=429, description="The caller's management budget (120 a minute, 2 at once) is used up")))]
async fn kill(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project): Path<Uuid>,
) -> Result<Json<KillView>, ApiError> {
    let mut tx = begin(&state).await?;
    lock_project(&mut tx, project).await?;
    require_manager(&mut tx, project, auth.identity_id).await?;
    let _permit = charge(&state, auth.identity_id)?;
    let current = policy::load(&mut tx, project, true).await?;
    let was_enabled = current.as_ref().is_some_and(|c| c.policy.enabled);
    // Only agent keys: a person's key holds no agent grant. One audit row per ended grant,
    // under the grant's own action, so the key holder's trail explains the change.
    let revoked: i64 = sqlx::query_scalar("with revoked as (update device_key_project_grant g set revoked_at=clock_timestamp() where g.project_id=$1 and g.revoked_at is null and exists(select 1 from device_key_class c where c.key_id=g.key_id and c.class='agent') returning g.key_id,g.scopes), logged as (insert into operation_audit(actor_id,action,resource_id,detail) select $2::uuid,'device.grant.revoked',r.key_id,jsonb_build_object('key_id',r.key_id,'project_id',$1::uuid,'scopes',to_jsonb(r.scopes),'reason','agent_killswitch') from revoked r returning 1) select count(*) from logged")
        .bind(project)
        .bind(auth.identity_id)
        .fetch_one(&mut *tx)
        .await?;
    // Disable, and mark the moment: sessions not issued strictly after it are refused for
    // good (the extractor compares them with sessions_valid_after), so enabling or granting
    // again revives none of them. The row is created if the project never saved a policy.
    // The cut-off is stamped as late as the transaction allows, after the grants are ended,
    // so the only sessions it can miss are those whose issuance began in the few
    // milliseconds between this statement and the commit; they die with the grants, and
    // come back only if the key holder grants again before they expire.
    let revision: i64 = sqlx::query_scalar("insert into project_agent_policy(project_id,enabled,updated_by,updated_at,sessions_valid_after) values($1,false,$2,clock_timestamp(),clock_timestamp()) on conflict(project_id) do update set enabled=false,revision=project_agent_policy.revision+1,updated_by=$2,updated_at=clock_timestamp(),sessions_valid_after=greatest(project_agent_policy.sessions_valid_after,clock_timestamp()) returning revision")
        .bind(project)
        .bind(auth.identity_id)
        .fetch_one(&mut *tx)
        .await?;
    audit(
        &mut tx,
        auth.identity_id,
        "agent.killswitch",
        project,
        json!({"was_enabled":was_enabled,"revision":revision,"revoked_grants":revoked}),
    )
    .await?;
    tx.commit().await?;
    purge_project(&state, project).await;
    Ok(Json(KillView {
        project_id: project,
        enabled: false,
        revision,
        was_enabled,
        revoked_grants: revoked,
    }))
}

// --- GET /projects/{id}/agent-usage -------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UsageQuery {
    from: Option<String>,
    to: Option<String>,
}

/// One day of one client claim.
#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct UsageRow {
    /// UTC calendar day.
    day: NaiveDate,
    /// The caller's own `X-Dynodoc-Client` claim (null when it sent none). Untrusted
    /// text; never used for authorisation.
    client: Option<String>,
    calls: i64,
    bytes_out: i64,
    /// Calls the policy or a grant check refused (`forbidden`, `policy_denied`,
    /// `budget_exceeded`).
    denials: i64,
    /// Calls that failed for another reason (bad request, replay failure and so on).
    errors: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct UsageView {
    project_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    /// Newest day first, then most calls; at most 500.
    items: Vec<UsageRow>,
    truncated: bool,
}

/// A day within [`USAGE_YEARS`]; anything else would panic chrono's arithmetic or leave
/// PostgreSQL's date range.
fn in_range(day: NaiveDate) -> Result<NaiveDate, ApiError> {
    if (USAGE_YEARS.0..=USAGE_YEARS.1).contains(&day.year()) {
        Ok(day)
    } else {
        Err(bad("Dates must fall between the years 2000 and 2200"))
    }
}

fn parse_day(raw: &str) -> Result<NaiveDate, ApiError> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| bad("Dates are YYYY-MM-DD, in UTC"))
        .and_then(in_range)
}

#[utoipa::path(operation_id="agent_usage", get, path="/projects/{id}/agent-usage", tag="agent policy", params(("id"=Uuid,Path,description="Project"),("from"=Option<String>,Query,description="First UTC day, YYYY-MM-DD (default: six days before to)"),("to"=Option<String>,Query,description="Last UTC day, inclusive (default: today)")), security(("paseto"=[])), responses((status=200, description="Calls, response bytes and denials of the project's agent grants by UTC day and client claim, from the call ledger (metadata only: no document text, title or request value). The range is at most 31 days and 500 rows", body=UsageView),(status=400, description="Bad date, a year outside 2000 to 2200, from after to, or a range over 31 days"),(status=401, description="Account sign-in required; a device, agent or connector token is refused"),(status=403, description="Not a project owner or manager"),(status=429, description="The caller's management budget (120 a minute, 2 at once) is used up")))]
async fn usage(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project): Path<Uuid>,
    parsed: Result<Query<UsageQuery>, QueryRejection>,
) -> Result<Json<UsageView>, ApiError> {
    let Query(range) = parsed.map_err(|_| bad("Use from and to as YYYY-MM-DD"))?;
    let to = match range.to.as_deref() {
        Some(raw) => parse_day(raw)?,
        None => in_range(Utc::now().date_naive())?,
    };
    let from = match range.from.as_deref() {
        Some(raw) => parse_day(raw)?,
        None => to
            .checked_sub_signed(chrono::Duration::days(6))
            .ok_or_else(|| bad("Dates must fall between the years 2000 and 2200"))
            .and_then(in_range)?,
    };
    if from > to || (to - from).num_days() >= MAX_USAGE_DAYS {
        return Err(bad(
            "Use a range of at most 31 days, with from not after to",
        ));
    }
    let mut db = state.pool.acquire().await?;
    require_manager(&mut db, project, auth.identity_id).await?;
    let _permit = charge(&state, auth.identity_id)?;
    let rows: Vec<(NaiveDate, Option<String>, i64, i64, i64, i64)> = sqlx::query_as("select (a.at at time zone 'utc')::date,a.client,count(*)::bigint,coalesce(sum(a.bytes_out),0)::bigint,(count(*) filter (where a.outcome in ('forbidden','policy_denied','budget_exceeded','unauthorized')))::bigint,(count(*) filter (where a.outcome not in ('ok','forbidden','policy_denied','budget_exceeded','unauthorized')))::bigint from agent_call a where a.project_id=$1 and a.at>=($2::date)::timestamp at time zone 'utc' and a.at<(($3::date)+1)::timestamp at time zone 'utc' group by 1,2 order by 1 desc,3 desc,2 nulls last limit $4")
        .bind(project)
        .bind(from)
        .bind(to)
        .bind(MAX_USAGE_ROWS as i64 + 1)
        .fetch_all(&mut *db)
        .await?;
    let truncated = rows.len() > MAX_USAGE_ROWS;
    Ok(Json(UsageView {
        project_id: project,
        from,
        to,
        items: rows
            .into_iter()
            .take(MAX_USAGE_ROWS)
            .map(
                |(day, client, calls, bytes_out, denials, errors)| UsageRow {
                    day,
                    client,
                    calls,
                    bytes_out,
                    denials,
                    errors,
                },
            )
            .collect(),
        truncated,
    }))
}

// --- PUT /documents/{id}/agent-policy -----------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct FilePolicyBody {
    /// `inherit` (the project policy decides), `none` (agents cannot see the file) or
    /// `read` (accepted for the day writes exist; it never widens the project's policy).
    access: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct FilePolicyView {
    document_id: Uuid,
    project_id: Uuid,
    access: String,
    /// False when the file already had this value; nothing was written.
    changed: bool,
}

#[utoipa::path(operation_id="agent_file_policy_put", put, path="/documents/{id}/agent-policy", tag="agent policy", params(("id"=Uuid,Path,description="File")), request_body=FilePolicyBody, security(("paseto"=[])), responses((status=200, description="The file's agent access. none hides the file from agents entirely (404 on every agent route, absent from the listing and the 8-character lookup) and purges its cached state. A file override can only narrow the project policy. Audited (agent.file_policy.changed) when it changes. Needs the file's owner or a project manager; loosening it (none to read or inherit, read to inherit) needs a project owner or manager, so a file's creator who is only an editor or viewer can tighten but not undo a manager's choice", body=FilePolicyView),(status=400, description="access is not inherit, none or read"),(status=401, description="Account sign-in required; a device, agent or connector token is refused"),(status=403, description="The caller can open the file but is neither its owner nor a manager, or is loosening an override without being a project owner or manager"),(status=404, description="No such file, or not one the caller can open, or not in a project"),(status=429, description="The caller's management budget (120 a minute, 2 at once) is used up")))]
async fn put_file_policy(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(document): Path<Uuid>,
    Json(body): Json<FilePolicyBody>,
) -> Result<Json<FilePolicyView>, ApiError> {
    let access = match body.access.as_str() {
        value @ ("inherit" | "none" | "read") => value.to_owned(),
        _ => return Err(bad("access must be inherit, none or read")),
    };
    let mut tx = begin(&state).await?;
    projects::lock_document_project(&mut tx, document).await?;
    let known: Option<(Option<Uuid>, Option<String>)> = sqlx::query_as("select document_project(d.id),document_member_role(d.id,$2) from document d where d.id=$1 and d.deleted_at is null")
        .bind(document)
        .bind(auth.identity_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((Some(project), Some(_))) = known else {
        return Err(ApiError::NotFound);
    };
    let may_manage: bool = sqlx::query_scalar("select can_manage_document($1,$2)")
        .bind(document)
        .bind(auth.identity_id)
        .fetch_one(&mut *tx)
        .await?;
    if !may_manage {
        return Err(ApiError::Forbidden);
    }
    let _permit = charge(&state, auth.identity_id)?;
    // One writer per file; the row may not exist yet, so a row lock would not serialise.
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text,382))")
        .bind(format!("file_agent_policy:{document}"))
        .execute(&mut *tx)
        .await?;
    let previous: String = sqlx::query_scalar(
        "select coalesce((select access from file_agent_policy where document_id=$1),'inherit')",
    )
    .bind(document)
    .fetch_one(&mut *tx)
    .await?;
    // Lower layers narrow, never widen: the file's creator may be only an editor, or a
    // viewer since demoted, and `can_manage_document` still lets them tighten the file, but
    // undoing a narrowing (none to read or inherit, read to inherit) is for a project owner
    // or manager.
    let openness = |value: &str| match value {
        "none" => 0,
        "read" => 1,
        _ => 2,
    };
    if openness(&access) > openness(&previous) {
        let role = projects::require(&mut tx, project, auth.identity_id).await?;
        if !matches!(role.as_str(), "owner" | "manager") {
            return Err(ApiError::Forbidden);
        }
    }
    let changed = previous != access;
    if changed {
        sqlx::query("insert into file_agent_policy(document_id,access,updated_by,updated_at) values($1,$2,$3,clock_timestamp()) on conflict(document_id) do update set access=excluded.access,updated_by=excluded.updated_by,updated_at=excluded.updated_at")
            .bind(document)
            .bind(&access)
            .bind(auth.identity_id)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            auth.identity_id,
            "agent.file_policy.changed",
            document,
            json!({"document_id":document,"project_id":project,"from":previous,"to":access}),
        )
        .await?;
    }
    tx.commit().await?;
    if changed && access == "none" {
        state.agent_cache.purge_document(document);
    }
    Ok(Json(FilePolicyView {
        document_id: document,
        project_id: project,
        access,
        changed,
    }))
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(get_policy, put_policy, kill, usage, put_file_policy),
    components(schemas(
        PolicyBody,
        PolicyView,
        FileOverride,
        KillView,
        UsageRow,
        UsageView,
        FilePolicyBody,
        FilePolicyView
    ))
)]
pub struct AgentManageApi;
