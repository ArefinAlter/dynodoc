//! Private project exchange through the real router and restricted runtime role.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use engine_api::{
    app,
    auth::{store, token, AuthConfig},
    AppState,
};
use engine_core::{
    materializer::DocumentState,
    repository::{
        device::{Challenge, Proof},
        local::LocalRepository,
        Commit, FileEntry, SignedCommit,
    },
    shared_checkpoint::{typed, Address, Limits, MemoryStore, ObjectStore},
};
use engine_shared::DocumentId;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;
mod common;
const KEY: [u8; 32] = [9; 32];
fn router(pool: PgPool) -> Router {
    let mut state = AppState::new(
        pool,
        AuthConfig {
            paseto_key: KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    );
    state.ref_signer = Some(std::sync::Arc::new(
        Ed25519KeyPair::from_seed_unchecked(&[99; 32]).unwrap(),
    ));
    app(state)
}
async fn new_actor(pool: &PgPool, name: &str) -> (Uuid, String) {
    let id = store::upsert_identity(pool, &format!("{name}@example.test"), None)
        .await
        .unwrap()
        .id
        .0;
    (id, token::mint(&KEY, id, 3600).unwrap())
}
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    token: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
use tower::ServiceExt;
async fn ok(app: &Router, method: &str, path: &str, token: &str, body: Value) -> Value {
    let (status, result) = request(app, method, path, token, body).await;
    assert_eq!(status, StatusCode::OK, "{path}: {result}");
    result
}
async fn setup(pool: &PgPool) -> (Router, Uuid, String, Uuid, Uuid, Ed25519KeyPair) {
    let app = router(pool.clone());
    let (actor, login) = new_actor(pool, "owner").await;
    let project: Uuid = serde_json::from_value(
        ok(
            &app,
            "POST",
            "/projects",
            &login,
            json!({"name":"Portable"}),
        )
        .await["id"]
            .clone(),
    )
    .unwrap();
    let doc: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by,space_id) values('Report',$1,$2) returning id",
    )
    .bind(actor)
    .bind(project)
    .fetch_one(pool)
    .await
    .unwrap();
    let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let challenge: Challenge = serde_json::from_value(
        ok(
            &app,
            "POST",
            "/devices/challenge",
            &login,
            json!({"public_key":hex::encode(key.public_key().as_ref())}),
        )
        .await,
    )
    .unwrap();
    let proof = Proof::sign(challenge.clone(), challenge.authority, actor, &key).unwrap();
    ok(&app, "POST", "/devices", &login, json!(proof)).await;
    (app, actor, login, project, doc, key)
}
#[derive(Default)]
struct Collect {
    inner: MemoryStore,
    objects: BTreeMap<Address, Vec<u8>>,
}
impl ObjectStore for Collect {
    async fn put(
        &mut self,
        doc: DocumentId,
        address: &Address,
        bytes: &[u8],
    ) -> Result<bool, engine_core::shared_checkpoint::Error> {
        self.objects.insert(address.clone(), bytes.to_vec());
        self.inner.put(doc, address, bytes).await
    }
    async fn get(
        &mut self,
        doc: DocumentId,
        address: &Address,
    ) -> Result<Vec<u8>, engine_core::shared_checkpoint::Error> {
        self.inner.get(doc, address).await
    }
}
async fn graph(doc: Uuid) -> (Address, Vec<Value>) {
    let mut store = Collect::default();
    let (root, _) = typed::write_state(
        &mut store,
        DocumentId(doc),
        &document_state(),
        Limits::default(),
        Default::default(),
    )
    .await
    .unwrap();
    (
        root,
        store
            .objects
            .into_iter()
            .map(|(address, bytes)| json!({"address":address,"bytes":STANDARD.encode(bytes)}))
            .collect(),
    )
}
fn document_state() -> DocumentState {
    let node = engine_core::materializer::MaterializedNode {
        id: engine_shared::NodeId("01ARZ3NDEKTSV4RRFFQ69G5FAV".into()),
        parent_id: None,
        node_type: "paragraph".into(),
        pos: "a0".into(),
        var_name: None,
        deleted: false,
        current_fields: json!({"label":"Résumé: team Δ", "runs":[{"text":"Résumé: ","bold":true},{"text":"team Δ","italic":true}]}),
    };
    DocumentState {
        nodes: BTreeMap::from([(node.id.clone(), node)]),
        ..Default::default()
    }
}
fn signed(
    project: Uuid,
    doc: Uuid,
    root: Address,
    key: &Ed25519KeyPair,
    parents: Vec<Address>,
    message: &str,
) -> SignedCommit {
    SignedCommit::sign(
        Commit {
            version: 1,
            project,
            parents,
            files: BTreeMap::from([(
                "Report.docx".into(),
                FileEntry {
                    document: DocumentId(doc),
                    state: root,
                },
            )]),
            author_key: String::new(),
            authored_at_ms: 1,
            message: message.into(),
            recovery: None,
            same_file_resolutions: Vec::new(),
        },
        key,
    )
    .unwrap()
}
async fn upload(app: &Router, login: &str, project: Uuid, doc: Uuid, objects: &[Value]) {
    ok(
        app,
        "POST",
        &format!("/projects/{project}/staging/objects"),
        login,
        json!({"document":doc,"objects":objects}),
    )
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn private_multifile_transfer_round_trips_without_publishing_or_exposing_other_drafts(
    pool: PgPool,
) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, owner, login, project, doc, key) = setup(&runtime.pool).await;
    let second: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by,space_id) values('Budget',$1,$2) returning id",
    )
    .bind(owner)
    .bind(project)
    .fetch_one(&pool)
    .await
    .unwrap();
    let base = format!("/projects/{project}/staging");
    let (root, mut objects) = graph(doc).await;
    let (other, other_login) = new_actor(&runtime.pool, "other").await;
    sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,'editor')")
        .bind(project)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    // Existing private shared checkpoint bytes are not evidence of own upload.
    let mut tx = pool.begin().await.unwrap();
    typed::write_state(
        &mut engine_core::shared_checkpoint::postgres::PgStore::new(&mut tx),
        DocumentId(doc),
        &document_state(),
        Limits::default(),
        Default::default(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let missing = ok(
        &app,
        "POST",
        &format!("{base}/objects/missing"),
        &login,
        json!({"document":doc,"addresses":[root]}),
    )
    .await;
    assert_eq!(missing["missing"], json!([root]));
    let mut initial = signed(
        project,
        doc,
        root.clone(),
        &key,
        vec![],
        "Initial two files",
    );
    initial.commit.files.insert(
        "Budget.xlsx".into(),
        FileEntry {
            document: DocumentId(second),
            state: root.clone(),
        },
    );
    initial = SignedCommit::sign(initial.commit, &key).unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/commits"),
            &login,
            json!(initial)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    objects.reverse(); // Transport order does not matter; complete graph is checked later.
    upload(&app, &login, project, doc, &objects[..1]).await;
    upload(&app, &login, project, doc, &objects[1..]).await;
    upload(&app, &login, project, second, &objects).await;
    let usage = ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await;
    upload(&app, &login, project, doc, &objects).await;
    assert_eq!(
        usage,
        ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/objects/{doc}/{}", root.as_str()),
            &other_login,
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        ok(
            &app,
            "GET",
            &format!("{base}/objects/{doc}"),
            &other_login,
            Value::Null
        )
        .await["addresses"],
        json!([])
    );
    let address = initial.verify().unwrap();
    let accepted = ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!(initial),
    )
    .await;
    assert_eq!(accepted["published"], false);
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/commits/{}", address.as_str()),
            &other_login,
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from event")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from workspace_draft")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    // A second local repository downloads via the same account, validates the
    // original signature and full graph, then explicitly chooses its local ref.
    let path = std::env::temp_dir().join(format!("dynodoc-staging-{}.sqlite", Uuid::new_v4()));
    let replica = LocalRepository::open(&path, project).await.unwrap();
    for document in [doc, second] {
        let mut after = String::new();
        loop {
            let page = ok(
                &app,
                "GET",
                &format!("{base}/objects/{document}?limit=2{after}"),
                &login,
                Value::Null,
            )
            .await;
            let addresses = page["addresses"].as_array().unwrap();
            if addresses.is_empty() {
                break;
            }
            for address in addresses {
                let packet = ok(
                    &app,
                    "GET",
                    &format!("{base}/objects/{document}/{}", address.as_str().unwrap()),
                    &login,
                    Value::Null,
                )
                .await;
                replica
                    .import_objects(&[engine_core::repository::local::ObjectPacket {
                        key: engine_core::repository::local::ObjectKey {
                            document: DocumentId(document),
                            address: serde_json::from_value(address.clone()).unwrap(),
                        },
                        bytes: STANDARD.decode(packet["bytes"].as_str().unwrap()).unwrap(),
                    }])
                    .await
                    .unwrap();
            }
            after = format!("&after={}", page["next"].as_str().unwrap());
        }
    }
    let downloaded: SignedCommit = serde_json::from_value(
        ok(
            &app,
            "GET",
            &format!("{base}/commits/{}", address.as_str()),
            &login,
            Value::Null,
        )
        .await,
    )
    .unwrap();
    assert_eq!(replica.import_commit(&downloaded).await.unwrap(), address);
    assert!(replica.head("main").await.unwrap().is_none());
    replica
        .compare_and_swap("main", None, &address)
        .await
        .unwrap();
    assert_eq!(
        replica.read_state(DocumentId(doc), &root).await.unwrap(),
        document_state()
    );
    replica.close().await;
    // SQLx's SQLite worker can release its final handle just after Pool::close
    // completes on Windows. Retry only that sharing violation, never I/O errors
    // generally, and never hide a failed cleanup.
    for attempt in 0..20 {
        match std::fs::remove_file(&path) {
            Ok(()) => break,
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) && attempt < 19 => {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            Err(error) => panic!("owned staging test database cleanup: {error}"),
        }
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn device_revocation_preserves_accepted_history_but_blocks_new_commits_and_scope_is_current(
    pool: PgPool,
) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, actor, login, project, doc, key) = setup(&runtime.pool).await;
    let base = format!("/projects/{project}/staging");
    let (root, objects) = graph(doc).await;
    upload(&app, &login, project, doc, &objects).await;
    let initial = signed(project, doc, root.clone(), &key, vec![], "Initial");
    let first = initial.verify().unwrap();
    let child = signed(
        project,
        doc,
        root.clone(),
        &key,
        vec![first.clone()],
        "Later",
    );
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/commits"),
            &login,
            json!(child)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!(initial),
    )
    .await;
    ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!(child),
    )
    .await;
    let devices = ok(&app, "GET", "/devices", &login, Value::Null).await;
    let device = devices["keys"][0]["id"].as_str().unwrap();
    ok(
        &app,
        "POST",
        &format!("/devices/{device}/revoke"),
        &login,
        json!({}),
    )
    .await;
    ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!(initial),
    )
    .await;
    ok(
        &app,
        "GET",
        &format!("{base}/commits/{}", first.as_str()),
        &login,
        Value::Null,
    )
    .await;
    let fresh = signed(
        project,
        doc,
        root,
        &key,
        vec![first.clone()],
        "New after revocation",
    );
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/commits"),
            &login,
            json!(fresh)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("update document set deleted_at=now() where id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/commits/{}", first.as_str()),
            &login,
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("update document set deleted_at=null,space_id=null where id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/commits/{}", first.as_str()),
            &login,
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("update document set space_id=$2 where id=$1")
        .bind(doc)
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(project)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/objects/{doc}"),
            &login,
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn budgets_are_atomic_retries_are_free_and_audited_erasure_removes_descendants(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, actor, login, project, doc, key) = setup(&runtime.pool).await;
    let base = format!("/projects/{project}/staging");
    let (root, objects) = graph(doc).await;
    upload(&app, &login, project, doc, &objects[..1]).await;
    let first = ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await;
    let physical: i64 = sqlx::query_scalar("select count(*) from checkpoint_object")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("update project_staging_storage set byte_limit=bytes_used where identity_id=$1")
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    let rejected = request(
        &app,
        "POST",
        &format!("{base}/objects"),
        &login,
        json!({"document":doc,"objects":objects}),
    )
    .await;
    assert_eq!(
        rejected.0,
        StatusCode::INSUFFICIENT_STORAGE,
        "{}",
        rejected.1
    );
    assert_eq!(rejected.1["code"], "storage_limit");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from checkpoint_object")
            .fetch_one(&pool)
            .await
            .unwrap(),
        physical
    );
    assert_eq!(
        ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await["objects_used"],
        first["objects_used"]
    );
    upload(&app, &login, project, doc, &objects[..1]).await;
    sqlx::query("update project_staging_storage set byte_limit=4294967296,object_limit=objects_used where identity_id=$1").bind(actor).execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/objects"),
            &login,
            json!({"document":doc,"objects":objects})
        )
        .await
        .0,
        StatusCode::INSUFFICIENT_STORAGE
    );
    sqlx::query("update project_staging_storage set object_limit=1000000,commit_limit=0 where identity_id=$1").bind(actor).execute(&pool).await.unwrap();
    upload(&app, &login, project, doc, &objects).await;
    let initial = signed(project, doc, root, &key, vec![], "Root");
    let address = initial.verify().unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/commits"),
            &login,
            json!(initial)
        )
        .await
        .0,
        StatusCode::INSUFFICIENT_STORAGE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from operation_audit where action='project.commit_staged'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    sqlx::query("update project_staging_storage set commit_limit=100000 where identity_id=$1")
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!(initial),
    )
    .await;
    let mut empty = initial.commit.clone();
    empty.parents = vec![address.clone()];
    empty.files.clear();
    empty.message = "File removed, ancestor still contains it".into();
    let empty = SignedCommit::sign(empty, &key).unwrap();
    let descendant = empty.verify().unwrap();
    ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!(empty),
    )
    .await;
    for query in [
        "update project_staging_storage set byte_limit=0",
        "delete from project_staged_commit",
        "update project_staged_object set charged_bytes=1",
        "truncate project_staged_object",
        "select erase_project_staging()",
    ] {
        assert!(
            sqlx::query(query).execute(&runtime.pool).await.is_err(),
            "runtime allowed {query}"
        );
    }
    assert!(sqlx::query("delete from project_staged_commit")
        .execute(&pool)
        .await
        .is_err());
    sqlx::query("update document set deleted_at=now() where id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/commits/{}", descendant.as_str()),
            &login,
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let mut tx = runtime.pool.begin().await.unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'documents',$2,'owner_request','{}')").bind(doc).bind(actor).execute(&mut *tx).await.unwrap();
    sqlx::query("delete from document where id=$1")
        .bind(doc)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for table in [
        "project_staged_commit",
        "project_staged_object",
        "checkpoint_object",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("select count(*) from {table}"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    let usage = ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await;
    assert_eq!(usage["bytes_used"], 0);
    assert_eq!(usage["objects_used"], 0);
    assert_eq!(usage["commits_used"], 0);
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/commits/{}", descendant.as_str()),
            &login,
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn contributor_recovery_validation_and_account_erasure_respect_private_ownership(
    pool: PgPool,
) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, actor, login, project, doc, key) = setup(&runtime.pool).await;
    let base = format!("/projects/{project}/staging");
    let (root, objects) = graph(doc).await;
    // A reader cannot stage even if it owns this particular document.
    sqlx::query("update space_member set role='viewer' where identity_id=$1")
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/objects"),
            &login,
            json!({"document":doc,"objects":objects})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("update space_member set role='reviewer' where identity_id=$1")
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    // Exercise product contributor conversion on a document owned by someone else.
    let (other, _) = new_actor(&runtime.pool, "creator").await;
    sqlx::query("update document set created_by=$2 where id=$1")
        .bind(doc)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    upload(&app, &login, project, doc, &objects).await;
    let initial = signed(project, doc, root, &key, vec![], "Root");
    let address = initial.verify().unwrap();
    ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!(initial),
    )
    .await;
    let plan = engine_core::repository::recovery::plan(
        engine_core::repository::recovery::Kind::Revert,
        &initial.commit,
        &initial.commit,
        None,
    )
    .unwrap();
    let valid = plan
        .resolve(&BTreeMap::new(), 2, "Undo root".into())
        .unwrap();
    let mut invalid = valid.clone();
    invalid.files = initial.commit.files.clone();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/commits"),
            &login,
            json!(SignedCommit::sign(invalid, &key).unwrap())
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!(SignedCommit::sign(valid, &key).unwrap()),
    )
    .await;
    let scope: Vec<Uuid> =
        sqlx::query_scalar("select scope from project_staged_commit where address=$1")
            .bind(address.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(scope, vec![doc]);
    let mut tx = runtime.pool.begin().await.unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'users',$2,'owner_request','{}')").bind(actor).bind(other).execute(&mut *tx).await.unwrap();
    sqlx::query("update identity set erased_at=now(),disabled_at=now() where id=$1")
        .bind(actor)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_staged_commit")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_staged_object")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    // Shared bytes belong to the document; removing this account's private access
    // must not delete another creator's shared history.
    assert!(
        sqlx::query_scalar::<_, i64>("select count(*) from checkpoint_object")
            .fetch_one(&pool)
            .await
            .unwrap()
            > 0
    );
    assert_eq!(
        request(&app, "GET", &format!("{base}/storage"), &login, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn waiting_upload_rechecks_project_membership_and_concurrent_quota_spending_is_serial(
    pool: PgPool,
) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, actor, login, project, doc, _) = setup(&runtime.pool).await;
    let base = format!("/projects/{project}/staging");
    let (_, objects) = graph(doc).await;
    let mut removal = pool.begin().await.unwrap();
    sqlx::query("select id from access_space where id=$1 for update")
        .bind(project)
        .fetch_one(&mut *removal)
        .await
        .unwrap();
    let pending_app = app.clone();
    let pending_login = login.clone();
    let pending_objects = objects.clone();
    let endpoint = format!("{base}/objects");
    let pending = tokio::spawn(async move {
        request(
            &pending_app,
            "POST",
            &endpoint,
            &pending_login,
            json!({"document":doc,"objects":pending_objects}),
        )
        .await
    });
    let started = std::time::Instant::now();
    loop {
        let waiting:bool=sqlx::query_scalar("select exists(select 1 from pg_stat_activity where datname=current_database() and wait_event_type='Lock' and query like 'with recursive a as%')").fetch_one(&pool).await.unwrap();
        if waiting {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "upload did not wait for membership lock"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(project)
        .bind(actor)
        .execute(&mut *removal)
        .await
        .unwrap();
    removal.commit().await.unwrap();
    assert_eq!(pending.await.unwrap().0, StatusCode::FORBIDDEN);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_staged_object")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,'owner')")
        .bind(project)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    // One remaining logical acknowledgement slot, contested by two documents.
    sqlx::query("insert into project_staging_storage(identity_id,object_limit) values($1,1)")
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    let second: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by,space_id) values('Other',$1,$2) returning id",
    )
    .bind(actor)
    .bind(project)
    .fetch_one(&pool)
    .await
    .unwrap();
    let endpoint = format!("{base}/objects");
    let (left, right) = tokio::join!(
        request(
            &app,
            "POST",
            &endpoint,
            &login,
            json!({"document":doc,"objects":[objects[0]]})
        ),
        request(
            &app,
            "POST",
            &endpoint,
            &login,
            json!({"document":second,"objects":[objects[0]]})
        )
    );
    let mut statuses = [left.0.as_u16(), right.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 507]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from checkpoint_object")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await["objects_used"],
        1
    );
    runtime.close().await;
}

async fn publish_commit(
    app: &Router,
    login: &str,
    project: Uuid,
    signed: &SignedCommit,
) -> Address {
    ok(
        app,
        "POST",
        &format!("/projects/{project}/staging/commits"),
        login,
        json!(signed),
    )
    .await;
    let address = signed.verify().unwrap();
    ok(
        app,
        "POST",
        &format!("/projects/{project}/history/commits"),
        login,
        json!({"address":address}),
    )
    .await;
    address
}
async fn member(pool: &PgPool, project: Uuid, name: &str, role: &str) -> (Uuid, String) {
    let (id, login) = new_actor(pool, name).await;
    sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,$3)")
        .bind(project)
        .bind(id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    (id, login)
}
async fn enroll(app: &Router, actor: Uuid, login: &str, seed: u8) -> Ed25519KeyPair {
    let key = Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap();
    let challenge: Challenge = serde_json::from_value(
        ok(
            app,
            "POST",
            "/devices/challenge",
            login,
            json!({"public_key":hex::encode(key.public_key().as_ref())}),
        )
        .await,
    )
    .unwrap();
    let proof = Proof::sign(challenge.clone(), challenge.authority, actor, &key).unwrap();
    ok(app, "POST", "/devices", login, json!(proof)).await;
    key
}
#[sqlx::test(migrations = "../../migrations")]
async fn shared_publication_exposes_only_reachable_objects_and_supports_another_authors_descendant(
    pool: PgPool,
) {
    install_receipts(&pool).await;
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, _, login, project, doc, key) = setup(&runtime.pool).await;
    let (contributor, other) = member(&runtime.pool, project, "contributor", "reviewer").await;
    let other_key = enroll(&app, contributor, &other, 8).await;
    let (root, objects) = graph(doc).await;
    upload(&app, &login, project, doc, &objects).await;
    // An unrelated private graph for the very same document must not be promoted.
    let mut private = Collect::default();
    let (secret, _) = typed::write_state(
        &mut private,
        DocumentId(doc),
        &DocumentState::default(),
        Limits::default(),
        Default::default(),
    )
    .await
    .unwrap();
    let extra: Vec<_> = private
        .objects
        .iter()
        .map(|(address, bytes)| json!({"address":address,"bytes":STANDARD.encode(bytes)}))
        .collect();
    upload(&app, &login, project, doc, &extra).await;
    let base = format!("/projects/{project}/history");
    let initial = signed(project, doc, root.clone(), &key, vec![], "Initial version");
    let address = initial.verify().unwrap();
    ok(
        &app,
        "POST",
        &format!("/projects/{project}/staging/commits"),
        &login,
        json!(initial),
    )
    .await;
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/commits/{address}", address = address.as_str()),
            &other,
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    publish_commit(&app, &login, project, &initial).await;
    let before = ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await;
    // Retry is free and does not move any ref.
    ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!({"address":address}),
    )
    .await;
    assert_eq!(
        before,
        ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await
    );
    assert_eq!(
        ok(&app, "GET", &format!("{base}/refs"), &other, Value::Null).await["items"],
        json!([])
    );
    let fetched: SignedCommit = serde_json::from_value(
        ok(
            &app,
            "GET",
            &format!("{base}/commits/{address}", address = address.as_str()),
            &other,
            Value::Null,
        )
        .await,
    )
    .unwrap();
    assert_eq!(fetched.verify().unwrap(), address);
    let inventory = ok(
        &app,
        "GET",
        &format!(
            "{base}/commits/{address}/objects/{doc}?limit=256",
            address = address.as_str()
        ),
        &other,
        Value::Null,
    )
    .await;
    assert_eq!(
        inventory["addresses"].as_array().unwrap().len(),
        objects.len()
    );
    assert!(!inventory["addresses"]
        .as_array()
        .unwrap()
        .contains(&json!(secret)));
    assert_eq!(
        request(
            &app,
            "GET",
            &format!(
                "{base}/commits/{address}/objects/{doc}/{secret}",
                address = address.as_str(),
                secret = secret.as_str()
            ),
            &other,
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let mut replica = MemoryStore::default();
    for object in inventory["addresses"].as_array().unwrap() {
        let packet = ok(
            &app,
            "GET",
            &format!(
                "{base}/commits/{address}/objects/{doc}/{}",
                object.as_str().unwrap(),
                address = address.as_str()
            ),
            &other,
            Value::Null,
        )
        .await;
        let key: Address = serde_json::from_value(packet["address"].clone()).unwrap();
        replica
            .put(
                DocumentId(doc),
                &key,
                &STANDARD.decode(packet["bytes"].as_str().unwrap()).unwrap(),
            )
            .await
            .unwrap();
    }
    let reconstructed = typed::read_state(
        &mut replica,
        DocumentId(doc),
        &root,
        Limits::default(),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(reconstructed, document_state());
    // Staging accepts another author's published ancestry and acknowledges only
    // its reached objects. No content bytes need to travel back to the server.
    let path = format!("{base}/commits/{}/acknowledge", address.as_str());
    assert_eq!(
        request(
            &app,
            "POST",
            &path,
            &other,
            json!({"document":doc,"addresses":[root,secret]})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from project_staged_object where identity_id=$1"
        )
        .bind(contributor)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    sqlx::query("insert into project_staging_storage(identity_id,object_limit) values($1,0)")
        .bind(contributor)
        .execute(&pool)
        .await
        .unwrap();
    let acknowledgements = json!({"document":doc,"addresses":inventory["addresses"]});
    assert_eq!(
        request(&app, "POST", &path, &other, acknowledgements.clone())
            .await
            .0,
        StatusCode::INSUFFICIENT_STORAGE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from project_staged_object where identity_id=$1"
        )
        .bind(contributor)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    sqlx::query("update project_staging_storage set object_limit=1000000 where identity_id=$1")
        .bind(contributor)
        .execute(&pool)
        .await
        .unwrap();
    ok(&app, "POST", &path, &other, acknowledgements.clone()).await;
    let used: i64 =
        sqlx::query_scalar("select bytes_used from project_staging_storage where identity_id=$1")
            .bind(contributor)
            .fetch_one(&pool)
            .await
            .unwrap();
    ok(&app, "POST", &path, &other, acknowledgements).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select bytes_used from project_staging_storage where identity_id=$1"
        )
        .bind(contributor)
        .fetch_one(&pool)
        .await
        .unwrap(),
        used
    );
    assert_eq!(
        ok(
            &app,
            "POST",
            &path,
            &other,
            json!({"document":doc,"addresses":vec![root.clone();256]})
        )
        .await["accepted"],
        1
    );
    assert_eq!(
        request(
            &app,
            "POST",
            &path,
            &other,
            json!({"document":doc,"addresses":vec![root.clone();257]})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let child = signed(
        project,
        doc,
        root,
        &other_key,
        vec![address.clone()],
        "Contributor change",
    );
    let child_address = publish_commit(&app, &other, project, &child).await;
    let target = json!({"branch":"feature/team","candidate":child_address,"expected":null});
    ok(&app, "POST", &format!("{base}/refs"), &other, target).await;
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &other,
            json!({"branch":"main","candidate":child_address,"expected":null})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    ok(
        &app,
        "POST",
        &format!("{base}/refs"),
        &login,
        json!({"branch":"main","candidate":address,"expected":null}),
    )
    .await;
    ok(&app,"POST",&format!("{base}/refs"),&login,json!({"branch":"main","candidate":child_address,"expected":{"commit":address,"generation":1}})).await;
    assert_eq!(
        ok(
            &app,
            "GET",
            &format!("{base}/refs?branch=main"),
            &other,
            Value::Null
        )
        .await["items"][0]["head"]["generation"],
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from event")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn shared_main_reviews_bind_candidate_and_base_and_recheck_roles_and_file_floors(
    pool: PgPool,
) {
    install_receipts(&pool).await;
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, _, login, project, doc, key) = setup(&runtime.pool).await;
    let (reviewer, review_login) = member(&runtime.pool, project, "reviewer", "approver").await;
    let (_, editor) = member(&runtime.pool, project, "editor", "editor").await;
    let (root, objects) = graph(doc).await;
    upload(&app, &login, project, doc, &objects).await;
    let first = publish_commit(
        &app,
        &login,
        project,
        &signed(project, doc, root.clone(), &key, vec![], "base"),
    )
    .await;
    let base = format!("/projects/{project}/history");
    ok(
        &app,
        "POST",
        &format!("{base}/refs"),
        &login,
        json!({"branch":"main","candidate":first,"expected":null}),
    )
    .await;
    let a = publish_commit(
        &app,
        &login,
        project,
        &signed(project, doc, root.clone(), &key, vec![first.clone()], "a"),
    )
    .await;
    let b = publish_commit(
        &app,
        &login,
        project,
        &signed(project, doc, root.clone(), &key, vec![first.clone()], "b"),
    )
    .await;
    sqlx::query("update project_settings set required_approvals=1 where space_id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
    let target = json!({"branch":"main","candidate":a,"expected":{"commit":first,"generation":1}});
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &login,
            target.clone()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    // Even project owners cannot approve their own publication or skip approvals.
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/reviews"),
            &login,
            json!({"target":target,"decision":"approve"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    ok(
        &app,
        "POST",
        &format!("{base}/reviews"),
        &review_login,
        json!({"target":target,"decision":"request_changes"}),
    )
    .await;
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &login,
            target.clone()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    ok(
        &app,
        "POST",
        &format!("{base}/reviews"),
        &review_login,
        json!({"target":target,"decision":"approve"}),
    )
    .await;
    let usage = ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await;
    ok(
        &app,
        "POST",
        &format!("{base}/reviews"),
        &review_login,
        json!({"target":target,"decision":"approve"}),
    )
    .await;
    assert_eq!(
        usage,
        ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await
    );
    sqlx::query("update space_member set role='viewer' where space_id=$1 and identity_id=$2")
        .bind(project)
        .bind(reviewer)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &login,
            target.clone()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    sqlx::query("update space_member set role='approver' where space_id=$1 and identity_id=$2")
        .bind(project)
        .bind(reviewer)
        .execute(&pool)
        .await
        .unwrap();
    // An approval for a cannot approve b at the same base.
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &login,
            json!({"branch":"main","candidate":b,"expected":{"commit":first,"generation":1}})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    sqlx::query("insert into document_policy(document_id,protect_team_version,required_approvals,merge_roles) values($1,true,1,'owners')").bind(doc).execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &editor,
            target.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    ok(
        &app,
        "POST",
        &format!("{base}/refs"),
        &login,
        target.clone(),
    )
    .await;
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &login,
            target.clone()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/reviews"),
            &review_login,
            json!({"target":target,"decision":"approve"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    // A sibling cannot reset history even with the current generation.
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &login,
            json!({"branch":"main","candidate":b,"expected":{"commit":a,"generation":2}})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        ok(
            &app,
            "GET",
            &format!("{base}/reviews?candidate={a}", a = a.as_str()),
            &login,
            Value::Null
        )
        .await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn shared_metadata_quotas_rollback_publication_and_erasure_never_resets_ref_generations(
    pool: PgPool,
) {
    install_receipts(&pool).await;
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, actor, login, project, doc, key) = setup(&runtime.pool).await;
    let (_, review_login) = member(&runtime.pool, project, "reviewer", "approver").await;
    let (root, objects) = graph(doc).await;
    upload(&app, &login, project, doc, &objects).await;
    let signed = signed(project, doc, root, &key, vec![], "base");
    let first = signed.verify().unwrap();
    ok(
        &app,
        "POST",
        &format!("/projects/{project}/staging/commits"),
        &login,
        json!(signed),
    )
    .await;
    sqlx::query("insert into project_history_storage(project_id,row_limit) values($1,1)")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
    let base = format!("/projects/{project}/history");
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/commits"),
            &login,
            json!({"address":first})
        )
        .await
        .0,
        StatusCode::INSUFFICIENT_STORAGE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_published_commit")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await["rows_used"],
        0
    );
    sqlx::query("update project_history_storage set row_limit=1000000 where project_id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
    publish_commit(&app, &login, project, &signed).await;
    let target = json!({"branch":"main","candidate":first,"expected":null});
    ok(
        &app,
        "POST",
        &format!("{base}/reviews"),
        &review_login,
        json!({"target":target,"decision":"approve"}),
    )
    .await;
    sqlx::query("update project_history_storage set branch_limit=0 where project_id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &login,
            target.clone()
        )
        .await
        .0,
        StatusCode::INSUFFICIENT_STORAGE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_ref_event")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("update project_history_storage set branch_limit=4096 where project_id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
    ok(&app, "POST", &format!("{base}/refs"), &login, target).await;
    let mut empty = signed.commit.clone();
    empty.parents = vec![first.clone()];
    empty.files.clear();
    empty.message = "Remove file".into();
    let empty = SignedCommit::sign(empty, &key).unwrap();
    let last = publish_commit(&app, &login, project, &empty).await;
    ok(
        &app,
        "POST",
        &format!("{base}/refs"),
        &login,
        json!({"branch":"main","candidate":last,"expected":{"commit":first,"generation":1}}),
    )
    .await;
    for query in [
        "update project_history_storage set byte_limit=0",
        "delete from project_published_commit",
        "truncate project_published_object",
        "update project_ref_event set generation=7",
        "delete from project_ref_event",
        "select erase_published_project_history()",
    ] {
        assert!(
            sqlx::query(query).execute(&runtime.pool).await.is_err(),
            "runtime allowed {query}"
        );
    }
    assert!(sqlx::query("delete from project_published_commit")
        .execute(&pool)
        .await
        .is_err());
    sqlx::query("update document set deleted_at=now() where id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/commits/{last}", last = last.as_str()),
            &review_login,
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let mut tx = runtime.pool.begin().await.unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'documents',$2,'owner_request','{}')").bind(doc).bind(actor).execute(&mut *tx).await.unwrap();
    sqlx::query("delete from document where id=$1")
        .bind(doc)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for table in [
        "project_published_commit",
        "project_published_object",
        "project_history_review",
        "checkpoint_object",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("select count(*) from {table}"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            0,
            "{table}"
        );
    }
    assert_eq!(
        ok(&app, "GET", &format!("{base}/storage"), &login, Value::Null).await["rows_used"],
        2
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/refs?branch=main"),
            &login,
            Value::Null
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select max(generation) from project_ref_event")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn shared_ref_races_have_one_winner_and_revocation_erasure_preserve_accepted_history(
    pool: PgPool,
) {
    install_receipts(&pool).await;
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, actor, login, project, doc, key) = setup(&runtime.pool).await;
    let (editor, other) = member(&runtime.pool, project, "editor", "editor").await;
    let (root, objects) = graph(doc).await;
    upload(&app, &login, project, doc, &objects).await;
    let initial = signed(project, doc, root.clone(), &key, vec![], "base");
    let first = initial.verify().unwrap();
    let child = signed(
        project,
        doc,
        root.clone(),
        &key,
        vec![first.clone()],
        "child",
    );
    let child_address = child.verify().unwrap();
    let base = format!("/projects/{project}/history");
    for envelope in [&initial, &child] {
        ok(
            &app,
            "POST",
            &format!("/projects/{project}/staging/commits"),
            &login,
            json!(envelope),
        )
        .await;
    }
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/commits"),
            &login,
            json!({"address":child_address})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let first = publish_commit(&app, &login, project, &initial).await;
    let child_address = publish_commit(&app, &login, project, &child).await;
    let sibling = publish_commit(
        &app,
        &login,
        project,
        &signed(
            project,
            doc,
            root.clone(),
            &key,
            vec![first.clone()],
            "sibling",
        ),
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_published_object")
            .fetch_one(&pool)
            .await
            .unwrap(),
        objects.len() as i64,
        "identical file roots reuse their reachability index"
    );
    let unpublished = signed(
        project,
        doc,
        root,
        &key,
        vec![child_address.clone()],
        "still private",
    );
    ok(
        &app,
        "POST",
        &format!("/projects/{project}/staging/commits"),
        &login,
        json!(unpublished),
    )
    .await;
    let devices = ok(&app, "GET", "/devices", &login, Value::Null).await;
    ok(
        &app,
        "POST",
        &format!(
            "/devices/{}/revoke",
            devices["keys"][0]["id"].as_str().unwrap()
        ),
        &login,
        json!({}),
    )
    .await;
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/commits"),
            &login,
            json!({"address":unpublished.verify().unwrap()})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    // Already published content and exact retries remain accepted after revocation.
    ok(
        &app,
        "POST",
        &format!("{base}/commits"),
        &login,
        json!({"address":first}),
    )
    .await;
    ok(
        &app,
        "POST",
        &format!("{base}/refs"),
        &login,
        json!({"branch":"main","candidate":first,"expected":null}),
    )
    .await;
    let path = format!("{base}/refs");
    let a = json!({"branch":"main","candidate":child_address,"expected":{"commit":first,"generation":1}});
    let b = json!({"branch":"main","candidate":sibling,"expected":{"commit":first,"generation":1}});
    let (a, b) = tokio::join!(
        request(&app, "POST", &path, &login, a),
        request(&app, "POST", &path, &other, b)
    );
    let mut statuses = vec![a.0, b.0];
    statuses.sort();
    assert_eq!(
        statuses,
        vec![StatusCode::OK, StatusCode::CONFLICT],
        "{a:?} {b:?}"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_ref_event")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    // Account erasure clears private work, never destroys team publications.
    let mut tx = runtime.pool.begin().await.unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'users',$2,'owner_request','{}')").bind(actor).bind(editor).execute(&mut *tx).await.unwrap();
    sqlx::query("update identity set erased_at=now(),disabled_at=now() where id=$1")
        .bind(actor)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_staged_commit")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let fetched: SignedCommit = serde_json::from_value(
        ok(
            &app,
            "GET",
            &format!("{base}/commits/{}", first.as_str()),
            &other,
            Value::Null,
        )
        .await,
    )
    .unwrap();
    assert_eq!(fetched.verify().unwrap(), first);
    ok(
        &app,
        "GET",
        &format!("{base}/refs?branch=main"),
        &other,
        Value::Null,
    )
    .await;
    runtime.close().await;
}

async fn install_receipts(pool: &PgPool) {
    let key = Ed25519KeyPair::from_seed_unchecked(&[99; 32]).unwrap();
    engine_core::repository::server_authority::install(
        pool,
        &hex::encode(key.public_key().as_ref()),
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn shared_receipts_are_atomic_fail_closed_and_verify_in_the_client_pin(pool: PgPool) {
    use engine_core::repository::attestation::{Pin, SignedReceipt};
    install_receipts(&pool).await;
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (app, _, login, project, doc, key) = setup(&runtime.pool).await;
    let (_, reviewer) = member(&runtime.pool, project, "reviewer", "approver").await;
    let (root, objects) = graph(doc).await;
    upload(&app, &login, project, doc, &objects).await;
    let first = publish_commit(
        &app,
        &login,
        project,
        &signed(project, doc, root.clone(), &key, vec![], "base"),
    )
    .await;
    let second = publish_commit(
        &app,
        &login,
        project,
        &signed(project, doc, root, &key, vec![first.clone()], "second"),
    )
    .await;
    let base = format!("/projects/{project}/history");
    let target = json!({"branch":"main","candidate":first,"expected":null});
    let config = AuthConfig {
        paseto_key: KEY,
        service_key: None,
        token_ttl_seconds: 3600,
        public_base_url: "http://test.local".into(),
    };
    let unsigned = engine_api::app(AppState::new(runtime.pool.clone(), config.clone()));
    assert_eq!(
        request(
            &unsigned,
            "POST",
            &format!("{base}/refs"),
            &login,
            target.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let mut wrong = AppState::new(runtime.pool.clone(), config);
    wrong.ref_signer = Some(std::sync::Arc::new(
        Ed25519KeyPair::from_seed_unchecked(&[98; 32]).unwrap(),
    ));
    assert_eq!(
        request(
            &engine_api::app(wrong),
            "POST",
            &format!("{base}/refs"),
            &login,
            target.clone()
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_ref_event")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let first_response = ok(&app, "POST", &format!("{base}/refs"), &login, target).await;
    let first_receipt: SignedReceipt =
        serde_json::from_value(first_response["signed_receipt"].clone()).unwrap();
    first_receipt.verify().unwrap();
    // Trust this independently supplied test key, not the endpoint/receipt's own claim.
    let trusted_key = Ed25519KeyPair::from_seed_unchecked(&[99; 32]).unwrap();
    let trusted_public = hex::encode(trusted_key.public_key().as_ref());
    let pin = Pin::trust_checkpoint(
        &first_receipt.receipt.scope,
        &trusted_public,
        first_receipt.clone(),
    )
    .unwrap();
    let authority = ok(
        &app,
        "GET",
        &format!("{base}/authority"),
        &login,
        Value::Null,
    )
    .await;
    assert_eq!(authority["public_key"], trusted_public);
    assert_eq!(
        authority["authority"],
        json!(first_receipt.receipt.scope.authority)
    );
    let second_target =
        json!({"branch":"main","candidate":second,"expected":{"commit":first,"generation":1}});
    sqlx::query("update project_settings set required_approvals=1 where space_id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &login,
            second_target.clone()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    ok(
        &app,
        "POST",
        &format!("{base}/reviews"),
        &reviewer,
        json!({"target":second_target,"decision":"approve"}),
    )
    .await;
    sqlx::query("update project_history_storage set row_limit=rows_used where project_id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/refs"),
            &login,
            second_target.clone()
        )
        .await
        .0,
        StatusCode::INSUFFICIENT_STORAGE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_ref_event")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from operation_audit where action='project.ref_advanced'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    sqlx::query("update project_history_storage set row_limit=1000000 where project_id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
    let second_response = ok(&app, "POST", &format!("{base}/refs"), &login, second_target).await;
    let second_receipt: SignedReceipt =
        serde_json::from_value(second_response["signed_receipt"].clone()).unwrap();
    assert_eq!(
        second_receipt.receipt.previous,
        Some(first_receipt.verify().unwrap())
    );
    let proof = ok(
        &app,
        "GET",
        &format!("{base}/receipts?branch=main&after_generation=1&limit=1"),
        &login,
        Value::Null,
    )
    .await;
    let receipts: Vec<SignedReceipt> = serde_json::from_value(proof["items"].clone()).unwrap();
    assert_eq!(receipts, vec![second_receipt.clone()]);
    let advanced = pin.advance(&receipts).unwrap();
    assert_eq!(
        &advanced.receipt().receipt.head,
        &second_receipt.receipt.head
    );
    assert!(
        advanced.advance(&[first_receipt]).is_err(),
        "rollback must fail against saved client trust"
    );
    assert_eq!(
        ok(
            &app,
            "GET",
            &format!("{base}/receipts?branch=main&after_generation=2"),
            &login,
            Value::Null
        )
        .await["items"],
        json!([])
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{base}/receipts?branch=main&after_generation=3"),
            &login,
            Value::Null
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        ok(
            &app,
            "GET",
            &format!("{base}/refs?branch=main"),
            &login,
            Value::Null
        )
        .await["receipts"],
        json!([second_receipt])
    );
    assert!(sqlx::query(
        "insert into project_ref_authority(singleton,public_key) values(true,repeat('a',64))"
    )
    .execute(&runtime.pool)
    .await
    .is_err());
    runtime.close().await;
}
