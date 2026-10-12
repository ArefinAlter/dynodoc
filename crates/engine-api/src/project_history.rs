//! Explicit shared candidates, reachable object reads and protected portable refs.
//! Centralized document events/editors are not advanced by these separate refs.
use crate::{
    access::{self, MemberRole, Policy},
    auth::{
        device::{HistoryRead, HistoryWrite, ProjectAuth},
        AuthContext,
    },
    devices,
    error::ApiError,
    product::audit,
    project_staging::{
        self, authorize, bad, content_error, history_error, start, PrivateStore, Stored, READ_LIMIT,
    },
    AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use engine_core::{
    repository::{
        attestation::{Receipt, Scope, SignedReceipt},
        server_authority, valid_branch, Head, SignedCommit,
    },
    shared_checkpoint::{self, typed, Address},
};
use ring::signature::KeyPair;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/:id/history/commits", post(publish))
        .route("/projects/:id/history/commits/:address", get(commit))
        .route(
            "/projects/:id/history/commits/:address/acknowledge",
            post(acknowledge),
        )
        .route(
            "/projects/:id/history/commits/:address/objects/:document",
            get(inventory),
        )
        .route(
            "/projects/:id/history/commits/:address/objects/:document/:object",
            get(object),
        )
        .route("/projects/:id/history/refs", get(refs).post(advance))
        .route("/projects/:id/history/reviews", get(reviews).post(review))
        .route("/projects/:id/history/storage", get(storage))
        .route("/projects/:id/history/authority", get(authority))
        .route("/projects/:id/history/receipts", get(receipts))
        .layer(DefaultBodyLimit::max(20 * 1024))
}

pub(crate) async fn load(
    db: &mut PgConnection,
    project: Uuid,
    address: &Address,
) -> Result<Stored, ApiError> {
    let (body, scope, height): (Vec<u8>, Vec<Uuid>, i64) = sqlx::query_as(
        "select body,scope,height from project_published_commit where project_id=$1 and address=$2",
    )
    .bind(project)
    .bind(address.as_str())
    .fetch_optional(db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let signed: SignedCommit =
        serde_json::from_slice(&body).map_err(|e| ApiError::Internal(e.to_string()))?;
    if signed.verify().map_err(history_error)? != *address || signed.commit.project != project {
        return Err(ApiError::Internal(
            "Published commit address/project mismatch".into(),
        ));
    }
    Ok(Stored {
        signed,
        scope,
        height,
    })
}
// Called only after document/space SHARE locks; membership/settings writers never
// take this advisory lock. Serializes shared mutations without upgrading a space
// lock (which would deadlock two contributors holding SHARE).
async fn serial(db: &mut PgConnection, project: Uuid) -> Result<(), ApiError> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text,367291))")
        .bind(project.to_string())
        .execute(db)
        .await?;
    Ok(())
}
/// A non-member gets 403 before any publication, receipt or review lookup can
/// answer 404/409 (no existence oracle). Lock-free on purpose: `authorize`, which
/// follows, keeps the document-then-space lock order and is the authoritative check.
async fn member(db: &mut PgConnection, auth: &AuthContext, project: Uuid) -> Result<(), ApiError> {
    crate::projects::require(db, project, auth.identity_id)
        .await
        .map(|_| ())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Publish {
    address: Address,
}
#[utoipa::path(operation_id="project_history_publish",post,path="/projects/{id}/history/commits",tag="project history",params(("id"=Uuid,Path)),request_body=Value,security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Explicit publication of own staged candidate and exactly its reachable objects; dependencies must already be published; no branch move",body=Value),(status=403,description="Current contributor access and active own signing key required"),(status=507,description="Project metadata budget reached")))]
async fn publish(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path(id): Path<Uuid>,
    Json(input): Json<Publish>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = start(&state, &auth).await?;
    member(&mut tx, &auth, id).await?;
    // An exact accepted retry is a read, including after device revocation.
    match load(&mut tx, id, &input.address).await {
        Ok(stored) => {
            authorize(&mut tx, &auth, id, &stored.scope, false).await?;
            tx.commit().await?;
            return Ok(Json(
                json!({"address":input.address,"visibility":"project","branch_changed":false}),
            ));
        }
        Err(ApiError::NotFound) => (),
        Err(error) => return Err(error),
    }
    let stored = project_staging::load(&mut tx, auth.identity_id, id, &input.address).await?;
    authorize(&mut tx, &auth, id, &stored.scope, true).await?;
    serial(&mut tx, id).await?;
    let _permit = state.workloads.enter(auth.identity_id, Some(id))?;
    let device =
        devices::current_key(&mut tx, auth.identity_id, &stored.signed.commit.author_key).await?;
    for dependency in stored.signed.commit.dependencies() {
        load(&mut tx, id, &dependency).await?;
    }
    // Traverse verified typed roots, never promote an account's upload inventory.
    let mut objects = PrivateStore {
        db: &mut tx,
        actor: auth.identity_id,
        project: id,
        budget: READ_LIMIT,
        prefetched: BTreeMap::new(),
        seen: BTreeSet::new(),
    };
    let mut reachable = Vec::new();
    for file in stored.signed.commit.files.values() {
        typed::read_state(
            &mut objects,
            file.document,
            &file.state,
            READ_LIMIT,
            Default::default(),
        )
        .await
        .map_err(content_error)?;
        reachable.extend(
            std::mem::take(&mut objects.seen)
                .into_iter()
                .map(|(doc, key)| (doc, file.state.clone(), key)),
        );
    }
    let parents: Vec<_> = stored
        .signed
        .commit
        .parents
        .iter()
        .map(Address::as_str)
        .collect();
    let body = serde_json::to_vec(&stored.signed).map_err(|e| ApiError::Internal(e.to_string()))?;
    sqlx::query("insert into project_published_commit(project_id,address,parents,body,scope,height,device_id,publisher) values($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(id).bind(input.address.as_str()).bind(parents).bind(body).bind(stored.scope).bind(stored.height).bind(device).bind(auth.identity_id).execute(&mut *tx).await?;
    // Bounded batches avoid a SQL statement/round trip for every reachable object.
    for batch in reachable.chunks(256) {
        let docs: Vec<_> = batch.iter().map(|(doc, _, _)| *doc).collect();
        let keys: Vec<_> = batch.iter().map(|(_, _, key)| key.as_str()).collect();
        let roots: Vec<_> = batch.iter().map(|(_, root, _)| root.as_str()).collect();
        sqlx::query("insert into project_published_object(project_id,state_root,document_id,address) select $1,u.r,u.d,u.a from unnest($2::text[],$3::uuid[],$4::text[]) as u(r,d,a) on conflict do nothing")
            .bind(id).bind(roots).bind(docs).bind(keys).execute(&mut *tx).await?;
    }
    audit(
        &mut tx,
        auth.identity_id,
        "project.commit_published",
        id,
        json!({"address":input.address}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"address":input.address,"visibility":"project","branch_changed":false}),
    ))
}
#[utoipa::path(operation_id="project_history_commit",get,path="/projects/{id}/history/commits/{address}",tag="project history",params(("id"=Uuid,Path),("address"=String,Path)),security(("paseto"=[]),("device_session"=["history:read"])),responses((status=200,description="Published signed envelope; entire transitive file scope requires current access",body=Value)))]
async fn commit(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryRead>,
    Path((id, address)): Path<(Uuid, Address)>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = start(&state, &auth).await?;
    member(&mut tx, &auth, id).await?;
    let stored = load(&mut tx, id, &address).await?;
    authorize(&mut tx, &auth, id, &stored.scope, false).await?;
    tx.commit().await?;
    Ok(Json(json!(stored.signed)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<Address>,
    limit: Option<i64>,
}
fn page_limit(limit: Option<i64>) -> Result<i64, ApiError> {
    let limit = limit.unwrap_or(64);
    if !(1..=256).contains(&limit) {
        return Err(bad("Page size must be 1 to 256"));
    }
    Ok(limit)
}
#[utoipa::path(operation_id="project_history_inventory",get,path="/projects/{id}/history/commits/{address}/objects/{document}",tag="project history",params(("id"=Uuid,Path),("address"=String,Path),("document"=Uuid,Path),("after"=Option<String>,Query),("limit"=Option<i64>,Query)),security(("paseto"=[]),("device_session"=["history:read"])),responses((status=200,description="Reachable object keys in this published file state only; maximum 256",body=Value)))]
async fn inventory(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryRead>,
    Path((id, address, document)): Path<(Uuid, Address, Uuid)>,
    Query(page): Query<Page>,
) -> Result<Json<Value>, ApiError> {
    let limit = page_limit(page.limit)?;
    let mut tx = start(&state, &auth).await?;
    member(&mut tx, &auth, id).await?;
    let stored = load(&mut tx, id, &address).await?;
    authorize(&mut tx, &auth, id, &stored.scope, false).await?;
    let root = stored
        .signed
        .commit
        .files
        .values()
        .find(|f| f.document.0 == document)
        .ok_or(ApiError::NotFound)?
        .state
        .as_str();
    let keys: Vec<String> = sqlx::query_scalar("select address from project_published_object where project_id=$1 and state_root=$2 and document_id=$3 and address>$4 order by address limit $5")
        .bind(id).bind(root).bind(document).bind(page.after.as_ref().map_or("",Address::as_str)).bind(limit).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"addresses":keys,"next":keys.last(),"visibility":"project"}),
    ))
}
#[utoipa::path(operation_id="project_history_object",get,path="/projects/{id}/history/commits/{address}/objects/{document}/{object}",tag="project history",params(("id"=Uuid,Path),("address"=String,Path),("document"=Uuid,Path),("object"=String,Path)),security(("paseto"=[]),("device_session"=["history:read"])),responses((status=200,description="Hash-verified base64 object reachable from this published file state",body=Value),(status=404,description="Not reachable from this publication")))]
async fn object(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryRead>,
    Path((id, address, document, object)): Path<(Uuid, Address, Uuid, Address)>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = start(&state, &auth).await?;
    member(&mut tx, &auth, id).await?;
    let stored = load(&mut tx, id, &address).await?;
    authorize(&mut tx, &auth, id, &stored.scope, false).await?;
    let root = stored
        .signed
        .commit
        .files
        .values()
        .find(|f| f.document.0 == document)
        .ok_or(ApiError::NotFound)?
        .state
        .as_str();
    let bytes: Vec<u8> = sqlx::query_scalar("select o.bytes from project_published_object p join checkpoint_object o using(document_id,address) where p.project_id=$1 and p.state_root=$2 and p.document_id=$3 and p.address=$4")
        .bind(id).bind(root).bind(document).bind(object.as_str()).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?;
    shared_checkpoint::validate_stored(&object, &bytes).map_err(content_error)?;
    tx.commit().await?;
    Ok(Json(
        json!({"address":object,"bytes":STANDARD.encode(bytes)}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Acknowledge {
    document: Uuid,
    addresses: Vec<Address>,
}
#[utoipa::path(operation_id="project_history_acknowledge",post,path="/projects/{id}/history/commits/{address}/acknowledge",tag="project history",params(("id"=Uuid,Path),("address"=String,Path)),request_body=Value,security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Privately acknowledge up to 256 objects reachable from an authorized published file state, without uploading their bytes again",body=Value),(status=404,description="An object is not reachable from this publication; nothing acknowledged"),(status=507,description="Private staging quota reached; nothing acknowledged")))]
async fn acknowledge(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path((id, address)): Path<(Uuid, Address)>,
    Json(input): Json<Acknowledge>,
) -> Result<Json<Value>, ApiError> {
    if input.addresses.is_empty() || input.addresses.len() > 256 {
        return Err(bad("Acknowledge 1 to 256 published objects"));
    }
    let mut tx = start(&state, &auth).await?;
    member(&mut tx, &auth, id).await?;
    let published = load(&mut tx, id, &address).await?;
    authorize(&mut tx, &auth, id, &published.scope, false).await?;
    authorize(&mut tx, &auth, id, &[input.document], true).await?;
    let root = published
        .signed
        .commit
        .files
        .values()
        .find(|f| f.document.0 == input.document)
        .ok_or(ApiError::NotFound)?
        .state
        .as_str();
    let keys: BTreeSet<_> = input.addresses.iter().map(Address::as_str).collect();
    let keys: Vec<_> = keys.into_iter().collect();
    // Immutable publication already verified these bytes. Only reached objects
    // qualify; staging and downloads still verify every content address on read.
    let rows:Vec<(String,i64)>=sqlx::query_as("select p.address,octet_length(o.bytes)::bigint from project_published_object p join checkpoint_object o using(document_id,address) where p.project_id=$1 and p.state_root=$2 and p.document_id=$3 and p.address=any($4)")
        .bind(id).bind(root).bind(input.document).bind(&keys).fetch_all(&mut *tx).await?;
    if rows.len() != keys.len() {
        return Err(ApiError::NotFound);
    }
    let addresses: Vec<_> = rows.iter().map(|r| r.0.as_str()).collect();
    let sizes: Vec<_> = rows.iter().map(|r| r.1).collect();
    sqlx::query("insert into project_staged_object(identity_id,project_id,document_id,address,charged_bytes) select $1,$2,$3,u.a,u.b from unnest($4::text[],$5::bigint[]) as u(a,b) on conflict do nothing")
        .bind(auth.identity_id).bind(id).bind(input.document).bind(addresses).bind(sizes).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"accepted":keys.len(),"published":false})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    branch: String,
    candidate: Address,
    expected: Option<Head>,
}
impl Target {
    fn validate(&self) -> Result<(), ApiError> {
        if !valid_branch(&self.branch)
            || self
                .expected
                .as_ref()
                .is_some_and(|h| h.generation < 1 || h.generation == i64::MAX)
        {
            return Err(bad("Invalid branch or expected head"));
        }
        Ok(())
    }
    fn generation(&self) -> i64 {
        self.expected.as_ref().map_or(0, |h| h.generation)
    }
    fn previous(&self) -> Option<&str> {
        self.expected.as_ref().map(|h| h.commit.as_str())
    }
}
async fn latest_receipt(
    db: &mut PgConnection,
    project: Uuid,
    branch: &str,
) -> Result<Option<SignedReceipt>, ApiError> {
    let row: Option<(String,i64,Vec<u8>,String)> = sqlx::query_as("select commit_address,generation,receipt,receipt_address from project_ref_event where project_id=$1 and branch=$2 order by generation desc limit 1")
        .bind(project).bind(branch).fetch_optional(&mut *db).await?;
    let Some(row) = row else { return Ok(None) };
    let binding = server_authority::binding(db).await?;
    Ok(Some(verify_receipt(project, branch, row, &binding)?))
}
fn verify_receipt(
    project: Uuid,
    branch: &str,
    row: (String, i64, Vec<u8>, String),
    binding: &(Uuid, String),
) -> Result<SignedReceipt, ApiError> {
    let signed: SignedReceipt =
        serde_json::from_slice(&row.2).map_err(|e| ApiError::Internal(e.to_string()))?;
    let address = signed
        .verify()
        .map_err(|_| ApiError::Internal("Invalid stored ref receipt".into()))?;
    if address.as_str() != row.3
        || signed.receipt.scope.project != project
        || signed.receipt.scope.branch != branch
        || signed.receipt.scope.authority != binding.0
        || signed.receipt.issuer_key != binding.1
        || signed.receipt.head.generation != row.1
        || signed.receipt.head.commit.as_str() != row.0
    {
        return Err(ApiError::Internal(
            "Ref receipt differs from its authority or stored head".into(),
        ));
    }
    Ok(signed)
}
async fn head(
    db: &mut PgConnection,
    project: Uuid,
    branch: &str,
) -> Result<Option<Head>, ApiError> {
    Ok(latest_receipt(db, project, branch)
        .await?
        .map(|r| r.receipt.head))
}
fn stale() -> ApiError {
    ApiError::Conflict {
        reason: "Branch changed; fetch and compare before retrying".into(),
    }
}
async fn role(db: &mut PgConnection, project: Uuid, actor: Uuid) -> Result<MemberRole, ApiError> {
    let stored: Option<String> = sqlx::query_scalar("select inherited_space_role($1,$2)")
        .bind(project)
        .bind(actor)
        .fetch_one(db)
        .await?;
    match stored.as_deref() {
        Some("owner") => Ok(MemberRole::Owner),
        Some("manager") => Ok(MemberRole::Manager),
        Some("editor") => Ok(MemberRole::Editor),
        Some("approver") => Ok(MemberRole::Reviewer),
        Some("reviewer") => Ok(MemberRole::Contributor),
        Some("viewer") => Ok(MemberRole::Viewer),
        _ => Err(ApiError::Forbidden),
    }
}
async fn rules(db: &mut PgConnection, project: Uuid, scope: &[Uuid]) -> Result<Policy, ApiError> {
    let (protect_team_version,required_approvals,merge_roles): (bool,i16,String) = sqlx::query_as("select protect_team_version,required_approvals,merge_roles from project_settings where space_id=$1")
        .bind(project).fetch_one(&mut *db).await?;
    let mut result = Policy {
        protect_team_version,
        required_approvals,
        merge_roles,
    };
    for doc in scope {
        result = result.with_floor(&access::policy(db, *doc).await?);
    }
    Ok(result)
}
// At most 10,000 unique parent edges/nodes, batched. Recovery source references
// are deliberately excluded: cherry-picking history does not merge its ancestry.
async fn fast_forward(
    db: &mut PgConnection,
    project: Uuid,
    before: &Address,
    after: &Address,
) -> Result<bool, ApiError> {
    let mut pending = BTreeSet::from([after.as_str().to_owned()]);
    let mut seen = BTreeSet::new();
    while !pending.is_empty() {
        if pending.contains(before.as_str()) {
            return Ok(true);
        }
        let batch: Vec<_> = pending.iter().take(256).cloned().collect();
        for key in &batch {
            pending.remove(key);
            seen.insert(key.clone());
        }
        if seen.len() > 10_000 {
            return Err(bad("Fast-forward check exceeds 10,000 commits"));
        }
        let parents: Vec<Vec<String>> = sqlx::query_scalar(
            "select parents from project_published_commit where project_id=$1 and address=any($2)",
        )
        .bind(project)
        .bind(&batch)
        .fetch_all(&mut *db)
        .await?;
        if parents.len() != batch.len() {
            return Err(ApiError::Conflict {
                reason: "Branch ancestry is unavailable after erasure".into(),
            });
        }
        for p in parents.into_iter().flatten() {
            if !seen.contains(&p) {
                pending.insert(p);
            }
        }
    }
    Ok(false)
}
async fn is_reviewer(
    db: &mut PgConnection,
    project: Uuid,
    scope: &[Uuid],
    actor: Uuid,
) -> Result<bool, ApiError> {
    // SHARE freezes disable/erasure through the final ref write. NOWAIT avoids
    // a cycle with a reviewer holding their account lock while waiting to write
    // this project. The caller retries a busy response with no ref change.
    let active: Option<bool> = sqlx::query_scalar(
        "select disabled_at is null and erased_at is null from identity where id=$1 for share nowait",
    )
    .bind(actor)
    .fetch_optional(&mut *db)
    .await.map_err(|e| {
        if matches!(&e, sqlx::Error::Database(d) if d.code().as_deref()==Some("55P03")) {
            ApiError::Busy
        } else {
            e.into()
        }
    })?;
    if active != Some(true) || !matches!(role(db,project,actor).await, Ok(r) if r.can_approve()) {
        return Ok(false);
    }
    let count: i64 = sqlx::query_scalar("select count(*) from document where id=any($1) and document_member_role(id,$2) in ('reviewer','editor','manager','owner')")
        .bind(scope).bind(actor).fetch_one(db).await?;
    Ok(count == scope.len() as i64)
}
async fn latest_reviews(
    db: &mut PgConnection,
    project: Uuid,
    target: &Target,
) -> Result<Vec<(Uuid, String)>, ApiError> {
    Ok(sqlx::query_as("select distinct on (reviewer) reviewer,decision from project_history_review where project_id=$1 and branch=$2 and candidate=$3 and expected_generation=$4 and expected_commit is not distinct from $5 order by reviewer,id desc limit 257")
        .bind(project).bind(&target.branch).bind(target.candidate.as_str()).bind(target.generation()).bind(target.previous()).fetch_all(db).await?)
}
#[utoipa::path(operation_id="project_history_advance",post,path="/projects/{id}/history/refs",tag="project history",params(("id"=Uuid,Path)),request_body=Value,security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Atomic fast-forward branch CAS; main enforces current project/file merge roles and approvals; no centralized document edits",body=Value),(status=409,description="Stale head, unavailable ancestry, non-fast-forward or unmet reviews")))]
async fn advance(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path(id): Path<Uuid>,
    Json(target): Json<Target>,
) -> Result<Json<Value>, ApiError> {
    target.validate()?;
    let mut tx = start(&state, &auth).await?;
    member(&mut tx, &auth, id).await?;
    let candidate = load(&mut tx, id, &target.candidate).await?;
    authorize(&mut tx, &auth, id, &candidate.scope, true).await?;
    serial(&mut tx, id).await?;
    let _permit = state.workloads.enter(auth.identity_id, Some(id))?;
    let current = head(&mut tx, id, &target.branch).await?;
    if current != target.expected {
        return Err(stale());
    }
    if let Some(current) = &current {
        if current.commit == target.candidate {
            return Err(bad("Branch already points to this commit"));
        }
        load(&mut tx, id, &current.commit)
            .await
            .map_err(|e| match e {
                ApiError::NotFound => ApiError::Conflict {
                    reason: "Branch head is unavailable after erasure".into(),
                },
                other => other,
            })?;
        if !fast_forward(&mut tx, id, &current.commit, &target.candidate).await? {
            return Err(ApiError::Conflict {
                reason: "Shared branches only fast-forward; create a merge or recovery commit"
                    .into(),
            });
        }
    }
    if target.branch == "main" {
        let policy = rules(&mut tx, id, &candidate.scope).await?;
        if !policy.can_merge(role(&mut tx, id, auth.identity_id).await?) {
            return Err(ApiError::Forbidden);
        }
        for doc in &candidate.scope {
            if !policy.can_merge(access::governance_role(&mut tx, *doc, auth.identity_id).await?) {
                return Err(ApiError::Forbidden);
            }
        }
        let publisher: Uuid = sqlx::query_scalar(
            "select publisher from project_published_commit where project_id=$1 and address=$2",
        )
        .bind(id)
        .bind(target.candidate.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let latest = latest_reviews(&mut tx, id, &target).await?;
        if latest.len() > 256 {
            return Err(bad("Too many reviewers"));
        }
        let mut approvals = 0;
        for (reviewer, decision) in latest {
            if reviewer == publisher
                || !is_reviewer(&mut tx, id, &candidate.scope, reviewer).await?
            {
                continue;
            }
            if decision == "request_changes" {
                return Err(ApiError::Conflict {
                    reason: "An eligible reviewer requested changes".into(),
                });
            }
            approvals += 1;
        }
        if approvals < policy.required_approvals {
            return Err(ApiError::Conflict {
                reason: format!(
                    "This candidate needs {} current independent approvals",
                    policy.required_approvals
                ),
            });
        }
    }
    let generation = target.generation() + 1;
    let signer=state.ref_signer.as_deref().ok_or_else(||ApiError::Denied { reason:"The operator must configure shared-branch receipt signing before branches can advance".into() })?;
    let (authority, public_key) = server_authority::binding(&mut tx).await?;
    if public_key != hex::encode(signer.public_key().as_ref()) {
        return Err(ApiError::Internal(
            "Receipt signing authority mismatch".into(),
        ));
    }
    let previous = latest_receipt(&mut tx, id, &target.branch)
        .await?
        .map(|r| r.verify().map_err(history_error))
        .transpose()?;
    let signed_receipt = SignedReceipt::sign(
        Receipt {
            version: 1,
            scope: Scope {
                authority,
                project: id,
                branch: target.branch.clone(),
            },
            head: Head {
                commit: target.candidate.clone(),
                generation,
            },
            previous,
            issued_at_ms: chrono::Utc::now().timestamp_millis(),
            issuer_key: String::new(),
        },
        signer,
    )
    .map_err(history_error)?;
    let receipt_address = signed_receipt.verify().map_err(history_error)?;
    let receipt =
        serde_json::to_vec(&signed_receipt).map_err(|e| ApiError::Internal(e.to_string()))?;
    sqlx::query("insert into project_ref_event(project_id,branch,generation,commit_address,previous_commit,actor,receipt,receipt_address) values($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(id).bind(&target.branch).bind(generation).bind(target.candidate.as_str()).bind(target.previous()).bind(auth.identity_id).bind(receipt).bind(receipt_address.as_str()).execute(&mut *tx).await?;
    audit(
        &mut tx,
        auth.identity_id,
        "project.ref_advanced",
        id,
        json!({"branch":target.branch,"generation":generation,"commit":target.candidate}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"branch":target.branch,"head":{"commit":target.candidate,"generation":generation},"signed_receipt":signed_receipt}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefPage {
    branch: Option<String>,
    after: Option<String>,
    limit: Option<i64>,
}
#[utoipa::path(operation_id="project_history_refs",get,path="/projects/{id}/history/refs",tag="project history",params(("id"=Uuid,Path),("branch"=Option<String>,Query),("after"=Option<String>,Query),("limit"=Option<i64>,Query)),security(("paseto"=[]),("device_session"=["history:read"])),responses((status=200,description="Current published branch heads, keyset page max 256; whole page fails closed when any head scope is unavailable",body=Value)))]
async fn refs(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryRead>,
    Path(id): Path<Uuid>,
    Query(page): Query<RefPage>,
) -> Result<Json<Value>, ApiError> {
    let limit = page_limit(page.limit)?;
    let mut tx = start(&state, &auth).await?;
    authorize(&mut tx, &auth, id, &[], false).await?;
    let rows: Vec<(String,String,i64,Vec<u8>,String)>=sqlx::query_as("select distinct on (branch) branch,commit_address,generation,receipt,receipt_address from project_ref_event where project_id=$1 and ($2::text is null or branch=$2) and branch>$3 order by branch,generation desc limit $4")
        .bind(id).bind(page.branch).bind(page.after.unwrap_or_default()).bind(limit).fetch_all(&mut *tx).await?;
    let mut scope = BTreeSet::new();
    let mut issued = Vec::new();
    let binding = if rows.is_empty() {
        None
    } else {
        Some(server_authority::binding(&mut tx).await?)
    };
    for (branch, address, generation, body, receipt_address) in &rows {
        issued.push(verify_receipt(
            id,
            branch,
            (
                address.clone(),
                *generation,
                body.clone(),
                receipt_address.clone(),
            ),
            binding.as_ref().expect("nonempty rows have authority"),
        )?);
        let address = Address::try_from(address.clone()).map_err(content_error)?;
        scope.extend(
            load(&mut tx, id, &address)
                .await
                .map_err(|e| match e {
                    ApiError::NotFound => ApiError::Conflict {
                        reason: "A branch head is unavailable after erasure".into(),
                    },
                    other => other,
                })?
                .scope,
        );
    }
    authorize(
        &mut tx,
        &auth,
        id,
        &scope.into_iter().collect::<Vec<_>>(),
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"items":rows.iter().map(|(branch,commit,generation,_,_)|json!({"branch":branch,"head":{"commit":commit,"generation":generation}})).collect::<Vec<_>>(),"next":rows.last().map(|r|&r.0),"receipts":issued}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    target: Target,
    decision: String,
}
#[utoipa::path(operation_id="project_history_review",post,path="/projects/{id}/history/reviews",tag="project history",params(("id"=Uuid,Path)),request_body=Value,security(("paseto"=[]),("device_session"=["history:write"])),responses((status=200,description="Append approve/request_changes for exact branch base generation and candidate; independent current reviewer required",body=Value),(status=409,description="Base changed")))]
async fn review(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryWrite>,
    Path(id): Path<Uuid>,
    Json(input): Json<Review>,
) -> Result<Json<Value>, ApiError> {
    input.target.validate()?;
    if !matches!(input.decision.as_str(), "approve" | "request_changes") {
        return Err(bad("Unknown review decision"));
    }
    let mut tx = start(&state, &auth).await?;
    member(&mut tx, &auth, id).await?;
    let candidate = load(&mut tx, id, &input.target.candidate).await?;
    authorize(&mut tx, &auth, id, &candidate.scope, false).await?;
    serial(&mut tx, id).await?;
    if head(&mut tx, id, &input.target.branch).await? != input.target.expected {
        return Err(stale());
    }
    let publisher: Uuid = sqlx::query_scalar(
        "select publisher from project_published_commit where project_id=$1 and address=$2",
    )
    .bind(id)
    .bind(input.target.candidate.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if publisher == auth.identity_id
        || !is_reviewer(&mut tx, id, &candidate.scope, auth.identity_id).await?
    {
        return Err(ApiError::Forbidden);
    }
    let latest = latest_reviews(&mut tx, id, &input.target).await?;
    if latest
        .iter()
        .any(|(who, decision)| *who == auth.identity_id && decision == &input.decision)
    {
        tx.commit().await?;
        return Ok(Json(json!({"recorded":true,"unchanged":true})));
    }
    if latest.len() >= 256 && !latest.iter().any(|(who, _)| *who == auth.identity_id) {
        return Err(bad("At most 256 reviewers per candidate/base"));
    }
    let review_id: i64=sqlx::query_scalar("insert into project_history_review(project_id,candidate,branch,expected_generation,expected_commit,reviewer,decision) values($1,$2,$3,$4,$5,$6,$7) returning id")
        .bind(id).bind(input.target.candidate.as_str()).bind(&input.target.branch).bind(input.target.generation()).bind(input.target.previous()).bind(auth.identity_id).bind(&input.decision).fetch_one(&mut *tx).await?;
    audit(
        &mut tx,
        auth.identity_id,
        "project.history_reviewed",
        id,
        json!({"review":review_id,"candidate":input.target.candidate,"decision":input.decision}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"recorded":true,"id":review_id})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewPage {
    candidate: Address,
    after: Option<i64>,
    limit: Option<i64>,
}
#[utoipa::path(operation_id="project_history_reviews",get,path="/projects/{id}/history/reviews",tag="project history",params(("id"=Uuid,Path),("candidate"=String,Query),("after"=Option<i64>,Query),("limit"=Option<i64>,Query)),security(("paseto"=[]),("device_session"=["history:read"])),responses((status=200,description="Bounded review history for a published candidate; no emails",body=Value)))]
async fn reviews(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryRead>,
    Path(id): Path<Uuid>,
    Query(page): Query<ReviewPage>,
) -> Result<Json<Value>, ApiError> {
    let limit = page_limit(page.limit)?;
    let mut tx = start(&state, &auth).await?;
    member(&mut tx, &auth, id).await?;
    let candidate = load(&mut tx, id, &page.candidate).await?;
    authorize(&mut tx, &auth, id, &candidate.scope, false).await?;
    let items:Vec<Value>=sqlx::query_scalar("select jsonb_build_object('id',id,'branch',branch,'expected_generation',expected_generation,'expected_commit',expected_commit,'reviewer',reviewer,'decision',decision,'created_at',created_at) from project_history_review where project_id=$1 and candidate=$2 and id>$3 order by id limit $4")
        .bind(id).bind(page.candidate.as_str()).bind(page.after.unwrap_or(0)).bind(limit).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"items":items,"next":items.last().map(|r|&r["id"])}),
    ))
}
#[utoipa::path(operation_id="project_history_storage",get,path="/projects/{id}/history/storage",tag="project history",params(("id"=Uuid,Path)),security(("paseto"=[]),("device_session"=["history:read"])),responses((status=200,description="Project published metadata budget; physical object bytes have separate document/creator limits",body=Value)))]
async fn storage(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryRead>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = start(&state, &auth).await?;
    authorize(&mut tx, &auth, id, &[], false).await?;
    let usage: Option<Value> = sqlx::query_scalar(
        "select to_jsonb(s)-'project_id' from project_history_storage s where project_id=$1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(usage.unwrap_or(json!({"bytes_used":0,"rows_used":0,"branches_used":0,"byte_limit":1073741824,"row_limit":1000000,"branch_limit":4096}))))
}
#[utoipa::path(operation_id="project_history_authority",get,path="/projects/{id}/history/authority",tag="project history",params(("id"=Uuid,Path)),security(("paseto"=[]),("device_session"=["history:read"])),responses((status=200,description="Installed public receipt authority; clients must verify this binding through an independently trusted channel",body=Value)))]
async fn authority(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryRead>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = start(&state, &auth).await?;
    authorize(&mut tx, &auth, id, &[], false).await?;
    let (authority, public_key) =
        server_authority::binding(&mut tx)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => ApiError::NotFound,
                other => other.into(),
            })?;
    tx.commit().await?;
    Ok(Json(
        json!({"authority":authority,"public_key":public_key,"independent_verification_required":true}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptPage {
    branch: String,
    after_generation: Option<i64>,
    limit: Option<i64>,
}
#[utoipa::path(operation_id="project_history_receipts",get,path="/projects/{id}/history/receipts",tag="project history",params(("id"=Uuid,Path),("branch"=String,Query),("after_generation"=Option<i64>,Query),("limit"=Option<i64>,Query)),security(("paseto"=[]),("device_session"=["history:read"])),responses((status=200,description="At most 256 consecutive signed ref receipts, oldest first; current head scope must remain accessible",body=Value),(status=409,description="Erased branch head or receipt gap")))]
async fn receipts(
    State(state): State<AppState>,
    auth: ProjectAuth<HistoryRead>,
    Path(id): Path<Uuid>,
    Query(page): Query<ReceiptPage>,
) -> Result<Json<Value>, ApiError> {
    let limit = page_limit(page.limit)?;
    let after = page.after_generation.unwrap_or(0);
    if after < 0 || !valid_branch(&page.branch) {
        return Err(bad("Invalid receipt cursor or branch"));
    }
    let mut tx = start(&state, &auth).await?;
    member(&mut tx, &auth, id).await?;
    let current = latest_receipt(&mut tx, id, &page.branch)
        .await?
        .ok_or(ApiError::NotFound)?;
    let candidate = load(&mut tx, id, &current.receipt.head.commit)
        .await
        .map_err(|e| match e {
            ApiError::NotFound => ApiError::Conflict {
                reason: "Branch head is unavailable after erasure".into(),
            },
            other => other,
        })?;
    authorize(&mut tx, &auth, id, &candidate.scope, false).await?;
    if after > current.receipt.head.generation {
        return Err(stale());
    }
    let binding = server_authority::binding(&mut tx).await?;
    let rows:Vec<(String,i64,Vec<u8>,String)>=sqlx::query_as("select commit_address,generation,receipt,receipt_address from project_ref_event where project_id=$1 and branch=$2 and generation>$3 and generation<=$4 order by generation limit $5")
        .bind(id).bind(&page.branch).bind(after).bind(current.receipt.head.generation).bind(limit).fetch_all(&mut *tx).await?;
    let mut previous_address = if after == 0 {
        None
    } else {
        let raw:(String,i64,Vec<u8>,String)=sqlx::query_as("select commit_address,generation,receipt,receipt_address from project_ref_event where project_id=$1 and branch=$2 and generation=$3")
            .bind(id).bind(&page.branch).bind(after).fetch_optional(&mut *tx).await?.ok_or_else(||ApiError::Conflict{reason:"Receipt cursor is unavailable".into()})?;
        Some(
            verify_receipt(id, &page.branch, raw, &binding)?
                .verify()
                .map_err(history_error)?,
        )
    };
    let mut items = Vec::new();
    for (index, row) in rows.into_iter().enumerate() {
        let signed = verify_receipt(id, &page.branch, row, &binding)?;
        if signed.receipt.head.generation != after + index as i64 + 1 {
            return Err(ApiError::Conflict {
                reason: "Stored receipt history has a gap".into(),
            });
        }
        if signed.receipt.previous != previous_address {
            return Err(ApiError::Internal("Stored receipt chain differs".into()));
        }
        previous_address = Some(signed.verify().map_err(history_error)?);
        items.push(signed);
    }
    if items.is_empty() && after < current.receipt.head.generation {
        return Err(ApiError::Conflict {
            reason: "Stored receipt history is unavailable".into(),
        });
    }
    tx.commit().await?;
    Ok(Json(
        json!({"next_generation":items.last().map(|r|r.receipt.head.generation),"items":items}),
    ))
}
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    publish,
    commit,
    inventory,
    object,
    advance,
    refs,
    review,
    reviews,
    storage,
    authority,
    receipts,
    acknowledge
))]
pub struct ProjectHistoryApi;
