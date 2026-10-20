//! Read-only operations overview and bounded anonymous performance samples.
use crate::{auth::AuthContext, error::ApiError, AppState};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/overview", get(overview))
        .route("/telemetry", post(telemetry))
        .route("/admin/records", get(records))
        .route("/admin/actions", post(action))
        .merge(crate::admin_controls::router())
}
pub fn is_admin(email: &str, configured: &str) -> bool {
    configured
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|s| s.eq_ignore_ascii_case(email))
}
#[utoipa::path(get, path="/admin/overview", tag="administration", security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn overview(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let email: String = sqlx::query_scalar("select email from identity where id=$1")
        .bind(auth.identity_id)
        .fetch_one(&state.pool)
        .await?;
    if !is_admin(&email, &state.admin_emails) {
        return Err(ApiError::Forbidden);
    }
    let counts: Value = sqlx::query_scalar("select jsonb_build_object('documents',(select count(*) from document),'events',(select count(*) from event),'versions',(select count(*) from document_version),'accounts',(select count(*) from identity),'uploads',(select count(*) from document_upload),'upload_bytes',(select coalesce(sum(octet_length(content)),0) from document_upload),'database_bytes',pg_database_size(current_database()),'connections',(select count(*) from pg_stat_activity where datname=current_database()),'migrations',(select count(*) from _sqlx_migrations where success),'content_patches',(select count(*) from event where type='RichTextPatched'),'content_full_edits',(select count(*) from event where type='FieldEdited' and payload->>'field'='content'))")
        .fetch_one(&state.pool).await?;
    let tables = sqlx::query("select relname, n_live_tup, pg_total_relation_size(relid) as bytes from pg_stat_user_tables order by pg_total_relation_size(relid) desc")
        .fetch_all(&state.pool).await?.iter().map(|r| json!({"name":r.get::<String,_>("relname"),"estimated_rows":r.get::<i64,_>("n_live_tup"),"bytes":r.get::<i64,_>("bytes")})).collect::<Vec<_>>();
    let traffic = sqlx::query("select host,page,count(*) as visits from web_metric where metric='visit' and recorded_at>now()-interval '30 days' group by host,page order by visits desc")
        .fetch_all(&state.pool).await?.iter().map(|r| json!({"host":r.get::<String,_>("host"),"page":r.get::<String,_>("page"),"visits":r.get::<i64,_>("visits")})).collect::<Vec<_>>();
    let vitals = sqlx::query("select host,metric,count(*) as samples,percentile_cont(0.75) within group(order by value) as p75 from web_metric where metric<>'visit' and recorded_at>now()-interval '30 days' group by host,metric order by host,metric")
        .fetch_all(&state.pool).await?.iter().map(|r| json!({"host":r.get::<String,_>("host"),"metric":r.get::<String,_>("metric"),"samples":r.get::<i64,_>("samples"),"p75":r.get::<f64,_>("p75")})).collect::<Vec<_>>();
    Ok(Json(
        json!({"checked_at":chrono::Utc::now(),"administrator":email,"counts":counts,"errors":state.counters.snapshot(),"retention":state.retention.snapshot(),"tables":tables,"traffic":traffic,"vitals":vitals}),
    ))
}
#[derive(Deserialize)]
struct Sample {
    page_id: Uuid,
    metric: String,
    host: String,
    page: String,
    value: f64,
}
async fn telemetry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(s): Json<Sample>,
) -> Result<Json<Value>, ApiError> {
    let supplied = headers
        .get("x-dynodoc-service-key")
        .and_then(|h| h.to_str().ok());
    if state.auth.service_key.is_none() || supplied != state.auth.service_key.as_deref() {
        return Err(ApiError::Forbidden);
    }
    if !["visit", "LCP", "INP", "CLS", "FCP", "TTFB"].contains(&s.metric.as_str())
        || !["homepage", "workspace", "admin"].contains(&s.host.as_str())
        || ![
            "home",
            "privacy",
            "terms",
            "login",
            "workspace",
            "editor",
            "admin",
            "other",
        ]
        .contains(&s.page.as_str())
        || !s.value.is_finite()
        || !(0.0..=3_600_000.0).contains(&s.value)
    {
        return Err(ApiError::BadRequest {
            reason: "Invalid performance sample".into(),
        });
    }
    sqlx::query("insert into web_metric(page_id,metric,host,page,value) values($1,$2,$3,$4,$5) on conflict(page_id,metric) do nothing")
        .bind(s.page_id).bind(s.metric).bind(s.host).bind(s.page).bind(s.value).execute(&state.pool).await?;
    Ok(Json(json!({"ok":true})))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn administrator_access_is_explicit() {
        assert!(is_admin(
            "Owner@Example.org",
            "other@example.org, owner@example.org"
        ));
        assert!(!is_admin("member@example.org", "owner@example.org"));
        assert!(!is_admin("any@example.org", ""));
        assert!(!is_admin(
            "owner@example.org.attacker.test",
            "owner@example.org"
        ));
    }
}

#[derive(Deserialize)]
struct RecordsQuery {
    kind: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    offset: i64,
    /// Documents only: `active`, `trash`, or empty for both.
    #[serde(default)]
    status: String,
    /// Audit only: an account id, or part of the actor's email.
    #[serde(default)]
    actor: String,
    /// Audit only: RFC 3339 time or `YYYY-MM-DD` (start of that UTC day), inclusive.
    #[serde(default)]
    since: String,
    /// Audit only: RFC 3339 time (exclusive) or `YYYY-MM-DD` (through the end of that UTC day).
    #[serde(default)]
    until: String,
}
/// Accepts an RFC 3339 instant, or a calendar date at the start (`end_of_day=false`)
/// or the exclusive end (`end_of_day=true`) of that UTC day.
fn bound(value: &str, end_of_day: bool) -> Result<Option<chrono::DateTime<chrono::Utc>>, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(Some(t.with_timezone(&chrono::Utc)));
    }
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|d| if end_of_day { d.succ_opt() } else { Some(d) })
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|t| Some(t.and_utc()))
        .ok_or_else(|| ApiError::BadRequest {
            reason: "Use dates as YYYY-MM-DD or RFC 3339 times".into(),
        })
}
const SECRET_KEYS: [&str; 12] = [
    "token",
    "secret",
    "password",
    "passwd",
    "passphrase",
    "authorization",
    "cookie",
    "credential",
    "private",
    "apikey",
    "api_key",
    "bearer",
];
/// Replace values stored under obviously secret key names; nothing else is rewritten.
fn redact(value: Value, depth: usize) -> Value {
    match value {
        Value::Object(map) if depth < 8 => Value::Object(
            map.into_iter()
                .map(|(key, v)| {
                    let lower = key.to_ascii_lowercase();
                    if lower == "key" || SECRET_KEYS.iter().any(|s| lower.contains(s)) {
                        (key, json!("[redacted]"))
                    } else {
                        (key, redact(v, depth + 1))
                    }
                })
                .collect(),
        ),
        Value::Array(items) if depth < 8 => {
            Value::Array(items.into_iter().map(|v| redact(v, depth + 1)).collect())
        }
        Value::Object(_) | Value::Array(_) => json!("[truncated]"),
        other => other,
    }
}
/// Redacted audit detail, at most 4 KiB of JSON; larger records keep only their key names.
fn bounded_detail(detail: Value) -> Value {
    let detail = redact(detail, 0);
    let size = detail.to_string().len();
    if size <= 4096 {
        return detail;
    }
    let keys: Vec<&String> = detail
        .as_object()
        .map(|m| m.keys().take(32).collect())
        .unwrap_or_default();
    json!({"truncated":true,"bytes":size,"keys":keys})
}
pub(crate) async fn require_admin(state: &AppState, auth: &AuthContext) -> Result<(), ApiError> {
    let email: String =
        sqlx::query_scalar("select email from identity where id=$1 and disabled_at is null")
            .bind(auth.identity_id)
            .fetch_one(&state.pool)
            .await?;
    if !is_admin(&email, &state.admin_emails) {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}
#[utoipa::path(get, path="/admin/records", tag="administration", params(("kind"=String,Query,description="users, documents, spaces, projects, audit or erasures"),("q"=Option<String>,Query,description="Case-insensitive substring search, up to 200 characters"),("offset"=Option<i64>,Query,description="Rows to skip, 50 per page"),("status"=Option<String>,Query,description="Documents only: active or trash"),("actor"=Option<String>,Query,description="Audit only: account id or part of the actor email"),("since"=Option<String>,Query,description="Audit only: RFC 3339 time or YYYY-MM-DD, inclusive"),("until"=Option<String>,Query,description="Audit only: RFC 3339 time (exclusive) or YYYY-MM-DD, through that day")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn records(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<RecordsQuery>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &auth).await?;
    if q.q.len() > 200 || !(0..=100000).contains(&q.offset) {
        return Err(ApiError::BadRequest {
            reason: "Invalid search or page".into(),
        });
    }
    let query=match q.kind.as_str(){
 "users"=>"select jsonb_build_object('id',id,'label',email,'name',display_name,'created_at',created_at,'removed_at',disabled_at,'erased_at',erased_at) from identity where erased_at is null and not exists(select 1 from agent_identity a where a.identity_id=identity.id) and (strpos(lower(email),lower($1))>0 or strpos(lower(coalesce(display_name,'')),lower($1))>0) order by created_at desc,id limit 50 offset $2",
 "documents"=>"select jsonb_build_object('id',d.id,'label',d.title,'owner',i.email,'created_at',d.created_at,'updated_at',d.updated_at,'removed_at',d.deleted_at,'kind',coalesce(d.settings->>'kind','questionnaire')) from document d left join identity i on i.id=d.created_by where strpos(lower(d.title),lower($1))>0 and ($3='' or ($3='trash')=(d.deleted_at is not null)) order by d.created_at desc,d.id limit 50 offset $2",
 "audit"=>"select jsonb_build_object('id',a.id,'label',a.action,'actor',i.email,'actor_id',a.actor_id,'resource_id',a.resource_id,'created_at',a.created_at,'detail',case when pg_column_size(a.detail)<=16384 then a.detail else jsonb_build_object('truncated',true,'bytes',pg_column_size(a.detail)) end) from operation_audit a join identity i on i.id=a.actor_id where strpos(lower(a.action),lower($1))>0 and ($3='' or a.actor_id::text=lower($3) or strpos(lower(i.email),lower($3))>0) and ($4::timestamptz is null or a.created_at>=$4) and ($5::timestamptz is null or a.created_at<$5) order by a.id desc limit 50 offset $2",
 "projects"=>"with recursive page as (select s.id,s.name,s.created_at from access_space s join project_settings p on p.space_id=s.id where s.kind='project' and strpos(lower(s.name),lower($1))>0 order by s.created_at desc,s.id limit 50 offset $2), tree(root,id) as (select id,id from page union select t.root,c.id from access_space c join tree t on c.parent_id=t.id), live as (select t.root,d.id,d.updated_at from tree t join document d on d.space_id=t.id and d.deleted_at is null), stats as (select l.root,count(*) as files,max(greatest(l.updated_at,(select e.created_at from event e where e.document_id=l.id order by e.created_at desc limit 1))) as last_activity,(coalesce(sum((select sum(octet_length(u.content)) from document_upload u where u.document_id=l.id)),0)+coalesce(sum((select c.bytes_used from checkpoint_storage_document c where c.document_id=l.id)),0))::bigint as bytes from live l group by l.root) select jsonb_build_object('id',p.id,'label',p.name,'name',p.name,'members',(select count(*) from space_member m join identity i on i.id=m.identity_id where m.space_id=p.id and i.disabled_at is null and i.erased_at is null),'files',coalesce(st.files,0),'bytes',coalesce(st.bytes,0),'created_at',p.created_at,'last_activity_at',greatest(p.created_at,st.last_activity)) from page p left join stats st on st.root=p.id order by p.created_at desc,p.id",
 "spaces"=>"select jsonb_build_object('id',s.id,'label',s.name,'name',s.kind,'created_at',s.created_at) from access_space s where strpos(lower(s.name),lower($1))>0 order by s.created_at desc,s.id limit 50 offset $2",
 "erasures"=>"select jsonb_build_object('id',resource_id,'label',kind||' erased','name',reason,'created_at',erased_at) from erasure_receipt where strpos(lower(kind||' '||reason),lower($1))>0 order by erased_at desc,resource_id limit 50 offset $2",
 _=>return Err(ApiError::BadRequest{reason:"Choose users, documents, spaces, projects, audit or erasures".into()})};
    if !["", "active", "trash"].contains(&q.status.as_str()) {
        return Err(ApiError::BadRequest {
            reason: "Filter documents by active or trash".into(),
        });
    }
    if q.actor.len() > 200 || q.since.len() > 64 || q.until.len() > 64 {
        return Err(ApiError::BadRequest {
            reason: "Invalid audit filter".into(),
        });
    }
    let mut query = sqlx::query_scalar(query).bind(q.q.trim()).bind(q.offset);
    match q.kind.as_str() {
        "documents" => query = query.bind(q.status.as_str()),
        "audit" => {
            query = query
                .bind(q.actor.trim())
                .bind(bound(&q.since, false)?)
                .bind(bound(&q.until, true)?)
        }
        _ => {}
    }
    let mut items: Vec<Value> = if q.kind == "projects" {
        // Per-project totals walk every file; bound them like search does.
        let mut tx = state.pool.begin().await?;
        sqlx::query("set local statement_timeout='5s'")
            .execute(&mut *tx)
            .await?;
        let rows = query.fetch_all(&mut *tx).await?;
        tx.commit().await?;
        rows
    } else {
        query.fetch_all(&state.pool).await?
    };
    if q.kind == "audit" {
        for item in &mut items {
            if let Some(detail) = item.get_mut("detail") {
                *detail = bounded_detail(detail.take());
            }
        }
    }
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
struct AdminAction {
    kind: String,
    id: Uuid,
    removed: bool,
    confirmation: String,
}
#[utoipa::path(post, path="/admin/actions", tag="administration", request_body=Value, security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn action(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(input): Json<AdminAction>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &auth).await?;
    let mut tx = state.pool.begin().await?;
    let action = match input.kind.as_str() {
        "users" => {
            let email: Option<String> = sqlx::query_scalar(
                "select email from identity where id=$1 and erased_at is null for update",
            )
            .bind(input.id)
            .fetch_optional(&mut *tx)
            .await?;
            let email = email.ok_or(ApiError::NotFound)?;
            if input.confirmation != email {
                return Err(ApiError::BadRequest {
                    reason: "Type the account email to confirm".into(),
                });
            }
            if input.removed
                && (input.id == auth.identity_id || is_admin(&email, &state.admin_emails))
            {
                return Err(ApiError::BadRequest {
                    reason: "Administrator accounts must be managed through the server allowlist"
                        .into(),
                });
            }
            sqlx::query(
                "update identity set disabled_at=case when $2 then now() else null end, session_generation=session_generation+case when $2 then 1 else 0 end where id=$1",
            )
            .bind(input.id)
            .bind(input.removed)
            .execute(&mut *tx)
            .await?;
            if input.removed {
                sqlx::query("update refresh_token set revoked_at=now() where identity_id=$1 and revoked_at is null").bind(input.id).execute(&mut *tx).await?;
                sqlx::query("update public_document_view set revoked_at=now() where created_by=$1 and revoked_at is null").bind(input.id).execute(&mut *tx).await?;
            }
            if input.removed {
                "account.removed"
            } else {
                "account.restored"
            }
        }
        "documents" => {
            let title: Option<String> =
                sqlx::query_scalar("select title from document where id=$1 for update")
                    .bind(input.id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if title.ok_or(ApiError::NotFound)? != input.confirmation {
                return Err(ApiError::BadRequest {
                    reason: "Type the document title to confirm".into(),
                });
            }
            sqlx::query("update document set deleted_at=case when $2 then now() else null end,updated_at=now() where id=$1").bind(input.id).bind(input.removed).execute(&mut *tx).await?;
            if input.removed {
                sqlx::query("update public_document_view set revoked_at=now() where document_id=$1 and revoked_at is null").bind(input.id).execute(&mut *tx).await?;
            }
            if input.removed {
                "document.trashed_by_admin"
            } else {
                "document.restored_by_admin"
            }
        }
        _ => {
            return Err(ApiError::BadRequest {
                reason: "Choose a user or document".into(),
            })
        }
    };
    crate::product::audit(&mut tx, auth.identity_id, action, input.id, json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(overview, records, action))]
pub struct AdminApi;
