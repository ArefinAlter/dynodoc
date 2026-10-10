//! Portable observed change bundles. Host/timestamps are claims; uploader and
//! reception time are server facts. Submission creates a review draft, never a merge.
use crate::{
    access::{self, MemberRole},
    auth::{random_token, sha256, AuthContext},
    error::ApiError,
    reviews, AppState,
};
use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{header, request::Parts},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use engine_shared::EventPayload;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/documents/:id/provenance",
            get(checkpoint)
                .post(push)
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route("/documents/:id/provenance/bundles/:bundle", get(bundle))
        .route("/documents/:id/connectors", get(grants).post(grant))
        .route("/documents/:id/connectors/:grant/revoke", post(revoke))
        .route("/connector/connection", get(connection))
        .route(
            "/connector/documents/:id/checkpoint",
            get(connector_checkpoint),
        )
        .route(
            "/connector/documents/:id/bundles",
            post(connector_push).layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
}
fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub host: String,
    pub mode: String,
    pub document_name: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub operation: EventPayload,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    pub bundle_id: Uuid,
    pub client_id: Uuid,
    pub document_id: Uuid,
    pub base_seq: i64,
    pub base_chain_hash: String,
    pub capture: Capture,
    pub changes: Vec<Change>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Push {
    pub name: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub submit: bool,
    pub bundle: Bundle,
}

pub(crate) async fn anchor(db: &mut PgConnection, id: Uuid, seq: i64) -> Result<String, ApiError> {
    if seq == 0 {
        return Ok("0".repeat(64));
    }
    sqlx::query_scalar("select encode(chain_hash,'hex') from event where document_id=$1 and seq=$2")
        .bind(id)
        .bind(seq)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| bad("The starting revision does not exist"))
}
// JSON object keys are sorted by serde_json's default map representation. Persist
// the typed request: input whitespace/key order cannot cause duplicate proposals.
pub(crate) async fn prepare(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    actor: Uuid,
    input: &Push,
) -> Result<(Vec<u8>, Option<Value>), ApiError> {
    let b = &input.bundle;
    if b.format != "dynodoc.change-bundle"
        || b.version != 1
        || b.document_id != id
        || b.base_seq < 0
        || ![
            "word",
            "google-docs",
            "excel",
            "google-sheets",
            "powerpoint",
            "google-slides",
            "portable",
        ]
        .contains(&b.capture.host.as_str())
        || !["observed_snapshot", "imported"].contains(&b.capture.mode.as_str())
        || b.capture.document_name.chars().count() > 500
        || b.changes.is_empty()
        || b.changes.len() > 20_000
    {
        return Err(bad(
            "Invalid version, document, capture description or changes",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    if b.changes.iter().any(|c| !ids.insert(c.id)) {
        return Err(bad("Change IDs must be unique inside a bundle"));
    }
    let body = serde_json::to_vec(&json!(input)).map_err(|e| ApiError::Internal(e.to_string()))?;
    let digest = sha256(&body);
    if let Some((previous_actor,previous_digest,receipt))=sqlx::query_as::<_,(Uuid,Vec<u8>,Value)>("select actor_id,digest,receipt from provenance_bundle where document_id=$1 and bundle_id=$2").bind(id).bind(b.bundle_id).fetch_optional(&mut **tx).await?{
        if previous_actor!=actor || previous_digest!=digest{return Err(ApiError::Conflict{reason:"This bundle ID was already used for a different upload".into()});}
        return Ok((digest,Some(receipt)));
    }
    if b.base_chain_hash != anchor(tx, id, b.base_seq).await? {
        return Err(ApiError::Conflict{reason:"The starting revision hash differs. Reconnect to the correct file; do not guess its history.".into()});
    }
    Ok((digest, None))
}
pub(crate) async fn record(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    actor: Uuid,
    draft: Uuid,
    input: &Push,
    digest: &[u8],
    receipt: &Value,
) -> Result<(), ApiError> {
    sqlx::query("insert into provenance_bundle(document_id,bundle_id,draft_id,actor_id,body,digest,receipt) values($1,$2,$3,$4,$5,$6,$7)")
        .bind(id).bind(input.bundle.bundle_id).bind(draft).bind(actor).bind(json!(input)).bind(digest).bind(receipt).execute(&mut **tx).await?;
    Ok(())
}
async fn propose(
    state: AppState,
    id: Uuid,
    auth: AuthContext,
    input: Push,
    credential: Option<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let req = reviews::CreateRequest {
        name: input.name.clone(),
        note: input.note.clone(),
        base_seq: input.bundle.base_seq,
        ops: input
            .bundle
            .changes
            .iter()
            .map(|c| c.operation.clone())
            .collect(),
        source: json!({"kind":"provenance","bundle_id":input.bundle.bundle_id,"host":input.bundle.capture.host,"capture_mode":input.bundle.capture.mode,"filename":input.bundle.capture.document_name}),
        submit: input.submit,
        provenance: Some(input),
        connector_grant: credential,
    };
    reviews::create(State(state), Path(id), auth, Json(req)).await
}
#[derive(Deserialize)]
struct CheckpointQuery {
    through_seq: Option<i64>,
}
async fn read_checkpoint(
    s: &AppState,
    id: Uuid,
    actor: Uuid,
    credential: Option<Uuid>,
    through_seq: Option<i64>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = s.pool.begin().await?;
    sqlx::query("select id from document where id=$1 for share")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    crate::projects::lock_document_project(&mut tx, id).await?;
    access::require_member(&mut tx, id, actor).await?;
    if let Some(grant) = credential {
        check_grant(&mut tx, grant, id, actor).await?;
    }
    let (title, kind): (String, String) = sqlx::query_as(
        "select title,coalesce(settings->>'kind','questionnaire') from document where id=$1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let head: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    let seq = through_seq.unwrap_or(head);
    if seq < 0 || seq > head {
        return Err(bad(
            "The requested checkpoint is outside this document's history",
        ));
    }
    let hash = anchor(&mut tx, id, seq).await?;
    let state = reviews::state_through(&mut tx, id, seq).await?;
    let proposals: Vec<Value> = sqlx::query_scalar("select jsonb_build_object('id',d.id,'status',case when d.merged_at is not null then 'merged' when d.review_outcome is not null then d.review_outcome when d.submitted_at is null then 'draft' else 'open' end) from workspace_draft d where d.document_id=$1 and d.created_by=$2 and d.source->>'kind'='provenance' order by d.created_at desc limit 100")
        .bind(id).bind(actor).fetch_all(&mut *tx).await?;
    Ok(Json(
        json!({"format":"dynodoc.checkpoint","version":1,"document_id":id,"title":title,"kind":kind,"through_seq":seq,"chain_hash":hash,"state":state,"proposals":proposals}),
    ))
}
#[utoipa::path(get,path="/documents/{id}/provenance",params(("id"=Uuid,Path),("through_seq"=Option<i64>,Query,description="Historical canonical revision; omitted means current head")),security(("paseto"=[])),responses((status=200,description="Consistent full checkpoint with stable block IDs",body=Value),(status=400,description="Revision outside history")))]
async fn checkpoint(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
    Query(query): Query<CheckpointQuery>,
) -> Result<Json<Value>, ApiError> {
    read_checkpoint(&s, id, a.identity_id, None, query.through_seq).await
}
#[utoipa::path(post,path="/documents/{id}/provenance",params(("id"=Uuid,Path)),request_body=Value,security(("paseto"=[])),responses((status=200,description="Durable idempotent review receipt",body=Value),(status=409,description="Wrong base or reused bundle ID")))]
async fn push(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
    Json(p): Json<Push>,
) -> Result<Json<Value>, ApiError> {
    propose(s, id, a, p, None).await
}
#[utoipa::path(get,path="/documents/{id}/provenance/bundles/{bundle}",params(("id"=Uuid,Path),("bundle"=Uuid,Path)),security(("paseto"=[])),responses((status=200,description="Preserved upload and server attribution; private until submitted",body=Value)))]
async fn bundle(
    State(s): State<AppState>,
    Path((id, bundle)): Path<(Uuid, Uuid)>,
    a: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut db = s.pool.acquire().await?;
    access::require_member(&mut db, id, a.identity_id).await?;
    let item:Value=sqlx::query_scalar("select jsonb_build_object('upload',b.body,'digest',encode(b.digest,'hex'),'uploaded_by',b.actor_id,'received_at',b.received_at,'receipt',b.receipt) from provenance_bundle b join workspace_draft d on d.id=b.draft_id where b.document_id=$1 and b.bundle_id=$2 and (d.submitted_at is not null or b.actor_id=$3)")
        .bind(id).bind(bundle).bind(a.identity_id).fetch_optional(&mut *db).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(item))
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct GrantInput {
    host: String,
    /// SHA-256 of a 256-bit secret held by the requesting editor. Never the secret.
    key_challenge: Option<String>,
    /// Approval link expiry as Unix seconds; at most ten minutes into the future.
    expires_at: Option<i64>,
}
#[utoipa::path(post,path="/documents/{id}/connectors",params(("id"=Uuid,Path)),request_body=GrantInput,security(("paseto"=[])),responses((status=200,description="Seven-day file-scoped credential; pairing approval returns no secret",body=Value)))]
async fn grant(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
    Json(p): Json<GrantInput>,
) -> Result<Json<Value>, ApiError> {
    let pairing_hash =
        match (&p.key_challenge, p.expires_at) {
            (None, None) => None,
            (Some(challenge), Some(expires))
                if challenge.len() == 64
                    && challenge
                        .bytes()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                    && expires > Utc::now().timestamp()
                    && expires <= Utc::now().timestamp() + 600 =>
            {
                Some(hex::decode(challenge).map_err(|_| bad("Invalid connection challenge"))?)
            }
            _ => return Err(bad(
                "This connection link is invalid or expired. Start sign-in again in the editor.",
            )),
        };
    if ![
        "word",
        "google-docs",
        "excel",
        "google-sheets",
        "powerpoint",
        "google-slides",
    ]
    .contains(&p.host.as_str())
    {
        return Err(bad("Choose a supported Microsoft or Google editor"));
    }
    let mut tx = s.pool.begin().await?;
    let kind: String = sqlx::query_scalar(
        "select coalesce(settings->>'kind','questionnaire') from document where id=$1 for update",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    crate::projects::lock_document_project(&mut tx, id).await?;
    if access::require_member(&mut tx, id, a.identity_id).await? < MemberRole::Contributor {
        return Err(ApiError::Forbidden);
    }
    let expected = match p.host.as_str() {
        "word" | "google-docs" => "document",
        "excel" | "google-sheets" => "spreadsheet",
        _ => "presentation",
    };
    if kind != expected {
        return Err(bad("Choose a connection for this file format"));
    }
    if let Some(hash) = &pairing_hash {
        let existing: Option<Value> = sqlx::query_scalar("select jsonb_build_object('id',id,'expires_at',expires_at,'host',host) from connector_grant where token_hash=$1 and document_id=$2 and identity_id=$3 and host=$4 and session_generation=$5 and revoked_at is null and expires_at>now()")
            .bind(hash).bind(id).bind(a.identity_id).bind(&p.host).bind(a.session_generation).fetch_optional(&mut *tx).await?;
        if let Some(grant) = existing {
            return Ok(Json(json!({"grant":grant,"document_id":id})));
        }
    }
    let active:i64=sqlx::query_scalar("select count(*) from connector_grant where document_id=$1 and identity_id=$2 and revoked_at is null and expires_at>now()").bind(id).bind(a.identity_id).fetch_one(&mut *tx).await?;
    if active >= 10 {
        return Err(bad("Revoke an existing connection before creating another"));
    }
    let (token, hash) = if let Some(hash) = pairing_hash {
        (None, hash)
    } else {
        let (token, hash) = random_token()?;
        (Some(token), hash)
    };
    let item:Value=sqlx::query_scalar("insert into connector_grant(document_id,identity_id,token_hash,session_generation,host) values($1,$2,$3,$4,$5) on conflict(token_hash) do nothing returning jsonb_build_object('id',id,'expires_at',expires_at,'host',host)")
        .bind(id).bind(a.identity_id).bind(hash).bind(a.session_generation).bind(&p.host).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError::Conflict { reason: "This connection was already used. Start sign-in again in the editor.".into() })?;
    crate::product::audit(
        &mut tx,
        a.identity_id,
        "document.connector_created",
        id,
        json!({"grant_id":item["id"],"host":p.host}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"grant":item,"token":token,"document_id":id})))
}

/// Completion needs the editor's secret, not the public approval challenge.
#[utoipa::path(get,path="/connector/connection",security(("connector_key"=[])),responses((status=200,description="Approved file and host for this scoped credential",body=Value),(status=401,description="Not approved, expired or revoked")))]
async fn connection(State(s): State<AppState>, c: Connector) -> Result<Json<Value>, ApiError> {
    let mut tx = s.pool.begin().await?;
    sqlx::query("select id from document where id=$1 for share")
        .bind(c.document)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    crate::projects::lock_document_project(&mut tx, c.document).await?;
    if access::require_member(&mut tx, c.document, c.auth.identity_id).await?
        < MemberRole::Contributor
    {
        return Err(ApiError::Forbidden);
    }
    check_grant(&mut tx, c.grant, c.document, c.auth.identity_id).await?;
    Ok(Json(
        json!({"document_id":c.document,"host":c.host,"grant_id":c.grant}),
    ))
}
#[utoipa::path(get,path="/documents/{id}/connectors",params(("id"=Uuid,Path)),security(("paseto"=[])),responses((status=200,description="Caller connections; no credentials",body=Value)))]
async fn grants(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    a: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut db = s.pool.acquire().await?;
    access::require_member(&mut db, id, a.identity_id).await?;
    let items:Vec<Value>=sqlx::query_scalar("select jsonb_build_object('id',id,'host',host,'created_at',created_at,'expires_at',expires_at,'revoked_at',revoked_at) from connector_grant where document_id=$1 and identity_id=$2 order by created_at desc limit 100").bind(id).bind(a.identity_id).fetch_all(&mut *db).await?;
    Ok(Json(json!({"items":items})))
}
#[utoipa::path(post,path="/documents/{id}/connectors/{grant}/revoke",params(("id"=Uuid,Path),("grant"=Uuid,Path)),security(("paseto"=[])),responses((status=200,description="Own connection revoked",body=Value)))]
async fn revoke(
    State(s): State<AppState>,
    Path((id, grant)): Path<(Uuid, Uuid)>,
    a: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut tx = s.pool.begin().await?;
    sqlx::query("select id from document where id=$1 for update")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    let changed=sqlx::query("update connector_grant set revoked_at=coalesce(revoked_at,now()) where id=$1 and document_id=$2 and identity_id=$3").bind(grant).bind(id).bind(a.identity_id).execute(&mut *tx).await?;
    if changed.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    crate::product::audit(
        &mut tx,
        a.identity_id,
        "document.connector_revoked",
        id,
        json!({"grant_id":grant}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"revoked":true})))
}
pub(crate) async fn check_grant(
    db: &mut PgConnection,
    grant: Uuid,
    doc: Uuid,
    actor: Uuid,
) -> Result<(), ApiError> {
    let valid:Option<Uuid>=sqlx::query_scalar("select g.id from connector_grant g join identity i on i.id=g.identity_id where g.id=$1 and g.document_id=$2 and g.identity_id=$3 and g.revoked_at is null and g.expires_at>now() and i.disabled_at is null and i.erased_at is null and i.session_generation=g.session_generation for share of g")
        .bind(grant).bind(doc).bind(actor).fetch_optional(db).await?;
    valid.ok_or(ApiError::Unauthorized)?;
    Ok(())
}
struct Connector {
    auth: AuthContext,
    document: Uuid,
    grant: Uuid,
    host: String,
}
#[axum::async_trait]
impl FromRequestParts<AppState> for Connector {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, s: &AppState) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .filter(|v| v.len() == 64)
            .ok_or(ApiError::Unauthorized)?;
        let (grant,document,identity,generation,host):(Uuid,Uuid,Uuid,i64,String)=sqlx::query_as("select g.id,g.document_id,g.identity_id,g.session_generation,g.host from connector_grant g join identity i on i.id=g.identity_id where g.token_hash=$1 and g.revoked_at is null and g.expires_at>now() and i.disabled_at is null and i.erased_at is null and i.session_generation=g.session_generation")
            .bind(sha256(token.as_bytes())).fetch_optional(&s.pool).await?.ok_or(ApiError::Unauthorized)?;
        Ok(Self {
            auth: AuthContext {
                identity_id: identity,
                session_generation: generation,
            },
            document,
            grant,
            host,
        })
    }
}
#[utoipa::path(get,path="/connector/documents/{id}/checkpoint",params(("id"=Uuid,Path),("through_seq"=Option<i64>,Query,description="Historical canonical revision; omitted means current head")),security(("connector_key"=[])),responses((status=200,description="File-scoped credential checkpoint",body=Value),(status=400,description="Revision outside history")))]
async fn connector_checkpoint(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    c: Connector,
    Query(query): Query<CheckpointQuery>,
) -> Result<Json<Value>, ApiError> {
    if c.document != id {
        return Err(ApiError::Forbidden);
    }
    read_checkpoint(&s, id, c.auth.identity_id, Some(c.grant), query.through_seq).await
}
#[utoipa::path(post,path="/connector/documents/{id}/bundles",params(("id"=Uuid,Path)),request_body=Value,security(("connector_key"=[])),responses((status=200,description="Scoped proposal; canonical history unchanged",body=Value)))]
async fn connector_push(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    c: Connector,
    Json(p): Json<Push>,
) -> Result<Json<Value>, ApiError> {
    if c.document != id || p.bundle.capture.host != c.host {
        return Err(ApiError::Forbidden);
    }
    propose(s, id, c.auth, p, Some(c.grant)).await
}
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        checkpoint,
        push,
        bundle,
        grant,
        grants,
        revoke,
        connection,
        connector_checkpoint,
        connector_push
    ),
    components(schemas(GrantInput))
)]
pub struct ProvenanceApi;
