//! Change requests (pushed files) and draft rebasing, driven through the real router.
//! Requires a reachable Postgres (DATABASE_URL), like the other API integration tests.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use engine_api::auth::{store as auth_store, token, AuthConfig};
use engine_api::{app, AppState};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const KEY: [u8; 32] = [9u8; 32];

fn test_state(pool: PgPool) -> AppState {
    AppState::new(
        pool,
        AuthConfig {
            paseto_key: KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    )
}
async fn identity(pool: &PgPool, email: &str) -> String {
    let id = auth_store::upsert_identity(pool, email, Some("Tester"))
        .await
        .unwrap()
        .id
        .0;
    format!("Bearer {}", token::mint(&KEY, id, 3600).unwrap())
}
async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn post(router: &Router, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header(header::AUTHORIZATION, token)
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    send(router, req).await
}
async fn get(router: &Router, uri: &str, token: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .header(header::AUTHORIZATION, token)
        .body(Body::empty())
        .unwrap();
    send(router, req).await
}

async fn document(router: &Router, owner: &str) -> (Uuid, String) {
    let (status, body) = post(
        router,
        "/documents",
        owner,
        json!({"title":"Shared report"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let doc = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    let root = ulid::Ulid::new().to_string();
    let block = ulid::Ulid::new().to_string();
    assert_eq!(
        post(
            router,
            &format!("/documents/{doc}/metadata"),
            owner,
            json!({"settings":{"kind":"document"}})
        )
        .await
        .0,
        StatusCode::OK
    );
    let ops = json!([
        {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"000000","fields":{"label":"Report"}},
        {"type":"NodeCreated","node_id":block,"node_type":"item","parent_id":root,"pos":"00000000","fields":{"label":"Original","content":{"type":"paragraph","content":[{"type":"text","text":"Original"}]}}}
    ]);
    let (status, body) = post(
        router,
        &format!("/documents/{doc}/batch"),
        owner,
        json!({"base_seq":0,"ops":ops}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (doc, block)
}
fn upload(doc: Uuid, checkpoint: &Value, block: &str, host: &str, submit: bool) -> Value {
    json!({"name":"Observed paragraph change","note":"Saved offline","submit":submit,"bundle":{
        "format":"dynodoc.change-bundle","version":1,"bundle_id":Uuid::new_v4(),"client_id":Uuid::new_v4(),"document_id":doc,
        "base_seq":checkpoint["through_seq"],"base_chain_hash":checkpoint["chain_hash"],
        "capture":{"host":host,"mode":"observed_snapshot","document_name":"renamed.docx"},
        "changes":[{"id":Uuid::new_v4(),"observed_at":"2026-10-06T01:02:03Z","operation":{"type":"FieldEdited","node_id":block,"field":"content","value":{"type":"paragraph","content":[{"type":"text","text":"Changed externally"}]}}}]
    }})
}

#[sqlx::test(migrations = "../../migrations")]
async fn projects_inherit_rules_collect_requests_and_hide_private_drafts(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "project-owner@example.test").await;
    let contributor = identity(&pool, "project-contributor@example.test").await;
    let editor = identity(&pool, "project-editor@example.test").await;
    let stranger = identity(&pool, "project-outsider@example.test").await;
    let (status, p) = post(
        &router,
        "/projects",
        &owner,
        json!({"name":"Research team"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let pid = p["id"].as_str().unwrap();
    for (email, role) in [
        ("project-contributor@example.test", "reviewer"),
        ("project-editor@example.test", "editor"),
    ] {
        assert_eq!(
            post(
                &router,
                &format!("/spaces/{pid}/members"),
                &owner,
                json!({"email":email,"role":role})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert!(get(&router, "/projects", &stranger).await.1["items"]
        .as_array()
        .unwrap()
        .is_empty());
    for suffix in ["", "/files", "/drafts", "/change-requests"] {
        assert_eq!(
            get(&router, &format!("/projects/{pid}{suffix}"), &stranger)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
    let (doc, block) = document(&router, &owner).await;
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/location"),
            &owner,
            json!({"space_id":pid})
        )
        .await
        .0,
        StatusCode::OK
    );
    let rules = get(&router, &format!("/documents/{doc}/people"), &editor)
        .await
        .1;
    assert_eq!(rules["policy_source"]["kind"], "project");
    assert_eq!(rules["policy"]["protect_team_version"], true);
    assert_eq!(rules["you"]["can"]["edit_team"], false);
    assert_eq!(
        post(
            &router,
            &format!("/projects/{pid}/watch"),
            &contributor,
            json!({"level":"requests"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let checkpoint = get(&router, &format!("/documents/{doc}/provenance"), &owner)
        .await
        .1;
    let private = upload(doc, &checkpoint, &block, "word", false);
    let owner_draft = post(
        &router,
        &format!("/documents/{doc}/provenance"),
        &owner,
        private.clone(),
    )
    .await
    .1;
    assert!(owner_draft["id"].is_string(), "{owner_draft}");
    assert_eq!(
        get(
            &router,
            &format!(
                "/documents/{doc}/provenance/bundles/{}",
                private["bundle"]["bundle_id"].as_str().unwrap()
            ),
            &contributor
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert!(
        get(&router, &format!("/projects/{pid}/drafts"), &contributor)
            .await
            .1["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let own = post(
        &router,
        &format!("/documents/{doc}/provenance"),
        &contributor,
        upload(doc, &checkpoint, &block, "google-docs", false),
    )
    .await
    .1;
    let owner_private = get(&router, &format!("/projects/{pid}/drafts"), &owner)
        .await
        .1;
    assert_eq!(owner_private["items"].as_array().unwrap().len(), 1);
    assert_eq!(owner_private["items"][0]["id"], owner_draft["id"]);
    assert_ne!(owner_private["items"][0]["id"], own["id"]);
    let request = post(
        &router,
        &format!("/documents/{doc}/provenance"),
        &editor,
        upload(doc, &checkpoint, &block, "word", true),
    )
    .await
    .1;
    let requests = get(
        &router,
        &format!("/projects/{pid}/change-requests"),
        &contributor,
    )
    .await
    .1;
    assert_eq!(requests["items"].as_array().unwrap().len(), 1);
    assert_eq!(requests["items"][0]["id"], request["id"]);
    let notices = get(&router, "/notifications", &contributor).await.1;
    assert_eq!(
        notices["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["kind"] == "change_request_submitted")
            .count(),
        1
    );
    let update = json!({"name":"Team","description":"External editors first","revision":1,"protect_team_version":true,"required_approvals":1,"merge_roles":"owners"});
    assert_eq!(
        post(
            &router,
            &format!("/projects/{pid}/settings"),
            &contributor,
            update.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(
            &router,
            &format!("/projects/{pid}/settings"),
            &owner,
            update.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        post(
            &router,
            &format!("/projects/{pid}/settings"),
            &owner,
            update
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let override_rules =
        json!({"protect_team_version":false,"required_approvals":0,"merge_roles":"editors"});
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/policy"),
            &owner,
            override_rules
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        get(&router, &format!("/documents/{doc}/people"), &editor)
            .await
            .1["policy_source"]["kind"],
        "file"
    );
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/policy/inherit"),
            &owner,
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        get(&router, &format!("/documents/{doc}/people"), &editor)
            .await
            .1["policy"]["required_approvals"],
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn bundles_are_atomic_idempotent_anchored_and_credentials_are_scoped(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "connector-owner@example.test").await;
    let viewer = identity(&pool, "connector-viewer@example.test").await;
    let (doc, block) = document(&router, &owner).await;
    let (other, _) = document(&router, &owner).await;
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/members"),
            &owner,
            json!({"email":"connector-viewer@example.test","role":"viewer"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/connectors"),
            &viewer,
            json!({"host":"word"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, grant) = post(
        &router,
        &format!("/documents/{doc}/connectors"),
        &owner,
        json!({"host":"word"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grant}");
    let connector = format!("Bearer {}", grant["token"].as_str().unwrap());
    assert_eq!(
        get(
            &router,
            &format!("/connector/documents/{other}/checkpoint"),
            &connector
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(&router, &format!("/documents/{doc}"), &connector)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let checkpoint = get(
        &router,
        &format!("/connector/documents/{doc}/checkpoint"),
        &connector,
    )
    .await
    .1;
    let input = upload(doc, &checkpoint, &block, "word", true);
    let mut invalid = input.clone();
    invalid["bundle"]["base_chain_hash"] = json!("f".repeat(64));
    let endpoint = format!("/connector/documents/{doc}/bundles");
    assert_eq!(
        post(&router, &endpoint, &connector, invalid).await.0,
        StatusCode::CONFLICT
    );
    let mut invalid = input.clone();
    invalid["bundle"]["changes"].as_array_mut().unwrap().push(json!({"id":Uuid::new_v4(),"observed_at":"2026-10-06T01:02:04Z","operation":{"type":"FieldEdited","node_id":ulid::Ulid::new().to_string(),"field":"label","value":"Missing node"}}));
    assert_ne!(
        post(&router, &endpoint, &connector, invalid).await.0,
        StatusCode::OK
    );
    let count: i64 =
        sqlx::query_scalar("select count(*) from workspace_draft where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    let (status, receipt) = post(&router, &endpoint, &connector, input.clone()).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let retries = futures::future::join_all(
        (0..3).map(|_| post(&router, &endpoint, &connector, input.clone())),
    )
    .await;
    for (status, retry) in retries {
        assert_eq!(status, StatusCode::OK);
        assert_eq!(retry, receipt);
    }
    let mut reused = input.clone();
    reused["note"] = json!("Different intent");
    assert_eq!(
        post(&router, &endpoint, &connector, reused).await.0,
        StatusCode::CONFLICT
    );
    let mut wrong_host = input.clone();
    wrong_host["bundle"]["capture"]["host"] = json!("google-docs");
    assert_eq!(
        post(&router, &endpoint, &connector, wrong_host).await.0,
        StatusCode::FORBIDDEN
    );
    let latest = get(
        &router,
        &format!("/connector/documents/{doc}/checkpoint"),
        &connector,
    )
    .await
    .1;
    assert_eq!(latest["through_seq"], checkpoint["through_seq"]);
    assert_eq!(latest["proposals"][0]["id"], receipt["id"]);
    let bundle = input["bundle"]["bundle_id"].as_str().unwrap().to_owned();
    assert!(
        sqlx::query("update provenance_bundle set body='{}' where document_id=$1")
            .bind(doc)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("delete from provenance_bundle where document_id=$1")
            .bind(doc)
            .execute(&pool)
            .await
            .is_err()
    );
    assert_eq!(
        get(
            &router,
            &format!("/documents/{doc}/provenance/bundles/{bundle}"),
            &owner
        )
        .await
        .1["receipt"],
        receipt
    );
    assert_eq!(
        post(
            &router,
            &format!(
                "/documents/{doc}/connectors/{}/revoke",
                grant["grant"]["id"].as_str().unwrap()
            ),
            &owner,
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        post(&router, &endpoint, &connector, input).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Audited erasure must still remove the new immutable child rows.
    let actor: Uuid =
        sqlx::query_scalar("select id from identity where email='connector-owner@example.test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("update document set deleted_at=now() where id=$1")
        .bind(doc)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'documents',$2,'owner_request','{}')").bind(doc).bind(actor).execute(&mut *tx).await.unwrap();
    sqlx::query("delete from document where id=$1")
        .bind(doc)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        get(
            &router,
            &format!("/documents/{doc}/provenance/bundles/{bundle}"),
            &owner
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn connector_rechecks_current_access_sessions_expiry_and_private_account_erasure(
    pool: PgPool,
) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "grant-owner@example.test").await;
    let author = identity(&pool, "grant-author@example.test").await;
    let (doc, block) = document(&router, &owner).await;
    let members = format!("/documents/{doc}/members");
    assert_eq!(
        post(
            &router,
            &members,
            &owner,
            json!({"email":"grant-author@example.test","role":"contributor"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let grant = post(
        &router,
        &format!("/documents/{doc}/connectors"),
        &author,
        json!({"host":"google-docs"}),
    )
    .await
    .1;
    let key = format!("Bearer {}", grant["token"].as_str().unwrap());
    let endpoint = format!("/connector/documents/{doc}/checkpoint");
    let checkpoint = get(&router, &endpoint, &key).await.1;
    let credential = Uuid::parse_str(grant["grant"]["id"].as_str().unwrap()).unwrap();
    sqlx::query("update connector_grant set expires_at=now()-interval '1 second' where id=$1")
        .bind(credential)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get(&router, &endpoint, &key).await.0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("update connector_grant set expires_at=now()+interval '1 day' where id=$1")
        .bind(credential)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("update identity set session_generation=1 where email='grant-author@example.test'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get(&router, &endpoint, &key).await.0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("update identity set session_generation=0 where email='grant-author@example.test'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        post(
            &router,
            &members,
            &owner,
            json!({"email":"grant-author@example.test","role":"remove"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(get(&router, &endpoint, &key).await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        post(
            &router,
            &members,
            &owner,
            json!({"email":"grant-author@example.test","role":"contributor"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, receipt) = post(
        &router,
        &format!("/connector/documents/{doc}/bundles"),
        &key,
        upload(doc, &checkpoint, &block, "google-docs", false),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let actor: Uuid =
        sqlx::query_scalar("select id from identity where email='grant-author@example.test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let manager: Uuid =
        sqlx::query_scalar("select id from identity where email='grant-owner@example.test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'users',$2,'owner_request','{}')").bind(actor).bind(manager).execute(&mut *tx).await.unwrap();
    sqlx::query("update identity set erased_at=now(),disabled_at=now(),session_generation=1,email=$2,display_name=null where id=$1").bind(actor).bind(format!("erased-{actor}@invalid.test")).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        get(&router, &endpoint, &key).await.0,
        StatusCode::UNAUTHORIZED
    );
    let remaining: i64 =
        sqlx::query_scalar("select count(*) from provenance_bundle where actor_id=$1")
            .bind(actor)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0);
}
