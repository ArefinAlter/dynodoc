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

/// A deleted paragraph and a live sibling, followed by 55 independent saves.
async fn recovery_fixture(pool: &PgPool) -> (Router, String, Uuid, String, String) {
    let router = app(test_state(pool.clone()));
    let owner = identity(pool, "recovery-owner@example.test").await;
    let (status, doc) = post(&router, "/documents", &owner, json!({"title":"Recovery"})).await;
    assert_eq!(status, StatusCode::OK, "{doc}");
    let doc = Uuid::parse_str(doc["id"].as_str().unwrap()).unwrap();
    let root = ulid::Ulid::new().to_string();
    let old = ulid::Ulid::new().to_string();
    let live = ulid::Ulid::new().to_string();
    let (status, body) = post(&router, &format!("/documents/{doc}/batch"), &owner, json!({"base_seq":0,"ops":[
        {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}},
        {"type":"NodeCreated","node_id":old,"node_type":"item","parent_id":root,"pos":"a0","fields":{"label":"Bring this paragraph back"}},
        {"type":"NodeCreated","node_id":live,"node_type":"item","parent_id":root,"pos":"a1","fields":{"label":"Original sibling"}}
    ]})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    engine_core::shared_checkpoint::postgres::take(
        pool,
        engine_shared::DocumentId(doc),
        3,
        Default::default(),
    )
    .await
    .unwrap();
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/batch"),
        &owner,
        json!({"base_seq":3,"ops":[{"type":"NodeDeleted","node_id":old}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for i in 0..55 {
        let (status, body) = post(&router, &format!("/documents/{doc}/batch"), &owner,
            json!({"base_seq":4+i,"ops":[{"type":"FieldEdited","node_id":live,"field":"label","value":format!("Later save {i}")}]})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    (router, owner, doc, old, live)
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_recovery_preserves_later_work_and_obeys_review_permissions(pool: PgPool) {
    let (router, owner, doc, old, live) = recovery_fixture(&pool).await;
    let contributor = identity(&pool, "recover-contributor@example.test").await;
    let viewer = identity(&pool, "recover-viewer@example.test").await;
    let editor = identity(&pool, "recover-editor@example.test").await;
    let outsider = identity(&pool, "recover-outsider@example.test").await;
    for (email, role) in [
        ("recover-contributor@example.test", "contributor"),
        ("recover-viewer@example.test", "viewer"),
        ("recover-editor@example.test", "editor"),
    ] {
        let (status, body) = post(
            &router,
            &format!("/documents/{doc}/members"),
            &owner,
            json!({"email":email,"role":role}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    sqlx::query("insert into document_policy(document_id,protect_team_version,required_approvals,merge_roles) values($1,true,1,'owners')")
        .bind(doc).execute(&pool).await.unwrap();
    let endpoint = format!("/documents/{doc}/history-recovery");
    let (status, historical) = get(
        &router,
        &format!("/documents/{doc}/provenance?through_seq=3"),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(historical["state"]["nodes"][&old]["deleted"], false);
    assert_eq!(
        historical["state"]["nodes"][&live]["current_fields"]["label"],
        "Original sibling"
    );
    let (status, preview) = post(
        &router,
        &endpoint,
        &viewer,
        json!({"through_seq":3,"node_ids":[old]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["base_seq"], 59);
    assert_eq!(preview["can_propose"], false);
    assert_eq!(
        preview["ops"],
        json!([{"type":"NodeRestored","node_id":old}])
    );
    let request_id = Uuid::new_v4();
    let request = json!({"action":"create","through_seq":3,"base_seq":59,"node_ids":[old],"request_id":request_id});
    for person in [&viewer, &outsider] {
        assert_eq!(
            post(&router, &endpoint, person, request.clone()).await.0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        post(&router, &endpoint, &outsider, json!({"through_seq":3}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(
            &router,
            &format!("/documents/{doc}/provenance?through_seq=3"),
            &outsider
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (left, right) = tokio::join!(
        post(&router, &endpoint, &contributor, request.clone()),
        post(&router, &endpoint, &contributor, request.clone())
    );
    assert_eq!(left.0, StatusCode::OK, "{:?}", left);
    assert_eq!(right.0, StatusCode::OK, "{:?}", right);
    assert_ne!(left.1["created"], right.1["created"]);
    assert_eq!(left.1["id"], right.1["id"]);
    assert_eq!(left.1["source"]["chain_hash"], historical["chain_hash"]);
    let uri = format!("/documents/{doc}/change-requests/{request_id}");
    assert_eq!(
        get(&router, &uri, &owner).await.0,
        StatusCode::NOT_FOUND,
        "recovery draft must be private"
    );
    let count: i64 = sqlx::query_scalar("select count(*) from event where document_id=$1")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 59,
        "creating and retrying recovery cannot alter canonical history"
    );
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/drafts/{request_id}/submit"),
        &contributor,
        json!({"note":"Recover the earlier wording"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let merge = json!({"team_seq":59,"revision":1});
    for person in [&contributor, &editor, &owner] {
        assert_eq!(
            post(&router, &format!("{uri}/merge"), person, merge.clone())
                .await
                .0,
            StatusCode::FORBIDDEN,
            "role and required approvals must still apply"
        );
    }
    let (status, body) = post(
        &router,
        &format!("{uri}/merge"),
        &owner,
        json!({"team_seq":59,"revision":1,"override_rules":true,"note":"Owner approved recovery"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["events"][0]["payload"],
        json!({"type":"NodeRestored","node_id":old})
    );
    let (_, current) = get(&router, &format!("/documents/{doc}/provenance"), &viewer).await;
    assert_eq!(current["through_seq"], 60);
    assert_eq!(current["state"]["nodes"][&old]["deleted"], false);
    assert_eq!(
        current["state"]["nodes"][&live]["current_fields"]["label"],
        "Later save 54"
    );
    let (_, unchanged) = get(
        &router,
        &format!("/documents/{doc}/provenance?through_seq=3"),
        &viewer,
    )
    .await;
    assert_eq!(unchanged["state"], historical["state"]);
    assert_eq!(unchanged["chain_hash"], historical["chain_hash"]);
    assert_eq!(
        get(&router, &format!("/documents/{doc}/verify"), &owner)
            .await
            .1["ok"],
        true
    );
    let (status, retry) = post(&router, &endpoint, &contributor, request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    assert_eq!(
        retry["created"], false,
        "receipt must survive merging and head advancement"
    );
    let mut changed = request.clone();
    changed["node_ids"] = json!([live]);
    assert_eq!(
        post(&router, &endpoint, &contributor, changed).await.0,
        StatusCode::CONFLICT
    );
    post(
        &router,
        &format!("/documents/{doc}/members"),
        &owner,
        json!({"email":"recover-contributor@example.test","role":"remove"}),
    )
    .await;
    assert_eq!(
        post(&router, &endpoint, &contributor, request).await.0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_recovery_rejects_stale_invalid_and_incomplete_selections(pool: PgPool) {
    let (router, owner, doc, old, live) = recovery_fixture(&pool).await;
    let endpoint = format!("/documents/{doc}/history-recovery");
    for body in [
        json!({"through_seq":-1}),
        json!({"through_seq":60}),
        json!({"through_seq":3,"node_ids":[]}),
        json!({"through_seq":3,"node_ids":[old,old]}),
        json!({"through_seq":3,"node_ids":["missing"]}),
    ] {
        assert_eq!(
            post(&router, &endpoint, &owner, body).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let forged = json!({"name":"Forged recovery","base_seq":59,
        "source":{"kind":"history_recovery","through_seq":3,"chain_hash":"unverified"},
        "ops":[{"type":"FieldEdited","node_id":live,"field":"label","value":"Not actually historical"}]});
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc}/change-requests"),
            &owner,
            forged
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let create = json!({"action":"create","through_seq":3,"base_seq":58,"node_ids":[old],"request_id":Uuid::new_v4()});
    assert_eq!(
        post(&router, &endpoint, &owner, create).await.0,
        StatusCode::CONFLICT
    );
    let (status, all) = post(&router, &endpoint, &owner, json!({"through_seq":3})).await;
    assert_eq!(status, StatusCode::OK, "{all}");
    assert_eq!(
        all["ops"].as_array().unwrap().len(),
        2,
        "whole content preview includes the changed sibling"
    );
    // Selecting a parent for removal without its live child must fail atomically.
    let parent: String = sqlx::query_scalar("select parent_id from node where id=$1")
        .bind(&live)
        .fetch_one(&pool)
        .await
        .unwrap();
    let invalid = json!({"action":"create","through_seq":0,"base_seq":59,"node_ids":[parent],"request_id":Uuid::new_v4()});
    assert_eq!(
        post(&router, &endpoint, &owner, invalid).await.0,
        StatusCode::BAD_REQUEST
    );
    let noop =
        json!({"action":"create","through_seq":59,"base_seq":59,"request_id":Uuid::new_v4()});
    assert_eq!(
        post(&router, &endpoint, &owner, noop).await.0,
        StatusCode::BAD_REQUEST
    );
    let count: i64 =
        sqlx::query_scalar("select count(*) from workspace_draft where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_recovery_conflicts_with_later_edits_instead_of_overwriting_them(pool: PgPool) {
    let (router, owner, doc, _old, live) = recovery_fixture(&pool).await;
    let recovery = Uuid::new_v4();
    let (status, body) = post(&router, &format!("/documents/{doc}/history-recovery"), &owner,
        json!({"action":"create","through_seq":3,"base_seq":59,"node_ids":[live],"request_id":recovery})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(&router, &format!("/documents/{doc}/batch"), &owner,
        json!({"base_seq":59,"ops":[{"type":"FieldEdited","node_id":live,"field":"label","value":"Concurrent wording after preview"}]})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/drafts/{recovery}/submit"),
        &owner,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let uri = format!("/documents/{doc}/change-requests/{recovery}");
    let (status, detail) = get(&router, &uri, &owner).await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert!(!detail["conflicts"].as_array().unwrap().is_empty());
    let (status, body) = post(
        &router,
        &format!("{uri}/merge"),
        &owner,
        json!({"team_seq":60,"revision":1}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (_, current) = get(&router, &format!("/documents/{doc}/provenance"), &owner).await;
    assert_eq!(current["through_seq"], 60);
    assert_eq!(
        current["state"]["nodes"][&live]["current_fields"]["label"],
        "Concurrent wording after preview"
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
fn para(text: &str) -> Value {
    json!({"type":"paragraph","content":[{"type":"text","text":text}]})
}
fn edit(node: &str, text: &str) -> Vec<Value> {
    vec![
        json!({"type":"FieldEdited","node_id":node,"field":"content","value":para(text)}),
        json!({"type":"FieldEdited","node_id":node,"field":"label","value":text}),
    ]
}

#[sqlx::test(migrations = "../../migrations")]
async fn pushed_files_become_reviewable_change_requests(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "owner-cr@example.test").await;
    let alice = identity(&pool, "alice-cr@example.test").await;
    let bob = identity(&pool, "bob-cr@example.test").await;
    let (_, created) = post(
        &router,
        "/documents",
        &owner,
        json!({"title":"Group report"}),
    )
    .await;
    let doc = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    for (email, role) in [
        ("alice-cr@example.test", "reviewer"),
        ("bob-cr@example.test", "author"),
    ] {
        let (status, body) = post(
            &router,
            &format!("/documents/{doc}/members"),
            &owner,
            json!({"email":email,"role":role}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let root = ulid::Ulid::new().to_string();
    let p: Vec<String> = (0..3).map(|_| ulid::Ulid::new().to_string()).collect();
    let mut ops = vec![
        json!({"type":"NodeCreated","node_id":root,"node_type":"form","pos":"000000","fields":{"label":"Group report"}}),
    ];
    for (i, id) in p.iter().enumerate() {
        let text = format!("Paragraph {i}");
        ops.push(json!({"type":"NodeCreated","node_id":id,"node_type":"item","parent_id":root,"pos":format!("0000000{i}"),"fields":{"label":text,"content":para(&text)}}));
    }
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/batch"),
        &owner,
        json!({"base_seq":0,"ops":ops}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, points) = get(&router, &format!("/documents/{doc}/history-points"), &owner).await;
    assert_eq!(points["initial_seq"], 4, "{points}");
    // The team edits paragraph 0 after everyone downloaded the original file.
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/batch"),
        &owner,
        json!({"base_seq":4,"ops":edit(&p[0], "Team intro")}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, old) = get(
        &router,
        &format!("/documents/{doc}/state?through_seq=4"),
        &alice,
    )
    .await;
    assert_eq!(
        old["state"]["nodes"][&p[0]]["current_fields"]["label"],
        "Paragraph 0"
    );
    let (status, _) = get(
        &router,
        &format!("/documents/{doc}/state?through_seq=99"),
        &alice,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Alice (comment-and-suggest role) pushes her copy from revision 4.
    let added = ulid::Ulid::new().to_string();
    let mut alice_ops = edit(&p[1], "Alice method");
    alice_ops.push(json!({"type":"NodeCreated","node_id":added,"node_type":"item","parent_id":root,"pos":"00000003","fields":{"label":"Alice appendix","content":para("Alice appendix")}}));
    let (status, alice_cr) = post(&router, &format!("/documents/{doc}/change-requests"), &alice, json!({"name":"Alice edits","note":"From my laptop","base_seq":4,"ops":alice_ops,"submit":true,"source":{"kind":"file","filename":"report.docx"}})).await;
    assert_eq!(status, StatusCode::OK, "{alice_cr}");
    let alice_id = alice_cr["id"].as_str().unwrap().to_string();
    // Bob (editor) overlaps the team's paragraph 0 edit and also edits paragraph 2.
    let mut bob_ops = edit(&p[0], "Bob intro");
    bob_ops.extend(edit(&p[2], "Bob results"));
    let (status, bob_cr) = post(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &bob,
        json!({"name":"Bob edits","base_seq":4,"ops":bob_ops,"submit":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bob_cr}");
    let bob_id = bob_cr["id"].as_str().unwrap().to_string();

    let (_, listed) = get(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &alice,
    )
    .await;
    let items = listed["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{listed}");
    assert!(items.iter().all(|r| r["status"] == "open"));

    let uri = format!("/documents/{doc}/change-requests/{alice_id}");
    let (_, review) = get(&router, &uri, &owner).await;
    assert!(
        review["conflicts"].as_array().unwrap().is_empty(),
        "{review}"
    );
    assert!(review["behind"].as_i64().unwrap() > 0, "{review}");
    assert_eq!(
        review["nodes"][&p[1]]["draft"]["current_fields"]["label"],
        "Alice method"
    );
    let body = json!({"team_seq":review["team_seq"],"revision":review["revision"]});
    // Reviewers cannot merge, not even their own request.
    let (status, _) = post(&router, &format!("{uri}/merge"), &alice, body.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, merged) = post(&router, &format!("{uri}/merge"), &owner, body).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(merged["completed"], true);
    let (_, team) = get(&router, &format!("/documents/{doc}"), &owner).await;
    let label = |id: &str| team["state"]["nodes"][id]["current_fields"]["label"].clone();
    assert_eq!(label(&p[0]), "Team intro", "the team's newer edit survives");
    assert_eq!(label(&p[1]), "Alice method");
    assert_eq!(label(&added), "Alice appendix");

    // Bob's overlap must be resolved; a partial merge keeps the rest open.
    let uri = format!("/documents/{doc}/change-requests/{bob_id}");
    let (_, review) = get(&router, &uri, &owner).await;
    let conflicts = review["conflicts"].as_array().unwrap().clone();
    assert!(
        conflicts.iter().any(|c| c["node_id"] == p[0].as_str()),
        "{review}"
    );
    let body = json!({"team_seq":review["team_seq"],"revision":review["revision"],"included_nodes":[p[0]]});
    let (status, _) = post(&router, &format!("{uri}/merge"), &owner, body).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let resolutions: serde_json::Map<String, Value> = conflicts
        .iter()
        .map(|c| (c["key"].as_str().unwrap().to_string(), json!("draft")))
        .collect();
    let body = json!({"team_seq":review["team_seq"],"revision":review["revision"],"included_nodes":[p[0]],"resolutions":resolutions});
    let (status, partial) = post(&router, &format!("{uri}/merge"), &owner, body).await;
    assert_eq!(status, StatusCode::OK, "{partial}");
    assert_eq!(partial["completed"], false);
    assert!(partial["remaining"].as_u64().unwrap() > 0);
    let (_, review) = get(&router, &uri, &owner).await;
    assert!(
        review["conflicts"].as_array().unwrap().is_empty(),
        "{review}"
    );
    assert!(
        review["ops"]
            .as_array()
            .unwrap()
            .iter()
            .all(|op| op["node_id"] == p[2].as_str()),
        "{review}"
    );
    let (status, _) = post(
        &router,
        &format!("{uri}/decline"),
        &alice,
        json!({"note":"Not mine"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(
        &router,
        &format!("{uri}/decline"),
        &owner,
        json!({"note":"Results need sources"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, team) = get(&router, &format!("/documents/{doc}"), &owner).await;
    assert_eq!(
        team["state"]["nodes"][&p[0]]["current_fields"]["label"],
        "Bob intro"
    );
    assert_eq!(
        team["state"]["nodes"][&p[2]]["current_fields"]["label"],
        "Paragraph 2"
    );
    let (_, listed) = get(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &owner,
    )
    .await;
    let statuses: Vec<String> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap().to_string())
        .collect();
    assert!(
        statuses.contains(&"merged".into()) && statuses.contains(&"declined".into()),
        "{listed}"
    );

    // Rebase: an old private copy moves onto the team version, dropping one change.
    let mut own_ops = edit(&p[1], "Owner rewrite");
    own_ops.extend(edit(&p[2], "Owner results"));
    let (status, draft) = post(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &owner,
        json!({"name":"Old laptop copy","base_seq":4,"ops":own_ops}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{draft}");
    let draft_id = draft["id"].as_str().unwrap();
    let sync = format!("/documents/{doc}/drafts/{draft_id}/sync");
    let (_, preview) = post(&router, &sync, &owner, json!({"action":"preview"})).await;
    assert!(
        preview["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["node_id"] == p[1].as_str()),
        "{preview}"
    );
    assert_eq!(preview["own"].as_array().unwrap().len(), 4, "{preview}");
    let body = json!({"action":"rebase","team_seq":preview["team_seq"],"revision":preview["revision"],"dropped_nodes":[p[1]]});
    let (status, rebased) = post(&router, &sync, &owner, body).await;
    assert_eq!(status, StatusCode::OK, "{rebased}");
    let rebased_label = |id: &str| rebased["state"]["nodes"][id]["current_fields"]["label"].clone();
    assert_eq!(rebased_label(&p[1]), "Alice method");
    assert_eq!(rebased_label(&p[2]), "Owner results");
    assert_eq!(rebased_label(&p[0]), "Bob intro");
    let (_, preview) = post(&router, &sync, &owner, json!({"action":"preview"})).await;
    assert!(preview["conflicts"].as_array().unwrap().is_empty());
    assert!(
        preview["ops"]
            .as_array()
            .unwrap()
            .iter()
            .all(|op| op["node_id"] == p[2].as_str()),
        "only the kept paragraph 2 edit remains: {preview}"
    );
    // Unsubmitted drafts stay private to their creator until submitted.
    let (status, _) = get(
        &router,
        &format!("/documents/{doc}/change-requests/{draft_id}"),
        &alice,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(
        &router,
        &format!("/documents/{doc}/drafts/{draft_id}/submit"),
        &owner,
        json!({"note":"Ready"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(
        &router,
        &format!("/documents/{doc}/change-requests/{draft_id}"),
        &alice,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post(
        &router,
        &format!("/documents/{doc}/drafts/{draft_id}/withdraw"),
        &owner,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, verification) = get(&router, &format!("/documents/{doc}/verify"), &owner).await;
    assert_eq!(verification["ok"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn viewers_cannot_push_and_outsiders_cannot_see_requests(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "owner-view@example.test").await;
    let viewer = identity(&pool, "viewer-view@example.test").await;
    let outsider = identity(&pool, "outsider-view@example.test").await;
    let (_, created) = post(&router, "/documents", &owner, json!({"title":"Private"})).await;
    let doc = created["id"].as_str().unwrap().to_string();
    post(
        &router,
        &format!("/documents/{doc}/members"),
        &owner,
        json!({"email":"viewer-view@example.test","role":"auditor"}),
    )
    .await;
    let root = ulid::Ulid::new().to_string();
    let op = json!({"type":"NodeCreated","node_id":root,"node_type":"form","pos":"000000","fields":{"label":"Private"}});
    post(
        &router,
        &format!("/documents/{doc}/batch"),
        &owner,
        json!({"base_seq":0,"ops":[op]}),
    )
    .await;
    let change = json!({"name":"Mine","base_seq":1,"submit":true,"ops":[{"type":"FieldEdited","node_id":root,"field":"label","value":"Changed"}]});
    let (status, _) = post(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &viewer,
        change.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &outsider,
        change.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &owner,
        change,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &outsider,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, listed) = get(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
}
