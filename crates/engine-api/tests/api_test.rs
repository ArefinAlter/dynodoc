//! Stage-18 API integration tests (DB-backed, real router via `tower::oneshot`).
//!
//! Each `#[sqlx::test(migrations = "../../migrations")]` provisions a fresh migrated
//! database; tests drive the actual Axum app, covering each endpoint's happy path,
//! auth/role rejection, cursor pagination, idempotency, SSE delivery, and the audit
//! surface. Requires a reachable Postgres (DATABASE_URL): the dev DB or CI service.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use engine_api::auth::{store as auth_store, token, AuthConfig};
use engine_api::{app, AppState};
use engine_core::governance::Role;
use engine_shared::DocumentId;
use futures::StreamExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const KEY: [u8; 32] = [7u8; 32];

#[sqlx::test(migrations = "../../migrations")]
async fn draft_sharing_remembers_exclusions_until_explicitly_included(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "selection@example.test").await);
    let doc = create_document(&router, &owner, "Selections").await;
    let root = ulid::Ulid::new().to_string();
    let child = ulid::Ulid::new().to_string();
    let (status,result)=send(&router,auth_post(&format!("/documents/{doc}/batch"),&owner,&json!({"base_seq":0,"ops":[
      {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{"label":"Original root"}},
      {"type":"NodeCreated","node_id":child,"node_type":"item","parent_id":root,"pos":"a0","fields":{"label":"Original child","type":"text","name":"question"}}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &owner,
            &json!({"name":"Private options"}),
        ),
    )
    .await;
    let uri = format!("/documents/{doc}/drafts/{}", draft["id"].as_str().unwrap());
    let (status, result) = send(
        &router,
        auth_post(
            &uri,
            &owner,
            &json!({"revision":0,"ops":[
              {"type":"FieldEdited","node_id":root,"field":"label","value":"Ready root"},
              {"type":"FieldEdited","node_id":child,"field":"label","value":"Private child"}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let sync = format!("{uri}/sync");
    let (_, preview) = send(
        &router,
        auth_post(&sync, &owner, &json!({"action":"preview"})),
    )
    .await;
    let(status,result)=send(&router,auth_post(&sync,&owner,&json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"],"included_nodes":[root],"remember_selection":true}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["completed"], false);
    let (_, preview) = send(
        &router,
        auth_post(&sync, &owner, &json!({"action":"preview"})),
    )
    .await;
    assert_eq!(preview["excluded_nodes"], json!([child]));
    let(status,result)=send(&router,auth_post(&sync,&owner,&json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"]}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert!(result["events"].as_array().unwrap().is_empty());
    let (_, team) = send(&router, auth_post_get(&format!("/documents/{doc}"), &owner)).await;
    assert_eq!(
        team["state"]["nodes"][&child]["current_fields"]["label"],
        "Original child"
    );
    let (_, preview) = send(
        &router,
        auth_post(&sync, &owner, &json!({"action":"preview"})),
    )
    .await;
    let(status,result)=send(&router,auth_post(&sync,&owner,&json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"],"included_nodes":[child],"remember_selection":true}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["completed"], true);
    let excluded: Vec<String> =
        sqlx::query_scalar("select excluded_nodes from workspace_draft where id=$1")
            .bind(Uuid::parse_str(draft["id"].as_str().unwrap()).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(excluded.is_empty());
    let (_, verification) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/verify"), &owner),
    )
    .await;
    assert_eq!(verification["ok"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn discussion_replies_resolution_permissions_and_replay(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner_id = seed_identity(&pool, "discussion-owner@example.test").await;
    let reviewer_id = seed_identity(&pool, "discussion-reviewer@example.test").await;
    let outsider = seed_identity(&pool, "discussion-outsider@example.test").await;
    let owner = bearer(owner_id);
    let reviewer = bearer(reviewer_id);
    let doc = create_document(&router, &owner, "Discussion").await;
    sqlx::query(
        "insert into document_access(document_id,identity_id,role) values($1,$2,'reviewer')",
    )
    .bind(doc)
    .bind(reviewer_id)
    .execute(&pool)
    .await
    .unwrap();
    let (status, root) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/comment"),
            &reviewer,
            &json!({"body":"Clarify the wording?"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{root}");
    let thread = root["event"]["id"].as_str().unwrap();
    let route = format!("/documents/{doc}/discussions/{thread}");
    let list = format!("/documents/{doc}/discussions");
    assert_eq!(
        send(&router, auth_post_get(&list, &bearer(outsider)))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (status, reply) = send(
        &router,
        auth_post(
            &route,
            &reviewer,
            &json!({"revision":1,"body":"Here is my suggested explanation."}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(reply["events"][0]["type"], "CommentReplied");
    assert_eq!(
        send(
            &router,
            auth_post(
                &route,
                &reviewer,
                &json!({"revision":1,"body":"Stale duplicate"})
            )
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &router,
            auth_post(&route, &reviewer, &json!({"revision":2,"resolved":true}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(&route, &owner, &json!({"revision":2,"resolved":true}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &route,
                &reviewer,
                &json!({"revision":3,"body":"Closed reply"})
            )
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let result = send(&router, auth_post_get(&list, &owner)).await.1;
    assert_eq!(result["items"][0]["resolved"], true);
    assert_eq!(
        result["items"][0]["replies"][0]["body"],
        "Here is my suggested explanation."
    );
    assert_eq!(
        send(
            &router,
            auth_post(&route, &owner, &json!({"revision":3,"resolved":false}))
        )
        .await
        .0,
        StatusCode::OK
    );
    let other = create_document(&router, &owner, "Other document").await;
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{other}/discussions/{thread}"),
                &owner,
                &json!({"revision":1,"body":"Cross-document attempt"})
            )
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .unwrap();
    let replay = engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .read_current_state(DocumentId(doc))
        .await
        .unwrap();
    assert_eq!(
        replay.comments.len(),
        1,
        "Replies are projected from canonical event IDs, not duplicated as root comments"
    );
    let (_, public) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":4}),
        ),
    )
    .await;
    let encoded = send(
        &router,
        get(&format!(
            "/public/documents/{}",
            public["token"].as_str().unwrap()
        )),
    )
    .await
    .1
    .to_string();
    assert!(!encoded.contains("Clarify") && !encoded.contains("suggested explanation"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_erasure_removes_whole_document_but_cannot_bypass_history_guards(pool: PgPool) {
    let admin_id = seed_identity(&pool, "erase-admin@example.test").await;
    let owner_id = seed_identity(&pool, "erase-owner@example.test").await;
    let mut state = test_state(pool.clone());
    state.admin_emails = "erase-admin@example.test".into();
    let router = app(state);
    let admin = bearer(admin_id);
    let owner = bearer(owner_id);
    let doc = create_document(&router, &owner, "Remove me").await;
    let keep = create_document(&router, &owner, "Keep me").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &owner,
            &json!({"node_type":"form","pos":"a","fields":{"label":"Private original"}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"].as_str().unwrap();
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &owner,
            &json!({"name":"Private draft"}),
        ),
    )
    .await;
    let draft_id = draft["id"].as_str().unwrap();
    assert_eq!(send(&router,auth_post(&format!("/documents/{doc}/drafts/{draft_id}"),&owner,&json!({"revision":0,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Private draft edit"}]}))).await.0,StatusCode::OK);
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/versions"),
                &owner,
                &json!({"name":"Checkpoint"})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, public) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":1}),
        ),
    )
    .await;
    sqlx::query("insert into document_upload(document_id,filename,content_type,content,created_by) values($1,'private.txt','text/plain',$2,$3)").bind(doc).bind(b"private bytes".as_slice()).bind(owner_id).execute(&pool).await.unwrap();
    let preview_uri = format!("/admin/impact?kind=documents&id={doc}");
    assert_eq!(
        send(&router, auth_post_get(&preview_uri, &owner)).await.0,
        StatusCode::FORBIDDEN
    );
    assert!(sqlx::query("delete from event where document_id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .is_err());
    assert!(
        sqlx::query("delete from document_version where document_id=$1")
            .bind(doc)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("delete from draft_edit")
        .execute(&pool)
        .await
        .is_err());
    assert!(sqlx::query("delete from document where id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .is_err());
    let (_, active_plan) = send(&router, auth_post_get(&preview_uri, &admin)).await;
    assert!(!active_plan["blockers"].as_array().unwrap().is_empty());
    assert_eq!(
        send(
            &router,
            auth_post(
                "/admin/actions",
                &admin,
                &json!({"kind":"documents","id":doc,"removed":true,"confirmation":"Remove me"})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, preview) = send(&router, auth_post_get(&preview_uri, &admin)).await;
    assert_eq!(preview["counts"]["events"], 1);
    let mut erase = json!({"kind":"documents","id":doc,"confirmation":"Remove me","revision":preview["revision"],"reason":"owner_request","acknowledge_backups":true});
    erase["confirmation"] = json!("wrong");
    assert_eq!(
        send(&router, auth_post("/admin/erase", &admin, &erase))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    erase["confirmation"] = json!("Remove me");
    // A direct history delete remains forbidden even with a same-transaction receipt.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'documents',$2,'other','{}')").bind(doc).bind(admin_id).execute(&mut *tx).await.unwrap();
    assert!(sqlx::query("delete from event where document_id=$1")
        .bind(doc)
        .execute(&mut *tx)
        .await
        .is_err());
    tx.rollback().await.unwrap();
    sqlx::query("insert into document_upload(document_id,filename,content_type,content,created_by) values($1,'later.txt','text/plain',$2,$3)").bind(doc).bind(b"later".as_slice()).bind(owner_id).execute(&pool).await.unwrap();
    assert_eq!(
        send(&router, auth_post("/admin/erase", &admin, &erase))
            .await
            .0,
        StatusCode::BAD_REQUEST,
        "A stale impact must be rejected"
    );
    erase["revision"] =
        send(&router, auth_post_get(&preview_uri, &admin)).await.1["revision"].clone();
    let (status, result) = send(&router, auth_post("/admin/erase", &admin, &erase)).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    for table in [
        "document",
        "event",
        "node",
        "snapshot",
        "document_upload",
        "document_version",
        "workspace_draft",
        "public_document_view",
        "document_access",
    ] {
        let column = if table == "document" {
            "id"
        } else {
            "document_id"
        };
        let count: i64 =
            sqlx::query_scalar(&format!("select count(*) from {table} where {column}=$1"))
                .bind(doc)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "{table} must be erased");
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from draft_edit")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        send(
            &router,
            get(&format!(
                "/public/documents/{}",
                public["token"].as_str().unwrap()
            ))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &router,
            auth_post_get(&format!("/documents/{keep}"), &owner)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from erasure_receipt where resource_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert!(sqlx::query("delete from erasure_receipt")
        .execute(&pool)
        .await
        .is_err());
    assert!(sqlx::query("delete from operation_audit")
        .execute(&pool)
        .await
        .is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_account_erasure_preserves_shared_chain_and_clears_private_drafts(pool: PgPool) {
    let admin_id = seed_identity(&pool, "admin-erasure@example.test").await;
    let user_id = seed_identity(&pool, "erased@example.test").await;
    let receiver = seed_identity(&pool, "receiver@example.test").await;
    let mut state = test_state(pool.clone());
    state.admin_emails = "admin-erasure@example.test".into();
    let router = app(state);
    let admin = bearer(admin_id);
    let user = bearer(user_id);
    let doc = create_document(&router, &user, "Shared document").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &user,
            &json!({"node_type":"form","pos":"a","fields":{"label":"Retained contribution"}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"].as_str().unwrap();
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &user,
            &json!({"name":"Unshared private work"}),
        ),
    )
    .await;
    let draft_id = draft["id"].as_str().unwrap();
    send(&router,auth_post(&format!("/documents/{doc}/drafts/{draft_id}"),&user,&json!({"revision":0,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Private"}]}))).await;
    assert_eq!(send(&router,auth_post("/admin/actions",&admin,&json!({"kind":"users","id":user_id,"removed":true,"confirmation":"erased@example.test"}))).await.0,StatusCode::OK);
    let uri = format!("/admin/impact?kind=users&id={user_id}");
    assert!(
        !send(&router, auth_post_get(&uri, &admin)).await.1["blockers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(send(&router,auth_post("/admin/control",&admin,&json!({"action":"transfer_document","id":doc,"confirmation":"Shared document","recipient":receiver}))).await.0,StatusCode::OK);
    let preview = send(&router, auth_post_get(&uri, &admin)).await.1;
    assert_eq!(preview["blockers"], json!([]));
    let(status,result)=send(&router,auth_post("/admin/erase",&admin,&json!({"kind":"users","id":user_id,"confirmation":"erased@example.test","revision":preview["revision"],"reason":"owner_request","acknowledge_backups":true}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let name: String = sqlx::query_scalar("select display_name from identity where id=$1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "Deleted account");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from workspace_draft where created_by=$1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from draft_edit")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .unwrap();
    assert_eq!(
        send(&router, auth_post_get("/auth/me", &user)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(
        sqlx::query("update identity set disabled_at=null where id=$1")
            .bind(user_id)
            .execute(&pool)
            .await
            .is_err()
    );
    let recreated = seed_identity(&pool, "erased@example.test").await;
    assert_ne!(
        recreated, user_id,
        "Signing up again must not recover the old identity"
    );
    assert_eq!(
        send(
            &router,
            auth_post_get(&format!("/documents/{doc}"), &bearer(recreated))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn rich_text_history_replays_compact_edits_and_rejects_stale_patches(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "richtext@example.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Research protocol").await;
    let node = ulid::Ulid::new().to_string();
    let root = ulid::Ulid::new().to_string();
    let content = json!({"type":"paragraph","content":[{"type":"text","text":"Research evidence. ".repeat(1200)}]});
    let (status, _) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &token, &json!({"base_seq":0,"ops":[{"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a","fields":{}},{"type":"NodeCreated","node_id":node,"node_type":"item","parent_id":root,"pos":"a","fields":{"content":content,"label":engine_core::richtext::plain_text(&content)}}]}))).await;
    assert_eq!(status, StatusCode::OK);
    let mut after = content.clone();
    after["content"][0]["marks"] = json!([{"type":"bold"}]);
    let (status, saved) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &token, &json!({"base_seq":2,"ops":[{"type":"FieldEdited","node_id":node,"field":"content","value":after}]}))).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["events"][0]["type"], "RichTextPatched");
    let patch = saved["events"][0]["payload"].clone();
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({"base_seq":3,"ops":[patch]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let replay = engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .read_current_state(DocumentId(doc))
        .await
        .unwrap();
    assert_eq!(
        replay.nodes[&engine_shared::NodeId(node.clone())].current_fields["content"],
        after
    );
    let row: Value = sqlx::query_scalar("select current_fields from node where id=$1")
        .bind(&node)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row["content"], after);
    engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .unwrap();
    let request = Request::builder()
        .uri(format!("/documents/{doc}/structure"))
        .header(header::AUTHORIZATION, &token)
        .body(Body::empty())
        .unwrap();
    let (status, analysis) = send(&router, request).await;
    assert_eq!(status, StatusCode::OK, "{analysis}");
    assert_eq!(analysis["through_seq"], 3);
    assert_eq!(analysis["blocks"][0]["analysis"]["formatting_runs"], 1);
    let stranger = bearer(seed_identity(&pool, "stranger@example.test").await);
    let request = Request::builder()
        .uri(format!("/documents/{doc}/structure"))
        .header(header::AUTHORIZATION, stranger)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&router, request).await.0, StatusCode::FORBIDDEN);
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

/// A live access token for `identity_id` (mints directly, bypassing the email flow).
fn bearer(identity_id: Uuid) -> String {
    format!("Bearer {}", token::mint(&KEY, identity_id, 3600).unwrap())
}

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn auth_post(uri: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header(header::AUTHORIZATION, token)
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

/// Seed an identity row and return its id.
async fn seed_identity(pool: &PgPool, email: &str) -> Uuid {
    auth_store::upsert_identity(pool, email, Some("Tester"))
        .await
        .unwrap()
        .id
        .0
}

/// Create a document (as Author) through the API and return its id.
async fn create_document(router: &Router, token: &str, title: &str) -> Uuid {
    let (status, body) = send(
        router,
        auth_post("/documents", token, &json!({ "title": title })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create document failed: {body}");
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

// --- documents ------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_then_read_document(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);

    let doc_id = create_document(&router, &token, "Survey").await;

    // Public read: metadata + (empty) materialized state.
    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get document failed: {body}");
    assert_eq!(body["title"], "Survey");
    assert!(body["state"]["nodes"].is_object());

    // Public list includes it.
    let (status, body) = send(&router, auth_post_get("/documents", &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["id"] == doc_id.to_string()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_document_requires_auth(pool: PgPool) {
    let router = app(test_state(pool));
    let (status, _) = send(
        &router,
        Request::builder()
            .method("POST")
            .uri("/documents")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "title": "x" }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn anonymous_read_requires_auth(pool: PgPool) {
    let router = app(test_state(pool));
    let (status, _) = send(&router, get(&format!("/documents/{}", Uuid::new_v4()))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// --- write path + governance ----------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn node_create_and_field_edit_round_trip(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    // Create a root form node.
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0", "fields": {} }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "node-create failed: {body}");
    let node_id = body["event"]["target_node_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Edit a field of that node.
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/field-edit"),
            &token,
            &json!({ "node_id": node_id, "field": "title", "value": "Hello" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "field-edit failed: {body}");

    // The materialized state reflects the edit.
    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/node/{node_id}"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["current_fields"]["title"], "Hello");
}

#[sqlx::test(migrations = "../../migrations")]
async fn auditor_cannot_write(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let auditor = seed_identity(&pool, "auditor@dynodoc.local").await;
    let doc_id = create_document(&router, &bearer(author), "Form").await;

    // Grant the auditor read-only access.
    auth_store::grant_access(&pool, DocumentId(doc_id), auditor, Role::Auditor)
        .await
        .unwrap();

    // An auditor's write is forbidden by the capability matrix.
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &bearer(auditor),
            &json!({ "node_type": "form", "pos": "a0" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn non_participant_is_forbidden(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let stranger = seed_identity(&pool, "stranger@dynodoc.local").await;
    let doc_id = create_document(&router, &bearer(author), "Form").await;

    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/comment"),
            &bearer(stranger),
            &json!({ "body": "hi" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// --- proposal lifecycle ---------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn suggest_then_accept_applies_the_wrapped_op(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let reviewer = seed_identity(&pool, "reviewer@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;
    auth_store::grant_access(&pool, DocumentId(doc_id), reviewer, Role::Reviewer)
        .await
        .unwrap();

    // Author creates a node.
    let (_, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0", "fields": { "title": "Old" } }),
        ),
    )
    .await;
    let node_id = body["event"]["target_node_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Reviewer suggests editing the title.
    let suggestion_id = "sug-1";
    let wrapped =
        json!({ "type": "FieldEdited", "node_id": node_id, "field": "title", "value": "New" });
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/suggest"),
            &bearer(reviewer),
            &json!({ "suggestion_id": suggestion_id, "detail": wrapped }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Author accepts; the wrapped edit becomes canonical.
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/proposals/{suggestion_id}/accept"),
            &token,
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/node/{node_id}"), &token),
    )
    .await;
    assert_eq!(body["current_fields"]["title"], "New");
}

// --- four-card conflicts (stage 21) ---------------------------------------------

/// Seed a form node, then two pending suggestions editing its `title` to different
/// values. Returns `(doc_id, node_id, author_token, reviewer_id)`.
async fn seed_conflict(pool: &PgPool, router: &Router) -> (Uuid, String, String, Uuid) {
    let author = seed_identity(pool, "author@dynodoc.local").await;
    let reviewer = seed_identity(pool, "reviewer@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(router, &token, "Form").await;
    auth_store::grant_access(pool, DocumentId(doc_id), reviewer, Role::Reviewer)
        .await
        .unwrap();

    let (_, body) = send(
        router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0", "fields": { "title": "Old" } }),
        ),
    )
    .await;
    let node_id = body["event"]["target_node_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Two concurrent pending suggestions on the same field, different values.
    for (sid, value) in [("sug-foo", "Foo"), ("sug-bar", "Bar")] {
        let wrapped =
            json!({ "type": "FieldEdited", "node_id": node_id, "field": "title", "value": value });
        let (status, _) = send(
            router,
            auth_post(
                &format!("/documents/{doc_id}/ops/suggest"),
                &bearer(reviewer),
                &json!({ "suggestion_id": sid, "detail": wrapped }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    (doc_id, node_id, token, reviewer)
}

#[sqlx::test(migrations = "../../migrations")]
async fn conflicts_list_groups_two_pending_edits_on_one_field(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, node_id, token, _reviewer) = seed_conflict(&pool, &router).await;

    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list conflicts failed: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "exactly one conflict");
    let c = &items[0];
    assert_eq!(c["node_id"], node_id);
    assert_eq!(c["field"], "title");
    assert_eq!(c["ancestor"], "Old");
    let options = c["options"].as_array().unwrap();
    assert_eq!(options.len(), 2);
    let values: Vec<&str> = options
        .iter()
        .map(|o| o["value"].as_str().unwrap())
        .collect();
    assert!(values.contains(&"Foo") && values.contains(&"Bar"));
    // Options are attributed to the proposing actor.
    assert!(options.iter().all(|o| o["actor"].is_string()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn resolve_accept_makes_chosen_value_canonical_and_rejects_others(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, node_id, token, _reviewer) = seed_conflict(&pool, &router).await;

    let (_, list) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    let conflict_id = list["items"][0]["conflict_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Accept the "Foo" suggestion.
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/conflicts/{conflict_id}/resolve"),
            &token,
            &json!({ "accept_suggestion_id": "sug-foo" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "resolve failed: {body}");
    assert!(body["events"].as_array().unwrap().len() >= 2);

    // The accepted value is now canonical.
    let (_, node) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/node/{node_id}"), &token),
    )
    .await;
    assert_eq!(node["current_fields"]["title"], "Foo");

    // Suggestion states: foo accepted, bar rejected (superseded).
    let (_, detail) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}"), &token),
    )
    .await;
    let suggestions = &detail["state"]["suggestions"];
    assert_eq!(suggestions["sug-foo"]["accepted"], true);
    assert_eq!(
        suggestions["sug-bar"]["rejected_reason"],
        "superseded by conflict resolution"
    );

    // The conflict is gone.
    let (_, list2) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    assert!(list2["items"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn resolve_custom_value_supersedes_all_suggestions(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, node_id, token, _reviewer) = seed_conflict(&pool, &router).await;

    let (_, list) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    let conflict_id = list["items"][0]["conflict_id"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/conflicts/{conflict_id}/resolve"),
            &token,
            &json!({ "custom_value": "Compromise" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, node) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/node/{node_id}"), &token),
    )
    .await;
    assert_eq!(node["current_fields"]["title"], "Compromise");

    // Every pending suggestion in the group is now rejected.
    let (_, detail) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}"), &token),
    )
    .await;
    let suggestions = &detail["state"]["suggestions"];
    assert_eq!(
        suggestions["sug-foo"]["rejected_reason"],
        "superseded by conflict resolution"
    );
    assert_eq!(
        suggestions["sug-bar"]["rejected_reason"],
        "superseded by conflict resolution"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn resolve_requires_author(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, _node_id, token, reviewer) = seed_conflict(&pool, &router).await;

    let (_, list) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    let conflict_id = list["items"][0]["conflict_id"]
        .as_str()
        .unwrap()
        .to_string();

    // A reviewer (non-author) cannot resolve.
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/conflicts/{conflict_id}/resolve"),
            &bearer(reviewer),
            &json!({ "accept_suggestion_id": "sug-foo" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn resolve_unknown_conflict_id_is_404(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, _node_id, token, _reviewer) = seed_conflict(&pool, &router).await;

    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/conflicts/bm90LXJlYWw/resolve"),
            &token,
            &json!({ "accept_suggestion_id": "sug-foo" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn single_suggestion_is_not_a_conflict(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    let (_, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0", "fields": { "title": "Old" } }),
        ),
    )
    .await;
    let node_id = body["event"]["target_node_id"].as_str().unwrap();

    let wrapped =
        json!({ "type": "FieldEdited", "node_id": node_id, "field": "title", "value": "Solo" });
    send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/suggest"),
            &token,
            &json!({ "suggestion_id": "only", "detail": wrapped }),
        ),
    )
    .await;

    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"].as_array().unwrap().is_empty());
}

// --- idempotency ----------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn idempotency_key_dedupes_writes(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    let body = json!({ "node_type": "form", "pos": "a0", "node_id": "01IDEMPOTENT0000000000000" });
    let req = || {
        Request::builder()
            .method("POST")
            .uri(format!("/documents/{doc_id}/ops/node-create"))
            .header("content-type", "application/json")
            .header(header::AUTHORIZATION, &token)
            .header("idempotency-key", "key-abc")
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    let (s1, b1) = send(&router, req()).await;
    let (s2, b2) = send(&router, req()).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    // Same response both times.
    assert_eq!(b1, b2);

    // Exactly one event was appended (the replay did not re-execute).
    let count: i64 = sqlx::query_scalar(
        "select count(*) from event where document_id = $1 and type = 'NodeCreated'",
    )
    .bind(doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "idempotent replay must not duplicate the event");
}

// --- pagination -----------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn events_endpoint_paginates_with_cursor(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    // One form node, then several comments (cheap events that need no new node row).
    send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0" }),
        ),
    )
    .await;
    for i in 0..4 {
        send(
            &router,
            auth_post(
                &format!("/documents/{doc_id}/ops/comment"),
                &token,
                &json!({ "body": format!("c{i}") }),
            ),
        )
        .await;
    }

    // First page of 2.
    let (status, page1) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/events?limit=2"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["items"].as_array().unwrap().len(), 2);
    let cursor = page1["next_cursor"].as_str().unwrap().to_string();

    // Next page continues strictly after the cursor (no overlap).
    let (status, page2) = send(
        &router,
        auth_post_get(
            &format!("/documents/{doc_id}/events?limit=2&cursor={cursor}"),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let first_seq_p2 = page2["items"][0]["seq"].as_i64().unwrap();
    let last_seq_p1 = page1["items"][1]["seq"].as_i64().unwrap();
    assert!(first_seq_p2 > last_seq_p1, "cursor page must not overlap");
}

/// A GET with a bearer token (the events endpoint is `[auth]`).
fn auth_post_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::AUTHORIZATION, token)
        .body(Body::empty())
        .unwrap()
}

// --- audit ----------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn verify_requires_membership_and_is_ok_for_a_clean_log(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;
    send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0" }),
        ),
    )
    .await;

    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/verify"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
}

// --- SSE ------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn sse_delivers_a_new_event_within_100ms(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    // Open the stream (no backfill yet — the document has no events).
    let stream_req = Request::builder()
        .uri(format!("/documents/{doc_id}/stream"))
        .header(header::AUTHORIZATION, &token)
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(stream_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-cache, no-transform"
    );
    let mut body = resp.into_body().into_data_stream();

    // An idle subscription must flush immediately without waiting for a write or
    // the 30-second heartbeat. The acknowledgement carries no event/cursor.
    let ready = tokio::time::timeout(std::time::Duration::from_secs(1), body.next())
        .await
        .expect("idle SSE subscription must acknowledge immediately")
        .expect("stream yielded an acknowledgement")
        .expect("acknowledgement is not an error");
    let ready = String::from_utf8_lossy(&ready);
    assert!(ready.starts_with(": connected"));
    assert!(!ready.contains("data:") && !ready.contains("id:"));

    // Append an event from "another connection".
    let writer = router.clone();
    let write_token = token.clone();
    tokio::spawn(async move {
        send(
            &writer,
            auth_post(
                &format!("/documents/{doc_id}/ops/node-create"),
                &write_token,
                &json!({ "node_type": "form", "pos": "a0" }),
            ),
        )
        .await;
    });

    // The subscriber receives the event frame within 100ms.
    let frame = tokio::time::timeout(std::time::Duration::from_millis(100), body.next())
        .await
        .expect("SSE frame should arrive within 100ms")
        .expect("stream yielded a frame")
        .expect("frame is not an error");
    let text = String::from_utf8_lossy(&frame);
    assert!(
        text.contains("NodeCreated"),
        "expected a NodeCreated SSE frame, got: {text}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn private_document_reads_are_scoped_to_members(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@example.test").await;
    let stranger = seed_identity(&pool, "stranger@example.test").await;
    let doc = create_document(&router, &bearer(owner), "Private study").await;
    for suffix in ["", "/events", "/stream", "/verify", "/conflicts"] {
        let (status, _) = send(
            &router,
            auth_post_get(&format!("/documents/{doc}{suffix}"), &bearer(stranger)),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "unrelated user read {suffix}"
        );
    }
    let (_, list) = send(&router, auth_post_get("/documents", &bearer(stranger))).await;
    assert!(list["items"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_field_edits_are_refused_but_disjoint_edits_work(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@example.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Study").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &token,
            &json!({"node_type":"form","pos":"a0","fields":{"label":"Original"}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"].as_str().unwrap();
    let first = json!({"node_id":node,"field":"label","value":"First edit","base_seq":1});
    let second = json!({"node_id":node,"field":"label","value":"Stale edit","base_seq":1});
    let third = json!({"node_id":node,"field":"hint","value":"Independent","base_seq":1});
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/ops/field-edit"), &token, &first)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/ops/field-edit"), &token, &second)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/ops/field-edit"), &token, &third)
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(
        read["state"]["nodes"][node]["current_fields"]["label"],
        "First edit"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_acceptance_only_applies_once(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc, node, token, _) = seed_conflict(&pool, &router).await;
    let uri = format!("/documents/{doc}/proposals/sug-foo/accept");
    let (a, b) = tokio::join!(
        send(&router, auth_post(&uri, &token, &json!({}))),
        send(&router, auth_post(&uri, &token, &json!({})))
    );
    assert_eq!(
        usize::from(a.0 == StatusCode::OK) + usize::from(b.0 == StatusCode::OK),
        1
    );
    let count: i64 = sqlx::query_scalar(
        "select count(*) from event where document_id=$1 and type='SuggestionAccepted'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let (_, read) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/node/{node}"), &token),
    )
    .await;
    assert_eq!(read["current_fields"]["title"], "Foo");
}

#[sqlx::test(migrations = "../../migrations")]
async fn batch_rolls_back_and_deploy_pins_its_exact_state(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@example.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Study").await;
    let node = engine_shared::NodeId(ulid::Ulid::new().to_string());
    let create = json!({"type":"NodeCreated","node_id":node,"node_type":"form","pos":"a0","fields":{"label":"Study"}});
    let bad = json!({"type":"FieldEdited","node_id":"missing","field":"label","value":"bad"});
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({"ops":[create.clone(),bad],"base_seq":0}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(read["latest_seq"], 0);
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/batch"),
                &token,
                &json!({"ops":[create],"base_seq":0})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/deploy"), &token, &json!({}))
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(read["status"], "deployed");
    let snapshot = read["deployed_snapshot_id"].as_str().unwrap();
    let (_, snap) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/snapshot/{snapshot}"), &token),
    )
    .await;
    assert_eq!(snap["through_seq"], 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn ten_researchers_merge_independent_cells_without_loss(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@team.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Ten researchers").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &token,
            &json!({"node_type":"form","pos":"a0","fields":{}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut tokens = Vec::new();
    for i in 0..10 {
        let email = format!("researcher{i}@team.test");
        let person = seed_identity(&pool, &email).await;
        let (status, _) = send(
            &router,
            auth_post(
                &format!("/documents/{doc}/members"),
                &token,
                &json!({"email":email,"role":"author"}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        tokens.push(bearer(person));
    }
    let futures=tokens.iter().enumerate().map(|(i,token)| {let router=router.clone();let node=node.clone();async move {send(&router,auth_post(&format!("/documents/{doc}/batch"),token,&json!({"base_seq":1,"ops":[{"type":"FieldEdited","node_id":node,"field":format!("cell_{i}"),"value":format!("Researcher {i}")}]}))).await}});
    for (status, body) in futures::future::join_all(futures).await {
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(read["latest_seq"], 11);
    for i in 0..10 {
        assert_eq!(
            read["state"]["nodes"][&node]["current_fields"][format!("cell_{i}")],
            format!("Researcher {i}")
        );
    }
    let (_, verify) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/verify"), &token),
    )
    .await;
    assert_eq!(verify["ok"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn personal_draft_conflicts_are_reviewed_and_old_versions_restore_stable_ids(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@draft.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Draft test").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &token,
            &json!({"node_type":"form","pos":"a0","fields":{"label":"Original"}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"].as_str().unwrap();
    let (status, version) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/versions"),
            &token,
            &json!({"name":"Original"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{version}");
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &token,
            &json!({"name":"Alternative"}),
        ),
    )
    .await;
    let draft = draft["id"].as_str().unwrap();
    let draft_uri = format!("/documents/{doc}/drafts/{draft}");
    let (status,edited)=send(&router,auth_post(&draft_uri,&token,&json!({"revision":0,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Draft"}]}))).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    let stale=send(&router,auth_post(&draft_uri,&token,&json!({"revision":0,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Stale"}]}))).await;
    assert_eq!(stale.0, StatusCode::CONFLICT);
    send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/field-edit"),
            &token,
            &json!({"node_id":node,"field":"label","value":"Team"}),
        ),
    )
    .await;
    let (_, preview) = send(
        &router,
        auth_post(
            &format!("{draft_uri}/sync"),
            &token,
            &json!({"action":"preview"}),
        ),
    )
    .await;
    assert_eq!(preview["conflicts"].as_array().unwrap().len(), 1);
    let (status,merged)=send(&router,auth_post(&format!("{draft_uri}/sync"),&token,&json!({"action":"share","team_seq":preview["team_seq"],"revision":1,"resolutions":{format!("{node}:label"):"draft"}}))).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(merged["completed"], true);
    send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({"base_seq":3,"ops":[{"type":"NodeDeleted","node_id":node}]}),
        ),
    )
    .await;
    let (status, restored) = send(
        &router,
        auth_post(
            &format!(
                "/documents/{doc}/versions/{}/restore",
                version["id"].as_str().unwrap()
            ),
            &token,
            &json!({"base_seq":4}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert!(restored["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "NodeRestored"));
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(read["state"]["nodes"][node]["deleted"], false);
    assert_eq!(
        read["state"]["nodes"][node]["current_fields"]["label"],
        "Original"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn content_search_never_returns_another_teams_document(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@search.test").await;
    let stranger = seed_identity(&pool, "stranger@search.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Fieldwork").await;
    send(&router,auth_post(&format!("/documents/{doc}/ops/node-create"),&token,&json!({"node_type":"form","pos":"a0","fields":{"label":"confidential community interviews"}}))).await;
    let (status, found) = send(&router, auth_post_get("/search?q=community", &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    let (_, hidden) = send(
        &router,
        auth_post_get("/search?q=community", &bearer(stranger)),
    )
    .await;
    assert!(hidden["items"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn archive_is_reversible_and_keeps_membership_boundaries(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@archive.test").await;
    let stranger = seed_identity(&pool, "stranger@archive.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Archived research").await;
    let uri = format!("/documents/{doc}/metadata");
    let (status, _) = send(&router, auth_post(&uri, &token, &json!({"archived":true}))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, active) = send(&router, auth_post_get("/documents", &token)).await;
    assert!(active["items"].as_array().unwrap().is_empty());
    let (_, archived) = send(&router, auth_post_get("/documents?status=archived", &token)).await;
    assert_eq!(archived["items"][0]["id"], doc.to_string());
    let (_, hidden) = send(
        &router,
        auth_post_get("/documents?status=archived", &bearer(stranger)),
    )
    .await;
    assert!(hidden["items"].as_array().unwrap().is_empty());
    let (status, _) = send(
        &router,
        auth_post(&uri, &bearer(stranger), &json!({"archived":false})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&router, auth_post(&uri, &token, &json!({"archived":false}))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, restored) = send(&router, auth_post_get("/documents", &token)).await;
    assert_eq!(restored["items"][0]["id"], doc.to_string());
    assert_eq!(restored["items"][0]["status"], "draft");
}

#[sqlx::test(migrations = "../../migrations")]
async fn file_import_batches_can_exceed_the_framework_default_limit(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@large-import.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Large import").await;
    let text = "a".repeat(2 * 1024 * 1024 + 1);
    let (status, response) = send(&router, auth_post(
        &format!("/documents/{doc}/batch"), &token,
        &json!({"base_seq":0,"ops":[{"type":"NodeCreated","node_id":ulid::Ulid::new().to_string(),"node_type":"form","pos":"a0","fields":{"label":text}}]})
    )).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["events"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn operations_dashboard_rejects_regular_members_and_anonymous_readers(pool: PgPool) {
    let member = seed_identity(&pool, "ordinary-member@example.test").await;
    let router = app(test_state(pool));
    assert_eq!(
        send(&router, get("/admin/overview")).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&router, auth_post_get("/admin/overview", &bearer(member)))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn performance_samples_need_service_auth_and_are_idempotent(pool: PgPool) {
    let mut state = test_state(pool.clone());
    state.auth.service_key = Some("test-service-key".into());
    let router = app(state);
    let body = json!({"page_id":Uuid::new_v4(),"metric":"visit","host":"homepage","page":"home","value":1});
    assert_eq!(
        send(&router, auth_post("/telemetry", "invalid", &body))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    for _ in 0..2 {
        let req = Request::builder()
            .method("POST")
            .uri("/telemetry")
            .header("content-type", "application/json")
            .header("x-dynodoc-service-key", "test-service-key")
            .body(Body::from(body.to_string()))
            .unwrap();
        assert_eq!(send(&router, req).await.0, StatusCode::OK);
    }
    let count: i64 = sqlx::query_scalar("select count(*) from web_metric")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let bad = json!({"page_id":Uuid::new_v4(),"metric":"visit","host":"homepage","page":"/documents/private-id","value":1});
    let req = Request::builder()
        .method("POST")
        .uri("/telemetry")
        .header("content-type", "application/json")
        .header("x-dynodoc-service-key", "test-service-key")
        .body(Body::from(bad.to_string()))
        .unwrap();
    assert_eq!(send(&router, req).await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn allowlisted_admin_sees_aggregate_storage_and_no_research_content(pool: PgPool) {
    let admin = seed_identity(&pool, "operator@example.test").await;
    let mut state = test_state(pool);
    state.admin_emails = "operator@example.test".into();
    let router = app(state);
    let (status, result) = send(&router, auth_post_get("/admin/overview", &bearer(admin))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["counts"]["accounts"], 1);
    assert!(result["counts"]["database_bytes"].as_i64().unwrap() > 0);
    assert!(result["tables"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "document_upload"));
    assert!(result.get("documents").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn product_hierarchy_has_inherited_roles_and_manager_boundaries(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "owner@example.org").await);
    let editor = bearer(seed_identity(&pool, "editor@example.org").await);
    let stranger = bearer(seed_identity(&pool, "stranger@example.org").await);
    let (status, org) = send(
        &router,
        auth_post(
            "/spaces",
            &owner,
            &json!({"name":"University","kind":"organization"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{org}");
    let org = org["id"].as_str().unwrap();
    let (_, team) = send(
        &router,
        auth_post(
            "/spaces",
            &owner,
            &json!({"name":"Field team","kind":"team","parent_id":org}),
        ),
    )
    .await;
    let team = team["id"].as_str().unwrap();
    let (_, folder) = send(
        &router,
        auth_post(
            "/spaces",
            &owner,
            &json!({"name":"Study","kind":"folder","parent_id":team}),
        ),
    )
    .await;
    let folder = folder["id"].as_str().unwrap();
    let (status, result) = send(
        &router,
        auth_post(
            &format!("/spaces/{org}/members"),
            &owner,
            &json!({"email":"editor@example.org","role":"editor"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let doc = create_document(&router, &owner, "Protocol").await;
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/location"),
                &owner,
                &json!({"space_id":folder})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}"), &editor),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "author");
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/trash"),
                &editor,
                &json!({"deleted":true})
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/members"),
                &editor,
                &json!({"email":"stranger@example.org","role":"author"})
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post_get(&format!("/documents/{doc}"), &stranger)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/spaces/{org}/members"),
                &owner,
                &json!({"email":"owner@example.org","role":"viewer"})
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/spaces/{org}/members"),
                &owner,
                &json!({"email":"editor@example.org","role":"remove"})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post_get(&format!("/documents/{doc}"), &editor)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn office_review_is_private_and_unresolved_changes_cannot_be_published(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "office-privacy@example.org").await);
    let doc = create_document(&router, &owner, "Office review").await;
    let root = ulid::Ulid::new().to_string();
    let node = ulid::Ulid::new().to_string();
    let content = json!({"type":"paragraph","content":[{"type":"text","text":"Public wording","marks":[{"type":"sourceComment","attrs":{"body":"Private Word comment","author":"Private author"}}]}]});
    let (status, body) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &owner, &json!({"base_seq":0,"ops":[
      {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a","fields":{"fonts":[{"family":"Private font","uploadId":"secret-file"}]}},
      {"type":"NodeCreated","node_id":node,"node_type":"item","parent_id":root,"pos":"a","fields":{"content":content,"format_0":{"note":"Private spreadsheet note","bold":true}}}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, link) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":2}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{link}");
    let (_, public) = send(
        &router,
        get(&format!(
            "/public/documents/{}",
            link["token"].as_str().unwrap()
        )),
    )
    .await;
    let encoded = public.to_string();
    assert!(
        !encoded.contains("Private") && !encoded.contains("secret-file"),
        "{encoded}"
    );
    assert!(encoded.contains("Public wording"));
    assert_eq!(
        public["state"]["nodes"][node.as_str()]["current_fields"]["format_0"]["bold"],
        true
    );
    let mut tracked = content.clone();
    tracked["content"][0]["marks"] =
        json!([{"type":"trackedChange","attrs":{"kind":"insert","id":"revision"}}]);
    assert_eq!(send(&router, auth_post(&format!("/documents/{doc}/batch"), &owner, &json!({"base_seq":2,"ops":[{"type":"FieldEdited","node_id":node,"field":"content","value":tracked}]}))).await.0, StatusCode::OK);
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":3}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.to_string().contains("Accept or reject"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn product_public_copy_is_frozen_private_and_revocable(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "publisher@example.org").await);
    let doc = create_document(&router, &owner, "Published protocol").await;
    let root = "01J00000000000000000000000";
    let item = "01J00000000000000000000001";
    let hidden = "01J00000000000000000000002";
    let child = "01J00000000000000000000003";
    let ops = json!({"base_seq":0,"ops":[
      {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a","fields":{}},
      {"type":"NodeCreated","node_id":item,"node_type":"item","parent_id":root,"pos":"a","fields":{"label":"Original question","research_source":{"token":"private metadata"}}},
      {"type":"NodeCreated","node_id":hidden,"node_type":"section","parent_id":root,"pos":"b","fields":{"label":"Removed private group"}},
      {"type":"NodeCreated","node_id":child,"node_type":"item","parent_id":hidden,"pos":"a","fields":{"label":"Descendant private content"}},
      {"type":"NodeDeleted","node_id":child},
      {"type":"NodeDeleted","node_id":hidden},
      {"type":"CommentAdded","node_id":item,"body":"Private reviewer conversation"}
    ]});
    let (status, body) = send(
        &router,
        auth_post(&format!("/documents/{doc}/batch"), &owner, &ops),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let seq: i64 = sqlx::query_scalar("select max(seq) from event where document_id=$1")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/public-links"),
                &owner,
                &json!({"base_seq":0})
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, link) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":seq,"expires_days":30}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{link}");
    let path = format!("/public/documents/{}", link["token"].as_str().unwrap());
    let (status, public) = send(&router, get(&path)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        public["state"]["nodes"][item]["current_fields"]["label"],
        "Original question"
    );
    assert!(public["state"]["nodes"][item]["current_fields"]
        .get("research_source")
        .is_none());
    assert!(public["state"]["nodes"].get(hidden).is_none());
    assert!(public["state"]["nodes"].get(child).is_none());
    assert_eq!(public["state"]["comments"], json!([]));
    let update = json!({"base_seq":seq,"ops":[{"type":"FieldEdited","node_id":item,"field":"label","value":"Private later edit"}]});
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/batch"), &owner, &update)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/metadata"),
                &owner,
                &json!({"title":"Private later title"})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, public) = send(&router, get(&path)).await;
    assert_eq!(public["title"], "Published protocol");
    assert_eq!(
        public["state"]["nodes"][item]["current_fields"]["label"],
        "Original question"
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/trash"),
                &owner,
                &json!({"deleted":true})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(send(&router, get(&path)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        send(&router, auth_post_get(&format!("/documents/{doc}"), &owner))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/trash"),
                &owner,
                &json!({"deleted":false})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(send(&router, get(&path)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        send(&router, auth_post_get(&format!("/documents/{doc}"), &owner))
            .await
            .0,
        StatusCode::OK
    );
    assert!(engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn product_admin_removal_invalidates_existing_tokens_after_restoration(pool: PgPool) {
    let admin_id = seed_identity(&pool, "admin@example.org").await;
    let user = seed_identity(&pool, "person@example.org").await;
    let mut state = test_state(pool.clone());
    state.admin_emails = "admin@example.org".into();
    let router = app(state);
    let admin = bearer(admin_id);
    let old = bearer(user);
    assert_eq!(
        send(&router, auth_post_get("/admin/records?kind=users", &old))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                "/admin/actions",
                &admin,
                &json!({"kind":"users","id":user,"removed":true,"confirmation":"wrong"})
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    use engine_api::auth::{sha256, store};
    let expires = chrono::Utc::now() + chrono::Duration::hours(1);
    store::issue_magic_link(
        &pool,
        engine_shared::IdentityId(user),
        &sha256(b"pending-link"),
        expires,
    )
    .await
    .unwrap();
    store::issue_refresh(&pool, user, &sha256(b"pending-refresh"), expires)
        .await
        .unwrap();
    for removed in [true, false] {
        let (status,body)=send(&router,auth_post("/admin/actions",&admin,&json!({"kind":"users","id":user,"removed":removed,"confirmation":"person@example.org"}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            send(&router, auth_post_get("/auth/me", &old)).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert!(
        store::consume_magic_link(&pool, &sha256(b"pending-link"))
            .await
            .is_err(),
        "A pre-removal email link cannot revive after restoration"
    );
    assert!(
        store::rotate_refresh(
            &pool,
            &sha256(b"pending-refresh"),
            &sha256(b"replacement"),
            expires
        )
        .await
        .is_err(),
        "A pre-removal refresh token cannot revive after restoration"
    );
    let fresh = format!(
        "Bearer {}",
        token::mint_generation(&KEY, user, 3600, 1).unwrap()
    );
    assert_eq!(
        send(&router, auth_post_get("/auth/me", &fresh)).await.0,
        StatusCode::OK
    );
    assert_eq!(send(&router,auth_post("/admin/actions",&admin,&json!({"kind":"users","id":admin_id,"removed":true,"confirmation":"admin@example.org"}))).await.0,StatusCode::BAD_REQUEST);
    let result = sqlx::query("delete from operation_audit")
        .execute(&pool)
        .await;
    assert!(result.is_err(), "Operations audit must be append-only");
}
