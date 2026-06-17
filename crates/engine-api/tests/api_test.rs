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

fn test_state(pool: PgPool) -> AppState {
    AppState::new(
        pool,
        AuthConfig {
            paseto_key: KEY,
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
    let (status, body) = send(&router, get(&format!("/documents/{doc_id}"))).await;
    assert_eq!(status, StatusCode::OK, "get document failed: {body}");
    assert_eq!(body["title"], "Survey");
    assert!(body["state"]["nodes"].is_object());

    // Public list includes it.
    let (status, body) = send(&router, get("/documents")).await;
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
async fn missing_document_is_404(pool: PgPool) {
    let router = app(test_state(pool));
    let (status, _) = send(&router, get(&format!("/documents/{}", Uuid::new_v4()))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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
    let (status, body) = send(&router, get(&format!("/documents/{doc_id}/node/{node_id}"))).await;
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

    let (_, body) = send(&router, get(&format!("/documents/{doc_id}/node/{node_id}"))).await;
    assert_eq!(body["current_fields"]["title"], "New");
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
async fn verify_is_public_and_ok_for_a_clean_log(pool: PgPool) {
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

    let (status, body) = send(&router, get(&format!("/documents/{doc_id}/verify"))).await;
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
