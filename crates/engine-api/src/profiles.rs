//! Account-owned profiles, readable only by the account or a shared project member.
use crate::{auth::AuthContext, error::ApiError, AppState};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::Datelike;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/profile", get(own).post(save))
        .route("/profile/storage", get(storage))
        .route("/profiles/:id", get(read))
        .route("/profiles/:id/activity", get(profile_activity))
}
/// $1 is the viewer and $2 the profile: the account itself or a shared project member.
const VISIBLE: &str = "i.id=$2 and i.disabled_at is null and ($1=$2 or exists(select 1 from access_space s where s.kind='project' and inherited_space_role(s.id,$1) is not null and inherited_space_role(s.id,$2) is not null))";
async fn profile(state: &AppState, actor: Uuid, id: Uuid) -> Result<Json<Value>, ApiError> {
    let value: Option<Value> = sqlx::query_scalar(&format!("select jsonb_build_object('id',i.id,'name',coalesce(i.display_name,''),'bio',i.bio,'revision',i.profile_revision) from identity i where {VISIBLE}"))
        .bind(actor).bind(id).fetch_optional(&state.pool).await?;
    Ok(Json(value.ok_or(ApiError::NotFound)?))
}
#[utoipa::path(get, path="/profile", security(("paseto"=[])), responses((status=200,description="Own profile, excluding email",body=Value)))]
async fn own(State(state): State<AppState>, auth: AuthContext) -> Result<Json<Value>, ApiError> {
    profile(&state, auth.identity_id, auth.identity_id).await
}

#[utoipa::path(get, path="/profile/storage", security(("paseto"=[])), responses((status=200,description="Own encoded shared-history usage and quotas; excludes uploads, events, legacy snapshots and database overhead",body=Value),(status=401,description="Sign in required")))]
async fn storage(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let usage: Option<Value> = sqlx::query_scalar("select jsonb_build_object('bytes_used',bytes_used,'objects_used',objects_used,'byte_limit',byte_limit,'object_limit',object_limit) from checkpoint_storage_account where identity_id=$1")
        .bind(auth.identity_id).fetch_optional(&state.pool).await?;
    Ok(Json(usage.unwrap_or_else(|| json!({"bytes_used":0,"objects_used":0,"byte_limit":4294967296_i64,"object_limit":1000000}))))
}
#[utoipa::path(get, path="/profiles/{id}", params(("id"=Uuid,Path)), security(("paseto"=[])), responses((status=200,description="Profile visible to shared project members",body=Value),(status=404,description="Not accessible")))]
async fn read(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    profile(&state, auth.identity_id, id).await
}
#[derive(Deserialize)]
struct ActivityYear {
    year: Option<i32>,
}
/// Events authored by one account per UTC day. Your own calendar counts everything you
/// wrote; anyone else's counts only files in projects and folders you both belong to,
/// so no title, project or count outside that overlap is revealed.
#[utoipa::path(get, path="/profiles/{id}/activity", params(("id"=Uuid,Path),("year"=Option<i32>,Query,description="UTC calendar year, 2000-2100; defaults to the current year")), security(("paseto"=[])), responses((status=200,description="Per-day counts of events authored by the profile (days without activity are omitted)",body=Value),(status=400,description="Invalid year"),(status=404,description="Not accessible"),(status=429,description="Too many activity reads"),(status=504,description="Deadline exceeded")))]
async fn profile_activity(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
    Query(query): Query<ActivityYear>,
) -> Result<Json<Value>, ApiError> {
    let year = query.year.unwrap_or_else(|| chrono::Utc::now().year());
    if !(2000..=2100).contains(&year) {
        return Err(ApiError::BadRequest {
            reason: "Choose a year between 2000 and 2100".into(),
        });
    }
    let january = |y: i32| {
        chrono::NaiveDate::from_ymd_opt(y, 1, 1)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|t| t.and_utc())
    };
    let (Some(start), Some(end)) = (january(year), january(year + 1)) else {
        return Err(ApiError::Internal("calendar bounds".into()));
    };
    let visible: bool = sqlx::query_scalar(&format!(
        "select exists(select 1 from identity i where {VISIBLE})"
    ))
    .bind(auth.identity_id)
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    if !visible {
        return Err(ApiError::NotFound);
    }
    let _budget = state.workloads.enter(auth.identity_id, None)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("set local statement_timeout='3s'")
        .execute(&mut *tx)
        .await?;
    let sql = if auth.identity_id == id {
        "select to_char(e.created_at at time zone 'UTC','YYYY-MM-DD') as day,count(*)::bigint as events from event e where e.actor_id=$2 and e.created_at>=$3 and e.created_at<$4 and $1=$2 group by 1 order by 1"
    } else {
        "with recursive v(id) as (select space_id from space_member where identity_id=$1 union select s.id from access_space s join v on s.parent_id=v.id), p(id) as (select space_id from space_member where identity_id=$2 union select s.id from access_space s join p on s.parent_id=p.id), shared as (select id from v intersect select id from p) select to_char(e.created_at at time zone 'UTC','YYYY-MM-DD') as day,count(*)::bigint as events from event e where e.actor_id=$2 and e.created_at>=$3 and e.created_at<$4 and e.document_id in (select d.id from document d where d.deleted_at is null and d.space_id in (select id from shared)) group by 1 order by 1"
    };
    let rows: Vec<(String, i64)> = sqlx::query_as(sql)
        .bind(auth.identity_id)
        .bind(id)
        .bind(start)
        .bind(end)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    let total: i64 = rows.iter().map(|(_, n)| n).sum();
    let days: Vec<Value> = rows
        .into_iter()
        .map(|(date, count)| json!({"date":date,"count":count}))
        .collect();
    Ok(Json(json!({"year":year,"total":total,"days":days})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditProfile {
    name: String,
    bio: String,
    revision: i64,
}
#[utoipa::path(post, path="/profile", request_body=Value, security(("paseto"=[])), responses((status=200,description="Profile saved",body=Value),(status=400,description="Invalid profile"),(status=409,description="Profile changed; reload")))]
async fn save(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(input): Json<EditProfile>,
) -> Result<Json<Value>, ApiError> {
    let name = input.name.trim();
    let bio = input.bio.trim();
    if name.is_empty()
        || name.chars().count() > 80
        || name.chars().any(char::is_control)
        || bio.chars().count() > 280
        || bio.contains('\0')
        || input.revision < 0
    {
        return Err(ApiError::BadRequest {
            reason: "Use a name of 1–80 characters and a bio of up to 280 characters".into(),
        });
    }
    let changed: Option<i64> = sqlx::query_scalar("update identity set display_name=$2,bio=$3,profile_revision=profile_revision+1 where id=$1 and disabled_at is null and profile_revision=$4 returning profile_revision")
        .bind(auth.identity_id).bind(name).bind(bio).bind(input.revision).fetch_optional(&state.pool).await?;
    let revision = changed.ok_or_else(|| ApiError::Conflict {
        reason: "Your profile changed in another tab. Reload before saving.".into(),
    })?;
    Ok(Json(
        json!({"id":auth.identity_id,"name":name,"bio":bio,"revision":revision}),
    ))
}
