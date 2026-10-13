//! Per-project device key grants, the review scope and the profile starred list,
//! through the real router and the restricted runtime login (schema 40).
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
    device_session::{SessionChallenge, SessionProof},
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
mod common;
const KEY: [u8; 32] = [9; 32];
const ALL_SCOPES: [&str; 4] = [
    "history:read",
    "history:review",
    "history:write",
    "project:read",
];

fn router(pool: PgPool) -> Router {
    let state = AppState::new(
        pool,
        AuthConfig {
            paseto_key: KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    );
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
/// Enroll through the real account flow. `mode` is the request's `project_access`
/// (`None` omits the field: the safe default).
async fn enroll(
    app: &Router,
    login: &str,
    seed: u8,
    mode: Option<&str>,
) -> (Ed25519KeyPair, Uuid, Value) {
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
    let mut body = json!(proof);
    if let Some(mode) = mode {
        body["project_access"] = json!(mode);
    }
    let (status, body) = call(app, "POST", "/devices", Some(login), body).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (key, challenge.id, body)
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
    let challenge: SessionChallenge = serde_json::from_value(body["challenge"].clone()).unwrap();
    let scopes: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
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
async fn open(
    app: &Router,
    project: Uuid,
    key: &Ed25519KeyPair,
    scopes: &[&str],
) -> (StatusCode, Value) {
    let proof = answer(app, project, key, scopes).await;
    call(
        app,
        "POST",
        &format!("/projects/{project}/device-sessions"),
        None,
        json!(proof),
    )
    .await
}
async fn token_for(app: &Router, project: Uuid, key: &Ed25519KeyPair, scopes: &[&str]) -> String {
    let (status, body) = open(app, project, key, scopes).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["access_token"].as_str().unwrap().to_owned()
}
async fn get(app: &Router, uri: &str, token: &str) -> u16 {
    call(app, "GET", uri, Some(token), Value::Null)
        .await
        .0
        .as_u16()
}
async fn grant(
    app: &Router,
    login: &str,
    key: Uuid,
    project: Uuid,
    scopes: &[&str],
) -> (StatusCode, Value) {
    call(
        app,
        "PUT",
        &format!("/devices/{key}/projects/{project}"),
        Some(login),
        json!({"scopes":scopes}),
    )
    .await
}
async fn ungrant(app: &Router, login: &str, key: Uuid, project: Uuid) -> (StatusCode, Value) {
    call(
        app,
        "DELETE",
        &format!("/devices/{key}/projects/{project}"),
        Some(login),
        Value::Null,
    )
    .await
}
async fn mode(app: &Router, login: &str, key: Uuid, body: Value) -> (StatusCode, Value) {
    call(app, "PATCH", &format!("/devices/{key}"), Some(login), body).await
}
async fn listed_key(app: &Router, login: &str, key: Uuid) -> Value {
    let (status, body) = call(app, "GET", "/devices", Some(login), Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["id"] == json!(key))
        .cloned()
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_key_reaches_nothing_until_granted_and_changes_apply_to_the_next_request(
    pool: PgPool,
) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (_owner, login) = actor(&runtime.pool, "owner").await;
    let p = project(&app, &login, "Board packs").await;
    let q = project(&app, &login, "Second project").await;
    let (granted_key, key_id, enrolled) = enroll(&app, &login, 7, None).await;
    assert_eq!(enrolled["project_access"], "granted");
    let (legacy, _, enrolled) = enroll(&app, &login, 8, Some("all")).await;
    assert_eq!(enrolled["project_access"], "all");

    // The list shows the safe default and the enumerable scope vocabulary.
    let (_, body) = call(&app, "GET", "/devices", Some(&login), Value::Null).await;
    assert_eq!(body["grant_limit"], 50);
    assert_eq!(body["scopes"], json!(ALL_SCOPES));
    let row = listed_key(&app, &login, key_id).await;
    assert_eq!(row["project_access"], "granted");
    assert_eq!(row["grants"], json!([]));

    // No grant: every scope on every project is refused with the uniform 403; the
    // legacy account-wide key is unchanged.
    for project in [p, q] {
        assert_eq!(
            open(&app, project, &granted_key, &["history:read"]).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            open(&app, project, &legacy, &ALL_SCOPES).await.0,
            StatusCode::OK
        );
    }
    let (_, uniform) = open(&app, p, &granted_key, &["history:read"]).await;
    let (_, non_member) = {
        let (_, stranger_login) = actor(&runtime.pool, "stranger").await;
        let (stranger, _, _) = enroll(&app, &stranger_login, 40, Some("all")).await;
        open(&app, p, &stranger, &["history:read"]).await
    };
    assert_eq!(
        uniform, non_member,
        "grant refusal looks like non-membership"
    );

    // Grant one project, read only.
    let (status, set) = grant(&app, &login, key_id, p, &["history:read"]).await;
    assert_eq!(status, StatusCode::OK, "{set}");
    assert_eq!(set["scopes"], json!(["history:read"]));
    assert_eq!(set["changed"], true);
    let (status, again) = grant(&app, &login, key_id, p, &["history:read"]).await;
    assert_eq!((status, &again["changed"]), (StatusCode::OK, &json!(false)));
    let row = listed_key(&app, &login, key_id).await;
    assert_eq!(row["grants"][0]["project_id"], json!(p));
    assert_eq!(row["grants"][0]["project_name"], "Board packs");
    assert_eq!(row["grants"][0]["role"], "owner");
    assert_eq!(row["grants"][0]["scopes"], json!(["history:read"]));

    // Subset only, for that project only.
    assert_eq!(
        open(&app, p, &granted_key, &["history:read", "history:write"])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        open(&app, p, &granted_key, &["project:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        open(&app, q, &granted_key, &["history:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    let read = token_for(&app, p, &granted_key, &["history:read"]).await;
    let refs = format!("/projects/{p}/history/refs");
    assert_eq!(get(&app, &refs, &read).await, 200);

    // Widening applies to the next issuance, not to the old token.
    let (_, widened) = grant(&app, &login, key_id, p, &ALL_SCOPES).await;
    assert_eq!(widened["changed"], true);
    let all = token_for(&app, p, &granted_key, &ALL_SCOPES).await;
    assert_eq!(get(&app, &format!("/projects/{p}/files"), &all).await, 200);
    assert_eq!(
        get(&app, &format!("/projects/{p}/staging/storage"), &all).await,
        200
    );

    // Narrowing and revocation reach the very next request of tokens already issued.
    let (_, narrowed) = grant(&app, &login, key_id, p, &["history:read"]).await;
    assert_eq!(narrowed["changed"], true);
    assert_eq!(get(&app, &refs, &all).await, 200, "history:read is kept");
    assert_eq!(
        get(&app, &format!("/projects/{p}/files"), &all).await,
        403,
        "project:read was narrowed away"
    );
    assert_eq!(
        get(&app, &format!("/projects/{p}/staging/storage"), &all).await,
        403
    );
    let (status, revoked) = ungrant(&app, &login, key_id, p).await;
    assert_eq!(
        (status, &revoked["changed"]),
        (StatusCode::OK, &json!(true))
    );
    assert_eq!(get(&app, &refs, &read).await, 403);
    assert_eq!(get(&app, &refs, &all).await, 403);
    assert_eq!(
        open(&app, p, &granted_key, &["history:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, repeat) = ungrant(&app, &login, key_id, p).await;
    assert_eq!(
        (status, &repeat["changed"]),
        (StatusCode::OK, &json!(false))
    );
    // The legacy key never needed any of this.
    assert_eq!(
        open(&app, p, &legacy, &["history:read"]).await.0,
        StatusCode::OK
    );

    // History is kept (rows revoked, not edited) and audited with `key_id`.
    let rows: Vec<(Vec<String>, bool)> = sqlx::query_as("select scopes,revoked_at is not null from device_key_project_grant where key_id=$1 order by created_at,id")
        .bind(key_id).fetch_all(&runtime.pool).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|(_, revoked)| *revoked));
    assert_eq!(rows[0].0, ["history:read"]);
    let audit: Vec<(String, Value)> = sqlx::query_as("select action,detail from operation_audit where action like 'device.grant.%' and resource_id=$1 order by id")
        .bind(key_id).fetch_all(&runtime.pool).await.unwrap();
    let actions: Vec<_> = audit.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        actions,
        [
            "device.grant.set",
            "device.grant.set",
            "device.grant.set",
            "device.grant.revoked"
        ]
    );
    assert_eq!(audit[0].1["key_id"], json!(key_id));
    assert_eq!(audit[0].1["previous_scopes"], Value::Null);
    assert_eq!(audit[1].1["previous_scopes"], json!(["history:read"]));
    assert_eq!(audit[3].1["scopes"], json!(["history:read"]));
    let enrolled_audit: Value = sqlx::query_scalar(
        "select detail from operation_audit where action='device.enrolled' and resource_id=$1",
    )
    .bind(key_id)
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    assert_eq!(enrolled_audit["project_access"], "granted");
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_decisions_need_their_own_scope_and_role_rank(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (_owner, login) = actor(&runtime.pool, "owner").await;
    let (reviewer, reviewer_login) = actor(&runtime.pool, "reviewer").await;
    let (contributor, contributor_login) = actor(&runtime.pool, "contributor").await;
    let p = project(&app, &login, "Reviews").await;
    member(&runtime.pool, p, reviewer, "approver").await;
    member(&runtime.pool, p, contributor, "reviewer").await;
    let (reviewer_key, reviewer_key_id, _) = enroll(&app, &reviewer_login, 7, None).await;
    let (contributor_key, contributor_key_id, _) = enroll(&app, &contributor_login, 8, None).await;
    for (login, key_id) in [
        (&reviewer_login, reviewer_key_id),
        (&contributor_login, contributor_key_id),
    ] {
        let (status, body) = grant(&app, login, key_id, p, &ALL_SCOPES).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let address = "ab".repeat(32);
    let review = json!({"target":{"branch":"main","candidate":address,"expected":null},"decision":"approve"});
    let uri = format!("/projects/{p}/history/reviews");

    // Write is not review: push-side routes work with write alone, the review route
    // does not, whatever else the token carries.
    let write = token_for(&app, p, &reviewer_key, &["history:read", "history:write"]).await;
    assert_eq!(
        call(&app, "POST", &uri, Some(&write), review.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let staging =
        json!({"document":Uuid::new_v4(),"objects":[{"address":"aa".repeat(32),"bytes":"eA=="}]});
    let (code, _) = call(
        &app,
        "POST",
        &format!("/projects/{p}/staging/objects"),
        Some(&write),
        staging.clone(),
    )
    .await;
    assert_ne!(code, StatusCode::FORBIDDEN, "push needs only history:write");
    assert_ne!(code, StatusCode::UNAUTHORIZED);

    // Review is not write either.
    let review_only = token_for(&app, p, &reviewer_key, &["history:review"]).await;
    let (code, body) = call(&app, "POST", &uri, Some(&review_only), review.clone()).await;
    assert!(
        !matches!(code, StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED),
        "{code} {body}: the extractor accepted history:review (the candidate is unknown)"
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/projects/{p}/staging/objects"),
            Some(&review_only),
            staging
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    // The browser session is unchanged: not an unauthorized, not a scope question.
    let (code, _) = call(&app, "POST", &uri, Some(&reviewer_login), review.clone()).await;
    assert!(!matches!(
        code,
        StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED
    ));

    // A contributor cannot be issued the review scope even with the grant; the
    // reviewer rank (approver) and above can.
    assert_eq!(
        open(&app, p, &contributor_key, &["history:review"]).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        open(&app, p, &contributor_key, &["history:write"]).await.0,
        StatusCode::OK
    );
    // Revoking the review scope from the grant ends review at once, keeps write.
    let (_, body) = grant(
        &app,
        &reviewer_login,
        reviewer_key_id,
        p,
        &["history:read", "history:write"],
    )
    .await;
    assert_eq!(body["changed"], true);
    assert_eq!(
        call(&app, "POST", &uri, Some(&review_only), review.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        open(&app, p, &reviewer_key, &["history:review"]).await.0,
        StatusCode::FORBIDDEN
    );
    // Legacy `all` keys can be issued review as before (rank still required).
    let (legacy, _, _) = enroll(&app, &login, 9, Some("all")).await;
    assert_eq!(
        open(&app, p, &legacy, &["history:review"]).await.0,
        StatusCode::OK
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn switching_modes_keeps_grants_and_all_needs_confirmation(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (_owner, login) = actor(&runtime.pool, "owner").await;
    let p = project(&app, &login, "Modes").await;
    let (device, key_id, _) = enroll(&app, &login, 7, Some("all")).await;
    let files = format!("/projects/{p}/files");
    let all_token = token_for(&app, p, &device, &["project:read"]).await;
    assert_eq!(get(&app, &files, &all_token).await, 200);

    // `all` -> `granted` needs no confirmation, applies to the existing token.
    let (status, body) = mode(&app, &login, key_id, json!({"project_access":"granted"})).await;
    assert_eq!((status, &body["changed"]), (StatusCode::OK, &json!(true)));
    assert_eq!(get(&app, &files, &all_token).await, 403);
    assert_eq!(
        open(&app, p, &device, &["project:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    let (_, body) = grant(&app, &login, key_id, p, &["project:read"]).await;
    assert_eq!(body["changed"], true);
    assert_eq!(get(&app, &files, &all_token).await, 200);

    // Back to `all` needs the explicit confirmation field; grants are not deleted.
    for refused in [
        json!({"project_access":"all"}),
        json!({"project_access":"all","confirm_all_projects":false}),
        json!({"project_access":"everything"}),
        json!({"project_access":"all","confirm_all_projects":true,"extra":1}),
    ] {
        let (status, body) = mode(&app, &login, key_id, refused).await;
        assert!(
            matches!(
                status,
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "{status} {body}"
        );
    }
    let still = listed_key(&app, &login, key_id).await;
    assert_eq!(still["project_access"], "granted");
    let (status, body) = mode(
        &app,
        &login,
        key_id,
        json!({"project_access":"all","confirm_all_projects":true}),
    )
    .await;
    assert_eq!((status, &body["changed"]), (StatusCode::OK, &json!(true)));
    let row = listed_key(&app, &login, key_id).await;
    assert_eq!(row["project_access"], "all");
    assert_eq!(row["grants"].as_array().unwrap().len(), 1);
    let (_, again) = mode(
        &app,
        &login,
        key_id,
        json!({"project_access":"all","confirm_all_projects":true}),
    )
    .await;
    assert_eq!(again["changed"], false);
    assert_eq!(
        open(&app, p, &device, &["history:read", "history:review"])
            .await
            .0,
        StatusCode::OK
    );
    let (_, back) = mode(&app, &login, key_id, json!({"project_access":"granted"})).await;
    assert_eq!(back["changed"], true);
    let changes: Vec<Value> = sqlx::query_scalar("select detail from operation_audit where action='device.access.changed' and resource_id=$1 order by id")
        .bind(key_id).fetch_all(&runtime.pool).await.unwrap();
    assert_eq!(changes.len(), 3, "no row for a no-op");
    assert_eq!(
        (&changes[0]["from"], &changes[0]["to"]),
        (&json!("all"), &json!("granted"))
    );
    assert_eq!(changes[0]["key_id"], json!(key_id));
    assert_eq!(
        (&changes[1]["from"], &changes[1]["to"]),
        (&json!("granted"), &json!("all"))
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn management_is_a_browser_session_and_the_keys_owner_only(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let (_other, other_login) = actor(&runtime.pool, "other").await;
    let p = project(&app, &login, "Own keys").await;
    let q = project(&app, &other_login, "Someone else's").await;
    let (device, key_id, _) = enroll(&app, &login, 7, None).await;
    let (_, other_key_id, _) = enroll(&app, &other_login, 8, None).await;
    let (_, body) = grant(&app, &login, key_id, p, &["history:read", "project:read"]).await;
    assert_eq!(body["changed"], true);
    let token = token_for(&app, p, &device, &["history:read"]).await;

    // A device token is not an account login on any management route.
    let uri = format!("/devices/{key_id}/projects/{p}");
    for (method, path, body) in [
        ("GET", "/devices".to_owned(), Value::Null),
        ("PUT", uri.clone(), json!({"scopes":["history:read"]})),
        ("DELETE", uri.clone(), Value::Null),
        (
            "PATCH",
            format!("/devices/{key_id}"),
            json!({"project_access":"granted"}),
        ),
    ] {
        assert_eq!(
            call(&app, method, &path, Some(&token), body.clone())
                .await
                .0,
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
        assert_eq!(
            call(&app, method, &path, None, body).await.0,
            StatusCode::UNAUTHORIZED,
            "{method} {path} without a login"
        );
    }
    // A key can never widen itself: the token still holds only what was granted.
    assert_eq!(
        get(&app, &format!("/projects/{p}/files"), &token).await,
        403
    );

    // Another account sees 404 for the key, whatever it asks, and changes nothing.
    for (method, path, body) in [
        ("PUT", uri.clone(), json!({"scopes":["history:write"]})),
        ("DELETE", uri.clone(), Value::Null),
        (
            "PATCH",
            format!("/devices/{key_id}"),
            json!({"project_access":"all","confirm_all_projects":true}),
        ),
    ] {
        assert_eq!(
            call(&app, method, &path, Some(&other_login), body).await.0,
            StatusCode::NOT_FOUND,
            "{method} {path}"
        );
    }
    let row = listed_key(&app, &login, key_id).await;
    assert_eq!(row["project_access"], "granted");
    assert_eq!(
        row["grants"][0]["scopes"],
        json!(["history:read", "project:read"])
    );
    assert_eq!(
        get(&app, &format!("/projects/{p}/history/refs"), &token).await,
        200
    );
    // The owner cannot grant another account's key, nor a non-member project.
    assert_eq!(
        grant(&app, &login, other_key_id, p, &["history:read"])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        grant(&app, &login, key_id, q, &["history:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        grant(&app, &login, key_id, Uuid::new_v4(), &["history:read"])
            .await
            .0,
        StatusCode::FORBIDDEN,
        "an unknown project looks like a project you are not in"
    );
    // Bad scope sets.
    for scopes in [json!([]), json!(["admin"]), json!(["history:read", "root"])] {
        let (status, _) = call(&app, "PUT", &uri, Some(&login), json!({"scopes":scopes})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    // A revoked key cannot be granted or switched, but its grant can be cleared.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/devices/{key_id}/revoke"),
        Some(&login),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        grant(&app, &login, key_id, p, &["history:read"]).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        mode(&app, &login, key_id, json!({"project_access":"granted"}))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(listed_key(&app, &login, key_id).await["grants"], json!([]));
    let (status, cleared) = ungrant(&app, &login, key_id, p).await;
    assert_eq!(
        (status, &cleared["changed"]),
        (StatusCode::OK, &json!(true))
    );
    let actor_ids: Vec<Uuid> = sqlx::query_scalar("select distinct actor_id from operation_audit where action like 'device.grant.%' or action='device.access.changed'")
        .fetch_all(&runtime.pool).await.unwrap();
    assert_eq!(actor_ids, [owner]);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_key_holds_at_most_fifty_active_grants(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (_owner, login) = actor(&runtime.pool, "owner").await;
    let (_, key_id, _) = enroll(&app, &login, 7, None).await;
    let mut projects = Vec::new();
    for n in 0..51 {
        projects.push(project(&app, &login, &format!("Project {n}")).await);
    }
    for p in &projects[..50] {
        let (status, body) = grant(&app, &login, key_id, *p, &["history:read"]).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (status, body) = grant(&app, &login, key_id, projects[50], &["history:read"]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    // Changing an existing grant at the limit is allowed; so is replacing one.
    let (status, _) = grant(&app, &login, key_id, projects[0], &["project:read"]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        ungrant(&app, &login, key_id, projects[1]).await.0,
        StatusCode::OK
    );
    let (status, _) = grant(&app, &login, key_id, projects[50], &["history:read"]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        listed_key(&app, &login, key_id).await["grants"]
            .as_array()
            .unwrap()
            .len(),
        50
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn membership_loss_and_concurrent_changes_never_leave_a_usable_session(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (_owner, login) = actor(&runtime.pool, "owner").await;
    let (editor, editor_login) = actor(&runtime.pool, "editor").await;
    let p = project(&app, &login, "Membership").await;
    member(&runtime.pool, p, editor, "editor").await;
    let (device, key_id, _) = enroll(&app, &editor_login, 7, None).await;
    let (status, _) = grant(
        &app,
        &editor_login,
        key_id,
        p,
        &["history:read", "project:read"],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = token_for(&app, p, &device, &["history:read", "project:read"]).await;
    let files = format!("/projects/{p}/files");
    assert_eq!(get(&app, &files, &token).await, 200);

    // Removal from the project: the grant is inert; the key cannot even see its name.
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(p)
        .bind(editor)
        .execute(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(get(&app, &files, &token).await, 403);
    assert_eq!(
        open(&app, p, &device, &["history:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    let row = listed_key(&app, &editor_login, key_id).await;
    assert_eq!(row["grants"][0]["project_id"], json!(p));
    assert_eq!(row["grants"][0]["project_name"], Value::Null);
    assert_eq!(row["grants"][0]["role"], Value::Null);
    // A non-member cannot add or change a grant, but may remove one.
    assert_eq!(
        grant(&app, &editor_login, key_id, p, &["history:read"])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // Re-adding the account makes the same grant effective again: nothing was lost.
    member(&runtime.pool, p, editor, "editor").await;
    assert_eq!(get(&app, &files, &token).await, 200);
    let row = listed_key(&app, &editor_login, key_id).await;
    assert_eq!(row["grants"][0]["project_name"], "Membership");

    // Issuance racing a revocation: whichever order, afterwards nothing works.
    for round in 0..6 {
        let (status, _) = grant(&app, &editor_login, key_id, p, &["history:read"]).await;
        assert_eq!(status, StatusCode::OK, "round {round}");
        let proof = answer(&app, p, &device, &["history:read"]).await;
        let sessions = format!("/projects/{p}/device-sessions");
        let issue = call(&app, "POST", &sessions, None, json!(proof));
        let revoke = ungrant(&app, &editor_login, key_id, p);
        let ((issued, issued_body), (revoked, _)) = tokio::join!(issue, revoke);
        assert_eq!(revoked, StatusCode::OK);
        assert!(
            matches!(issued, StatusCode::OK | StatusCode::FORBIDDEN),
            "round {round}: {issued} {issued_body}"
        );
        if issued == StatusCode::OK {
            let late = issued_body["access_token"].as_str().unwrap();
            assert_eq!(
                get(&app, &format!("/projects/{p}/history/refs"), late).await,
                403,
                "round {round}: a token issued while the grant was revoked is dead"
            );
        }
        assert_eq!(
            open(&app, p, &device, &["history:read"]).await.0,
            StatusCode::FORBIDDEN,
            "round {round}"
        );
    }
    // Grants and revocations racing each other leave exactly one coherent state.
    let (a, b) = tokio::join!(
        grant(&app, &editor_login, key_id, p, &["history:read"]),
        grant(&app, &editor_login, key_id, p, &["history:write"]),
    );
    assert_eq!((a.0, b.0), (StatusCode::OK, StatusCode::OK));
    let active: i64 = sqlx::query_scalar("select count(*) from device_key_project_grant where key_id=$1 and project_id=$2 and revoked_at is null")
        .bind(key_id).bind(p).fetch_one(&runtime.pool).await.unwrap();
    assert_eq!(active, 1);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn forged_tokens_cannot_borrow_a_grant_and_enrollment_retries_keep_the_mode(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let p = project(&app, &login, "Forged").await;
    let (_, key_id, _) = enroll(&app, &login, 7, None).await;
    // A service-key holder can mint any claims; the database still has the last
    // word, so a token naming a key with no grant gets nothing.
    let generation: i64 = sqlx::query_scalar("select session_generation from identity where id=$1")
        .bind(owner)
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
    let forged = token::mint_project_device(
        &KEY,
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
    assert_eq!(
        get(&app, &format!("/projects/{p}/files"), &forged).await,
        403
    );
    grant(&app, &login, key_id, p, &["project:read"]).await;
    assert_eq!(
        get(&app, &format!("/projects/{p}/files"), &forged).await,
        200
    );

    // An unknown mode is refused; an exact retry keeps the stored mode.
    let key = key(55);
    let (_, challenge) = call(
        &app,
        "POST",
        "/devices/challenge",
        Some(&login),
        json!({"public_key":hex::encode(key.public_key().as_ref())}),
    )
    .await;
    let challenge: Challenge = serde_json::from_value(challenge).unwrap();
    let proof = Proof::sign(challenge.clone(), challenge.authority, owner, &key).unwrap();
    let mut bad = json!(proof);
    bad["project_access"] = json!("everything");
    assert_eq!(
        call(&app, "POST", "/devices", Some(&login), bad).await.0,
        StatusCode::BAD_REQUEST
    );
    let (status, first) = call(&app, "POST", "/devices", Some(&login), json!(proof)).await;
    assert_eq!(
        (status, &first["project_access"]),
        (StatusCode::OK, &json!("granted"))
    );
    let mut retry = json!(proof);
    retry["project_access"] = json!("all");
    let (status, second) = call(&app, "POST", "/devices", Some(&login), retry).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["project_access"], "granted", "a retry never widens");
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn grant_rows_only_ever_move_revoked_at_and_the_runtime_cannot_delete_them(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (owner, login) = actor(&runtime.pool, "owner").await;
    let p = project(&app, &login, "Rows").await;
    let (_, key_id, _) = enroll(&app, &login, 7, None).await;
    grant(&app, &login, key_id, p, &["history:read"]).await;
    let id: Uuid = sqlx::query_scalar("select id from device_key_project_grant where key_id=$1")
        .bind(key_id)
        .fetch_one(&runtime.pool)
        .await
        .unwrap();

    for (table, update, delete) in [
        ("device_key_project_grant", true, false),
        ("device_key_access", true, false),
    ] {
        let (can_update, can_delete, can_truncate): (bool, bool, bool) = sqlx::query_as(
            "select has_table_privilege(current_user,$1,'UPDATE'),has_table_privilege(current_user,$1,'DELETE'),has_table_privilege(current_user,$1,'TRUNCATE')",
        )
        .bind(table)
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
        assert_eq!(
            (can_update, can_delete, can_truncate),
            (update, delete, false),
            "{table}"
        );
    }
    let denied = sqlx::query("delete from device_key_project_grant where id=$1")
        .bind(id)
        .execute(&runtime.pool)
        .await
        .unwrap_err();
    assert_eq!(
        denied.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501")
    );
    // The trigger refuses every edit except the one-time revocation, even for the
    // owner-level role used to build fixtures.
    for statement in [
        "update device_key_project_grant set scopes=array['history:write'] where id=$1",
        "update device_key_project_grant set project_id=gen_random_uuid() where id=$1",
        "update device_key_project_grant set created_by=key_id where id=$1",
        "update device_key_project_grant set created_at=created_at-interval '1 day' where id=$1",
        "update device_key_project_grant set revoked_at=null where id=$1",
    ] {
        for pool in [&runtime.pool, &runtime.owner] {
            let result = sqlx::query(statement).bind(id).execute(pool).await;
            assert!(result.is_err(), "{statement}");
        }
    }
    sqlx::query("update device_key_project_grant set revoked_at=now() where id=$1")
        .bind(id)
        .execute(&runtime.pool)
        .await
        .unwrap();
    for statement in [
        "update device_key_project_grant set revoked_at=null where id=$1",
        "update device_key_project_grant set revoked_at=now()+interval '1 day' where id=$1",
    ] {
        assert!(
            sqlx::query(statement)
                .bind(id)
                .execute(&runtime.pool)
                .await
                .is_err(),
            "{statement}: a revoked grant is final"
        );
    }
    // One active grant per (key, project); scopes stay a known non-empty set.
    grant(&app, &login, key_id, p, &["history:read"]).await;
    let second = sqlx::query("insert into device_key_project_grant(key_id,project_id,scopes,created_by) select key_id,project_id,scopes,created_by from device_key_project_grant where revoked_at is null")
        .execute(&runtime.pool).await;
    assert!(second.is_err());
    let unknown = sqlx::query("insert into device_key_project_grant(key_id,project_id,scopes,created_by) values($1,$2,array['admin'],$3)")
        .bind(key_id).bind(p).bind(owner).execute(&runtime.pool).await;
    assert!(unknown.is_err());
    let moved =
        sqlx::query("update device_key_access set key_id=gen_random_uuid() where key_id=$1")
            .bind(key_id)
            .execute(&runtime.pool)
            .await;
    assert!(moved.is_err());
    // The key binding itself is still immutable.
    assert!(
        sqlx::query("update device_signing_key set identity_id=identity_id where id=$1")
            .bind(key_id)
            .execute(&runtime.owner)
            .await
            .is_err()
    );
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn keys_that_existed_before_schema_40_keep_account_wide_access(pool: PgPool) {
    let before = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            engine_core::database::MIGRATOR
                .iter()
                .filter(|m| m.version <= 39)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    before.run(&pool).await.unwrap();
    let identity = store::upsert_identity(&pool, "legacy@example.test", None)
        .await
        .unwrap()
        .id
        .0;
    let legacy = key(31);
    let legacy_id = Uuid::new_v4();
    sqlx::query("insert into device_signing_key(id,identity_id,public_key,proof) values($1,$2,$3,'{}'::jsonb)")
        .bind(legacy_id)
        .bind(identity)
        .bind(hex::encode(legacy.public_key().as_ref()))
        .execute(&pool)
        .await
        .unwrap();
    engine_core::database::MIGRATOR.run(&pool).await.unwrap();
    let mode: String =
        sqlx::query_scalar("select project_access from device_key_access where key_id=$1")
            .bind(legacy_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(mode, "all");
    // Through the API: the legacy key is listed `all` and opens a project it has
    // no grant for; a key with no access row at all would be `granted`.
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let login = token::mint(&KEY, identity, 3600).unwrap();
    let p = project(&app, &login, "Legacy").await;
    assert_eq!(
        listed_key(&app, &login, legacy_id).await["project_access"],
        "all"
    );
    assert_eq!(
        open(&app, p, &legacy, &["history:read", "history:write"])
            .await
            .0,
        StatusCode::OK
    );
    sqlx::query("delete from device_key_access where key_id=$1")
        .bind(legacy_id)
        .execute(&runtime.owner)
        .await
        .unwrap();
    assert_eq!(
        listed_key(&app, &login, legacy_id).await["project_access"],
        "granted"
    );
    assert_eq!(
        open(&app, p, &legacy, &["history:read"]).await.0,
        StatusCode::FORBIDDEN,
        "unknown keys fail closed"
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn starred_projects_show_only_what_the_viewer_can_also_access(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let app = router(runtime.pool.clone());
    let (maya, maya_login) = actor(&runtime.pool, "maya").await;
    let (sam, sam_login) = actor(&runtime.pool, "sam").await;
    let (_, outsider_login) = actor(&runtime.pool, "outsider").await;
    let shared = project(&app, &maya_login, "Shared plan").await;
    let private = project(&app, &maya_login, "Maya only").await;
    let unstarred = project(&app, &maya_login, "Not starred").await;
    member(&runtime.pool, shared, sam, "viewer").await;
    member(&runtime.pool, unstarred, sam, "viewer").await;
    for project in [shared, private] {
        let (status, _) = call(
            &app,
            "PUT",
            &format!("/projects/{project}/star"),
            Some(&maya_login),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, _) = call(
        &app,
        "PUT",
        &format!("/projects/{shared}/star"),
        Some(&sam_login),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, issue) = call(
        &app,
        "POST",
        &format!("/projects/{shared}/issues"),
        Some(&maya_login),
        json!({"title":"Check the totals","body":""}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issue}");

    // Own profile: everything starred. Another member: only the shared project.
    let (status, own) = call(
        &app,
        "GET",
        &format!("/profiles/{maya}/starred"),
        Some(&maya_login),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{own}");
    let ids: Vec<_> = own["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].clone())
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&json!(shared)) && ids.contains(&json!(private)));
    assert_eq!(own["truncated"], false);
    let (status, seen) = call(
        &app,
        "GET",
        &format!("/profiles/{maya}/starred"),
        Some(&sam_login),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{seen}");
    assert_eq!(seen["items"].as_array().unwrap().len(), 1);
    let item = &seen["items"][0];
    assert_eq!(item["id"], json!(shared));
    assert_eq!(item["name"], "Shared plan");
    assert_eq!(item["star_count"], 2);
    assert_eq!(item["open_issue_count"], 1);
    assert!(!seen.to_string().contains("Maya only"));
    // The project page and the list agree on the count.
    let (_, detail) = call(
        &app,
        "GET",
        &format!("/projects/{shared}"),
        Some(&sam_login),
        Value::Null,
    )
    .await;
    assert_eq!(detail["star_count"], item["star_count"]);

    // An account that shares no project with the profile gets the same 404 as
    // the profile itself; so does an unknown account.
    for target in [maya, Uuid::new_v4()] {
        let (status, _) = call(
            &app,
            "GET",
            &format!("/profiles/{target}/starred"),
            Some(&outsider_login),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    // Sam leaves the shared project but still shares another one with Maya: the
    // profile stays visible and lists nothing Sam can no longer open. Sam's star
    // stops counting for the project once Sam is not a member.
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(shared)
        .bind(sam)
        .execute(&runtime.pool)
        .await
        .unwrap();
    let (status, left) = call(
        &app,
        "GET",
        &format!("/profiles/{maya}/starred"),
        Some(&sam_login),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{left}");
    assert_eq!(left["items"], json!([]));
    let (_, own) = call(
        &app,
        "GET",
        &format!("/profiles/{maya}/starred"),
        Some(&maya_login),
        Value::Null,
    )
    .await;
    let counted = own["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == json!(shared))
        .unwrap();
    assert_eq!(counted["star_count"], 1);
    // With no project in common the profile is not visible at all.
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(unstarred)
        .bind(sam)
        .execute(&runtime.pool)
        .await
        .unwrap();
    let (status, _) = call(
        &app,
        "GET",
        &format!("/profiles/{maya}/starred"),
        Some(&sam_login),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The list is bounded at 100.
    let mut ids = Vec::new();
    for n in 0..101 {
        ids.push(project(&app, &maya_login, &format!("Starred {n}")).await);
    }
    sqlx::query("insert into project_star(project_id,identity_id) select unnest($1::uuid[]),$2")
        .bind(&ids)
        .bind(maya)
        .execute(&runtime.pool)
        .await
        .unwrap();
    let (status, many) = call(
        &app,
        "GET",
        &format!("/profiles/{maya}/starred"),
        Some(&maya_login),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(many["items"].as_array().unwrap().len(), 100);
    assert_eq!(many["truncated"], true);
    runtime.close().await;
}
