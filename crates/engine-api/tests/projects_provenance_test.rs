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

mod common;

const KEY: [u8; 32] = [9u8; 32];

#[sqlx::test(migrations = "../../migrations")]
async fn project_floor_applies_to_file_owners_and_existing_overrides(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(test_state(runtime.pool.clone()));
    let manager = identity(&pool, "floor-manager@example.test").await;
    let creator = identity(&pool, "floor-creator@example.test").await;
    let (_, project) = post(&router, "/projects", &manager, json!({"name":"Protected"})).await;
    let project = project["id"].as_str().unwrap();
    assert_eq!(
        post(
            &router,
            &format!("/spaces/{project}/members"),
            &manager,
            json!({"email":"floor-creator@example.test","role":"editor"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(post(&router, &format!("/projects/{project}/settings"), &manager,
        json!({"name":"Protected","description":"","revision":1,"protect_team_version":true,"required_approvals":1,"merge_roles":"editors"})).await.0, StatusCode::OK);
    let (doc, block) = document(&router, &creator).await;
    let weak = json!({"protect_team_version":false,"required_approvals":0,"merge_roles":"editors"});
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/policy"),
            &creator,
            weak.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/location"),
            &creator,
            json!({"space_id":project})
        )
        .await
        .0,
        StatusCode::OK
    );
    let people = get(&router, &format!("/documents/{doc}/people"), &creator)
        .await
        .1;
    assert_eq!(people["you"]["role"], "owner");
    for capability in ["edit_team", "manage_rules", "archive", "edit_settings"] {
        assert_eq!(people["you"]["can"][capability], false, "{people}");
    }
    assert_eq!(people["policy"]["protect_team_version"], true);
    assert_eq!(people["policy"]["required_approvals"], 1);
    assert_eq!(people["policy_source"]["file_rules"], weak);
    for (suffix, body) in [
        ("policy", weak.clone()),
        ("policy/inherit", json!({})),
        ("metadata", json!({"settings":{"kind":"presentation"}})),
        ("metadata", json!({"title":"Bypass"})),
        ("metadata", json!({"archived":true})),
        ("location", json!({"space_id":null})),
        (
            "batch",
            json!({"base_seq":2,"ops":[{"type":"FieldEdited","node_id":block,"field":"label","value":"Bypass"}]}),
        ),
    ] {
        let result = post(
            &router,
            &format!("/documents/{doc}/{suffix}"),
            &creator,
            body,
        )
        .await;
        assert_eq!(result.0, StatusCode::FORBIDDEN, "{suffix}: {}", result.1);
    }
    // Even project managers change the floor in Project settings, not via a file exception.
    assert_eq!(
        post(&router, &format!("/documents/{doc}/policy"), &manager, weak)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let strong = json!({"protect_team_version":true,"required_approvals":2,"merge_roles":"owners"});
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/policy"),
            &manager,
            strong.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    let people = get(&router, &format!("/documents/{doc}/people"), &creator)
        .await
        .1;
    assert_eq!(people["policy"], strong);
    assert_eq!(people["you"]["can"]["merge"], false);
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/policy/inherit"),
            &manager,
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    let checkpoint = get(&router, &format!("/documents/{doc}/provenance"), &creator)
        .await
        .1;
    let proposed = post(
        &router,
        &format!("/documents/{doc}/provenance"),
        &creator,
        upload(doc, &checkpoint, &block, "word", true),
    )
    .await;
    assert_eq!(proposed.0, StatusCode::OK, "{}", proposed.1);
    let draft = proposed.1["id"].as_str().unwrap();
    let endpoint = format!("/documents/{doc}/change-requests/{draft}");
    let detail = get(&router, &endpoint, &creator).await.1;
    assert_eq!(detail["review"]["can_merge"], true);
    assert_eq!(detail["review"]["can_override"], false);
    let merge =
        json!({"team_seq":detail["team_seq"],"revision":detail["revision"],"override_rules":true});
    assert_eq!(
        post(
            &router,
            &format!("{endpoint}/merge"),
            &creator,
            merge.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let result = post(&router, &format!("{endpoint}/merge"), &manager, merge).await;
    assert_eq!(result.0, StatusCode::OK, "{}", result.1);
    // Project authority follows folders and the destination when managers move a file.
    let (_, folder) = post(
        &router,
        "/spaces",
        &manager,
        json!({"name":"Nested","kind":"folder","parent_id":project}),
    )
    .await;
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/location"),
            &manager,
            json!({"space_id":folder["id"]})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        get(&router, &format!("/documents/{doc}/people"), &creator)
            .await
            .1["policy_source"]["id"],
        project
    );
    assert_eq!(
        post(
            &router,
            &format!("/spaces/{}/members", folder["id"].as_str().unwrap()),
            &manager,
            json!({"email":"floor-creator@example.test","role":"owner"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        get(&router, &format!("/documents/{doc}/people"), &creator)
            .await
            .1["you"]["can"]["manage_rules"],
        false
    );
    let (_, second) = post(&router, "/projects", &manager, json!({"name":"Second"})).await;
    let second = second["id"].as_str().unwrap();
    assert_eq!(post(&router, &format!("/projects/{second}/settings"), &manager,
        json!({"name":"Second","description":"","revision":1,"protect_team_version":true,"required_approvals":3,"merge_roles":"owners"})).await.0, StatusCode::OK);
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/location"),
            &manager,
            json!({"space_id":second})
        )
        .await
        .0,
        StatusCode::OK
    );
    let moved = get(&router, &format!("/documents/{doc}/people"), &creator)
        .await
        .1;
    assert_eq!(moved["policy_source"]["id"], second);
    assert_eq!(moved["policy"]["required_approvals"], 3);
    assert_eq!(moved["you"]["can"]["merge"], false);
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/location"),
            &manager,
            json!({"space_id":null})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        get(&router, &format!("/documents/{doc}/people"), &creator)
            .await
            .1["you"]["can"]["edit_team"],
        true
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn metadata_authority_schema_and_audit_are_atomic(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(test_state(runtime.pool.clone()));
    let owner = identity(&pool, "metadata-owner@example.test").await;
    let editor = identity(&pool, "metadata-editor@example.test").await;
    let stranger = identity(&pool, "metadata-stranger@example.test").await;
    let (doc, _) = document(&router, &owner).await;
    let (private, _) = document(&router, &stranger).await;
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/members"),
            &owner,
            json!({"email":"metadata-editor@example.test","role":"editor"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let uri = format!("/documents/{doc}/metadata");
    for body in [
        json!({"settings":{"kind":"presentation"}}),
        json!({"archived":true}),
    ] {
        assert_eq!(
            post(&router, &uri, &editor, body).await.0,
            StatusCode::FORBIDDEN
        );
    }
    for settings in [
        json!([]),
        json!({"merge_roles":"editors"}),
        json!({"kind":"wrong"}),
        json!({"import_warnings":[{}]}),
        json!({"research_source":{"token":"secret"}}),
        json!({"copied_from":doc}),
        json!({"import_warnings":["x".repeat(65_537)]}),
    ] {
        assert_eq!(
            post(&router, &uri, &owner, json!({"settings":settings}))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        post(
            &router,
            &uri,
            &owner,
            json!({"settings":{"copied_from":private}})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(&router, &uri, &owner, json!({"protect_team_version":false}))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        post(
            &router,
            &uri,
            &owner,
            json!({"settings":{"import_warnings":["Imported subset"]}})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        post(&router, &uri, &editor, json!({"title":" Renamed "}))
            .await
            .0,
        StatusCode::OK
    );
    let current = get(&router, &format!("/documents/{doc}"), &owner).await.1;
    assert_eq!(current["settings"]["kind"], "document");
    assert_eq!(
        current["settings"]["import_warnings"],
        json!(["Imported subset"])
    );
    let audit: Value = sqlx::query_scalar("select detail from operation_audit where resource_id=$1 and action='document.metadata_changed' order by id desc limit 1")
        .bind(doc).fetch_one(&pool).await.unwrap();
    assert_eq!(audit["format"], "dynodoc.metadata-audit.v1");
    assert_eq!(audit["fields"], json!(["title"]));
    assert_eq!(audit["settings_keys"], json!([]));
    assert_ne!(audit["before_hash"], audit["after_hash"]);
    for key in ["before_hash", "after_hash"] {
        assert_eq!(audit[key].as_str().unwrap().len(), 64);
    }
    assert!(!audit.to_string().contains("Renamed"));
    assert!(!audit.to_string().contains("Imported subset"));
    let before: i64 =
        sqlx::query_scalar("select count(*) from operation_audit where resource_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post(&router, &uri, &editor, json!({"title":"Renamed"}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        post(&router, &uri, &owner, json!({})).await.0,
        StatusCode::OK
    );
    let after: i64 =
        sqlx::query_scalar("select count(*) from operation_audit where resource_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    // Make audit insertion fail after the metadata UPDATE. Both must roll back.
    sqlx::raw_sql("alter table operation_audit add constraint reject_test_metadata check(action <> 'document.metadata_changed') not valid").execute(&pool).await.unwrap();
    assert_eq!(
        post(&router, &uri, &owner, json!({"title":"Must roll back"}))
            .await
            .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        get(&router, &format!("/documents/{doc}"), &owner).await.1["title"],
        "Renamed"
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn policy_and_move_wait_for_project_authority_changes(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "race-owner@example.test").await;
    let creator = identity(&pool, "race-creator@example.test").await;
    let (_, p) = post(&router, "/projects", &owner, json!({"name":"Race"})).await;
    let project = Uuid::parse_str(p["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        post(
            &router,
            &format!("/spaces/{project}/members"),
            &owner,
            json!({"email":"race-creator@example.test","role":"manager"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (doc, _) = document(&router, &creator).await;
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/location"),
            &creator,
            json!({"space_id":project})
        )
        .await
        .0,
        StatusCode::OK
    );
    for moving in [false, true] {
        let mut tx = pool.begin().await.unwrap();
        let pid: i32 = sqlx::query_scalar("select pg_backend_pid()")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        sqlx::query("select id from access_space where id=$1 for update")
            .bind(project)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        if moving {
            sqlx::query("update space_member set role='editor' where space_id=$1 and identity_id=(select id from identity where email='race-creator@example.test')").bind(project).execute(&mut *tx).await.unwrap();
        } else {
            sqlx::query("update project_settings set required_approvals=2 where space_id=$1")
                .bind(project)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        let router = router.clone();
        let actor = creator.clone();
        let pending = tokio::spawn(async move {
            if moving {
                post(
                    &router,
                    &format!("/documents/{doc}/location"),
                    &actor,
                    json!({"space_id":null}),
                )
                .await
            } else {
                post(&router, &format!("/documents/{doc}/policy"), &actor, json!({"protect_team_version":true,"required_approvals":0,"merge_roles":"editors"})).await
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let blocked: bool = sqlx::query_scalar("select exists(select 1 from pg_stat_activity where $1=any(pg_blocking_pids(pid)))").bind(pid).fetch_one(&pool).await.unwrap();
                if blocked { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("request must wait on the project lock");
        tx.commit().await.unwrap();
        let result = pending.await.unwrap();
        assert_eq!(
            result.0,
            if moving {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::BAD_REQUEST
            },
            "{}",
            result.1
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn browser_pairing_keeps_secrets_in_editor_and_rechecks_permissions(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "pair-owner@example.test").await;
    let viewer = identity(&pool, "pair-viewer@example.test").await;
    let (doc, _) = document(&router, &owner).await;
    let (other, _) = document(&router, &owner).await;
    post(
        &router,
        &format!("/documents/{doc}/members"),
        &owner,
        json!({"email":"pair-viewer@example.test","role":"viewer"}),
    )
    .await;
    let (secret, hash) = engine_api::auth::random_token().unwrap();
    let challenge = hex::encode(&hash);
    let bearer = format!("Bearer {secret}");
    // A browser clock 50 seconds ahead still gets its ten-minute approval window.
    let input = json!({"host":"word","key_challenge":challenge,"expires_at":chrono::Utc::now().timestamp()+650});
    let endpoint = format!("/documents/{doc}/connectors");
    assert_eq!(
        get(&router, "/connector/connection", &bearer).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post(&router, &endpoint, &viewer, input.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    let (first, second) = tokio::join!(
        post(&router, &endpoint, &owner, input.clone()),
        post(&router, &endpoint, &owner, input.clone())
    );
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(second.0, StatusCode::OK, "{}", second.1);
    assert_eq!(first.1["grant"], second.1["grant"]);
    assert!(first.1["token"].is_null());
    let stored: Vec<u8> =
        sqlx::query_scalar("select token_hash from connector_grant where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, hash);
    assert_eq!(
        get(
            &router,
            "/connector/connection",
            &format!("Bearer {challenge}")
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, connected) = get(&router, "/connector/connection", &bearer).await;
    assert_eq!(status, StatusCode::OK, "{connected}");
    assert_eq!(connected["document_id"], doc.to_string());
    assert_eq!(connected["host"], "word");
    assert_eq!(
        get(
            &router,
            &format!("/connector/documents/{other}/checkpoint"),
            &bearer
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(
            &router,
            &format!("/documents/{other}/connectors"),
            &owner,
            input.clone()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let mut wrong_host = input.clone();
    wrong_host["host"] = json!("excel");
    assert_eq!(
        post(&router, &endpoint, &owner, wrong_host).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut expired = input.clone();
    expired["expires_at"] = json!(chrono::Utc::now().timestamp() + 720);
    assert_eq!(
        post(&router, &endpoint, &owner, expired.clone()).await.0,
        StatusCode::BAD_REQUEST
    );
    expired["expires_at"] = json!(chrono::Utc::now().timestamp() - 1);
    assert_eq!(
        post(&router, &endpoint, &owner, expired).await.0,
        StatusCode::BAD_REQUEST
    );
    let grant = first.1["grant"]["id"].as_str().unwrap();
    post(
        &router,
        &format!("/documents/{doc}/connectors/{grant}/revoke"),
        &owner,
        json!({}),
    )
    .await;
    assert_eq!(
        get(&router, "/connector/connection", &bearer).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post(&router, &endpoint, &owner, input).await.0,
        StatusCode::CONFLICT
    );
}

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
    let root = ulid::Ulid::generate().to_string();
    let block = ulid::Ulid::generate().to_string();
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
async fn structured_editors_keep_host_file_kind_and_retry_scope(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "structured-connectors@example.test").await;
    for (kind, hosts, field) in [
        ("spreadsheet", ["excel", "google-sheets"], "cell_0"),
        ("presentation", ["powerpoint", "google-slides"], "text"),
    ] {
        let (doc, block) = document(&router, &owner).await;
        assert_eq!(
            post(
                &router,
                &format!("/documents/{doc}/metadata"),
                &owner,
                json!({"settings":{"kind":kind}})
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            post(
                &router,
                &format!("/documents/{doc}/connectors"),
                &owner,
                json!({"host":"word"})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let checkpoint = get(&router, &format!("/documents/{doc}/provenance"), &owner)
            .await
            .1;
        for (i, host) in hosts.iter().enumerate() {
            let (status, grant) = post(
                &router,
                &format!("/documents/{doc}/connectors"),
                &owner,
                json!({"host":host}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{grant}");
            let key = format!("Bearer {}", grant["token"].as_str().unwrap());
            let mut bundle = upload(doc, &checkpoint, &block, host, true);
            bundle["bundle"]["changes"][0]["operation"] = json!({"type":"FieldEdited","node_id":block,"field":field,"value":if kind=="spreadsheet" {"=SUM(A1:A2)"} else {"Edited slide text"}});
            let mut wrong_host = bundle.clone();
            wrong_host["bundle"]["capture"]["host"] = json!(hosts[1 - i]);
            assert_eq!(
                post(
                    &router,
                    &format!("/connector/documents/{doc}/bundles"),
                    &key,
                    wrong_host
                )
                .await
                .0,
                StatusCode::FORBIDDEN
            );
            let (status, receipt) = post(
                &router,
                &format!("/connector/documents/{doc}/bundles"),
                &key,
                bundle.clone(),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{receipt}");
            let retry = post(
                &router,
                &format!("/connector/documents/{doc}/bundles"),
                &key,
                bundle,
            )
            .await;
            assert_eq!(retry.0, StatusCode::OK);
            assert_eq!(retry.1, receipt);
            let current = get(
                &router,
                &format!("/connector/documents/{doc}/checkpoint"),
                &key,
            )
            .await
            .1;
            assert_eq!(current["through_seq"], checkpoint["through_seq"]);
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
                get(
                    &router,
                    &format!("/connector/documents/{doc}/checkpoint"),
                    &key
                )
                .await
                .0,
                StatusCode::UNAUTHORIZED
            );
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_checkpoints_retain_exact_anchors_and_current_access(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "historical-owner@example.test").await;
    let stranger = identity(&pool, "historical-outsider@example.test").await;
    let (doc, block) = document(&router, &owner).await;
    let (other, _) = document(&router, &owner).await;
    let original = get(&router, &format!("/documents/{doc}/provenance"), &owner)
        .await
        .1;
    let grant = post(
        &router,
        &format!("/documents/{doc}/connectors"),
        &owner,
        json!({"host":"word"}),
    )
    .await
    .1;
    let key = format!("Bearer {}", grant["token"].as_str().unwrap());
    let old_seq = original["through_seq"].as_i64().unwrap();
    let (status, _) = post(&router, &format!("/documents/{doc}/batch"), &owner, json!({"base_seq":old_seq,"ops":[{"type":"FieldEdited","node_id":block,"field":"content","value":{"type":"paragraph","content":[{"type":"text","text":"New team text"}]}}]})).await;
    assert_eq!(status, StatusCode::OK);
    let endpoint = format!("/connector/documents/{doc}/checkpoint");
    let (status, historical) =
        get(&router, &format!("{endpoint}?through_seq={old_seq}"), &key).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(historical, original);
    let current = get(&router, &endpoint, &key).await.1;
    assert_eq!(current["through_seq"], old_seq + 1);
    assert_ne!(current["chain_hash"], historical["chain_hash"]);
    assert_eq!(
        current["state"]["nodes"][&block]["current_fields"]["content"]["content"][0]["text"],
        "New team text"
    );
    let zero = get(&router, &format!("{endpoint}?through_seq=0"), &key)
        .await
        .1;
    assert_eq!(zero["chain_hash"], "0".repeat(64));
    assert_eq!(zero["state"]["nodes"], json!({}));
    assert_eq!(
        get(
            &router,
            &format!("/documents/{doc}/provenance?through_seq={old_seq}"),
            &owner
        )
        .await
        .1,
        historical
    );
    for seq in [
        "-1".to_owned(),
        (old_seq + 2).to_string(),
        "1.5".to_owned(),
        "invalid".to_owned(),
    ] {
        assert_eq!(
            get(&router, &format!("{endpoint}?through_seq={seq}"), &key)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        get(
            &router,
            &format!("/documents/{doc}/provenance?through_seq={old_seq}"),
            &stranger
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(
            &router,
            &format!("/connector/documents/{other}/checkpoint?through_seq={old_seq}"),
            &key
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(&router, &endpoint, &key).await.1["through_seq"],
        current["through_seq"]
    );
    post(
        &router,
        &format!(
            "/documents/{doc}/connectors/{}/revoke",
            grant["grant"]["id"].as_str().unwrap()
        ),
        &owner,
        json!({}),
    )
    .await;
    assert_eq!(
        get(&router, &format!("{endpoint}?through_seq={old_seq}"), &key)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
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
        json!({"protect_team_version":true,"required_approvals":2,"merge_roles":"owners"});
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
        "project"
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
    let runtime = common::RuntimeDatabase::new(pool).await;
    let pool = runtime.pool.clone();
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
    invalid["bundle"]["changes"].as_array_mut().unwrap().push(json!({"id":Uuid::new_v4(),"observed_at":"2026-10-06T01:02:04Z","operation":{"type":"FieldEdited","node_id":ulid::Ulid::generate().to_string(),"field":"label","value":"Missing node"}}));
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
    // Audited erasure must still remove the new immutable child rows under the
    // real runtime login, which cannot directly DELETE these tables.
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
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn connector_rechecks_current_access_sessions_expiry_and_private_account_erasure(
    pool: PgPool,
) {
    let runtime = common::RuntimeDatabase::new(pool).await;
    let pool = runtime.pool.clone();
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
    runtime.close().await;
}
