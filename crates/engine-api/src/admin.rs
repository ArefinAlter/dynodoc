//! Read-only operations overview and bounded anonymous performance samples.
use crate::{auth::AuthContext, error::ApiError, AppState};
use axum::{
    extract::State,
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
}
pub fn is_admin(email: &str, configured: &str) -> bool {
    configured
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|s| s.eq_ignore_ascii_case(email))
}
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
    let counts: Value = sqlx::query_scalar("select jsonb_build_object('documents',(select count(*) from document),'events',(select count(*) from event),'versions',(select count(*) from document_version),'accounts',(select count(*) from identity),'uploads',(select count(*) from document_upload),'upload_bytes',(select coalesce(sum(octet_length(content)),0) from document_upload),'database_bytes',pg_database_size(current_database()),'connections',(select count(*) from pg_stat_activity where datname=current_database()),'migrations',(select count(*) from _sqlx_migrations where success))")
        .fetch_one(&state.pool).await?;
    let tables = sqlx::query("select relname, n_live_tup, pg_total_relation_size(relid) as bytes from pg_stat_user_tables order by pg_total_relation_size(relid) desc")
        .fetch_all(&state.pool).await?.iter().map(|r| json!({"name":r.get::<String,_>("relname"),"estimated_rows":r.get::<i64,_>("n_live_tup"),"bytes":r.get::<i64,_>("bytes")})).collect::<Vec<_>>();
    let traffic = sqlx::query("select host,page,count(*) as visits from web_metric where metric='visit' and recorded_at>now()-interval '30 days' group by host,page order by visits desc")
        .fetch_all(&state.pool).await?.iter().map(|r| json!({"host":r.get::<String,_>("host"),"page":r.get::<String,_>("page"),"visits":r.get::<i64,_>("visits")})).collect::<Vec<_>>();
    let vitals = sqlx::query("select host,metric,count(*) as samples,percentile_cont(0.75) within group(order by value) as p75 from web_metric where metric<>'visit' and recorded_at>now()-interval '30 days' group by host,metric order by host,metric")
        .fetch_all(&state.pool).await?.iter().map(|r| json!({"host":r.get::<String,_>("host"),"metric":r.get::<String,_>("metric"),"samples":r.get::<i64,_>("samples"),"p75":r.get::<f64,_>("p75")})).collect::<Vec<_>>();
    Ok(Json(
        json!({"checked_at":chrono::Utc::now(),"administrator":email,"counts":counts,"tables":tables,"traffic":traffic,"vitals":vitals}),
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
