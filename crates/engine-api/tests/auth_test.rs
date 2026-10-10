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
use futures::StreamExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

const SERVICE_KEY: &str = "isolated-auth-test-service-key-32-chars";

fn test_state(pool: PgPool) -> AppState {
    AppState::new(
        pool,
        AuthConfig {
            paseto_key: [7u8; 32],
            service_key: Some(SERVICE_KEY.into()),
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
        .header("x-dynodoc-service-key", SERVICE_KEY)
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

async fn login(router: &Router, email: &str) -> Value {
    let token = request_link(router, email).await;
    let (status, tokens) = send(router, post("/auth/verify", &json!({"token":token}))).await;
    assert_eq!(status, StatusCode::OK);
    tokens
}

async fn me_status(router: &Router, access: &Value) -> StatusCode {
    send(
        router,
        Request::builder()
            .uri("/auth/me")
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", access.as_str().unwrap()),
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .0
}

#[sqlx::test(migrations = "../../migrations")]
async fn issuance_requires_configured_service_credentials_in_every_environment(pool: PgPool) {
    for key in [
        None,
        Some(""),
        Some("short"),
        Some("                                "),
        Some(SERVICE_KEY),
    ] {
        let mut state = test_state(pool.clone());
        state.auth.service_key = key.map(str::to_owned);
        let router = app(state);
        for endpoint in ["/auth/magic-link", "/auth/exchange"] {
            for presented in [None, Some("wrong"), Some(SERVICE_KEY)] {
                let mut req = post(endpoint, &json!({"email":"guard@example.test"}));
                req.headers_mut().remove("x-dynodoc-service-key");
                if let Some(presented) = presented {
                    req.headers_mut()
                        .insert("x-dynodoc-service-key", presented.parse().unwrap());
                }
                let (status, body) = send(&router, req).await;
                if key == Some(SERVICE_KEY) && presented == Some(SERVICE_KEY) {
                    assert_eq!(status, StatusCode::OK, "{endpoint}: {body}");
                } else {
                    assert_eq!(status, StatusCode::FORBIDDEN, "{endpoint}: {body}");
                    assert!(body.get("magic_link_token").is_none());
                    assert!(body.get("access_token").is_none());
                }
            }
        }
    }
    let count: i64 = sqlx::query_scalar("select count(*) from identity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_revokes_rotated_login_but_preserves_another_device(pool: PgPool) {
    let router = app(test_state(pool));
    let first = login(&router, "logout@example.test").await;
    let other = login(&router, "logout@example.test").await;
    let (status, rotated) = send(
        &router,
        post(
            "/auth/refresh",
            &json!({"refresh_token":first["refresh_token"]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Logout accepts the ancestor secret: the browser may still have the old cookie.
    for _ in 0..2 {
        assert_eq!(
            send(
                &router,
                post(
                    "/auth/logout",
                    &json!({"refresh_token":first["refresh_token"]})
                )
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    for session in [&first, &rotated] {
        assert_eq!(
            me_status(&router, &session["access_token"]).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                &router,
                post(
                    "/auth/refresh",
                    &json!({"refresh_token":session["refresh_token"]})
                )
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        me_status(&router, &other["access_token"]).await,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            post("/auth/logout", &json!({"refresh_token":"unknown"}))
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_rotation_cannot_escape_logout(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    for n in 0..8 {
        let session = login(&router, &format!("race{n}@example.test")).await;
        let refresh = post(
            "/auth/refresh",
            &json!({"refresh_token":session["refresh_token"]}),
        );
        let logout = post(
            "/auth/logout",
            &json!({"refresh_token":session["refresh_token"]}),
        );
        let ((refresh_status, rotated), (logout_status, _)) =
            tokio::join!(send(&router, refresh), send(&router, logout));
        assert_eq!(logout_status, StatusCode::OK);
        assert_eq!(
            me_status(&router, &session["access_token"]).await,
            StatusCode::UNAUTHORIZED
        );
        if refresh_status == StatusCode::OK {
            assert_eq!(
                me_status(&router, &rotated["access_token"]).await,
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                send(
                    &router,
                    post(
                        "/auth/refresh",
                        &json!({"refresh_token":rotated["refresh_token"]})
                    )
                )
                .await
                .0,
                StatusCode::UNAUTHORIZED
            );
        } else {
            assert_eq!(refresh_status, StatusCode::UNAUTHORIZED);
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn legacy_logout_revokes_generation_once_without_reactivating_on_login(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let identity = store::upsert_identity(&pool, "legacy@example.test", None)
        .await
        .unwrap()
        .id
        .0;
    let access = engine_api::auth::token::mint(&[7u8; 32], identity, 3600).unwrap();
    let (refresh, hash) = engine_api::auth::random_token().unwrap();
    store::issue_refresh(
        &pool,
        identity,
        &hash,
        chrono::Utc::now() + chrono::Duration::days(30),
    )
    .await
    .unwrap();
    assert_eq!(me_status(&router, &json!(access)).await, StatusCode::OK);
    assert_eq!(
        send(
            &router,
            post("/auth/logout", &json!({"refresh_token":refresh}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        me_status(&router, &json!(access)).await,
        StatusCode::UNAUTHORIZED
    );
    let new_login = login(&router, "legacy@example.test").await;
    assert_eq!(
        send(
            &router,
            post("/auth/logout", &json!({"refresh_token":refresh}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        me_status(&router, &new_login["access_token"]).await,
        StatusCode::OK
    );
    assert_eq!(store::session_generation(&pool, identity).await.unwrap(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_stops_data_on_an_already_open_stream(pool: PgPool) {
    let state = test_state(pool.clone());
    let router = app(state.clone());
    let session = login(&router, "stream-logout@example.test").await;
    let access = session["access_token"].as_str().unwrap();
    let identity = engine_api::auth::token::verify(&[7u8; 32], access).unwrap();
    let mut create = post("/documents", &json!({"title":"Revoked stream"}));
    create.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {access}").parse().unwrap(),
    );
    let (status, document) = send(&router, create).await;
    assert_eq!(status, StatusCode::OK);
    let doc = DocumentId(uuid::Uuid::parse_str(document["id"].as_str().unwrap()).unwrap());
    let request = Request::builder()
        .uri(format!("/documents/{}/stream", doc.0))
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    assert!(
        String::from_utf8_lossy(&stream.next().await.unwrap().unwrap()).contains(": connected")
    );
    assert_eq!(
        send(
            &router,
            post(
                "/auth/logout",
                &json!({"refresh_token":session["refresh_token"]})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut tx = pool.begin().await.unwrap();
    let event = engine_core::log::append_in_tx(
        &mut tx,
        doc,
        &engine_shared::EventPayload::CommentAdded {
            node_id: None,
            body: "must not be delivered".into(),
        },
        engine_shared::IdentityId(identity),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    state.subscriptions.publish(doc, event);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .is_none()
    );
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
