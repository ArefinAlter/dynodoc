//! Receipt-authority epochs through the real router, the restricted runtime login
//! and the API binary. Rotation, loss and compromise are operator actions (migration
//! login); the runtime login can only read the epoch chain.
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
        attestation::{Pin, Scope, SignedReceipt},
        authority_epoch::{Chain, Kind, Refusal, SignedTransition, Transition},
        device::{Challenge, Proof},
        server_authority, Commit, FileEntry, SignedCommit,
    },
    shared_checkpoint::{typed, Address, Limits, MemoryStore, ObjectStore},
};
use engine_shared::DocumentId;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::BTreeMap;
use tower::ServiceExt;
use uuid::Uuid;
mod common;
const KEY: [u8; 32] = [9; 32];

fn signer(seed: u8) -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap()
}
fn public(seed: u8) -> String {
    hex::encode(signer(seed).public_key().as_ref())
}
fn config() -> AuthConfig {
    AuthConfig {
        paseto_key: KEY,
        service_key: None,
        token_ttl_seconds: 3600,
        public_base_url: "http://test.local".into(),
    }
}
/// The API as a deployment with `seed`'s key mounted (or none).
fn router(pool: PgPool, seed: Option<u8>) -> Router {
    let mut state = AppState::new(pool, config());
    state.ref_signer = seed.map(|s| std::sync::Arc::new(signer(s)));
    app(state)
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
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn ok(app: &Router, method: &str, path: &str, token: &str, body: Value) -> Value {
    let (status, result) = request(app, method, path, token, body).await;
    assert_eq!(status, StatusCode::OK, "{path}: {result}");
    result
}

/// One project, one enrolled author key and three published commits in a line.
struct World {
    login: String,
    project: Uuid,
    commits: Vec<Address>,
    authority: Uuid,
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
fn state() -> DocumentState {
    let node = engine_core::materializer::MaterializedNode {
        id: engine_shared::NodeId("01ARZ3NDEKTSV4RRFFQ69G5FAV".into()),
        parent_id: None,
        node_type: "paragraph".into(),
        pos: "a0".into(),
        var_name: None,
        deleted: false,
        current_fields: json!({"label":"Rotation", "runs":[{"text":"Rotation"}]}),
    };
    DocumentState {
        nodes: BTreeMap::from([(node.id.clone(), node)]),
        ..Default::default()
    }
}
async fn world(runtime: &PgPool, app: &Router) -> World {
    let (actor, login) = {
        let id = store::upsert_identity(runtime, "owner@example.test", None)
            .await
            .unwrap()
            .id
            .0;
        (id, token::mint(&KEY, id, 3600).unwrap())
    };
    let project: Uuid = serde_json::from_value(
        ok(app, "POST", "/projects", &login, json!({"name":"Epochs"})).await["id"].clone(),
    )
    .unwrap();
    let doc: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by,space_id) values('Report',$1,$2) returning id",
    )
    .bind(actor)
    .bind(project)
    .fetch_one(runtime)
    .await
    .unwrap();
    let key = signer(7);
    let challenge: Challenge = serde_json::from_value(
        ok(
            app,
            "POST",
            "/devices/challenge",
            &login,
            json!({"public_key":public(7)}),
        )
        .await,
    )
    .unwrap();
    let authority = challenge.authority;
    let proof = Proof::sign(challenge.clone(), challenge.authority, actor, &key).unwrap();
    ok(app, "POST", "/devices", &login, json!(proof)).await;
    let mut store = Collect::default();
    let (root, _) = typed::write_state(
        &mut store,
        DocumentId(doc),
        &state(),
        Limits::default(),
        Default::default(),
    )
    .await
    .unwrap();
    let objects: Vec<Value> = store
        .objects
        .into_iter()
        .map(|(address, bytes)| json!({"address":address,"bytes":STANDARD.encode(bytes)}))
        .collect();
    ok(
        app,
        "POST",
        &format!("/projects/{project}/staging/objects"),
        &login,
        json!({"document":doc,"objects":objects}),
    )
    .await;
    let mut commits: Vec<Address> = Vec::new();
    for message in ["one", "two", "three"] {
        let signed = SignedCommit::sign(
            Commit {
                version: 1,
                project,
                parents: commits.last().cloned().into_iter().collect(),
                files: BTreeMap::from([(
                    "Report.docx".into(),
                    FileEntry {
                        document: DocumentId(doc),
                        state: root.clone(),
                    },
                )]),
                author_key: String::new(),
                authored_at_ms: 1,
                message: message.into(),
                recovery: None,
                same_file_resolutions: Vec::new(),
            },
            &key,
        )
        .unwrap();
        ok(
            app,
            "POST",
            &format!("/projects/{project}/staging/commits"),
            &login,
            json!(signed),
        )
        .await;
        let address = signed.verify().unwrap();
        ok(
            app,
            "POST",
            &format!("/projects/{project}/history/commits"),
            &login,
            json!({"address":address}),
        )
        .await;
        commits.push(address);
    }
    World {
        login,
        project,
        commits,
        authority,
    }
}
fn scope(world: &World) -> Scope {
    Scope {
        authority: world.authority,
        project: world.project,
        branch: "main".into(),
    }
}
/// POST /refs for main; `index` is the candidate, `generation` the current head.
async fn advance(
    app: &Router,
    w: &World,
    index: usize,
    expected: Option<(usize, i64)>,
) -> (StatusCode, Value) {
    let expected = expected.map(|(i, g)| json!({"commit":w.commits[i],"generation":g}));
    request(
        app,
        "POST",
        &format!("/projects/{}/history/refs", w.project),
        &w.login,
        json!({"branch":"main","candidate":w.commits[index],"expected":expected}),
    )
    .await
}
fn receipt_of(response: &Value) -> SignedReceipt {
    serde_json::from_value(response["signed_receipt"].clone()).unwrap()
}
async fn events(pool: &PgPool) -> i64 {
    sqlx::query_scalar("select count(*) from project_ref_event")
        .fetch_one(pool)
        .await
        .unwrap()
}
/// Operator-side statement for the current database state (receipt writers stopped).
async fn statement(owner: &PgPool, kind: Kind, old: u8, new: u8) -> SignedTransition {
    let state = server_authority::state(owner).await.unwrap();
    let transition = Transition {
        version: 1,
        kind,
        authority: state.authority,
        from_epoch: state.epoch,
        to_epoch: state.epoch + 1,
        old_key: public(old),
        new_key: public(new),
        sealed: state.sealed,
        reason: if kind == Kind::Recovery {
            "private key lost".into()
        } else {
            String::new()
        },
        declared_at_ms: 1_700_000_000_000,
    };
    match kind {
        Kind::Rotation => SignedTransition::sign_rotation(transition, &signer(old), &signer(new)),
        Kind::Recovery => SignedTransition::sign_recovery(transition, &signer(new)),
    }
    .unwrap()
}
async fn served(app: &Router, w: &World) -> Value {
    ok(
        app,
        "GET",
        &format!("/projects/{}/history/authority", w.project),
        &w.login,
        Value::Null,
    )
    .await
}
fn chain_of(value: &Value, authority: Uuid) -> Chain {
    let statements: Vec<SignedTransition> = value["rotations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| serde_json::from_value(item["statement"].clone()).unwrap())
        .collect();
    Chain::new(authority, statements).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn planned_rotation_keeps_old_receipts_verifiable_and_a_pinned_client_follows_it(
    pool: PgPool,
) {
    server_authority::install(&pool, &public(99)).await.unwrap();
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let old_api = router(runtime.pool.clone(), Some(99));
    let w = world(&runtime.pool, &old_api).await;
    // Epoch 1: the only key, reported as installed.
    let before = served(&old_api, &w).await;
    assert_eq!(before["epoch"], 1);
    assert_eq!(before["public_key"], public(99));
    assert_eq!(before["rotations"], json!([]));
    assert_eq!(before["genesis"]["public_key"], public(99));
    let r1 = receipt_of(&advance(&old_api, &w, 0, None).await.1);
    assert_eq!(r1.receipt.issuer_key, public(99));
    // A client pins the independently obtained epoch-1 key at generation 1.
    let pin = Pin::trust_checkpoint(&scope(&w), &public(99), r1.clone()).unwrap();

    // Operator rotates with writers stopped; the old key is refused afterwards.
    let rotation = statement(&pool, Kind::Rotation, 99, 98).await;
    let installed = server_authority::install_rotation(&pool, &rotation)
        .await
        .unwrap();
    assert_eq!(installed.epoch, 2);
    let (status, _) = advance(&old_api, &w, 1, Some((0, 1))).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "stale mounted key"
    );
    assert_eq!(events(&pool).await, 1);

    // The new key issues; the chain continues from the old receipt.
    let new_api = router(runtime.pool.clone(), Some(98));
    let second = advance(&new_api, &w, 1, Some((0, 1))).await;
    assert_eq!(second.0, StatusCode::OK, "{}", second.1);
    let r2 = receipt_of(&second.1);
    assert_eq!(r2.receipt.issuer_key, public(98));
    assert_eq!(r2.receipt.previous, Some(r1.verify().unwrap()));

    // Served chain: epoch, key, statement and the whole receipt history.
    let after = served(&new_api, &w).await;
    assert_eq!(after["epoch"], 2);
    assert_eq!(after["public_key"], public(98));
    assert_eq!(after["genesis"]["public_key"], public(99));
    assert_eq!(after["revocation"], Value::Null);
    let chain = chain_of(&after, w.authority);
    assert_eq!(chain.statements(), std::slice::from_ref(&rotation));
    let page = ok(
        &new_api,
        "GET",
        &format!(
            "/projects/{}/history/authority/rotations?from_epoch=1&limit=1",
            w.project
        ),
        &w.login,
        Value::Null,
    )
    .await;
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["epoch"], 2);
    assert_eq!(page["next_epoch"], Value::Null);
    let later = ok(
        &new_api,
        "GET",
        &format!("/projects/{}/history/authority?from_epoch=2", w.project),
        &w.login,
        Value::Null,
    )
    .await;
    assert_eq!(later["rotations"], json!([]));
    assert_eq!(
        request(
            &new_api,
            "GET",
            &format!("/projects/{}/history/authority?from_epoch=0", w.project),
            &w.login,
            Value::Null
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let receipts = ok(
        &new_api,
        "GET",
        &format!("/projects/{}/history/receipts?branch=main", w.project),
        &w.login,
        Value::Null,
    )
    .await;
    let items: Vec<SignedReceipt> = serde_json::from_value(receipts["items"].clone()).unwrap();
    assert_eq!(items, vec![r1.clone(), r2.clone()]);
    assert_eq!(receipts["epoch"], 2);
    assert_eq!(receipts["rotations"].as_array().unwrap().len(), 1);
    let refs = ok(
        &new_api,
        "GET",
        &format!("/projects/{}/history/refs?branch=main", w.project),
        &w.login,
        Value::Null,
    )
    .await;
    assert_eq!(refs["receipts"], json!([r2]));

    // The client with the old pin cannot move without the verified statement,
    // and follows the chain with it. Old receipts keep verifying on their own.
    assert!(pin.advance(std::slice::from_ref(&r2)).is_err());
    let (moved, used) = pin
        .advance_chain(&chain, std::slice::from_ref(&r2))
        .unwrap();
    assert_eq!(used, vec![rotation.clone()]);
    assert_eq!(moved.receipt(), &r2);
    assert_eq!(moved.trusted_key(), public(98));
    r1.verify().unwrap();
    // Replay and wrong-order statements stay refused after the fact.
    assert!(
        server_authority::install_rotation(&pool, &rotation)
            .await
            .unwrap()
            .already_installed
    );
    let third = statement(&pool, Kind::Rotation, 98, 97).await;
    server_authority::install_rotation(&pool, &third)
        .await
        .unwrap();
    assert!(server_authority::install_rotation(&pool, &rotation)
        .await
        .is_err());
    let mut wrong_old = statement(&pool, Kind::Rotation, 97, 96).await;
    wrong_old.transition.old_key = public(98);
    assert!(server_authority::install_rotation(&pool, &wrong_old)
        .await
        .is_err());
    // The runtime login can read the chain and nothing else about it.
    for sql in [
        "insert into project_ref_authority_epoch(epoch,public_key,kind,predecessor,statement,statement_address,sealed_receipts,sealed_digest) values(9,repeat('a',64),'rotation',3,'{}','x',0,repeat('0',64))",
        "update project_ref_authority_epoch set kind='genesis'",
        "delete from project_ref_authority_epoch",
        "insert into project_ref_authority_revocation(epoch,effective_at,reason) values(1,now(),'x')",
    ] {
        assert!(sqlx::query(sql).execute(&runtime.pool).await.is_err(), "{sql}");
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn rotation_and_issuance_race_has_one_winner_and_never_a_stale_key_receipt(pool: PgPool) {
    server_authority::install(&pool, &public(99)).await.unwrap();
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let old_api = router(runtime.pool.clone(), Some(99));
    let w = world(&runtime.pool, &old_api).await;
    let rotation = statement(&pool, Kind::Rotation, 99, 98).await;
    let (issued, installed) = tokio::join!(
        advance(&old_api, &w, 0, None),
        server_authority::install_rotation(&pool, &rotation)
    );
    // Either the receipt landed first (the seal in the statement is then stale) or
    // the rotation did (and the old key is refused). Never both, never neither.
    assert_ne!(
        issued.0 == StatusCode::OK,
        installed.is_ok(),
        "{issued:?} {installed:?}"
    );
    let epoch: i32 = sqlx::query_scalar("select max(epoch) from project_ref_authority_epoch")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(epoch, if installed.is_ok() { 2 } else { 1 });
    assert_eq!(events(&pool).await, i64::from(issued.0 == StatusCode::OK));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn revocation_stops_issuance_in_api_and_database_and_recovery_is_a_visible_break(
    pool: PgPool,
) {
    server_authority::install(&pool, &public(99)).await.unwrap();
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let old_api = router(runtime.pool.clone(), Some(99));
    let w = world(&runtime.pool, &old_api).await;
    let r1 = receipt_of(&advance(&old_api, &w, 0, None).await.1);
    let pin = Pin::trust_checkpoint(&scope(&w), &public(99), r1.clone()).unwrap();

    server_authority::revoke(&pool, "key leaked", None, None)
        .await
        .unwrap();
    // The API refuses; so does the database for anyone holding the runtime login.
    let refused = advance(&old_api, &w, 1, Some((0, 1))).await;
    assert_eq!(refused.0, StatusCode::FORBIDDEN, "{}", refused.1);
    assert_eq!(events(&pool).await, 1);
    let forged = serde_json::to_vec(&json!({"receipt":{"issuer_key":public(99)}})).unwrap();
    let direct = sqlx::query("insert into project_ref_event(project_id,branch,generation,commit_address,receipt,receipt_address,actor) values($1,'side',1,$2,$3,$2,$4)")
        .bind(w.project)
        .bind(w.commits[0].as_str())
        .bind(forged)
        .bind(
            sqlx::query_scalar::<_, Uuid>("select id from identity limit 1")
                .fetch_one(&pool)
                .await
                .unwrap(),
        )
        .execute(&runtime.pool)
        .await;
    assert!(
        format!("{:?}", direct.unwrap_err()).contains("revoked"),
        "the trigger names the revocation"
    );
    // A revoked epoch cannot sign its own successor; only a recovery is possible.
    let forged_rotation = statement(&pool, Kind::Rotation, 99, 98).await;
    assert!(server_authority::install_rotation(&pool, &forged_rotation)
        .await
        .is_err());
    let marker_view = served(&old_api, &w).await;
    assert_eq!(marker_view["revocation"]["epoch"], 1);
    assert_eq!(marker_view["revocation"]["reason"], "key leaked");
    // Reads keep working: the stored history is untouched.
    assert_eq!(
        ok(
            &old_api,
            "GET",
            &format!("/projects/{}/history/receipts?branch=main", w.project),
            &w.login,
            Value::Null
        )
        .await["items"],
        json!([r1])
    );

    let recovery = statement(&pool, Kind::Recovery, 99, 98).await;
    assert!(recovery.old_signature.is_none());
    server_authority::install_recovery(&pool, &recovery)
        .await
        .unwrap();
    let new_api = router(runtime.pool.clone(), Some(98));
    let r2 = receipt_of(&advance(&new_api, &w, 1, Some((0, 1))).await.1);
    assert_eq!(r2.receipt.previous, Some(r1.verify().unwrap()));
    let view = served(&new_api, &w).await;
    assert_eq!(view["epoch"], 2);
    assert_eq!(view["rotations"][0]["kind"], "recovery");
    assert_eq!(view["rotations"][0]["revocation"], Value::Null);
    assert_eq!(view["genesis"]["revocation"]["reason"], "key leaked");

    // Client side: the break is never crossed implicitly; revocation of the pinned
    // key is reported; an explicit independent re-pin extends the same chain.
    let chain = chain_of(&view, w.authority);
    assert!(pin
        .advance_chain(&chain, std::slice::from_ref(&r2))
        .is_err());
    let served_authority = engine_core::repository::authority_epoch::Served::new(
        chain,
        public(98),
        2,
        BTreeMap::from([(
            1,
            serde_json::from_value(view["genesis"]["revocation"].clone()).unwrap(),
        )]),
    )
    .unwrap();
    assert!(matches!(
        served_authority.vet(&public(99), &public(98)),
        Err(Refusal::Revoked { own: true, .. })
    ));
    served_authority.vet(&public(98), &public(98)).unwrap();
    let repinned = pin
        .accept_break(&recovery, &public(98), &r2)
        .expect("explicit re-pin with the independently known new key");
    assert_eq!(repinned.trusted_key(), public(98));
    assert!(pin.accept_break(&recovery, &public(97), &r2).is_err());
    // An attacker holding the old key cannot extend the re-pinned chain.
    let minted = SignedReceipt::sign(
        {
            let mut receipt = r2.receipt.clone();
            receipt.head.generation = 3;
            receipt.previous = Some(r2.verify().unwrap());
            receipt
        },
        &signer(99),
    )
    .unwrap();
    assert!(repinned.advance(&[minted]).is_err());
    runtime.close().await;
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
/// Start the real API binary against the restricted login: true once it accepts
/// connections, false if it exits first (within 30 seconds either way).
async fn boot(url: &str, key_file: &std::path::Path) -> (bool, String) {
    let port = free_port();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_engine-api"))
        .env("DATABASE_URL", url)
        .env("PASETO_LOCAL_KEY", "11".repeat(32))
        .env("WEB_SERVICE_KEY", "s".repeat(40))
        .env("API_HOST", "127.0.0.1")
        .env("API_PORT", port.to_string())
        .env("DYNODOC_REF_SIGNING_KEY_FILE", key_file)
        .env("RUST_LOG", "warn")
        .env_remove("MIGRATION_DATABASE_URL")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut alive = false;
    for _ in 0..120 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            alive = true;
            break;
        }
    }
    if alive || child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
    }
    let output = child.wait_with_output().unwrap();
    (alive, String::from_utf8_lossy(&output.stderr).into_owned())
}
fn key_file(seed: u8) -> std::path::PathBuf {
    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    der.extend([seed; 32]);
    let path = std::env::temp_dir().join(format!("dynodoc-rot-key-{}-{seed}.der", Uuid::new_v4()));
    std::fs::write(&path, der).unwrap();
    path
}

#[sqlx::test(migrations = "../../migrations")]
async fn api_binary_refuses_a_mounted_key_that_is_not_the_active_epochs(pool: PgPool) {
    let role = format!("runtime_boot_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::raw_sql(&format!("create role {role} login password '{password}' nosuperuser nocreatedb nocreaterole noreplication nobypassrls"))
        .execute(&pool).await.unwrap();
    engine_core::database::grant_runtime(&pool, &role)
        .await
        .unwrap();
    let options = pool.connect_options();
    let url = format!(
        "postgres://{role}:{password}@{}:{}/{}",
        options.get_host(),
        options.get_port(),
        options.get_database().unwrap()
    );
    server_authority::install(&pool, &public(99)).await.unwrap();
    let (old, new) = (key_file(99), key_file(98));
    let (alive, _) = boot(&url, &old).await;
    assert!(alive, "the installed key starts");
    let (alive, stderr) = boot(&url, &new).await;
    assert!(!alive, "an unknown key must not start");
    assert!(stderr.contains("does not match"), "{stderr}");

    // After rotation the roles swap: the old mount fails closed, the new one starts.
    let rotation = statement(&pool, Kind::Rotation, 99, 98).await;
    server_authority::install_rotation(&pool, &rotation)
        .await
        .unwrap();
    let (alive, stderr) = boot(&url, &old).await;
    assert!(!alive && stderr.contains("does not match"), "{stderr}");
    let (alive, _) = boot(&url, &new).await;
    assert!(alive, "the new epoch's key starts");
    // A revoked active epoch still starts (the rest of the API keeps working).
    server_authority::revoke(&pool, "rehearsal", None, None)
        .await
        .unwrap();
    let (alive, _) = boot(&url, &new).await;
    assert!(alive, "revocation stops receipts, not the service");
    for path in [old, new] {
        let _ = std::fs::remove_file(path);
    }
    sqlx::raw_sql(&format!("drop owned by {role}; drop role {role}"))
        .execute(&pool)
        .await
        .unwrap();
}
