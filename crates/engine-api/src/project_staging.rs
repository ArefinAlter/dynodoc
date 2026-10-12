//! Account-private, resumable project transfers. No shared ref is published here.
use crate::{
    auth::{
        device::{HistoryWrite, ProjectAuth},
        AuthContext,
    },
    devices,
    error::ApiError,
    AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use engine_core::{
    repository::{recovery, SignedCommit, MAX_COMMIT_BYTES},
    shared_checkpoint::{self, postgres::PgStore, typed, Address, Limits, ObjectStore},
};
use engine_shared::DocumentId;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgConnection, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const MAX_SCOPE: usize = 256;
pub(crate) const READ_LIMIT: Limits = Limits {
    max_objects: 65_536,
    max_bytes: 64 * 1024 * 1024,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/:id/staging/objects", post(upload))
        .route("/projects/:id/staging/objects/missing", post(missing))
        .route("/projects/:id/staging/objects/:document", get(inventory))
        .route(
            "/projects/:id/staging/objects/:document/:address",
            get(object),
        )
        .route("/projects/:id/staging/commits", post(stage))
        .route("/projects/:id/staging/commits/:address", get(commit))
        .route("/projects/:id/staging/storage", get(storage))
        .layer(DefaultBodyLimit::max(1_500_000))
}
pub(crate) fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}
pub(crate) fn content_error(error: shared_checkpoint::Error) -> ApiError {
    match error {
        shared_checkpoint::Error::Db(e) => e.into(),
        _ => bad("Invalid, unavailable or oversized staged object graph"),
    }
}
pub(crate) fn history_error(error: engine_core::repository::Error) -> ApiError {
    match error {
        engine_core::repository::Error::Db(e) => e.into(),
        _ => bad("Invalid signed project history or recovery"),
    }
}

/// Account first, documents in UUID order, then their immutable space ancestry.
/// Document SHARE locks also serialize Trash, moves and direct membership changes.
/// Each permission is read again after waiting for the locks.
pub(crate) async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    auth: &AuthContext,
    project: Uuid,
    scope: &[Uuid],
    writing: bool,
) -> Result<(), ApiError> {
    // A device session is bound to one project; the extractor already refused a
    // different route project, this keeps the invariant for every caller.
    if auth.device.as_ref().is_some_and(|d| d.project != project) {
        return Err(ApiError::Forbidden);
    }
    if scope.len() > MAX_SCOPE {
        return Err(bad(
            "Project history scope is limited to 256 historical files",
        ));
    }
    let locked: Vec<Uuid> =
        sqlx::query_scalar("select id from document where id=any($1) order by id for share")
            .bind(scope)
            .fetch_all(&mut **tx)
            .await?;
    if locked.len() != scope.len() {
        return Err(ApiError::NotFound);
    }
    // Parent links have no update route; space membership changes lock their space.
    sqlx::query("with recursive a as (select id,parent_id from access_space where id=$1 or id in (select space_id from document where id=any($2)) union select s.id,s.parent_id from access_space s join a on a.parent_id=s.id) select id from access_space where id in (select id from a) order by id for share")
        .bind(project).bind(scope).fetch_all(&mut **tx).await?;
    let role: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "select inherited_space_role(space_id,$2) from project_settings where space_id=$1",
    )
    .bind(project)
    .bind(auth.identity_id)
    .fetch_optional(&mut **tx)
    .await?
    .flatten();
    let role = role.ok_or(ApiError::Forbidden)?;
    if writing
        && !matches!(
            role.as_str(),
            "owner" | "manager" | "editor" | "reviewer" | "approver"
        )
    {
        return Err(ApiError::Forbidden);
    }
    let allowed: i64 = sqlx::query_scalar("select count(*) from document where id=any($1) and deleted_at is null and document_project(id)=$2 and document_member_role(id,$3) is not null and (not $4 or document_member_role(id,$3)<>'viewer')")
        .bind(scope).bind(project).bind(auth.identity_id).bind(writing)
        .fetch_one(&mut **tx).await?;
    if allowed != scope.len() as i64 {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}
pub(crate) async fn start(
    state: &AppState,
    auth: &AuthContext,
) -> Result<Transaction<'static, Postgres>, ApiError> {
    let mut tx = state.pool.begin().await?;
    devices::account_lock(&mut tx, auth).await?;
    Ok(tx)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Packet {
    address: Address,
    bytes: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Upload {
    document: Uuid,
    objects: Vec<Packet>,
}

#[utoipa::path(post,path="/projects/{id}/staging/objects",tag="project staging",params(("id"=Uuid,Path)),request_body=Value,security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Private upload acknowledged; base64 bytes, at most 64 objects and 1 MiB decoded; no ref published",body=Value),(status=400,description="Invalid batch"),(status=403,description="Current project/file contribution access required"),(status=507,description="Durable quota reached")))]
async fn upload(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path(id): Path<Uuid>,
    Json(input): Json<Upload>,
) -> Result<Json<Value>, ApiError> {
    if input.objects.is_empty() || input.objects.len() > 64 {
        return Err(bad("Upload 1 to 64 objects"));
    }
    let mut size = 0usize;
    let objects: Vec<_> = input
        .objects
        .into_iter()
        .map(|packet| {
            let bytes = STANDARD
                .decode(packet.bytes)
                .map_err(|_| bad("Invalid base64 object bytes"))?;
            size += bytes.len();
            if size > 1024 * 1024 {
                return Err(bad("Upload batch exceeds 1 MiB"));
            }
            Ok((packet.address, bytes))
        })
        .collect::<Result<_, ApiError>>()?;
    let mut tx = start(&state, &auth).await?;
    authorize(&mut tx, &auth, id, &[input.document], true).await?;
    let _permit = state.workloads.enter(auth.identity_id, Some(id))?;
    PgStore::new(&mut tx)
        .put_batch(DocumentId(input.document), &objects)
        .await
        .map_err(content_error)?;
    for (address, bytes) in &objects {
        sqlx::query("insert into project_staged_object(identity_id,project_id,document_id,address,charged_bytes) values($1,$2,$3,$4,$5) on conflict do nothing")
            .bind(auth.identity_id).bind(id).bind(input.document).bind(address.as_str()).bind(bytes.len() as i64).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    // Do not disclose whether another member's private bytes already existed.
    Ok(Json(json!({"accepted":objects.len(),"published":false})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Missing {
    document: Uuid,
    addresses: Vec<Address>,
}
#[utoipa::path(post,path="/projects/{id}/staging/objects/missing",tag="project staging",params(("id"=Uuid,Path)),request_body=Value,security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Missing from this account's uploads only, at most 256 addresses; never searches other drafts",body=Value)))]
async fn missing(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path(id): Path<Uuid>,
    Json(input): Json<Missing>,
) -> Result<Json<Value>, ApiError> {
    if input.addresses.len() > 256 {
        return Err(bad("Negotiate at most 256 objects"));
    }
    let mut tx = start(&state, &auth).await?;
    authorize(&mut tx, &auth, id, &[input.document], false).await?;
    let keys: Vec<_> = input.addresses.iter().map(Address::as_str).collect();
    let present: Vec<String> = sqlx::query_scalar("select address from project_staged_object where identity_id=$1 and project_id=$2 and document_id=$3 and address=any($4)")
        .bind(auth.identity_id).bind(id).bind(input.document).bind(&keys).fetch_all(&mut *tx).await?;
    let present: BTreeSet<_> = present.into_iter().collect();
    tx.commit().await?;
    Ok(Json(
        json!({"missing":keys.into_iter().filter(|k|!present.contains(*k)).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<Address>,
    limit: Option<i64>,
}
#[utoipa::path(get,path="/projects/{id}/staging/objects/{document}",tag="project staging",params(("id"=Uuid,Path),("document"=Uuid,Path),("after"=Option<String>,Query),("limit"=Option<i64>,Query)),security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Own upload inventory; bounded keyset page, maximum 256",body=Value)))]
async fn inventory(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path((id, document)): Path<(Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> Result<Json<Value>, ApiError> {
    let limit = page.limit.unwrap_or(64);
    if !(1..=256).contains(&limit) {
        return Err(bad("Page size must be 1 to 256"));
    }
    let mut tx = start(&state, &auth).await?;
    authorize(&mut tx, &auth, id, &[document], false).await?;
    let keys: Vec<String> = sqlx::query_scalar("select address from project_staged_object where identity_id=$1 and project_id=$2 and document_id=$3 and address>$4 order by address limit $5")
        .bind(auth.identity_id).bind(id).bind(document).bind(page.after.as_ref().map_or("",Address::as_str)).bind(limit).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"addresses":keys,"next":keys.last(),"visibility":"private"}),
    ))
}

/// Only acknowledged bytes, with a global budget across every file in a commit.
pub(crate) struct PrivateStore<'a> {
    pub(crate) db: &'a mut PgConnection,
    pub(crate) actor: Uuid,
    pub(crate) project: Uuid,
    pub(crate) budget: Limits,
    pub(crate) prefetched: BTreeMap<(Uuid, Address), Vec<u8>>,
    pub(crate) seen: BTreeSet<(Uuid, Address)>,
}
impl ObjectStore for PrivateStore<'_> {
    async fn prefetch(
        &mut self,
        document: DocumentId,
        addresses: &[Address],
    ) -> Result<(), shared_checkpoint::Error> {
        self.prefetched.clear();
        if addresses.len() > shared_checkpoint::READ_BATCH_OBJECTS {
            return Err(shared_checkpoint::Error::Limit("read batch objects"));
        }
        if addresses.is_empty() {
            return Ok(());
        }
        let keys: Vec<_> = addresses.iter().map(Address::as_str).collect();
        let rows:Vec<(String,Vec<u8>)>=sqlx::query_as("select o.address,o.bytes from project_staged_object a join checkpoint_object o using(document_id,address) where a.identity_id=$1 and a.project_id=$2 and a.document_id=$3 and a.address=any($4)")
            .bind(self.actor).bind(self.project).bind(document.0).bind(keys).fetch_all(&mut *self.db).await?;
        for (address, bytes) in rows {
            self.prefetched
                .insert((document.0, Address::try_from(address)?), bytes);
        }
        Ok(())
    }
    async fn put(
        &mut self,
        _: DocumentId,
        _: &Address,
        _: &[u8],
    ) -> Result<bool, shared_checkpoint::Error> {
        Err(shared_checkpoint::Error::Invalid("read-only staged store"))
    }
    async fn get(
        &mut self,
        document: DocumentId,
        address: &Address,
    ) -> Result<Vec<u8>, shared_checkpoint::Error> {
        if self.budget.max_objects == 0 {
            return Err(shared_checkpoint::Error::Limit("objects"));
        }
        let bytes = if let Some(bytes) = self.prefetched.get(&(document.0, address.clone())) {
            bytes.clone()
        } else {
            sqlx::query_scalar::<_,Vec<u8>>("select o.bytes from project_staged_object a join checkpoint_object o using(document_id,address) where a.identity_id=$1 and a.project_id=$2 and a.document_id=$3 and a.address=$4")
            .bind(self.actor).bind(self.project).bind(document.0).bind(address.as_str()).fetch_optional(&mut *self.db).await?
            .ok_or_else(||shared_checkpoint::Error::Missing(address.as_str().into()))?
        };
        self.budget.max_objects -= 1;
        self.budget.max_bytes = self
            .budget
            .max_bytes
            .checked_sub(bytes.len())
            .ok_or(shared_checkpoint::Error::Limit("bytes"))?;
        shared_checkpoint::validate_stored(address, &bytes)?;
        self.seen.insert((document.0, address.clone()));
        Ok(bytes)
    }
}
#[utoipa::path(get,path="/projects/{id}/staging/objects/{document}/{address}",tag="project staging",params(("id"=Uuid,Path),("document"=Uuid,Path),("address"=String,Path)),security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Own acknowledged object, base64 encoded and hash verified",body=Value),(status=404,description="Not in own uploads")))]
async fn object(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path((id, document, address)): Path<(Uuid, Uuid, Address)>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = start(&state, &auth).await?;
    authorize(&mut tx, &auth, id, &[document], false).await?;
    let bytes = PrivateStore {
        db: &mut tx,
        actor: auth.identity_id,
        project: id,
        budget: READ_LIMIT,
        prefetched: BTreeMap::new(),
        seen: BTreeSet::new(),
    }
    .get(DocumentId(document), &address)
    .await
    .map_err(|e| match e {
        shared_checkpoint::Error::Missing(_) => ApiError::NotFound,
        other => content_error(other),
    })?;
    tx.commit().await?;
    Ok(Json(
        json!({"address":address,"bytes":STANDARD.encode(bytes)}),
    ))
}

pub(crate) struct Stored {
    pub(crate) signed: SignedCommit,
    pub(crate) scope: Vec<Uuid>,
    pub(crate) height: i64,
}
pub(crate) async fn load(
    db: &mut PgConnection,
    actor: Uuid,
    project: Uuid,
    address: &Address,
) -> Result<Stored, ApiError> {
    let (body,scope,height):(Vec<u8>,Vec<Uuid>,i64)=sqlx::query_as("select body,scope,height from project_staged_commit where identity_id=$1 and project_id=$2 and address=$3")
        .bind(actor).bind(project).bind(address.as_str()).fetch_optional(db).await?.ok_or(ApiError::NotFound)?;
    let signed: SignedCommit =
        serde_json::from_slice(&body).map_err(|e| ApiError::Internal(e.to_string()))?;
    if signed.verify().map_err(history_error)? != *address || signed.commit.project != project {
        return Err(ApiError::Internal(
            "stored project commit differs from its address or project".into(),
        ));
    }
    Ok(Stored {
        signed,
        scope,
        height,
    })
}
#[utoipa::path(post,path="/projects/{id}/staging/commits",tag="project staging",params(("id"=Uuid,Path)),request_body=Value,security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Private signed commit accepted after all dependencies and typed file graphs verify; no branch published",body=Value),(status=400,description="Invalid signature, graph or recovery"),(status=403,description="Current account, key and contribution rights required"),(status=404,description="Missing private dependency"),(status=507,description="Durable quota reached")))]
async fn stage(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path(id): Path<Uuid>,
    Json(signed): Json<SignedCommit>,
) -> Result<Json<Value>, ApiError> {
    let address = signed.verify().map_err(history_error)?;
    if signed.commit.project != id {
        return Err(bad("Commit belongs to another project"));
    }
    let body = serde_json::to_vec(&signed).map_err(|e| ApiError::Internal(e.to_string()))?;
    if body.len() > MAX_COMMIT_BYTES + 1024 {
        return Err(bad("Commit envelope too large"));
    }
    let mut tx = start(&state, &auth).await?;
    let exists:bool=sqlx::query_scalar("select exists(select 1 from project_staged_commit where identity_id=$1 and project_id=$2 and address=$3)")
        .bind(auth.identity_id).bind(id).bind(address.as_str()).fetch_one(&mut *tx).await?;
    if exists {
        let stored = load(&mut tx, auth.identity_id, id, &address).await?;
        authorize(&mut tx, &auth, id, &stored.scope, false).await?;
        if stored.signed.commit != signed.commit {
            return Err(bad("Commit collision"));
        }
        tx.commit().await?;
        return Ok(Json(
            json!({"address":address,"visibility":"private","published":false}),
        ));
    }
    let mut scope: BTreeSet<Uuid> = signed.commit.files.values().map(|f| f.document.0).collect();
    let mut dependencies = BTreeMap::new();
    let mut height = 0;
    for dependency in signed.commit.dependencies() {
        let stored = match load(&mut tx, auth.identity_id, id, &dependency).await {
            Ok(stored) => stored,
            Err(ApiError::NotFound) => {
                crate::project_history::load(&mut tx, id, &dependency).await?
            }
            Err(error) => return Err(error),
        };
        scope.extend(stored.scope.iter());
        if signed.commit.parents.contains(&dependency) {
            height = height.max(stored.height + 1);
        }
        dependencies.insert(dependency, stored);
    }
    if height > 100_000 {
        return Err(bad("Private history height limit reached"));
    }
    let scope: Vec<_> = scope.into_iter().collect();
    authorize(&mut tx, &auth, id, &scope, true).await?;
    let _permit = state.workloads.enter(auth.identity_id, Some(id))?;
    let device = devices::current_key(&mut tx, auth.identity_id, &signed.commit.author_key).await?;
    if let Some(source) = &signed.commit.recovery {
        let plan = recovery::plan(
            source.kind,
            &dependencies[&source.commit].signed.commit,
            &dependencies[&signed.commit.parents[0]].signed.commit,
            source
                .mainline
                .as_ref()
                .map(|p| &dependencies[p].signed.commit),
        )
        .map_err(history_error)?;
        if plan.source != *source {
            return Err(bad("Recovery mainline differs"));
        }
        plan.validate_files(&signed.commit.files)
            .map_err(history_error)?;
    }
    let mut store = PrivateStore {
        db: &mut tx,
        actor: auth.identity_id,
        project: id,
        budget: READ_LIMIT,
        prefetched: BTreeMap::new(),
        seen: BTreeSet::new(),
    };
    for file in signed.commit.files.values() {
        typed::read_state(
            &mut store,
            file.document,
            &file.state,
            READ_LIMIT,
            Default::default(),
        )
        .await
        .map_err(content_error)?;
    }
    sqlx::query("insert into project_staged_commit(identity_id,project_id,address,body,scope,height,device_id) values($1,$2,$3,$4,$5,$6,$7)")
        .bind(auth.identity_id).bind(id).bind(address.as_str()).bind(body).bind(scope).bind(height).bind(device).execute(&mut *tx).await?;
    // No filenames or messages in the audit record (whole-document erasure).
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "project.commit_staged",
        id,
        json!({"address":address}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"address":address,"visibility":"private","published":false}),
    ))
}
#[utoipa::path(get,path="/projects/{id}/staging/commits/{address}",tag="project staging",params(("id"=Uuid,Path),("address"=String,Path)),security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Own signed commit; current access checked for its entire dependency scope, even after signing-key revocation",body=Value),(status=404,description="No such own staged commit")))]
async fn commit(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path((id, address)): Path<(Uuid, Address)>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = start(&state, &auth).await?;
    let stored = load(&mut tx, auth.identity_id, id, &address).await?;
    authorize(&mut tx, &auth, id, &stored.scope, false).await?;
    tx.commit().await?;
    Ok(Json(json!(stored.signed)))
}
#[utoipa::path(get,path="/projects/{id}/staging/storage",tag="project staging",params(("id"=Uuid,Path)),security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Own durable private-transfer usage across all projects; shared physical bytes have separate document/creator budgets",body=Value)))]
async fn storage(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = start(&state, &auth).await?;
    authorize(&mut tx, &auth, id, &[], false).await?;
    let usage:Option<Value>=sqlx::query_scalar("select jsonb_build_object('bytes_used',bytes_used,'objects_used',objects_used,'commits_used',commits_used,'byte_limit',byte_limit,'object_limit',object_limit,'commit_limit',commit_limit) from project_staging_storage where identity_id=$1")
        .bind(auth.identity_id).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(usage.unwrap_or(json!({"bytes_used":0,"objects_used":0,"commits_used":0,"byte_limit":4294967296i64,"object_limit":1000000,"commit_limit":100000}))))
}
#[derive(utoipa::OpenApi)]
#[openapi(paths(upload, missing, inventory, object, stage, commit, storage))]
pub struct ProjectStagingApi;
