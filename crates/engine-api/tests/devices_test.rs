//! Enrollment and revocation through the real router and restricted database login.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use engine_api::{
    app,
    auth::{store, token, AuthConfig},
    AppState,
};
use engine_core::repository::{
    device::{Challenge, Proof},
    Commit, SignedCommit,
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
mod common;
const KEY: [u8; 32] = [9; 32];

#[sqlx::test(migrations = "../../migrations")]
async fn enrollment_rechecks_logout_after_waiting_for_the_account_lock(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (id, initial) = actor(&runtime.pool, "race").await;
    let (proof, _) = proof(&app, &initial, 7).await;
    let session = Uuid::new_v4();
    store::issue_bound_refresh(
        &runtime.pool,
        id,
        &[4; 32],
        chrono::Utc::now() + chrono::Duration::hours(1),
        0,
        Some(session),
    )
    .await
    .unwrap();
    let login = token::mint_bound_session(&KEY, id, 3600, 0, Some(session)).unwrap();
    let mut logout = pool.begin().await.unwrap();
    sqlx::query("select id from identity where id=$1 for update")
        .bind(id)
        .fetch_one(&mut *logout)
        .await
        .unwrap();
    let request_app = app.clone();
    let pending = tokio::spawn(async move {
        request(&request_app, "POST", "/devices", &login, json!(proof)).await
    });
    // Observe the actual lock wait, rather than depending on a scheduling sleep.
    let started = std::time::Instant::now();
    loop {
        let waiting: bool = sqlx::query_scalar("select exists(select 1 from pg_stat_activity where datname=current_database() and wait_event_type='Lock' and query='select id from identity where id=$1 for update')").fetch_one(&pool).await.unwrap();
        if waiting {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "enrollment did not wait on account lock"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    sqlx::query("update refresh_token set revoked_at=now() where identity_id=$1 and session_id=$2")
        .bind(id)
        .bind(session)
        .execute(&mut *logout)
        .await
        .unwrap();
    logout.commit().await.unwrap();
    assert_eq!(pending.await.unwrap().0, StatusCode::UNAUTHORIZED);
    let count: i64 =
        sqlx::query_scalar("select count(*) from device_signing_key where identity_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_registration_cannot_exceed_active_key_budget(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool).await;
    let app = router(runtime.pool.clone());
    let (_, login) = actor(&runtime.pool, "quota").await;
    for seed in 1..20 {
        // Test setup resets just the rate window, so the distinct active-key quota
        // can be exercised without waiting fifteen minutes between enrollments.
        sqlx::query("delete from auth_request_window")
            .execute(&runtime.pool)
            .await
            .unwrap();
        let (proof, _) = proof(&app, &login, seed).await;
        assert_eq!(
            request(&app, "POST", "/devices", &login, json!(proof))
                .await
                .0,
            StatusCode::OK
        );
    }
    let (left, _) = proof(&app, &login, 20).await;
    let (right, _) = proof(&app, &login, 21).await;
    let (left, right) = tokio::join!(
        request(&app, "POST", "/devices", &login, json!(left)),
        request(&app, "POST", "/devices", &login, json!(right))
    );
    let mut statuses = [left.0.as_u16(), right.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    assert_eq!(
        request(&app, "GET", "/devices", &login, Value::Null)
            .await
            .1["keys"]
            .as_array()
            .unwrap()
            .len(),
        20
    );
    runtime.close().await;
}

fn router(pool: PgPool) -> Router {
    app(AppState::new(
        pool,
        AuthConfig {
            paseto_key: KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    ))
}
async fn actor(pool: &PgPool, name: &str) -> (Uuid, String) {
    let id = store::upsert_identity(pool, &format!("{name}@example.test"), None)
        .await
        .unwrap()
        .id
        .0;
    (id, token::mint(&KEY, id, 3600).unwrap())
}
async fn request(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn proof(app: &Router, token: &str, seed: u8) -> (Proof, Ed25519KeyPair) {
    let key = Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap();
    let (status, challenge) = request(
        app,
        "POST",
        "/devices/challenge",
        token,
        json!({"public_key":hex::encode(key.public_key().as_ref())}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{challenge}");
    let challenge: Challenge = serde_json::from_value(challenge).unwrap();
    (
        Proof::sign(
            challenge.clone(),
            challenge.authority,
            challenge.identity,
            &key,
        )
        .unwrap(),
        key,
    )
}
fn commit(key: &Ed25519KeyPair) -> Value {
    json!(SignedCommit::sign(
        Commit {
            version: 1,
            project: Uuid::new_v4(),
            parents: vec![],
            files: Default::default(),
            author_key: String::new(),
            authored_at_ms: 1,
            message: "Local history".into(),
            recovery: None,
            same_file_resolutions: Vec::new(),
        },
        key
    )
    .unwrap())
}

#[sqlx::test(migrations = "../../migrations")]
async fn enrollment_is_private_account_bound_immutable_and_permanently_revocable(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (alice, login) = actor(&runtime.pool, "alice").await;
    let (_, other) = actor(&runtime.pool, "bob").await;
    let (proof, key) = proof(&app, &login, 7).await;
    let signed = commit(&key);
    assert_eq!(
        request(
            &app,
            "POST",
            "/devices/verify-commit",
            &login,
            signed.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "POST", "/devices", &other, json!(proof))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let mut altered = proof.clone();
    altered.challenge.authority = Uuid::new_v4();
    assert_eq!(
        request(&app, "POST", "/devices", &login, json!(altered))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    // Even a valid signature cannot replace any field of the issued challenge.
    altered = Proof::sign(
        altered.challenge.clone(),
        altered.challenge.authority,
        alice,
        &key,
    )
    .unwrap();
    assert_eq!(
        request(&app, "POST", "/devices", &login, json!(altered))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (a, b) = tokio::join!(
        request(&app, "POST", "/devices", &login, json!(proof)),
        request(&app, "POST", "/devices", &login, json!(proof))
    );
    assert_eq!(a.0, StatusCode::OK, "{}", a.1);
    assert_eq!(a, b);
    let verified = request(
        &app,
        "POST",
        "/devices/verify-commit",
        &login,
        signed.clone(),
    )
    .await;
    assert_eq!(verified.0, StatusCode::OK);
    assert_eq!(verified.1["published"], false);
    assert_eq!(
        request(
            &app,
            "POST",
            "/devices/verify-commit",
            &other,
            signed.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "GET", "/devices", &other, Value::Null)
            .await
            .1["keys"],
        json!([])
    );
    let own = request(&app, "GET", "/devices", &login, Value::Null)
        .await
        .1;
    assert_eq!(own["keys"].as_array().unwrap().len(), 1);
    assert!(own["keys"][0].get("proof").is_none());
    for sql in [
        "update device_signing_key set public_key=repeat('0',64)",
        "delete from device_signing_key",
        "truncate device_signing_key cascade",
        "delete from device_authority",
    ] {
        assert!(
            sqlx::query(sql).execute(&runtime.pool).await.is_err(),
            "{sql}"
        );
    }
    // Owner credentials cannot accidentally mutate through normal DML either.
    assert!(sqlx::query("update device_signing_key set identity_id=$1")
        .bind(alice)
        .execute(&pool)
        .await
        .is_err());
    let revoke = format!("/devices/{}/revoke", proof.challenge.id);
    assert_eq!(
        request(&app, "POST", &revoke, &other, json!({})).await.0,
        StatusCode::NOT_FOUND
    );
    for _ in 0..2 {
        assert_eq!(
            request(&app, "POST", &revoke, &login, json!({})).await.0,
            StatusCode::OK
        );
    }
    assert_eq!(
        request(&app, "POST", "/devices", &login, json!(proof))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(&app, "POST", "/devices/verify-commit", &login, signed)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/devices/challenge",
            &other,
            json!({"public_key":proof.challenge.public_key})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let audit: i64 = sqlx::query_scalar("select count(*) from operation_audit where actor_id=$1 and action in ('device.enrolled','device.revoked')").bind(alice).fetch_one(&pool).await.unwrap();
    assert_eq!(audit, 2);
    assert!(sqlx::query("delete from device_key_revocation")
        .execute(&runtime.pool)
        .await
        .is_err());
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn enrollment_expiry_rate_bounds_and_account_removal_survive_restart(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool).await;
    let app = router(runtime.pool.clone());
    let (id, login) = actor(&runtime.pool, "limits").await;
    let (expired, _) = proof(&app, &login, 7).await;
    sqlx::query(
        "update device_enrollment_challenge set expires_at=now()-interval '1 second' where id=$1",
    )
    .bind(expired.challenge.id)
    .execute(&runtime.pool)
    .await
    .unwrap();
    assert_eq!(
        request(&app, "POST", "/devices", &login, json!(expired))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (enroll, key) = proof(&app, &login, 8).await;
    assert_eq!(
        request(&app, "POST", "/devices", &login, json!(enroll))
            .await
            .0,
        StatusCode::OK
    );
    for seed in 10..15 {
        proof(&app, &login, seed).await;
    }
    for _ in 0..5 {
        assert_eq!(
            request(
                &router(runtime.pool.clone()),
                "POST",
                "/devices/challenge",
                &login,
                json!({"public_key":"01".repeat(32)})
            )
            .await
            .0,
            StatusCode::TOO_MANY_REQUESTS
        );
    }
    let count: i64 =
        sqlx::query_scalar("select count(*) from device_enrollment_challenge where identity_id=$1")
            .bind(id)
            .fetch_one(&runtime.pool)
            .await
            .unwrap();
    assert_eq!(count, 5);
    sqlx::query(
        "update identity set disabled_at=now(),session_generation=session_generation+1 where id=$1",
    )
    .bind(id)
    .execute(&runtime.pool)
    .await
    .unwrap();
    assert_eq!(
        request(&app, "GET", "/devices", &login, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("update identity set disabled_at=null where id=$1")
        .bind(id)
        .execute(&runtime.pool)
        .await
        .unwrap();
    let restored = token::mint_generation(&KEY, id, 3600, 1).unwrap();
    let own = request(&app, "GET", "/devices", &restored, Value::Null)
        .await
        .1;
    assert!(!own["keys"][0]["revoked_at"].is_null());
    assert_eq!(
        request(
            &app,
            "POST",
            "/devices/verify-commit",
            &restored,
            commit(&key)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "POST", "/devices", &restored, json!(enroll))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let count: i64 =
        sqlx::query_scalar("select count(*) from device_enrollment_challenge where identity_id=$1")
            .bind(id)
            .fetch_one(&runtime.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        request(
            &app,
            "POST",
            "/devices/challenge",
            &restored,
            json!({"public_key":"A".repeat(64)})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/devices",
            &restored,
            json!({"padding":"a".repeat(2049)})
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    runtime.close().await;
}
