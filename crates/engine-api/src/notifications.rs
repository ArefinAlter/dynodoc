//! In-app notifications: one inbox per person across all documents.
//!
//! Rows hold identifiers and scores. Names, titles and file names are read when the
//! inbox is shown, so renaming reflects immediately and erasing an account or a
//! document does not leave copies of its details in other people's inboxes.
use crate::{auth::AuthContext, error::ApiError, AppState};
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(list))
        .route("/notifications/count", get(count))
        .route("/notifications/read", post(mark_read))
}

/// Queue a notification. People are never notified about their own actions, and
/// erased or disabled accounts receive nothing.
pub async fn notify(
    db: &mut PgConnection,
    recipient: Uuid,
    kind: &str,
    document: Option<Uuid>,
    actor: Option<Uuid>,
    subject: Option<Uuid>,
    payload: Value,
) -> Result<(), ApiError> {
    if Some(recipient) == actor {
        return Ok(());
    }
    sqlx::query("insert into notification(recipient_id,kind,document_id,actor_id,subject_id,payload) select $1,$2,$3,$4,$5,$6 where exists(select 1 from identity where id=$1 and disabled_at is null and erased_at is null)")
        .bind(recipient)
        .bind(kind)
        .bind(document)
        .bind(actor)
        .bind(subject)
        .bind(payload)
        .execute(&mut *db)
        .await?;
    Ok(())
}

/// Notify the owner and everyone given an editor or reviewer role on the document.
pub async fn notify_reviewers(
    db: &mut PgConnection,
    document: Uuid,
    actor: Uuid,
    kind: &str,
    subject: Option<Uuid>,
    payload: Value,
) -> Result<(), ApiError> {
    let people: Vec<Uuid> = sqlx::query_scalar("select created_by from document where id=$1 union select identity_id from document_access where document_id=$1 and role in ('author','approver')")
        .bind(document)
        .fetch_all(&mut *db)
        .await?;
    for person in people {
        notify(
            db,
            person,
            kind,
            Some(document),
            Some(actor),
            subject,
            payload.clone(),
        )
        .await?;
    }
    Ok(())
}

const ITEM_JSON: &str = "jsonb_build_object(
    'id',n.id,'kind',n.kind,'created_at',n.created_at,'read',n.read_at is not null,'payload',n.payload,
    'document',case when d.id is null then null else jsonb_build_object('id',d.id,'title',d.title,'kind',coalesce(d.settings->>'kind','questionnaire'),'role',document_member_role(d.id,$1)) end,
    'actor',case when a.id is null then null else jsonb_build_object('id',a.id,'name',a.display_name,'email',case when a.erased_at is null then a.email end) end,
    'source',(select jsonb_build_object('id',s.id,'title',s.title,'kind',coalesce(s.settings->>'kind','questionnaire'),'role',document_member_role(s.id,$1),
        'filename',(select u.filename from document_upload u where u.document_id=s.id and coalesce(u.report->>'kind','')<>'font' order by u.created_at limit 1))
      from document s where s.id=case when n.payload->>'source_document_id' ~ '^[0-9a-f-]{36}$' then (n.payload->>'source_document_id')::uuid end and s.deleted_at is null),
    'match',(select jsonb_build_object('id',m.id,'status',m.status,'relation',m.relation,'score',m.score) from document_match m where m.id=n.subject_id),
    'request',(select jsonb_build_object('id',w.id,'name',w.name,'status',case when w.merged_at is not null then 'merged' when w.review_outcome is not null then w.review_outcome when w.submitted_at is null then 'draft' else 'open' end) from workspace_draft w where w.id=n.subject_id)
)";

#[derive(Deserialize)]
struct ListQuery {
    before: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    unread: bool,
}
#[utoipa::path(get, path="/notifications", tag="notifications", params(("before" = Option<String>, Query, description = "Page before this time"), ("unread" = Option<bool>, Query, description = "Only unread")), security(("paseto" = [])), responses((status=200, description="Newest notifications first, 50 per page", body=Value),(status=401, description="Sign in required")))]
async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let items: Vec<Value> = sqlx::query_scalar(&format!("select {ITEM_JSON} from notification n left join document d on d.id=n.document_id left join identity a on a.id=n.actor_id where n.recipient_id=$1 and ($2::timestamptz is null or n.created_at<$2) and (not $3 or n.read_at is null) order by n.created_at desc limit 51"))
        .bind(auth.identity_id)
        .bind(query.before)
        .bind(query.unread)
        .fetch_all(&state.pool)
        .await?;
    let more = items.len() > 50;
    let unread: i64 = sqlx::query_scalar(
        "select count(*) from notification where recipient_id=$1 and read_at is null",
    )
    .bind(auth.identity_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(
        json!({"items":items.into_iter().take(50).collect::<Vec<_>>(),"more":more,"unread":unread}),
    ))
}

#[utoipa::path(get, path="/notifications/count", tag="notifications", security(("paseto" = [])), responses((status=200, description="Unread notification count", body=Value),(status=401, description="Sign in required")))]
async fn count(State(state): State<AppState>, auth: AuthContext) -> Result<Json<Value>, ApiError> {
    let unread: i64 = sqlx::query_scalar(
        "select count(*) from notification where recipient_id=$1 and read_at is null",
    )
    .bind(auth.identity_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({"unread":unread})))
}

#[derive(Deserialize)]
struct ReadRequest {
    #[serde(default)]
    ids: Vec<Uuid>,
    #[serde(default)]
    all: bool,
}
#[utoipa::path(post, path="/notifications/read", tag="notifications", request_body=Value, security(("paseto" = [])), responses((status=200, description="Marked as read", body=Value),(status=401, description="Sign in required")))]
async fn mark_read(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<ReadRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.ids.len() > 500 {
        return Err(ApiError::BadRequest {
            reason: "Mark up to 500 notifications at a time".into(),
        });
    }
    let updated = sqlx::query("update notification set read_at=now() where recipient_id=$1 and read_at is null and ($2 or id=any($3))")
        .bind(auth.identity_id)
        .bind(req.all)
        .bind(&req.ids)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({"updated":updated.rows_affected()})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(list, count, mark_read))]
pub struct NotificationApi;
