//! Project device sessions through the real router and restricted database login.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use engine_api::{
    app,
    auth::{
        store,
        token::{self, DeviceClaims},
        AuthConfig,
    },
    AppState,
};
use engine_core::repository::{
    device::{Challenge, Proof},
    device_session::SessionProof,
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
mod common;
const KEY: [u8; 32] = [9; 32];

fn router(pool: PgPool) -> Router {
    let mut state = AppState::new(
        pool,
        AuthConfig {
            paseto_key: KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    );
    // Administrators are real: a confused token would otherwise reach /admin.
    state.admin_emails = "owner@example.test".into();
    app(state)
}
async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        request = request.header("content-type", "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn actor(pool: &PgPool, name: &str) -> (Uuid, String) {
    let id = store::upsert_identity(pool, &format!("{name}@example.test"), None)
        .await
        .unwrap()
        .id
        .0;
    (id, token::mint(&KEY, id, 3600).unwrap())
}
fn key(seed: u8) -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap()
}
/// Enroll through the real account flow; the device key ID is the challenge ID.
async fn enroll(app: &Router, login: &str, seed: u8) -> (Ed25519KeyPair, Uuid) {
    let key = key(seed);
    let (status, challenge) = call(
        app,
        "POST",
        "/devices/challenge",
        Some(login),
        json!({"public_key":hex::encode(key.public_key().as_ref())}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{challenge}");
    let challenge: Challenge = serde_json::from_value(challenge).unwrap();
    let proof = Proof::sign(
        challenge.clone(),
        challenge.authority,
        challenge.identity,
        &key,
    )
    .unwrap();
    // These tests pin the legacy account-wide behaviour; per-project grants are
    // covered by device_key_grants_test.rs, where the default (`granted`) is used.
    let mut body = json!(proof);
    body["project_access"] = json!("all");
    let (status, body) = call(app, "POST", "/devices", Some(login), body).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["project_access"], "all");
    (key, challenge.id)
}
async fn project(app: &Router, login: &str, name: &str) -> Uuid {
    let (status, body) = call(app, "POST", "/projects", Some(login), json!({"name":name})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_value(body["id"].clone()).unwrap()
}
async fn member(pool: &PgPool, project: Uuid, identity: Uuid, role: &str) {
    sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,$3)")
        .bind(project)
        .bind(identity)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}
/// Ask for a challenge and answer it with `key`.
async fn answer(
    app: &Router,
    project: Uuid,
    key: &Ed25519KeyPair,
    scopes: &[&str],
) -> SessionProof {
    let (status, body) = call(
        app,
        "POST",
        &format!("/projects/{project}/device-sessions/challenge"),
        None,
        json!({"public_key":hex::encode(key.public_key().as_ref()),"scopes":scopes}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let challenge = serde_json::from_value(body["challenge"].clone()).unwrap();
    let scopes: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
    let challenge: engine_core::repository::device_session::SessionChallenge = challenge;
    SessionProof::sign(
        challenge.clone(),
        body["mac"].as_str().unwrap().to_owned(),
        challenge.authority,
        project,
        &scopes,
        key,
    )
    .unwrap()
}
async fn submit(app: &Router, project: Uuid, proof: &SessionProof) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("/projects/{project}/device-sessions"),
        None,
        json!(proof),
    )
    .await
}
async fn open(
    app: &Router,
    project: Uuid,
    key: &Ed25519KeyPair,
    scopes: &[&str],
) -> (StatusCode, Value) {
    submit(app, project, &answer(app, project, key, scopes).await).await
}
async fn token_for(app: &Router, project: Uuid, key: &Ed25519KeyPair, scopes: &[&str]) -> String {
    let (status, body) = open(app, project, key, scopes).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["access_token"].as_str().unwrap().to_owned()
}
async fn status(app: &Router, method: &str, uri: &str, token: &str) -> u16 {
    call(app, method, uri, Some(token), json!({}))
        .await
        .0
        .as_u16()
}
async fn status_get(app: &Router, uri: &str, token: &str) -> u16 {
    call(app, "GET", uri, Some(token), Value::Null)
        .await
        .0
        .as_u16()
}

#[sqlx::test(migrations = "../../migrations")]
async fn issue_use_renew_replay_and_the_explicit_route_allow_list(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (_owner, login) = actor(&runtime.pool, "owner").await;
    let p = project(&app, &login, "Device sessions").await;
    let other = project(&app, &login, "Second project").await;
    let (device, _) = enroll(&app, &login, 7).await;
    engine_core::repository::server_authority::install(
        &runtime.owner,
        &hex::encode(key(99).public_key().as_ref()),
    )
    .await
    .unwrap();

    let proof = answer(&app, p, &device, &["history:read", "project:read"]).await;
    let (code, issued) = submit(&app, p, &proof).await;
    assert_eq!(code, StatusCode::OK, "{issued}");
    assert_eq!(issued["expires_in"], 3600);
    assert_eq!(issued["scopes"], json!(["history:read", "project:read"]));
    assert_eq!(issued["project"], json!(p));
    let read = issued["access_token"].as_str().unwrap().to_owned();
    // The bearer is not stored anywhere; the ledger holds the challenge, not it.
    let stored: String =
        sqlx::query_scalar("select to_jsonb(s)::text from device_session s where id=$1")
            .bind(proof.challenge.id)
            .fetch_one(&runtime.pool)
            .await
            .unwrap();
    assert!(!stored.contains(&read) && !stored.contains("v4.local"));

    // Allowed: exactly the read scopes, for this project.
    for uri in [
        format!("/projects/{p}/files"),
        format!("/projects/{p}/activity"),
        format!("/projects/{p}/history/refs"),
        format!("/projects/{p}/history/authority"),
        format!("/projects/{p}/history/storage"),
        format!("/projects/{p}/history/receipts?branch=main"),
    ] {
        let expected = if uri.ends_with("receipts?branch=main") {
            404
        } else {
            200
        };
        assert_eq!(status_get(&app, &uri, &read).await, expected, "{uri}");
    }
    // The same routes still accept the ordinary account login.
    assert_eq!(
        status_get(&app, &format!("/projects/{p}/files"), &login).await,
        200
    );
    // Missing write scope: private staging, publication, review and ref updates.
    for (method, uri) in [
        ("GET", format!("/projects/{p}/staging/storage")),
        ("POST", format!("/projects/{p}/staging/objects/missing")),
        ("POST", format!("/projects/{p}/staging/objects")),
        ("POST", format!("/projects/{p}/staging/commits")),
        ("POST", format!("/projects/{p}/history/commits")),
        ("POST", format!("/projects/{p}/history/refs")),
        ("POST", format!("/projects/{p}/history/reviews")),
    ] {
        assert_eq!(status(&app, method, &uri, &read).await, 403, "{uri}");
    }
    // Another project, even one the account owns.
    for uri in [
        format!("/projects/{other}/files"),
        format!("/projects/{other}/history/refs"),
    ] {
        assert_eq!(status_get(&app, &uri, &read).await, 403, "{uri}");
    }
    // Routes that are not on the allow-list never decrypt the token.
    for (method, uri) in [
        ("GET", "/auth/me".to_owned()),
        ("GET", "/profile".to_owned()),
        ("GET", "/devices".to_owned()),
        ("POST", "/devices/challenge".to_owned()),
        ("GET", "/projects".to_owned()),
        ("GET", format!("/projects/{p}")),
        ("GET", format!("/projects/{p}/contributors")),
        ("GET", format!("/projects/{p}/drafts")),
        ("GET", format!("/projects/{p}/change-requests")),
        ("POST", format!("/projects/{p}/settings")),
        ("POST", format!("/projects/{p}/watch")),
        ("GET", "/documents".to_owned()),
        ("GET", "/notifications".to_owned()),
        ("GET", "/admin/overview".to_owned()),
        ("GET", "/connector/connection".to_owned()),
    ] {
        assert_eq!(status(&app, method, &uri, &read).await, 401, "{uri}");
    }

    // One use: the identical signed answer is refused, concurrently and later.
    assert_eq!(submit(&app, p, &proof).await.0, StatusCode::UNAUTHORIZED);
    let racing = answer(&app, p, &device, &["history:read"]).await;
    let (a, b) = tokio::join!(submit(&app, p, &racing), submit(&app, p, &racing));
    let mut codes = [a.0.as_u16(), b.0.as_u16()];
    codes.sort();
    assert_eq!(codes, [200, 401]);
    // A challenge for one project cannot open another.
    let misrouted = answer(&app, p, &device, &["history:read"]).await;
    assert_eq!(
        submit(&app, other, &misrouted).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Renewal is another signed challenge; old and new tokens both live to expiry.
    let renewed = token_for(&app, p, &device, &["history:read", "history:write"]).await;
    assert_ne!(renewed, read);
    assert_eq!(
        status_get(&app, &format!("/projects/{p}/staging/storage"), &renewed).await,
        200
    );
    assert_eq!(
        status_get(&app, &format!("/projects/{p}/files"), &read).await,
        200
    );
    // History write scope is independent: this token cannot read project files.
    assert_eq!(
        status_get(&app, &format!("/projects/{p}/files"), &renewed).await,
        403
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn token_confusion_matrix_fails_closed_in_both_directions(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let (other, other_login) = actor(&runtime.pool, "other").await;
    let p = project(&app, &login, "Confusion").await;
    let doc: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by,space_id) values('Report',$1,$2) returning id",
    )
    .bind(owner)
    .bind(p)
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    let (device, key_id) = enroll(&app, &login, 7).await;
    let (_, other_key_id) = enroll(&app, &other_login, 8).await;
    let issued = token_for(
        &app,
        p,
        &device,
        &["history:read", "history:write", "project:read"],
    )
    .await;
    // The administrator's identity is in the token; /admin must still refuse it.
    let matrix: Vec<(&str, String)> = vec![
        ("GET", "/auth/me".into()),
        ("GET", "/profile".into()),
        ("GET", "/profile/storage".into()),
        ("GET", "/devices".into()),
        ("POST", format!("/devices/{key_id}/revoke")),
        ("POST", "/devices/verify-commit".into()),
        ("GET", "/documents".into()),
        ("GET", format!("/documents/{doc}")),
        ("GET", format!("/documents/{doc}/events")),
        ("GET", format!("/documents/{doc}/versions")),
        ("GET", format!("/documents/{doc}/members")),
        ("GET", format!("/documents/{doc}/stream")),
        ("POST", format!("/documents/{doc}/ops/comment")),
        ("POST", format!("/documents/{doc}/connectors")),
        ("GET", "/admin/overview".into()),
        ("GET", "/admin/records?kind=audit".into()),
        ("POST", "/admin/actions".into()),
        ("GET", "/connector/connection".into()),
        ("GET", format!("/connector/documents/{doc}/checkpoint")),
        ("GET", "/notifications/count".into()),
        ("GET", "/workspace/index".into()),
        ("GET", "/spaces".into()),
        ("GET", "/search?q=report".into()),
    ];
    for (method, uri) in &matrix {
        assert_eq!(status(&app, method, uri, &issued).await, 401, "{uri}");
    }
    // /auth: the token is neither a refresh token nor a logout secret.
    let (code, _) = call(
        &app,
        "POST",
        "/auth/refresh",
        None,
        json!({"refresh_token":issued}),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (code, _) = call(
        &app,
        "POST",
        "/auth/logout",
        None,
        json!({"refresh_token":issued}),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "logout stays idempotent and silent");
    assert_eq!(
        status_get(&app, &format!("/projects/{p}/files"), &issued).await,
        200,
        "a logout request with a device token revokes nothing"
    );
    // A 64-hex connector key shape is not a device token and vice versa.
    assert_eq!(
        status_get(&app, &format!("/projects/{p}/files"), &"ab".repeat(32)).await,
        401
    );
    // Account token on a device endpoint is ignored: those take no bearer at all.
    let (code, _) = call(
        &app,
        "POST",
        &format!("/projects/{p}/device-sessions"),
        Some(&login),
        json!({}),
    )
    .await;
    assert_ne!(code, StatusCode::OK);

    // Forged envelopes: the service key holder can mint any claims, so the
    // database must still disagree where the account/key/generation do not.
    let generation: i64 = sqlx::query_scalar("select session_generation from identity where id=$1")
        .bind(owner)
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
    let claims = |identity: Uuid, key: Uuid, project: Uuid, scopes: &[&str], generation: i64| {
        token::mint_project_device(
            &KEY,
            &DeviceClaims {
                identity,
                key,
                project,
                session: Uuid::new_v4(),
                scopes: scopes.iter().map(|s| s.to_string()).collect(),
                generation,
            },
            3600,
        )
        .unwrap()
    };
    let uri = format!("/projects/{p}/files");
    let good = claims(owner, key_id, p, &["project:read"], generation);
    assert_eq!(status_get(&app, &uri, &good).await, 200);
    for forged in [
        // another account's key, a wrong generation, an unknown key
        claims(owner, other_key_id, p, &["project:read"], generation),
        claims(other, key_id, p, &["project:read"], generation),
        claims(owner, key_id, p, &["project:read"], generation + 1),
        claims(owner, Uuid::new_v4(), p, &["project:read"], generation),
    ] {
        assert_eq!(status_get(&app, &uri, &forged).await, 401);
    }
    // Wrong service key and expired lifetime.
    let foreign = token::mint_project_device(
        &[1u8; 32],
        &DeviceClaims {
            identity: owner,
            key: key_id,
            project: p,
            session: Uuid::new_v4(),
            scopes: vec!["project:read".into()],
            generation,
        },
        3600,
    )
    .unwrap();
    assert_eq!(status_get(&app, &uri, &foreign).await, 401);
    let expired = token::mint_project_device(
        &KEY,
        &DeviceClaims {
            identity: owner,
            key: key_id,
            project: p,
            session: Uuid::new_v4(),
            scopes: vec!["project:read".into()],
            generation,
        },
        -10,
    )
    .unwrap();
    assert_eq!(status_get(&app, &uri, &expired).await, 401);
    // Account tokens are not device tokens: they pass only where accounts do.
    assert_eq!(status_get(&app, &uri, &login).await, 200);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn key_revocation_membership_changes_and_account_state_apply_to_the_next_request(
    pool: PgPool,
) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let (editor, editor_login) = actor(&runtime.pool, "editor").await;
    let p = project(&app, &login, "Membership").await;
    member(&runtime.pool, p, editor, "editor").await;
    let doc: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by,space_id) values('Report',$1,$2) returning id",
    )
    .bind(owner)
    .bind(p)
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    let (editor_key, editor_key_id) = enroll(&app, &editor_login, 8).await;
    let scopes = ["history:read", "history:write", "project:read"];
    let token = token_for(&app, p, &editor_key, &scopes).await;
    let upload = json!({"document":doc,"objects":[{"address":"aa".repeat(32),"bytes":"eA=="}]});
    let files = format!("/projects/{p}/files");
    let staging = format!("/projects/{p}/staging/objects");
    // Authorized but with invalid object bytes: 400 proves the role gate was passed.
    assert_eq!(
        call(&app, "POST", &staging, Some(&token), upload.clone())
            .await
            .0,
        400
    );
    assert_eq!(status_get(&app, &files, &token).await, 200);

    // Demotion: read stays, write is refused on the very next request.
    sqlx::query("update space_member set role='viewer' where space_id=$1 and identity_id=$2")
        .bind(p)
        .bind(editor)
        .execute(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, "POST", &staging, Some(&token), upload.clone())
            .await
            .0,
        403
    );
    assert_eq!(status_get(&app, &files, &token).await, 200);
    // Cannot mint a writing session as a viewer; reading is still possible.
    assert_eq!(
        open(&app, p, &editor_key, &["history:write"]).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        open(&app, p, &editor_key, &["history:read"]).await.0,
        StatusCode::OK
    );
    // Removal: everything is refused, and so is renewal.
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(p)
        .bind(editor)
        .execute(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(status_get(&app, &files, &token).await, 403);
    assert_eq!(
        status_get(&app, &format!("/projects/{p}/history/refs"), &token).await,
        403
    );
    assert_eq!(
        open(&app, p, &editor_key, &["history:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    // Restore membership, then revoke the key through the real account route.
    member(&runtime.pool, p, editor, "editor").await;
    let token = token_for(&app, p, &editor_key, &scopes).await;
    assert_eq!(status_get(&app, &files, &token).await, 200);
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/devices/{editor_key_id}/revoke"),
            Some(&editor_login),
            json!({})
        )
        .await
        .0,
        200
    );
    assert_eq!(status_get(&app, &files, &token).await, 401);
    assert_eq!(
        call(&app, "POST", &staging, Some(&token), upload.clone())
            .await
            .0,
        401
    );
    // The challenge looks the same, but the key can no longer open a session.
    assert_eq!(
        open(&app, p, &editor_key, &["history:read"]).await.0,
        StatusCode::UNAUTHORIZED
    );

    // Account disablement (keys are revoked, generation advances) and a bare
    // generation change are both immediate.
    let (member_id, member_login) = actor(&runtime.pool, "member").await;
    member(&runtime.pool, p, member_id, "editor").await;
    let (member_key, _) = enroll(&app, &member_login, 9).await;
    let first = token_for(&app, p, &member_key, &scopes).await;
    assert_eq!(status_get(&app, &files, &first).await, 200);
    sqlx::query("update identity set session_generation=session_generation+1 where id=$1")
        .bind(member_id)
        .execute(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(status_get(&app, &files, &first).await, 401);
    // A new session after the change works: the key itself was not revoked.
    let second = token_for(&app, p, &member_key, &scopes).await;
    assert_eq!(status_get(&app, &files, &second).await, 200);
    sqlx::query(
        "update identity set disabled_at=now(),session_generation=session_generation+1 where id=$1",
    )
    .bind(member_id)
    .execute(&runtime.owner)
    .await
    .unwrap();
    assert_eq!(status_get(&app, &files, &second).await, 401);
    assert_eq!(
        open(&app, p, &member_key, &["history:read"]).await.0,
        StatusCode::UNAUTHORIZED
    );

    // An administrator ending an account's sessions must also retire its device
    // keys, which could otherwise mint a fresh session without any login.
    let (target, target_login) = actor(&runtime.pool, "target").await;
    member(&runtime.pool, p, target, "editor").await;
    let (target_key, _) = enroll(&app, &target_login, 10).await;
    let live = token_for(&app, p, &target_key, &scopes).await;
    assert_eq!(status_get(&app, &files, &live).await, 200);
    let (code, body) = call(
        &app,
        "POST",
        "/admin/control",
        Some(&login),
        json!({"action":"revoke_sessions","id":target,"confirmation":"target@example.test"}),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(status_get(&app, &files, &live).await, 401);
    assert_eq!(
        open(&app, p, &target_key, &["history:read"]).await.0,
        StatusCode::UNAUTHORIZED
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn issuance_is_uniform_bounded_pruned_and_audited_once(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let (_, outsider_login) = actor(&runtime.pool, "outsider").await;
    let p = project(&app, &login, "Budgets").await;
    let (device, key_id) = enroll(&app, &login, 7).await;
    let (outsider, _) = enroll(&app, &outsider_login, 8).await;

    // Unknown key, enrolled key and unknown project get the same challenge shape.
    let unknown = key(55);
    let shape = |body: &Value| {
        let mut names: Vec<_> = body.as_object().unwrap().keys().cloned().collect();
        names.sort();
        let mut challenge: Vec<_> = body["challenge"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        challenge.sort();
        (names, challenge)
    };
    let challenge_for = |project: Uuid, key: &Ed25519KeyPair| {
        let app = app.clone();
        let public = hex::encode(key.public_key().as_ref());
        async move {
            call(
                &app,
                "POST",
                &format!("/projects/{project}/device-sessions/challenge"),
                None,
                json!({"public_key":public,"scopes":["history:read"]}),
            )
            .await
        }
    };
    let (known, known_body) = challenge_for(p, &device).await;
    let (unk, unknown_body) = challenge_for(p, &unknown).await;
    let (nowhere, nowhere_body) = challenge_for(Uuid::new_v4(), &device).await;
    assert_eq!(
        (known, unk, nowhere),
        (StatusCode::OK, StatusCode::OK, StatusCode::OK)
    );
    assert_eq!(shape(&known_body), shape(&unknown_body));
    assert_eq!(shape(&known_body), shape(&nowhere_body));
    // ...and asking wrote nothing.
    let rows: i64 = sqlx::query_scalar("select count(*) from device_session")
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    // Bad input is a 400, never a state-dependent answer.
    for body in [
        json!({"public_key":"zz","scopes":["history:read"]}),
        json!({"public_key":hex::encode(device.public_key().as_ref()),"scopes":[]}),
        json!({"public_key":hex::encode(device.public_key().as_ref()),"scopes":["admin"]}),
    ] {
        let (code, _) = call(
            &app,
            "POST",
            &format!("/projects/{p}/device-sessions/challenge"),
            None,
            body,
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    // Unknown key, tampered challenge, non-member: nothing is issued.
    let forged = answer(&app, p, &unknown, &["history:read"]).await;
    assert_eq!(submit(&app, p, &forged).await.0, StatusCode::UNAUTHORIZED);
    let mut tampered = answer(&app, p, &device, &["history:read"]).await;
    tampered.challenge.scopes = vec!["history:read".into(), "history:write".into()];
    let resigned = SessionProof::sign(
        tampered.challenge.clone(),
        tampered.mac.clone(),
        tampered.challenge.authority,
        p,
        &tampered.challenge.scopes,
        &device,
    )
    .unwrap();
    assert_eq!(submit(&app, p, &resigned).await.0, StatusCode::UNAUTHORIZED);
    let mut bad_mac = answer(&app, p, &device, &["history:read"]).await;
    bad_mac.mac = "00".repeat(32);
    assert_eq!(submit(&app, p, &bad_mac).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        open(&app, p, &outsider, &["history:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    let none: i64 = sqlx::query_scalar("select count(*) from device_session")
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(none, 0);

    // Audit: one row per issuance, none per use, no credential in it.
    let first = token_for(&app, p, &device, &["history:read", "project:read"]).await;
    for _ in 0..5 {
        assert_eq!(
            status_get(&app, &format!("/projects/{p}/files"), &first).await,
            200
        );
    }
    let audit: Vec<(Uuid, Uuid, String)> = sqlx::query_as("select actor_id,resource_id,detail::text from operation_audit where action='device.session_issued'")
        .fetch_all(&runtime.pool).await.unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!((audit[0].0, audit[0].1), (owner, p));
    assert!(audit[0].2.contains(&key_id.to_string()) && audit[0].2.contains("history:read"));
    assert!(!audit[0].2.contains(&first) && !audit[0].2.contains("v4.local"));
    // The administrator audit view shows which key opened which project; a field
    // named `key` would have been redacted.
    let (code, view) = call(
        &app,
        "GET",
        "/admin/records?kind=audit&q=device.session_issued",
        Some(&login),
        Value::Null,
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{view}");
    let view = view.to_string();
    assert!(view.contains(&key_id.to_string()) && view.contains("key_id"));
    assert!(!view.contains("[redacted]"), "{view}");
    // The ledger is created and pruned, never edited: no UPDATE grant at all.
    let (insert, delete, update): (bool, bool, bool) = sqlx::query_as("select has_table_privilege(current_user,'device_session','INSERT'),has_table_privilege(current_user,'device_session','DELETE'),has_table_privilege(current_user,'device_session','UPDATE')")
        .fetch_one(&runtime.pool).await.unwrap();
    assert_eq!((insert, delete, update), (true, true, false));

    // Budgets: 30 an hour and 240 a day per key, persisted, 429 with Retry-After.
    sqlx::query("insert into device_session(id,key_id,identity_id,project_id,scopes,expires_at) select gen_random_uuid(),$1,$2,$3,array['history:read'],now()+interval '1 hour' from generate_series(1,29)")
        .bind(key_id).bind(owner).bind(p).execute(&runtime.pool).await.unwrap();
    assert_eq!(
        open(&app, p, &device, &["history:read"]).await.0,
        StatusCode::TOO_MANY_REQUESTS,
        "1 + 29 issuances in the hour"
    );
    sqlx::query("delete from device_session where key_id=$1")
        .bind(key_id)
        .execute(&runtime.pool)
        .await
        .unwrap();
    sqlx::query("insert into device_session(id,key_id,identity_id,project_id,scopes,issued_at,expires_at) select gen_random_uuid(),$1,$2,$3,array['history:read'],now()-interval '2 hours',now()-interval '1 hour' from generate_series(1,240)")
        .bind(key_id).bind(owner).bind(p).execute(&runtime.pool).await.unwrap();
    assert_eq!(
        open(&app, p, &device, &["history:read"]).await.0,
        StatusCode::TOO_MANY_REQUESTS,
        "240 issuances in the day"
    );
    // Entries older than a day are pruned by the next issuance, restoring the budget.
    // The runtime role has no UPDATE on the ledger, so backdating uses the owner pool.
    sqlx::query("update device_session set issued_at=now()-interval '2 days 2 hours',expires_at=now()-interval '2 days 1 hour' where key_id=$1")
        .bind(key_id).execute(&pool).await.unwrap();
    assert_eq!(
        open(&app, p, &device, &["history:read"]).await.0,
        StatusCode::OK
    );
    let left: i64 = sqlx::query_scalar("select count(*) from device_session where key_id=$1")
        .bind(key_id)
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(left, 1);
    // After the installation ID is cached, a challenge needs no database at all.
    runtime.pool.close().await;
    let (code, _) = challenge_for(p, &unknown).await;
    assert_eq!(code, StatusCode::OK);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn replaying_a_captured_proof_spends_no_budget_and_the_token_is_never_cacheable(
    pool: PgPool,
) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (_, login) = actor(&runtime.pool, "owner").await;
    let p = project(&app, &login, "Replay").await;
    let (device, _) = enroll(&app, &login, 7).await;
    let scopes = ["history:read", "history:write", "project:read"];
    let proof = answer(&app, p, &device, &scopes).await;
    let request = |proof: &SessionProof| {
        Request::builder()
            .method("POST")
            .uri(format!("/projects/{p}/device-sessions"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(proof).unwrap()))
            .unwrap()
    };
    let response = app.clone().oneshot(request(&proof)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let issued: Value = serde_json::from_slice(&body).unwrap();
    let token = issued["access_token"].as_str().unwrap().to_owned();
    // A captured answer replayed well past the per-minute account budget (120).
    for _ in 0..130 {
        assert_eq!(
            app.clone().oneshot(request(&proof)).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    // Neither the account's shared expensive-request budget nor issuance is spent.
    assert_eq!(
        status_get(&app, &format!("/projects/{p}/activity"), &token).await,
        200
    );
    token_for(&app, p, &device, &scopes).await;
    let rows: i64 = sqlx::query_scalar("select count(*) from device_session")
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(rows, 2);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn non_members_get_a_uniform_403_before_any_history_lookup(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let (_, outsider) = actor(&runtime.pool, "outsider").await;
    let p = project(&app, &login, "Private").await;
    let doc: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by,space_id) values('Report',$1,$2) returning id",
    )
    .bind(owner)
    .bind(p)
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    let address = "ab".repeat(32);
    let missing = Uuid::new_v4();
    let reads = [
        format!("/projects/{p}/history/commits/{address}"),
        format!("/projects/{p}/history/commits/{address}/objects/{doc}"),
        format!("/projects/{p}/history/commits/{address}/objects/{doc}/{address}"),
        format!("/projects/{p}/history/receipts?branch=main"),
        format!("/projects/{p}/history/reviews?candidate={address}"),
        format!("/projects/{missing}/history/receipts?branch=main"),
        format!("/projects/{missing}/history/commits/{address}"),
    ];
    for uri in &reads {
        assert_eq!(status_get(&app, uri, &outsider).await, 403, "{uri}");
    }
    let target = json!({"branch":"main","candidate":address,"expected":null});
    let writes = [
        (
            format!("/projects/{p}/history/commits"),
            json!({"address":address}),
        ),
        (
            format!("/projects/{p}/history/commits/{address}/acknowledge"),
            json!({"document":doc,"addresses":[address]}),
        ),
        (format!("/projects/{p}/history/refs"), target.clone()),
        (
            format!("/projects/{p}/history/reviews"),
            json!({"target":target,"decision":"approve"}),
        ),
    ];
    for (uri, body) in writes {
        let (code, _) = call(&app, "POST", &uri, Some(&outsider), body).await;
        assert_eq!(code, StatusCode::FORBIDDEN, "{uri}");
    }
    // A member still learns the honest answer.
    for uri in [
        format!("/projects/{p}/history/commits/{address}"),
        format!("/projects/{p}/history/receipts?branch=main"),
    ] {
        assert_eq!(status_get(&app, &uri, &login).await, 404, "{uri}");
    }
    runtime.close().await;
}
