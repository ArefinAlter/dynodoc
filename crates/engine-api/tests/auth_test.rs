//! Stage-17 auth integration tests (DB-backed, real router).
//!
//! Each `#[sqlx::test(migrations = "../../migrations")]` provisions a fresh migrated
//! database; the tests drive the actual Axum app via `tower::ServiceExt::oneshot`, so
//! the magic-link → verify → access-token flow, single-use enforcement, refresh
//! rotation, the bearer extractor, and the access-list lookup are all exercised
//! end-to-end. Requires a reachable Postgres (DATABASE_URL): the dev DB or CI service.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use engine_api::auth::{store, AuthConfig};
use engine_api::{app, AppState};
use engine_core::governance::Role;
use engine_shared::DocumentId;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

fn test_state(pool: PgPool) -> AppState {
    AppState::new(
        pool,
        AuthConfig {
            paseto_key: [7u8; 32],
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    )
}

fn post(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
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

/// Request a magic link for `email` and return the one-time token from the response.
async fn request_link(router: &Router, email: &str) -> String {
    let (status, body) = send(
        router,
        post(
            "/auth/magic-link",
            &json!({ "email": email, "display_name": "Dr. Test" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "magic-link request failed: {body}");
    body["magic_link_token"].as_str().unwrap().to_string()
}

#[sqlx::test(migrations = "../../migrations")]
async fn register_verify_me_round_trip(pool: PgPool) {
    let router = app(test_state(pool));

    let token = request_link(&router, "author@dynodoc.local").await;

    // Exchange the magic-link token for an access token.
    let (status, body) = send(&router, post("/auth/verify", &json!({ "token": token }))).await;
    assert_eq!(status, StatusCode::OK, "verify failed: {body}");
    let access = body["access_token"].as_str().unwrap();
    assert_eq!(body["token_type"], "Bearer");
    assert!(body["refresh_token"].as_str().is_some());

    // The access token authenticates GET /auth/me.
    let req = Request::builder()
        .uri("/auth/me")
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&router, req).await;
    assert_eq!(status, StatusCode::OK, "me failed: {body}");
    assert_eq!(body["email"], "author@dynodoc.local");
    assert_eq!(body["display_name"], "Dr. Test");
}

#[sqlx::test(migrations = "../../migrations")]
async fn magic_link_is_single_use(pool: PgPool) {
    let router = app(test_state(pool));
    let token = request_link(&router, "once@dynodoc.local").await;

    let (first, _) = send(&router, post("/auth/verify", &json!({ "token": token }))).await;
    assert_eq!(first, StatusCode::OK);

    // The same token cannot be redeemed twice.
    let (second, _) = send(&router, post("/auth/verify", &json!({ "token": token }))).await;
    assert_eq!(second, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_magic_link_is_rejected(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let token = request_link(&router, "stale@dynodoc.local").await;

    // Force the link past its expiry, then attempt to verify.
    sqlx::query("update magic_link set expires_at = now() - interval '1 hour'")
        .execute(&pool)
        .await
        .unwrap();

    let (status, _) = send(&router, post("/auth/verify", &json!({ "token": token }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn refresh_rotates_and_invalidates_the_old_token(pool: PgPool) {
    let router = app(test_state(pool));
    let token = request_link(&router, "rotate@dynodoc.local").await;
    let (_, verified) = send(&router, post("/auth/verify", &json!({ "token": token }))).await;
    let r1 = verified["refresh_token"].as_str().unwrap().to_string();

    // First rotation succeeds and yields a new refresh token.
    let (status, body) = send(
        &router,
        post("/auth/refresh", &json!({ "refresh_token": r1 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "first refresh failed: {body}");
    let r2 = body["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(r1, r2);
    assert!(body["access_token"].as_str().is_some());

    // The rotated (old) token is now dead.
    let (replay, _) = send(
        &router,
        post("/auth/refresh", &json!({ "refresh_token": r1 })),
    )
    .await;
    assert_eq!(
        replay,
        StatusCode::UNAUTHORIZED,
        "rotated token must be rejected"
    );

    // The successor still works.
    let (ok, _) = send(
        &router,
        post("/auth/refresh", &json!({ "refresh_token": r2 })),
    )
    .await;
    assert_eq!(ok, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn me_requires_a_valid_bearer_token(pool: PgPool) {
    let router = app(test_state(pool));

    // No Authorization header.
    let (status, _) = send(
        &router,
        Request::builder()
            .uri("/auth/me")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Garbage bearer token.
    let req = Request::builder()
        .uri("/auth/me")
        .header(header::AUTHORIZATION, "Bearer not-a-real-token")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&router, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn role_for_resolves_the_access_list(pool: PgPool) {
    // Seed an identity and a document, then grant a role.
    let identity = store::upsert_identity(&pool, "reviewer@dynodoc.local", Some("Rev"))
        .await
        .unwrap();
    let document_id: uuid::Uuid =
        sqlx::query_scalar("insert into document (title) values ($1) returning id")
            .bind("Instrument")
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc = DocumentId(document_id);

    // No access yet.
    assert_eq!(
        store::role_for(&pool, doc, identity.id.0).await.unwrap(),
        None
    );

    store::grant_access(&pool, doc, identity.id.0, Role::Reviewer)
        .await
        .unwrap();
    assert_eq!(
        store::role_for(&pool, doc, identity.id.0).await.unwrap(),
        Some(Role::Reviewer)
    );

    // Re-granting changes the role (upsert).
    store::grant_access(&pool, doc, identity.id.0, Role::Author)
        .await
        .unwrap();
    assert_eq!(
        store::role_for(&pool, doc, identity.id.0).await.unwrap(),
        Some(Role::Author)
    );

    // An unrelated document has no access entry.
    let other = DocumentId(uuid::Uuid::from_u128(0xBEEF));
    assert_eq!(
        store::role_for(&pool, other, identity.id.0).await.unwrap(),
        None
    );
}
