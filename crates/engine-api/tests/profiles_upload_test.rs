//! Profile privacy and original-file upload boundaries through the real router.
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

#[sqlx::test(migrations = "../../migrations")]
async fn profiles_require_shared_project_membership_and_preserve_manual_names(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "profile-owner@example.test").await;
    let member = identity(&pool, "profile-member@example.test").await;
    let stranger = identity(&pool, "profile-stranger@example.test").await;
    let (status, own) = get(&router, "/profile", &owner).await;
    assert_eq!(status, StatusCode::OK);
    assert!(own.get("email").is_none());
    let id = Uuid::parse_str(own["id"].as_str().unwrap()).unwrap();
    let endpoint = format!("/profiles/{id}");
    assert_eq!(
        get(&router, "/profile", "").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&router, &endpoint, &member).await.0,
        StatusCode::NOT_FOUND
    );
    let edit = json!({"name":"  Chosen name  ","bio":"Works on team reports","revision":0});
    let (status, saved) = post(&router, "/profile", &owner, edit.clone()).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["name"], "Chosen name");
    assert_eq!(saved["revision"], 1);
    assert_eq!(
        post(&router, "/profile", &owner, edit).await.0,
        StatusCode::CONFLICT
    );
    auth_store::upsert_identity(&pool, "profile-owner@example.test", Some("Provider name"))
        .await
        .unwrap();
    assert_eq!(
        get(&router, "/profile", &owner).await.1["name"],
        "Chosen name"
    );
    for (name, bio) in [
        ("".to_owned(), "".to_owned()),
        ("A".repeat(81), "".to_owned()),
        ("Name".into(), "B".repeat(281)),
        ("New\nname".into(), "".into()),
    ] {
        assert_eq!(
            post(
                &router,
                "/profile",
                &owner,
                json!({"name":name,"bio":bio,"revision":1})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    // Sharing a standalone document does not make a profile visible.
    let (status, doc) = post(
        &router,
        "/documents",
        &owner,
        json!({"title":"Private report"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let doc_id = doc["id"].as_str().unwrap();
    assert_eq!(
        post(
            &router,
            &format!("/documents/{doc_id}/members"),
            &owner,
            json!({"email":"profile-member@example.test","role":"viewer","send_email":false})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        get(&router, &endpoint, &member).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, project) =
        post(&router, "/projects", &owner, json!({"name":"Profile team"})).await;
    assert_eq!(status, StatusCode::OK);
    let pid = project["id"].as_str().unwrap();
    let members = format!("/spaces/{pid}/members");
    let invite = json!({"email":"profile-member@example.test","role":"viewer","send_email":false});
    assert_eq!(
        post(&router, &members, &owner, invite.clone()).await.0,
        StatusCode::OK
    );
    assert_eq!(get(&router, &endpoint, &member).await.1, saved);
    assert_eq!(
        get(&router, &endpoint, &stranger).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post(
            &router,
            &members,
            &owner,
            json!({"email":"profile-member@example.test","role":"remove"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        get(&router, &endpoint, &member).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post(&router, &members, &owner, invite).await.0,
        StatusCode::OK
    );
    sqlx::query("update identity set disabled_at=now() where id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get(&router, &endpoint, &member).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn uploads_enforce_three_million_original_bytes_without_creating_oversize_rows(pool: PgPool) {
    use base64::Engine;
    let router = app(test_state(pool.clone()));
    let owner = identity(&pool, "upload-owner@example.test").await;
    let viewer = identity(&pool, "upload-viewer@example.test").await;
    let (status, doc) = post(
        &router,
        "/documents",
        &owner,
        json!({"title":"Boundary report"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = doc["id"].as_str().unwrap();
    let endpoint = format!("/documents/{id}/uploads");
    let body = |size| json!({"filename":"boundary.txt","content_type":"text/plain","content":base64::engine::general_purpose::STANDARD.encode(vec![b'x';size]),"report":{}});
    assert_eq!(
        post(&router, &endpoint, &owner, body(3_000_000)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        post(&router, &endpoint, &owner, body(3_000_001)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(&router, &endpoint, &owner, body(4_000_000)).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        post(
            &router,
            &format!("/documents/{id}/members"),
            &owner,
            json!({"email":"upload-viewer@example.test","role":"viewer","send_email":false})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        post(&router, &endpoint, &viewer, body(1)).await.0,
        StatusCode::FORBIDDEN
    );
    let rows: (i64, i64) = sqlx::query_as("select count(*), sum(octet_length(content))::bigint from document_upload where document_id=$1")
        .bind(Uuid::parse_str(id).unwrap()).fetch_one(&pool).await.unwrap();
    assert_eq!(rows, (1, 3_000_000));
}
