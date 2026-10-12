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
    post(url, "devices", login, json!(proof)).await;
    key
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
