use super::*;
use axum::{routing::get, Json, Router};
use engine_api::{
    app,
    auth::{store, token, AuthConfig},
    AppState,
};
use engine_core::{
    materializer::{DocumentState, MaterializedNode},
    repository::{
        attestation::Receipt,
        device::{Challenge, Proof},
        Commit, FileEntry,
    },
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use sqlx::PgPool;
use std::sync::Arc;
#[path = "../../../engine-api/tests/common/mod.rs"]
mod common;
const TOKEN_KEY: [u8; 32] = [9; 32];
fn key(seed: u8) -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap()
}
fn remote(url: String, authority: Uuid) -> Remote {
    Remote {
        version: 1,
        url,
        authority,
        public_key: hex::encode(key(99).public_key().as_ref()),
    }
}
async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (url, server)
}
async fn post(url: &str, path: &str, login: &str, body: Value) -> Value {
    let response = reqwest::Client::new()
        .post(format!("{url}{path}"))
        .bearer_auth(login)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let value: Value = response.json().await.unwrap();
    assert!(status.is_success(), "{path}: {value}");
    value
}
async fn actor(pool: &PgPool, name: &str) -> (Uuid, String) {
    let id = store::upsert_identity(pool, &format!("{name}@example.test"), None)
        .await
        .unwrap()
        .id
        .0;
    (id, token::mint(&TOKEN_KEY, id, 3600).unwrap())
}
async fn enroll(url: &str, login: &str, actor: Uuid, seed: u8) -> Ed25519KeyPair {
    let key = key(seed);
    let challenge: Challenge = serde_json::from_value(
        post(
            url,
            "devices/challenge",
            login,
            json!({"public_key":hex::encode(key.public_key().as_ref())}),
        )
        .await,
    )
    .unwrap();
    let proof = Proof::sign(challenge.clone(), challenge.authority, actor, &key).unwrap();
    // Account-wide keys, as before per-project grants; `enroll_granted` is the new
    // default mode.
    let mut body = json!(proof);
    body["project_access"] = json!("all");
    post(url, "devices", login, body).await;
    key
}
async fn enroll_granted(url: &str, login: &str, actor: Uuid, seed: u8) -> (Ed25519KeyPair, Uuid) {
    let key = key(seed);
    let challenge: Challenge = serde_json::from_value(
        post(
            url,
            "devices/challenge",
            login,
            json!({"public_key":hex::encode(key.public_key().as_ref())}),
        )
        .await,
    )
    .unwrap();
    let proof = Proof::sign(challenge.clone(), challenge.authority, actor, &key).unwrap();
    let enrolled = post(url, "devices", login, json!(proof)).await;
    assert_eq!(enrolled["project_access"], "granted");
    (key, challenge.id)
}
async fn repo(project: Uuid) -> (PathBuf, LocalRepository) {
    let path = std::env::temp_dir().join(format!("dynodoc-sync-{}.sqlite", Uuid::new_v4()));
    let repo = LocalRepository::open(&path, project).await.unwrap();
    (path, repo)
}
async fn journal() -> (PathBuf, ReceiptJournal) {
    let path = std::env::temp_dir().join(format!("dynodoc-sync-pin-{}.sqlite", Uuid::new_v4()));
    let journal = ReceiptJournal::open(&path).await.unwrap();
    (path, journal)
}
async fn remove(path: PathBuf) {
    for attempt in 0..20 {
        match std::fs::remove_file(&path) {
            Ok(()) => return,
            Err(e) if matches!(e.raw_os_error(), Some(32 | 33)) && attempt < 19 => {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await
            }
            Err(e) => panic!("test SQLite cleanup: {e}"),
        }
    }
}
fn state(text: &str) -> DocumentState {
    let node = MaterializedNode {
        id: engine_shared::NodeId("01ARZ3NDEKTSV4RRFFQ69G5FAV".into()),
        parent_id: None,
        node_type: "paragraph".into(),
        pos: "a0".into(),
        var_name: None,
        deleted: false,
        current_fields: json!({"text":text}),
    };
    DocumentState {
        nodes: BTreeMap::from([(node.id.clone(), node)]),
        ..Default::default()
    }
}
async fn commit(
    repo: &LocalRepository,
    doc: Uuid,
    key: &Ed25519KeyPair,
    parents: Vec<Address>,
    text: &str,
) -> SignedCommit {
    let (root, _) = repo
        .write_state(DocumentId(doc), &state(text))
        .await
        .unwrap();
    let signed = SignedCommit::sign(
        Commit {
            version: 1,
            project: repo.project(),
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
            message: text.into(),
            recovery: None,
            same_file_resolutions: Vec::new(),
        },
        key,
    )
    .unwrap();
    repo.import_commit(&signed).await.unwrap();
    signed
}

#[test]
fn explicit_origin_and_token_rules_reject_credential_urls_and_insecure_hosts() {
    for url in [
        "http://example.test/",
        "http://localhost/",
        "ftp://127.0.0.1/",
        "https://user:secret@example.test/",
        "https://example.test/?token=secret",
        "https://example.test/#x",
        "https://example.test/api",
    ] {
        assert!(
            Client::new(
                remote(url.into(), Uuid::new_v4()),
                Uuid::new_v4(),
                "synthetic",
                true
            )
            .is_err(),
            "{url}"
        );
    }
    assert!(Client::new(
        remote("http://127.0.0.1/".into(), Uuid::new_v4()),
        Uuid::new_v4(),
        "synthetic",
        false
    )
    .is_err());
    assert!(Client::new(
        remote("https://example.test/".into(), Uuid::new_v4()),
        Uuid::new_v4(),
        "bad\r\ntoken",
        false
    )
    .is_err());
}

#[tokio::test]
async fn redirects_and_oversize_bodies_are_rejected_without_reflecting_remote_secrets() {
    let router = Router::new()
        .route(
            "/projects/:id/redirect",
            get(|| async { axum::response::Redirect::temporary("/secret") }),
        )
        .route(
            "/projects/:id/large",
            get(|| async { Json(json!({"value":"private-remote-content".repeat(300)})) }),
        );
    let (url, server) = serve(router).await;
    let client = Client::new(
        remote(url, Uuid::new_v4()),
        Uuid::new_v4(),
        "synthetic",
        true,
    )
    .unwrap();
    let err = client
        .get::<Value>("redirect", &[], 1024)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("307"));
    let err = client
        .get::<Value>("large", &[], 128)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("byte limit") && !err.contains("private-remote-content"));
    server.abort();
    let _ = server.await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_http_selected_push_fetch_reuse_review_and_stale_plan_are_restart_safe(pool: PgPool) {
    let authority = engine_core::repository::server_authority::install(
        &pool,
        &hex::encode(key(99).public_key().as_ref()),
    )
    .await
    .unwrap();
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let mut api = AppState::new(
        runtime.pool.clone(),
        AuthConfig {
            paseto_key: TOKEN_KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    );
    api.ref_signer = Some(Arc::new(key(99)));
    let (url, server) = serve(app(api)).await;
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let project: Uuid = serde_json::from_value(
        post(&url, "projects", &login, json!({"name":"Sync"})).await["id"].clone(),
    )
    .unwrap();
    let doc: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by,space_id) values('Report',$1,$2) returning id",
    )
    .bind(owner)
    .bind(project)
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    let owner_key = enroll(&url, &login, owner, 7).await;
    let client = Client::new(remote(url.clone(), authority), project, &login, true).unwrap();
    client.authority().await.unwrap();
    let scope = Scope {
        authority,
        project,
        branch: "main".into(),
    };
    let (p1, r1) = repo(project).await;
    let (j1, journal1) = journal().await;
    let first = commit(&r1, doc, &owner_key, vec![], "visible base").await;
    let a = first.verify().unwrap();
    let secret = commit(&r1, doc, &owner_key, vec![], "private unrelated branch").await;
    r1.compare_and_swap("private", None, &secret.verify().unwrap())
        .await
        .unwrap();
    let approved = plan(&r1, &client, &journal1, &scope, a.clone(), true)
        .await
        .unwrap();
    assert_eq!(approved.publish.len(), 1);
    // No remote mutation during preview, and removing a required manifest is rejected.
    let mut truncated = approved.clone();
    truncated.publish.clear();
    assert!(push(&r1, &client, &journal1, &scope, truncated)
        .await
        .unwrap_err()
        .to_string()
        .contains("reviewed plan"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from project_staged_object")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let result = push(&r1, &client, &journal1, &scope, approved.clone())
        .await
        .unwrap();
    assert_eq!(result["head"]["generation"], 1);
    assert!(client
        .published(&secret.verify().unwrap())
        .await
        .unwrap()
        .is_none());
    assert!(!sqlx::query_scalar::<_, bool>(
        "select exists(select 1 from checkpoint_object where document_id=$1 and address=$2)"
    )
    .bind(doc)
    .bind(secret.commit.files["Report.docx"].state.as_str())
    .fetch_one(&pool)
    .await
    .unwrap());
    let bytes: i64 =
        sqlx::query_scalar("select sum(octet_length(bytes))::bigint from checkpoint_object")
            .fetch_one(&pool)
            .await
            .unwrap();
    // Reopen the pin and retry a lost success acknowledgement: no new generation or bytes.
    journal1.close().await;
    let journal1 = ReceiptJournal::open(&j1).await.unwrap();
    assert_eq!(
        push(&r1, &client, &journal1, &scope, approved)
            .await
            .unwrap()["already_applied"],
        true
    );
    assert!(r1.head("main").await.unwrap().is_none());
    let (other, other_login) = actor(&runtime.pool, "contributor").await;
    sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,'reviewer')")
        .bind(project)
        .bind(other)
        .execute(&runtime.pool)
        .await
        .unwrap();
    let other_key = enroll(&url, &other_login, other, 8).await;
    let c2 = Client::new(remote(url.clone(), authority), project, &other_login, true).unwrap();
    let (p2, r2) = repo(project).await;
    let (j2, journal2) = journal().await;
    assert_eq!(transfer::fetch(&r2, &c2, &a).await.unwrap(), 1);
    let receipt = c2.head(&scope).await.unwrap().unwrap();
    track(&c2, &journal2, &scope, receipt).await.unwrap();
    assert_eq!(
        r2.read_state(DocumentId(doc), &first.commit.files["Report.docx"].state)
            .await
            .unwrap(),
        state("visible base")
    );
    assert_eq!(transfer::fetch(&r2, &c2, &a).await.unwrap(), 0);
    // Second author reuses the entire root through published acknowledgements.
    let second = commit(&r2, doc, &other_key, vec![a.clone()], "visible base").await;
    let b = second.verify().unwrap();
    let feature = Scope {
        branch: "feature/local".into(),
        ..scope.clone()
    };
    let feature_plan = plan(&r2, &c2, &journal2, &feature, b.clone(), true)
        .await
        .unwrap();
    assert_eq!(feature_plan.publish.len(), 1);
    push(&r2, &c2, &journal2, &feature, feature_plan)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select sum(octet_length(bytes))::bigint from checkpoint_object"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        bytes
    );
    assert!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from project_staged_object where identity_id=$1"
        )
        .bind(other)
        .fetch_one(&pool)
        .await
        .unwrap()
            > 0
    );
    assert!(r2.head("feature/local").await.unwrap().is_none());
    // Competing plans bind the original generation; later push cannot overwrite it.
    transfer::fetch(&r1, &client, &b).await.unwrap();
    let divergent = commit(
        &r1,
        doc,
        &owner_key,
        vec![a.clone()],
        "independent alternative",
    )
    .await;
    let stale = plan(
        &r1,
        &client,
        &journal1,
        &scope,
        divergent.verify().unwrap(),
        true,
    )
    .await
    .unwrap();
    let advance = plan(&r1, &client, &journal1, &scope, b.clone(), true)
        .await
        .unwrap();
    push(&r1, &client, &journal1, &scope, advance)
        .await
        .unwrap();
    assert!(push(&r1, &client, &journal1, &scope, stale)
        .await
        .unwrap_err()
        .to_string()
        .contains("branch changed"));
    assert!(client
        .published(&divergent.verify().unwrap())
        .await
        .unwrap()
        .is_none());
    // Publish-only leaves the protected/main ref alone and makes a reviewable candidate.
    let third = commit(&r2, doc, &other_key, vec![b.clone()], "review me").await;
    let review_plan = plan(&r2, &c2, &journal2, &scope, third.verify().unwrap(), false)
        .await
        .unwrap();
    assert_eq!(
        push(&r2, &c2, &journal2, &scope, review_plan)
            .await
            .unwrap()["branch_changed"],
        false
    );
    assert_eq!(
        c2.head(&scope).await.unwrap().unwrap().receipt.head.commit,
        b
    );
    assert_eq!(
        r1.head("private").await.unwrap().unwrap().commit,
        secret.verify().unwrap()
    );
    // A selected merge explicitly discloses private ancestry. Removing that
    // ancestor from the saved approval cannot turn it into implicit publication.
    let mut merge = first.commit.clone();
    merge.parents = vec![a, secret.verify().unwrap()];
    merge.message = "merge a private line".into();
    let merge = SignedCommit::sign(merge, &owner_key).unwrap();
    r1.import_commit(&merge).await.unwrap();
    let mut disclosure = plan(
        &r1,
        &client,
        &journal1,
        &scope,
        merge.verify().unwrap(),
        false,
    )
    .await
    .unwrap();
    assert_eq!(disclosure.publish.len(), 2);
    assert_eq!(
        disclosure.publish[0].verify().unwrap(),
        secret.verify().unwrap()
    );
    disclosure.publish.remove(0);
    assert!(push(&r1, &client, &journal1, &scope, disclosure)
        .await
        .is_err());
    assert!(client
        .published(&secret.verify().unwrap())
        .await
        .unwrap()
        .is_none());
    let old = r2
        .compare_and_swap("work", None, &first.verify().unwrap())
        .await
        .unwrap();
    let pulled = pull(&r2, &c2, &journal2, &scope, "work", Some(&old))
        .await
        .unwrap();
    assert_eq!(pulled["local_head"]["commit"], json!(b));
    let work = r2.head("work").await.unwrap().unwrap();
    assert_eq!(work.generation, 2);
    assert_eq!(
        pull(&r2, &c2, &journal2, &scope, "fresh", None)
            .await
            .unwrap()["local_head"]["generation"],
        1
    );
    assert_eq!(
        pull(&r2, &c2, &journal2, &scope, "work", Some(&work))
            .await
            .unwrap()["local_branch_changed"],
        false
    );
    assert!(pull(&r2, &c2, &journal2, &scope, "work", None)
        .await
        .is_err());
    // A local-only descendant must survive pulling the older shared main.
    let local = r2
        .compare_and_swap("work", Some(&work), &third.verify().unwrap())
        .await
        .unwrap();
    assert!(pull(&r2, &c2, &journal2, &scope, "work", Some(&local))
        .await
        .unwrap_err()
        .to_string()
        .contains("diverged"));
    assert_eq!(r2.head("work").await.unwrap(), Some(local));
    server.abort();
    let _ = server.await;
    journal1.close().await;
    journal2.close().await;
    r1.close().await;
    r2.close().await;
    for path in [p1, p2, j1, j2] {
        remove(path).await;
    }
    runtime.close().await;
}

fn receipt(
    scope: Scope,
    generation: i64,
    previous: Option<Address>,
    commit: Address,
) -> SignedReceipt {
    SignedReceipt::sign(
        Receipt {
            version: 1,
            scope,
            head: Head { commit, generation },
            previous,
            issued_at_ms: 1,
            issuer_key: String::new(),
        },
        &key(99),
    )
    .unwrap()
}

#[tokio::test]
async fn up_to_date_push_rechecks_head_before_pinning_a_concurrent_unfetched_update() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let project = Uuid::new_v4();
    let authority = Uuid::new_v4();
    let scope = Scope {
        project,
        authority,
        branch: "main".into(),
    };
    let (file, repo) = repo(project).await;
    let signed = commit(&repo, Uuid::new_v4(), &key(7), vec![], "local head").await;
    let address = signed.verify().unwrap();
    let first = receipt(scope.clone(), 1, None, address.clone());
    let next = receipt(
        scope.clone(),
        2,
        Some(first.verify().unwrap()),
        Address::try_from("b".repeat(64)).unwrap(),
    );
    let count = Arc::new(AtomicUsize::new(0));
    let expected = first.receipt.head.clone();
    let (url,server) = serve(Router::new()
        .route("/projects/:id/history/refs",get(move || {
            let r = if count.fetch_add(1,Ordering::Relaxed) == 0 {first.clone()} else {next.clone()};
            async move { Json(json!({"items":[{"branch":"main","head":r.receipt.head}],"receipts":[r]})) }
        }))
        .route("/projects/:id/history/commits/:commit",get(move || {let signed = signed.clone();async move {Json(json!(signed))}}))
    ).await;
    let client = Client::new(remote(url, authority), project, "synthetic", true).unwrap();
    let (pin_path, journal) = journal().await;
    let approved = Plan {
        version: 1,
        remote: client.remote.clone(),
        project,
        branch: "main".into(),
        candidate: address,
        expected: Some(expected),
        update_branch: true,
        publish: vec![],
    };
    assert!(push(&repo, &client, &journal, &scope, approved)
        .await
        .unwrap_err()
        .to_string()
        .contains("up-to-date"));
    assert!(journal.pin(&scope).await.unwrap().is_none());
    server.abort();
    let _ = server.await;
    repo.close().await;
    journal.close().await;
    remove(file).await;
    remove(pin_path).await;
}
#[tokio::test]
async fn rollback_fork_wrong_key_revocation_and_gapped_receipts_never_replace_tracking() {
    let project = Uuid::new_v4();
    let authority = Uuid::new_v4();
    let scope = Scope {
        project,
        authority,
        branch: "main".into(),
    };
    let a = Address::try_from("a".repeat(64)).unwrap();
    let b = Address::try_from("b".repeat(64)).unwrap();
    let first = receipt(scope.clone(), 1, None, a.clone());
    let second = receipt(scope.clone(), 2, Some(first.verify().unwrap()), b.clone());
    let gap = receipt(scope.clone(), 4, Some(second.verify().unwrap()), a.clone());
    let served = gap.clone();
    let (url, server) = serve(Router::new().route(
        "/projects/:id/history/receipts",
        get(move || {
            let r = served.clone();
            async move { Json(json!({"items":[r]})) }
        }),
    ))
    .await;
    let client = Client::new(remote(url, authority), project, "synthetic", true).unwrap();
    let (path, journal) = journal().await;
    journal
        .trust_checkpoint(&scope, &client.remote.public_key, second.clone())
        .await
        .unwrap();
    assert!(track(&client, &journal, &scope, first).await.is_err());
    let fork = receipt(scope.clone(), 2, second.receipt.previous.clone(), a);
    assert!(track(&client, &journal, &scope, fork).await.is_err());
    assert!(track(&client, &journal, &scope, gap).await.is_err());
    let wrong_key = SignedReceipt::sign(second.receipt.clone(), &key(3)).unwrap();
    assert!(track(&client, &journal, &scope, wrong_key).await.is_err());
    assert_eq!(
        journal.pin(&scope).await.unwrap().unwrap().receipt(),
        &second
    );
    journal.revoke(&scope).await.unwrap();
    assert!(track(&client, &journal, &scope, second).await.is_err());
    server.abort();
    let _ = server.await;
    journal.close().await;
    remove(path).await;
}

#[tokio::test]
async fn interrupted_or_corrupt_download_retains_verified_objects_and_never_imports_a_commit() {
    let project = Uuid::new_v4();
    let doc = Uuid::new_v4();
    let (p1, r1) = repo(project).await;
    let (p2, r2) = repo(project).await;
    let signed = commit(&r1, doc, &key(7), vec![], "selected history").await;
    let address = signed.verify().unwrap();
    let selected = signed.clone();
    let root = selected.commit.files["Report.docx"].state.clone();
    let packet = r1
        .object(&ObjectKey {
            document: DocumentId(doc),
            address: root.clone(),
        })
        .await
        .unwrap();
    let served_root = root.clone();
    let served_bytes = STANDARD.encode(packet.bytes);
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let corrupt = Arc::new(AtomicBool::new(true));
    let switch = corrupt.clone();
    let root_calls = Arc::new(AtomicUsize::new(0));
    let calls = root_calls.clone();
    let source = r1.clone();
    let router = Router::new()
        .route(
            "/projects/:id/history/commits/:address",
            get(move || {
                let c = selected.clone();
                async move { Json(json!(c)) }
            }),
        )
        .route(
            "/projects/:id/history/commits/:commit/objects/:doc/:address",
            get(
                move |axum::extract::Path((_id, _commit, _doc, object)): axum::extract::Path<(
                    String,
                    String,
                    String,
                    String,
                )>| {
                    let root = served_root.clone();
                    let bytes = served_bytes.clone();
                    let switch = switch.clone();
                    let calls = calls.clone();
                    let source = source.clone();
                    async move {
                        let bytes = if object == root.as_str() {
                            calls.fetch_add(1, Ordering::Relaxed);
                            bytes
                        } else if switch.load(Ordering::Relaxed) {
                            STANDARD.encode(b"corrupt")
                        } else {
                            STANDARD.encode(
                                source
                                    .object(&ObjectKey {
                                        document: DocumentId(doc),
                                        address: Address::try_from(object.clone()).unwrap(),
                                    })
                                    .await
                                    .unwrap()
                                    .bytes,
                            )
                        };
                        Json(json!({"address":object,"bytes":bytes}))
                    }
                },
            ),
        );
    let (url, server) = serve(router).await;
    let client = Client::new(remote(url, Uuid::new_v4()), project, "synthetic", true).unwrap();
    assert!(transfer::fetch(&r2, &client, &address).await.is_err());
    assert!(r2.commit(&address).await.is_err());
    assert!(r2.head("main").await.unwrap().is_none());
    assert!(r2
        .missing(&[ObjectKey {
            document: DocumentId(doc),
            address: root
        }])
        .await
        .unwrap()
        .is_empty());
    assert_eq!(r2.storage_usage().await.unwrap().objects_used, 1);
    corrupt.store(false, Ordering::Relaxed);
    assert_eq!(transfer::fetch(&r2, &client, &address).await.unwrap(), 1);
    assert_eq!(
        root_calls.load(Ordering::Relaxed),
        1,
        "retry must reuse the already verified root"
    );
    assert_eq!(
        r2.commit(&address).await.unwrap().verify().unwrap(),
        address
    );
    server.abort();
    let _ = server.await;
    r1.close().await;
    r2.close().await;
    remove(p1).await;
    remove(p2).await;
}

// --- renewal and project device sessions -------------------------------------------

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dynodoc-credentials-{}", Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    dir
}
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}
async fn get_json(url: &str, path: &str, login: &str) -> Value {
    let response = reqwest::Client::new()
        .get(format!("{url}{path}"))
        .bearer_auth(login)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "{path}");
    response.json().await.unwrap()
}
async fn api(
    pool: &PgPool,
    signer: Option<Arc<Ed25519KeyPair>>,
) -> (String, tokio::task::JoinHandle<()>) {
    let mut state = AppState::new(
        pool.clone(),
        AuthConfig {
            paseto_key: TOKEN_KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    );
    state.ref_signer = signer;
    serve(app(state)).await
}
fn device(url: &str, authority: Uuid, project: Uuid, seed: u8, scopes: &[&str]) -> Client {
    Client::with_credential(
        remote(url.to_owned(), authority),
        project,
        Credential::Device {
            key: key(seed),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
        },
        None,
        true,
    )
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn refresh_rotation_retries_once_and_replaces_files_atomically(pool: PgPool) {
    let authority = engine_core::repository::server_authority::install(
        &pool,
        &hex::encode(key(99).public_key().as_ref()),
    )
    .await
    .unwrap();
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (url, server) = api(&runtime.pool, None).await;
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let project: Uuid = serde_json::from_value(
        post(&url, "projects", &login, json!({"name":"Renewal"})).await["id"].clone(),
    )
    .unwrap();
    // A real login: bound session, an access token that has already expired, and
    // a refresh secret the server knows only by hash.
    let session = Uuid::new_v4();
    let secret = "ab".repeat(32);
    sqlx::query("insert into refresh_token(identity_id,token_hash,expires_at,session_generation,session_id) values($1,$2,now()+interval '1 day',0,$3)")
        .bind(owner)
        .bind(engine_api::auth::sha256(secret.as_bytes()))
        .bind(session)
        .execute(&runtime.pool)
        .await
        .unwrap();
    let expired = token::mint_bound_session(&TOKEN_KEY, owner, -30, 0, Some(session)).unwrap();
    let dir = scratch();
    let (access_file, refresh_file) = (dir.join("access.txt"), dir.join("refresh.txt"));
    std::fs::write(&access_file, &expired).unwrap();
    std::fs::write(&refresh_file, &secret).unwrap();
    let credential = || Credential::Refresh {
        refresh_file: refresh_file.clone(),
        access_file: Some(access_file.clone()),
    };

    let client = Client::with_credential(
        remote(url.clone(), authority),
        project,
        credential(),
        Some(&expired),
        true,
    )
    .unwrap();
    let files: Value = client.get("files", &[], 1 << 20).await.unwrap();
    assert!(files["items"].is_array());
    // Rotation persisted both secrets completely and left no temporary files.
    let rotated = std::fs::read_to_string(&refresh_file).unwrap();
    let access = std::fs::read_to_string(&access_file).unwrap();
    assert!(rotated.len() == 64 && rotated != secret);
    assert!(!access.is_empty() && access != expired);
    assert_eq!(entries(&dir), ["access.txt", "refresh.txt"]);
    // The consumed refresh secret is dead; the new files work without renewal.
    let stale = reqwest::Client::new()
        .post(format!("{url}auth/refresh"))
        .json(&json!({"refresh_token":secret}))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(stale, reqwest::StatusCode::UNAUTHORIZED);
    let second = Client::with_credential(
        remote(url.clone(), authority),
        project,
        credential(),
        Some(access.trim()),
        true,
    )
    .unwrap();
    second.get::<Value>("files", &[], 1 << 20).await.unwrap();
    assert_eq!(std::fs::read_to_string(&refresh_file).unwrap(), rotated);
    // A start with only the refresh file renews before its first request.
    let only_refresh = Client::with_credential(
        remote(url.clone(), authority),
        project,
        Credential::Refresh {
            refresh_file: refresh_file.clone(),
            access_file: None,
        },
        None,
        true,
    )
    .unwrap();
    only_refresh
        .get::<Value>("files", &[], 1 << 20)
        .await
        .unwrap();
    let rotated_again = std::fs::read_to_string(&refresh_file).unwrap();
    assert_ne!(rotated_again, rotated);
    // An invalid refresh secret changes nothing and reports no secret.
    std::fs::write(&refresh_file, "cd".repeat(32)).unwrap();
    let broken = Client::with_credential(
        remote(url.clone(), authority),
        project,
        credential(),
        Some(&expired),
        true,
    )
    .unwrap();
    let error = broken
        .get::<Value>("files", &[], 1 << 20)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("401") && !error.contains(&"cd".repeat(32)),
        "{error}"
    );
    assert_eq!(
        std::fs::read_to_string(&refresh_file).unwrap(),
        "cd".repeat(32)
    );
    assert_eq!(entries(&dir), ["access.txt", "refresh.txt"]);
    // A fixed expired token stops with the server's refusal rather than looping.
    let fixed = Client::new(remote(url.clone(), authority), project, &expired, true).unwrap();
    assert!(fixed
        .get::<Value>("files", &[], 1 << 20)
        .await
        .unwrap_err()
        .to_string()
        .contains("401"));
    std::fs::remove_dir_all(dir).unwrap();
    server.abort();
    let _ = server.await;
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn device_sessions_push_fetch_keep_policy_and_end_with_revocation_or_removal(pool: PgPool) {
    let authority = engine_core::repository::server_authority::install(
        &pool,
        &hex::encode(key(99).public_key().as_ref()),
    )
    .await
    .unwrap();
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (url, server) = api(&runtime.pool, Some(Arc::new(key(99)))).await;
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let (contributor, contributor_login) = actor(&runtime.pool, "contributor").await;
    let project: Uuid = serde_json::from_value(
        post(&url, "projects", &login, json!({"name":"Device"})).await["id"].clone(),
    )
    .unwrap();
    let doc: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by,space_id) values('Report',$1,$2) returning id",
    )
    .bind(owner)
    .bind(project)
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,'reviewer')")
        .bind(project)
        .bind(contributor)
        .execute(&runtime.pool)
        .await
        .unwrap();
    let owner_key = enroll(&url, &login, owner, 7).await;
    let contributor_key = enroll(&url, &contributor_login, contributor, 8).await;
    let scope = Scope {
        authority,
        project,
        branch: "main".into(),
    };
    let rw = ["history:read", "history:write"];

    // The owner publishes main with no account token anywhere.
    let owner_client = device(&url, authority, project, 7, &rw);
    owner_client.authority().await.unwrap();
    let (p1, r1) = repo(project).await;
    let (j1, journal1) = journal().await;
    let first = commit(&r1, doc, &owner_key, vec![], "base").await;
    let a = first.verify().unwrap();
    let approved = plan(&r1, &owner_client, &journal1, &scope, a.clone(), true)
        .await
        .unwrap();
    let result = push(&r1, &owner_client, &journal1, &scope, approved)
        .await
        .unwrap();
    assert_eq!(result["head"]["generation"], 1);

    // A read-only session fetches but cannot stage, publish or review.
    let (p2, r2) = repo(project).await;
    let (j2, journal2) = journal().await;
    let reader = device(&url, authority, project, 8, &["history:read"]);
    assert_eq!(transfer::fetch(&r2, &reader, &a).await.unwrap(), 1);
    let second = commit(&r2, doc, &contributor_key, vec![a.clone()], "feature").await;
    let b = second.verify().unwrap();
    let review = plan(&r2, &reader, &journal2, &scope, b.clone(), false)
        .await
        .unwrap();
    assert_eq!(review.publish.len(), 1);
    let denied = push(&r2, &reader, &journal2, &scope, review.clone())
        .await
        .unwrap_err()
        .to_string();
    assert!(denied.contains("403"), "{denied}");
    assert!(reader.published(&b).await.unwrap().is_none());

    // A writing session publishes a candidate, but the contributor role still
    // cannot move protected main: the server's rules are unchanged.
    let writer = device(&url, authority, project, 8, &rw);
    assert_eq!(
        push(&r2, &writer, &journal2, &scope, review).await.unwrap()["branch_changed"],
        false
    );
    assert!(writer.published(&b).await.unwrap().is_some());
    let advance = plan(&r2, &writer, &journal2, &scope, b.clone(), true)
        .await
        .unwrap();
    let refused = push(&r2, &writer, &journal2, &scope, advance)
        .await
        .unwrap_err();
    assert!(format!("{refused:#}").contains("403"), "{refused:#}");
    assert_eq!(
        owner_client
            .head(&scope)
            .await
            .unwrap()
            .unwrap()
            .receipt
            .head
            .commit,
        a
    );
    // Review rules are the same too: with one required approval the owner cannot
    // merge the contributor's candidate until a current approval exists.
    sqlx::query("update project_settings set required_approvals=1 where space_id=$1")
        .bind(project)
        .execute(&runtime.pool)
        .await
        .unwrap();
    transfer::fetch(&r1, &owner_client, &b).await.unwrap();
    let merge = plan(&r1, &owner_client, &journal1, &scope, b.clone(), true)
        .await
        .unwrap();
    let unreviewed = push(&r1, &owner_client, &journal1, &scope, merge.clone())
        .await
        .unwrap_err();
    assert!(format!("{unreviewed:#}").contains("409"), "{unreviewed:#}");
    // Approving is its own scope: a session that can push (history:write) cannot
    // approve, and one that can approve (history:review) cannot push.
    let decision = json!({"target":{"branch":"main","candidate":b,"expected":{"commit":a,"generation":1}},"decision":"approve"});
    let refused = owner_client
        .post("history/reviews", decision.clone())
        .await
        .unwrap_err();
    assert!(format!("{refused:#}").contains("403"), "{refused:#}");
    let approver = device(
        &url,
        authority,
        project,
        7,
        &["history:read", "history:review"],
    );
    approver.post("history/reviews", decision).await.unwrap();
    assert!(push(&r1, &approver, &journal1, &scope, merge.clone())
        .await
        .is_err());
    let merged = push(&r1, &owner_client, &journal1, &scope, merge)
        .await
        .unwrap();
    assert_eq!(merged["head"]["generation"], 2);
    // One audit row per issuance, with no token material.
    let issued: Vec<String> = sqlx::query_scalar(
        "select detail::text from operation_audit where action='device.session_issued'",
    )
    .fetch_all(&runtime.pool)
    .await
    .unwrap();
    assert!(issued.len() >= 3 && issued.iter().all(|d| !d.contains("v4.local")));

    // Revoking the key ends this client; its renewal cannot succeed either.
    let owner_key_id = get_json(&url, "devices", &login).await["keys"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    post(
        &url,
        &format!("devices/{owner_key_id}/revoke"),
        &login,
        json!({}),
    )
    .await;
    let error = format!("{:#}", owner_client.head(&scope).await.unwrap_err());
    assert!(error.contains("401"), "{error}");
    let fresh = device(&url, authority, project, 7, &rw);
    assert!(fresh.authority().await.is_err());

    // Removing the member ends the contributor even with an unexpired token.
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(project)
        .bind(contributor)
        .execute(&runtime.pool)
        .await
        .unwrap();
    let error = format!("{:#}", writer.head(&scope).await.unwrap_err());
    assert!(error.contains("403"), "{error}");
    assert!(device(&url, authority, project, 8, &rw)
        .authority()
        .await
        .is_err());
    server.abort();
    let _ = server.await;
    journal1.close().await;
    journal2.close().await;
    r1.close().await;
    r2.close().await;
    for path in [p1, p2, j1, j2] {
        remove(path).await;
    }
    runtime.close().await;
}

#[test]
fn credential_options_conflict_and_require_a_source_for_network_actions() {
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        options: Options,
    }
    let parse = |extra: &[&str], action: &str| {
        let mut args = vec![
            "sync",
            "repo.sqlite",
            "11111111-1111-4111-8111-111111111111",
            "--remote",
            "remote.json",
            "--journal",
            "pins.sqlite",
            "--branch",
            "main",
        ];
        args.extend(extra);
        args.push(action);
        Cli::try_parse_from(args)
    };
    assert!(parse(&["--device-key", "k.der"], "fetch").is_ok());
    assert!(parse(&["--token-file", "a", "--refresh-token-file", "r"], "fetch").is_ok());
    assert!(parse(&["--device-key", "k", "--token-file", "a"], "fetch").is_err());
    assert!(parse(&["--device-key", "k", "--refresh-token-file", "r"], "fetch").is_err());
    // Rotation replaces both files, so they must be two files.
    let same = parse(
        &[
            "--token-file",
            "same.txt",
            "--refresh-token-file",
            "same.txt",
        ],
        "fetch",
    )
    .unwrap()
    .options;
    let error = credentials(
        &same,
        remote("https://example.test/".into(), Uuid::new_v4()),
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("different files"), "{error}");
    // Offline tracking needs no credential; any network action does.
    let none = parse(&[], "fetch").unwrap().options;
    assert!(credentials(
        &none,
        remote("https://example.test/".into(), Uuid::new_v4())
    )
    .is_err());
    assert!(parse(&[], "tracking").is_ok());
}

#[tokio::test]
async fn hostile_token_lifetimes_neither_panic_nor_lose_the_rotated_secret() {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    let lifetime = Arc::new(AtomicU64::new(0));
    let rotations = Arc::new(AtomicUsize::new(0));
    let rotated = "cd".repeat(32);
    let (served, counted, successor) = (lifetime.clone(), rotations.clone(), rotated.clone());
    let router = Router::new()
        .route(
            "/auth/refresh",
            axum::routing::post(move || {
                counted.fetch_add(1, Ordering::Relaxed);
                let (successor, expires_in) = (successor.clone(), served.load(Ordering::Relaxed));
                async move {
                    Json(json!({
                        "access_token":"fresh-token",
                        "token_type":"Bearer",
                        "refresh_token":successor,
                        "expires_in":expires_in,
                    }))
                }
            }),
        )
        .route(
            "/projects/:id/files",
            get(|headers: axum::http::HeaderMap| async move {
                let bearer = headers.get("authorization").and_then(|v| v.to_str().ok());
                if bearer == Some("Bearer fresh-token") {
                    Ok(Json(json!({"items":[]})))
                } else {
                    Err(axum::http::StatusCode::UNAUTHORIZED)
                }
            }),
        );
    let (url, server) = serve(router).await;
    let dir = scratch();
    let refresh_file = dir.join("refresh.txt");
    // Overflowing, absurd, zero and ordinary values: one rotation each, no panic,
    // the successor secret saved, and no renewal on the second request.
    for expires_in in [u64::MAX, u64::MAX / 2, 1 << 40, 0, 3600] {
        lifetime.store(expires_in, Ordering::Relaxed);
        std::fs::write(&refresh_file, "ab".repeat(32)).unwrap();
        let before = rotations.load(Ordering::Relaxed);
        let client = Client::with_credential(
            remote(url.clone(), Uuid::new_v4()),
            Uuid::new_v4(),
            Credential::Refresh {
                refresh_file: refresh_file.clone(),
                access_file: None,
            },
            None,
            true,
        )
        .unwrap();
        for _ in 0..2 {
            client
                .get::<Value>("files", &[], 1 << 16)
                .await
                .unwrap_or_else(|e| panic!("expires_in {expires_in}: {e:#}"));
        }
        assert_eq!(
            rotations.load(Ordering::Relaxed),
            before + 1,
            "{expires_in}"
        );
        assert_eq!(std::fs::read_to_string(&refresh_file).unwrap(), rotated);
        assert_eq!(entries(&dir), ["refresh.txt"]);
    }
    std::fs::remove_dir_all(dir).unwrap();
    server.abort();
    let _ = server.await;
}

async fn send(url: &str, method: &str, path: &str, login: &str, body: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .request(method.parse().unwrap(), format!("{url}{path}"))
        .bearer_auth(login)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[test]
fn device_scopes_are_the_least_the_action_needs_and_never_include_review() {
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        options: Options,
    }
    let scopes = |action: &[&str]| {
        let mut args = vec![
            "sync",
            "repo.sqlite",
            "11111111-1111-4111-8111-111111111111",
            "--remote",
            "remote.json",
            "--journal",
            "pins.sqlite",
            "--branch",
            "main",
            "--device-key",
            "key.der",
        ];
        args.extend(action);
        device_scopes(&Cli::try_parse_from(args).unwrap().options.action)
    };
    assert_eq!(scopes(&["fetch"]), ["history:read"]);
    assert_eq!(scopes(&["pull", "main"]), ["history:read"]);
    assert_eq!(scopes(&["plan", "ab", "--out", "p.json"]), ["history:read"]);
    assert_eq!(
        scopes(&["push", "plan.json"]),
        ["history:read", "history:write"]
    );
    for action in [
        &["fetch"][..],
        &["push", "plan.json"],
        &["plan", "ab", "--out", "p.json"],
    ] {
        assert!(!scopes(action).iter().any(|s| s == "history:review"));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_granted_key_signs_in_only_for_what_its_account_holder_granted(pool: PgPool) {
    let authority = engine_core::repository::server_authority::install(
        &pool,
        &hex::encode(key(99).public_key().as_ref()),
    )
    .await
    .unwrap();
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let (url, server) = api(&runtime.pool, None).await;
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let project: Uuid = serde_json::from_value(
        post(&url, "projects", &login, json!({"name":"Granted"})).await["id"].clone(),
    )
    .unwrap();
    let (_, key_id) = enroll_granted(&url, &login, owner, 7).await;
    let scope = Scope {
        authority,
        project,
        branch: "main".into(),
    };
    let grant = format!("devices/{key_id}/projects/{project}");

    // Possession of the key file alone is not enough, and the message says where
    // to look.
    let reader = device(&url, authority, project, 7, &["history:read"]);
    let error = format!("{:#}", reader.authority().await.unwrap_err());
    assert!(
        error.contains("403") && error.contains("granted"),
        "{error}"
    );

    // Only the signed-in account holder can grant; the grant is read-only.
    let (code, _) = send(
        &url,
        "PUT",
        &grant,
        &login,
        json!({"scopes":["history:read"]}),
    )
    .await;
    assert_eq!(code, 200);
    reader.authority().await.unwrap();
    let writer = device(
        &url,
        authority,
        project,
        7,
        &["history:read", "history:write"],
    );
    assert!(writer.authority().await.is_err());
    let reviewer = device(
        &url,
        authority,
        project,
        7,
        &["history:read", "history:review"],
    );
    assert!(reviewer.authority().await.is_err());

    // Widening is picked up by the next sign-in; narrowing and revocation by the
    // next request of a live session (the client does not retry a 403).
    let (code, _) = send(
        &url,
        "PUT",
        &grant,
        &login,
        json!({"scopes":["history:read","history:write"]}),
    )
    .await;
    assert_eq!(code, 200);
    writer.authority().await.unwrap();
    assert!(reader.head(&scope).await.is_ok());
    let (code, _) = send(&url, "DELETE", &grant, &login, Value::Null).await;
    assert_eq!(code, 200);
    for client in [&reader, &writer] {
        let error = format!("{:#}", client.head(&scope).await.unwrap_err());
        assert!(error.contains("403"), "{error}");
    }
    // The key cannot undo it: a device-session token is no account login.
    let (code, _) = send(&url, "GET", "devices", &login, Value::Null).await;
    assert_eq!(code, 200);
    server.abort();
    let _ = server.await;
    runtime.close().await;
}
