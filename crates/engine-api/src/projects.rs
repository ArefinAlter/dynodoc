//! Invite-only repositories. Unsubmitted drafts remain private to their creator.
use crate::{auth::AuthContext, error::ApiError, product::audit, AppState};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list).post(create))
        .route("/projects/:id", get(detail))
        .route("/projects/:id/files", get(files))
        .route("/projects/:id/change-requests", get(requests))
        .route("/projects/:id/drafts", get(drafts))
        .route("/projects/:id/settings", post(settings))
        .route("/projects/:id/watch", post(watch))
}
fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}
/// Serialize inherited role/rule checks with project settings and member changes.
pub(crate) async fn lock_document_project(
    db: &mut PgConnection,
    document: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("select id from access_space where id=document_project($1) for share")
        .bind(document)
        .fetch_optional(db)
        .await?;
    Ok(())
}
async fn require(db: &mut PgConnection, id: Uuid, person: Uuid) -> Result<String, ApiError> {
    sqlx::query_scalar::<_, Option<String>>(
        "select inherited_space_role(id,$2) from access_space where id=$1 and kind='project'",
    )
    .bind(id)
    .bind(person)
    .fetch_optional(db)
    .await?
    .flatten()
    .ok_or(ApiError::Forbidden)
}
async fn lock(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    person: Uuid,
) -> Result<String, ApiError> {
    sqlx::query("select id from access_space where id=$1 and kind='project' for update")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    require(tx, id, person).await
}
const PROJECT: &str = "jsonb_build_object('id',s.id,'name',s.name,'description',p.description,'revision',p.revision,'role',inherited_space_role(s.id,$1),'policy',jsonb_build_object('protect_team_version',p.protect_team_version,'required_approvals',p.required_approvals,'merge_roles',p.merge_roles),'watch',coalesce((select level from project_watch where space_id=s.id and identity_id=$1),'ignore'))";
#[utoipa::path(get, path="/projects", security(("paseto"=[])), responses((status=200, description="Invited projects", body=Value)))]
async fn list(State(state): State<AppState>, auth: AuthContext) -> Result<Json<Value>, ApiError> {
    let items: Vec<Value> = sqlx::query_scalar(&format!("select {PROJECT} from access_space s join project_settings p on p.space_id=s.id where inherited_space_role(s.id,$1) is not null order by s.created_at desc,s.id limit 1000"))
        .bind(auth.identity_id).fetch_all(&state.pool).await?;
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
struct NewProject {
    name: String,
    #[serde(default)]
    description: String,
}
#[utoipa::path(post, path="/projects", request_body=Value, security(("paseto"=[])), responses((status=200, description="Project created", body=Value)))]
async fn create(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(input): Json<NewProject>,
) -> Result<Json<Value>, ApiError> {
    if input.name.trim().is_empty()
        || input.name.chars().count() > 160
        || input.description.chars().count() > 2000
    {
        return Err(bad(
            "Name the project in 1–160 characters; keep its description under 2,000 characters",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "insert into access_space(name,kind,created_by) values($1,'project',$2) returning id",
    )
    .bind(input.name.trim())
    .bind(auth.identity_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("insert into project_settings(space_id,description) values($1,$2)")
        .bind(id)
        .bind(input.description.trim())
        .execute(&mut *tx)
        .await?;
    sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,'owner')")
        .bind(id)
        .bind(auth.identity_id)
        .execute(&mut *tx)
        .await?;
    audit(&mut tx, auth.identity_id, "project.created", id, json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
#[utoipa::path(get, path="/projects/{id}", params(("id"=Uuid,Path)), security(("paseto"=[])), responses((status=200, description="Project and rules", body=Value)))]
async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut db = state.pool.acquire().await?;
    require(&mut db, id, auth.identity_id).await?;
    let item:Value=sqlx::query_scalar(&format!("select {PROJECT} from access_space s join project_settings p on p.space_id=s.id where s.id=$2"))
        .bind(auth.identity_id).bind(id).fetch_one(&mut *db).await?;
    Ok(Json(item))
}
#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    offset: i64,
}
const TREE: &str="with recursive tree as (select id from access_space where id=$1 union select s.id from access_space s join tree t on s.parent_id=t.id)";
async fn page(
    state: &AppState,
    id: Uuid,
    actor: Uuid,
    offset: i64,
    kind: &str,
) -> Result<Json<Value>, ApiError> {
    if !(0..=1_000_000).contains(&offset) {
        return Err(bad("Invalid page offset"));
    }
    let mut tx = state.pool.begin().await?;
    lock(&mut tx, id, actor).await?;
    let select=match kind {
        "files"=>"select jsonb_build_object('id',d.id,'title',d.title,'kind',coalesce(d.settings->>'kind','questionnaire'),'updated_at',d.updated_at,'role',document_member_role(d.id,$2),'latest_seq',(select coalesce(max(seq),0) from event where document_id=d.id)) from document d join tree t on t.id=d.space_id where d.deleted_at is null and document_member_role(d.id,$2) is not null order by d.updated_at desc,d.id limit 101 offset $3".to_owned(),
        "requests"=>format!("select {}||jsonb_build_object('document',jsonb_build_object('id',f.id,'title',f.title,'kind',coalesce(f.settings->>'kind','questionnaire'))) from workspace_draft d join document f on f.id=d.document_id join tree t on t.id=f.space_id join identity a on a.id=d.created_by left join identity r on r.id=d.reviewed_by where f.deleted_at is null and document_member_role(f.id,$2) is not null and d.submitted_at is not null order by d.submitted_at desc,d.id limit 101 offset $3",crate::reviews::REQUEST_JSON),
        _=>"select jsonb_build_object('id',d.id,'name',d.name,'base_seq',d.base_seq,'revision',d.revision,'created_at',d.created_at,'document',jsonb_build_object('id',f.id,'title',f.title,'kind',coalesce(f.settings->>'kind','questionnaire'))) from workspace_draft d join document f on f.id=d.document_id join tree t on t.id=f.space_id where f.deleted_at is null and document_member_role(f.id,$2) is not null and d.created_by=$2 and d.submitted_at is null and d.merged_at is null and d.review_outcome is null order by d.created_at desc,d.id limit 101 offset $3".into()
    };
    let items: Vec<Value> = sqlx::query_scalar(&format!("{TREE} {select}"))
        .bind(id)
        .bind(actor)
        .bind(offset)
        .fetch_all(&mut *tx)
        .await?;
    let more = items.len() > 100;
    Ok(Json(
        json!({"items":items.into_iter().take(100).collect::<Vec<_>>(),"more":more,"offset":offset}),
    ))
}
#[utoipa::path(get, path="/projects/{id}/files", params(("id"=Uuid,Path),("offset"=Option<i64>,Query)), security(("paseto"=[])), responses((status=200,description="Accessible project files",body=Value)))]
async fn files(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
    Query(p): Query<Page>,
) -> Result<Json<Value>, ApiError> {
    page(&s, id, a.identity_id, p.offset, "files").await
}
#[utoipa::path(get, path="/projects/{id}/change-requests", params(("id"=Uuid,Path),("offset"=Option<i64>,Query)), security(("paseto"=[])), responses((status=200,description="Submitted requests across files",body=Value)))]
async fn requests(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
    Query(p): Query<Page>,
) -> Result<Json<Value>, ApiError> {
    page(&s, id, a.identity_id, p.offset, "requests").await
}
#[utoipa::path(get, path="/projects/{id}/drafts", params(("id"=Uuid,Path),("offset"=Option<i64>,Query)), security(("paseto"=[])), responses((status=200,description="Only caller's private drafts",body=Value)))]
async fn drafts(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
    Query(p): Query<Page>,
) -> Result<Json<Value>, ApiError> {
    page(&s, id, a.identity_id, p.offset, "drafts").await
}
#[derive(Deserialize)]
struct Settings {
    name: String,
    description: String,
    revision: i64,
    protect_team_version: bool,
    required_approvals: i16,
    merge_roles: String,
}
#[utoipa::path(post, path="/projects/{id}/settings", params(("id"=Uuid,Path)),request_body=Value, security(("paseto"=[])), responses((status=200,description="Rules saved",body=Value),(status=409,description="Concurrent change")))]
async fn settings(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
    Json(p): Json<Settings>,
) -> Result<Json<Value>, ApiError> {
    if p.name.trim().is_empty()
        || p.name.chars().count() > 160
        || p.description.chars().count() > 2000
        || !(0..=5).contains(&p.required_approvals)
        || !["owners", "editors"].contains(&p.merge_roles.as_str())
    {
        return Err(bad("Check the project name, description and review rules"));
    }
    let mut tx = s.pool.begin().await?;
    let role = lock(&mut tx, id, a.identity_id).await?;
    if !["owner", "manager"].contains(&role.as_str()) {
        return Err(ApiError::Forbidden);
    }
    let revision:Option<i64>=sqlx::query_scalar("update project_settings set description=$2,protect_team_version=$3,required_approvals=$4,merge_roles=$5,revision=revision+1 where space_id=$1 and revision=$6 returning revision")
        .bind(id).bind(p.description.trim()).bind(p.protect_team_version).bind(p.required_approvals).bind(&p.merge_roles).bind(p.revision).fetch_optional(&mut *tx).await?;
    let revision = revision.ok_or_else(|| ApiError::Conflict {
        reason: "Project settings changed. Reload before saving.".into(),
    })?;
    sqlx::query("update access_space set name=$2 where id=$1")
        .bind(id)
        .bind(p.name.trim())
        .execute(&mut *tx)
        .await?;
    audit(&mut tx,a.identity_id,"project.settings_changed",id,json!({"revision":revision,"protect_team_version":p.protect_team_version,"required_approvals":p.required_approvals,"merge_roles":p.merge_roles})).await?;
    tx.commit().await?;
    Ok(Json(json!({"revision":revision})))
}
#[derive(Deserialize)]
struct Watch {
    level: String,
}
#[utoipa::path(post,path="/projects/{id}/watch",params(("id"=Uuid,Path)),request_body=Value,security(("paseto"=[])),responses((status=200,description="Submission notification preference saved",body=Value)))]
async fn watch(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
    Json(p): Json<Watch>,
) -> Result<Json<Value>, ApiError> {
    if !["requests", "ignore"].contains(&p.level.as_str()) {
        return Err(bad("Choose requests or ignore"));
    }
    let mut tx = s.pool.begin().await?;
    lock(&mut tx, id, a.identity_id).await?;
    sqlx::query("insert into project_watch(space_id,identity_id,level) values($1,$2,$3) on conflict(space_id,identity_id) do update set level=excluded.level").bind(id).bind(a.identity_id).bind(&p.level).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"level":p.level})))
}
#[derive(utoipa::OpenApi)]
#[openapi(paths(list, create, detail, files, requests, drafts, settings, watch))]
pub struct ProjectsApi;
