//! Generated operations of every type, sent through the API one at a time. After each,
//! the node table must match a verified replay apart from the two known encodings.
#[path = "../../engine-core/tests/support/gen.rs"]
mod gen;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use engine_api::auth::{store as auth_store, token, AuthConfig};
use engine_api::{app, AppState};
use engine_core::materializer::DocumentState;
use engine_core::projection::verify_projection;
use engine_shared::{DocumentId, EventPayload};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::BTreeSet;
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

async fn send(
    router: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header(header::AUTHORIZATION, token)
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// The discussion revision the reply and resolve endpoint expects.
async fn revision(pool: &PgPool, doc: Uuid, thread: &str) -> i64 {
    sqlx::query_scalar("select coalesce(max(seq),(select seq from event where id=$2)) from event where document_id=$1 and type in ('CommentReplied','CommentResolved') and payload->>'thread_id'=$2")
        .bind(doc)
        .bind(thread)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn node_table_matches_replay_after_every_generated_operation(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let actor = auth_store::upsert_identity(&pool, "generated@example.test", Some("Tester"))
        .await
        .unwrap()
        .id
        .0;
    let token = format!("Bearer {}", token::mint(&KEY, actor, 3600).unwrap());
    let mut kinds = BTreeSet::new();
    for seed in 1..=100u64 {
        let (status, created) = send(
            &router,
            "POST",
            "/documents",
            &token,
            json!({"title": format!("Generated {seed}")}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let doc: Uuid = created["id"].as_str().unwrap().parse().unwrap();
        let mut gen = gen::Gen::new(seed);
        let mut threads: Vec<gen::Thread> = Vec::new();
        let mut state = DocumentState::default();
        let mut sent = 0;
        while sent < 20 {
            let Some(op) = gen.op(&state, &threads) else {
                continue;
            };
            let kind = op.event_type().as_str();
            let (uri, body) = match &op {
                EventPayload::CommentReplied { thread_id, body } => (
                    format!("/documents/{doc}/discussions/{thread_id}"),
                    json!({"revision": revision(&pool, doc, thread_id).await, "body": body}),
                ),
                EventPayload::CommentResolved {
                    thread_id,
                    resolved,
                } => (
                    format!("/documents/{doc}/discussions/{thread_id}"),
                    json!({"revision": revision(&pool, doc, thread_id).await, "resolved": resolved}),
                ),
                EventPayload::SuggestionAccepted { suggestion_id } => (
                    format!("/documents/{doc}/proposals/{suggestion_id}/accept"),
                    json!({}),
                ),
                EventPayload::SuggestionRejected {
                    suggestion_id,
                    reason,
                } => (
                    format!("/documents/{doc}/proposals/{suggestion_id}/reject"),
                    json!({ "reason": reason }),
                ),
                EventPayload::Deployed { .. } => (format!("/documents/{doc}/deploy"), json!({})),
                _ => (format!("/documents/{doc}/batch"), json!({ "ops": [op] })),
            };
            let (status, response) = send(&router, "POST", &uri, &token, body).await;
            assert_eq!(status, StatusCode::OK, "seed {seed}, {kind}: {response}");
            match &op {
                EventPayload::CommentAdded { .. } => {
                    threads.push((response["events"][0]["id"].as_str().unwrap().into(), false));
                }
                EventPayload::CommentResolved {
                    thread_id,
                    resolved,
                } => {
                    threads
                        .iter_mut()
                        .filter(|t| &t.0 == thread_id)
                        .for_each(|t| t.1 = *resolved);
                }
                _ => {}
            }
            kinds.insert(kind);
            let (_, current) = send(
                &router,
                "GET",
                &format!("/documents/{doc}"),
                &token,
                json!({}),
            )
            .await;
            state = serde_json::from_value(current["state"].clone()).unwrap();
            let report = verify_projection(&pool, DocumentId(doc)).await.unwrap();
            let unexplained: Vec<_> = report.unexplained().collect();
            assert!(
                unexplained.is_empty(),
                "seed {seed}, after {kind}: {unexplained:?}"
            );
            sent += 1;
        }
    }
    assert_eq!(kinds.len(), gen::KINDS, "{kinds:?}");
}
