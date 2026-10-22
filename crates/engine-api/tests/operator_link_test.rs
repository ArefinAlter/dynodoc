//! Operator-issued sign-in links (docs/decisions/018-operator-issued-sign-in-links.md), against a
//! real PostgreSQL and the real router. Every test is a fresh migrated database (`#[sqlx::test]`).
//! The connection the tests use is the owner (what `engine-cli auth issue-link` uses); the
//! runtime-login tests use the restricted login production runs the API with.
mod common;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use common::RuntimeDatabase;
use engine_api::auth::operator::{
    self, Expect, IssueError, IssueRequest, Issued, NeedsConfirmation, RevokeError,
};
use engine_api::auth::AuthConfig;
use engine_api::{app, AppState};
use futures::future::join_all;
use ring::digest::{digest, SHA256};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use uuid::Uuid;

const SERVICE_KEY: &str = "isolated-operator-link-test-service-key-32";
/// The installation's `ADMIN_EMAILS` in these tests.
const ADMINS: &str = "admin@example.test, second-admin@example.test";

fn state(pool: PgPool) -> AppState {
    let mut state = AppState::new(
        pool,
        AuthConfig {
            paseto_key: [7u8; 32],
            service_key: Some(SERVICE_KEY.into()),
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    );
    state.admin_emails = ADMINS.into();
    state
}

fn router(pool: &PgPool) -> Router {
    app(state(pool.clone()))
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

fn post(uri: &str, body: &Value, service: bool) -> Request<Body> {
    let mut req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    if service {
        req = req.header("x-dynodoc-service-key", SERVICE_KEY);
    }
    req.body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

fn bearer_get(uri: &str, access: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap()
}

fn bearer_post(uri: &str, access: &str, body: &Value, service: bool) -> Request<Body> {
    let mut req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {access}"));
    if service {
        req = req.header("x-dynodoc-service-key", SERVICE_KEY);
    }
    req.body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

/// What the web server forwards when a person presses Continue on `/login?token=`.
async fn verify(router: &Router, token: &str) -> (StatusCode, Value) {
    send(
        router,
        post("/auth/verify", &json!({"token": token}), false),
    )
    .await
}

async fn me(router: &Router, access: &str) -> Uuid {
    let (status, body) = send(router, bearer_get("/auth/me", access)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["identity_id"].as_str().unwrap().parse().unwrap()
}

async fn account(pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("insert into identity(email) values($1) returning id")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn request(email: &str) -> IssueRequest<'_> {
    IssueRequest {
        email,
        ttl_seconds: operator::DEFAULT_TTL_SECONDS,
        create: false,
        confirm_create: None,
        expect: None,
        operator: "tester",
        reason: "test",
        admin_emails: ADMINS,
    }
}

async fn issue(pool: &PgPool, email: &str) -> Issued {
    operator::issue(pool, &request(email)).await.unwrap()
}

fn sha(text: &str) -> Vec<u8> {
    digest(&SHA256, text.as_bytes()).as_ref().to_vec()
}

async fn audit(pool: &PgPool) -> Vec<(String, Uuid, Value)> {
    sqlx::query_as(
        "select action, actor_id, detail from operation_audit where action like 'auth.operator_link.%' order by id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn actions(pool: &PgPool) -> Vec<String> {
    audit(pool).await.into_iter().map(|(a, _, _)| a).collect()
}

async fn outcome(pool: &PgPool, token: &str) -> Option<String> {
    sqlx::query_scalar("select outcome from operator_sign_in_link where token_hash=$1")
        .bind(sha(token))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("select count(*) from {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A link row that has been waiting for `age` (a Postgres interval), valid for `life`. The table
/// refuses to edit a row's times, so tests that need the past make the row in the past. Returns the token.
async fn link_in_the_past(pool: &PgPool, identity: Uuid, age: &str, life: &str) -> String {
    let (token, hash) = engine_api::auth::random_token().unwrap();
    sqlx::query(&format!(
        "insert into operator_sign_in_link(identity_id,token_hash,session_generation,operator,created_at,expires_at)
         values($1,$2,0,'tester', now() - interval '{age}', now() - interval '{age}' + interval '{life}')"
    ))
    .bind(identity)
    .bind(hash)
    .execute(pool)
    .await
    .unwrap();
    token
}

// --- the flow ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn a_link_signs_the_account_in_once_and_a_replay_is_refused(pool: PgPool) {
    let router = router(&pool);
    let id = account(&pool, "olivia@example.test").await;
    let issued = issue(&pool, "olivia@example.test").await;
    assert_eq!(issued.identity_id, id);
    assert!(!issued.created_account && !issued.administrator && issued.superseded == 0);
    let token = issued.token.expose().to_string();
    assert_eq!(token.len(), 64);

    let (status, tokens) = verify(&router, &token).await;
    assert_eq!(status, StatusCode::OK, "{tokens}");
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(
        me(&router, tokens["access_token"].as_str().unwrap()).await,
        id
    );
    assert_eq!(outcome(&pool, &token).await.as_deref(), Some("used"));

    // The same token again: refused, with the answer an unknown token gets.
    let (replay, replay_body) = verify(&router, &token).await;
    let (unknown, unknown_body) = verify(&router, &"0".repeat(64)).await;
    assert_eq!(replay, StatusCode::UNAUTHORIZED);
    assert_eq!((replay, &replay_body), (unknown, &unknown_body));
    // Used up once: one login, whatever happened afterwards.
    let logins: i64 = sqlx::query_scalar(
        "select count(distinct session_id) from refresh_token where identity_id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(logins, 1);
    assert_eq!(
        actions(&pool).await,
        [
            "auth.operator_link.issued",
            "auth.operator_link.used",
            "auth.operator_link.refused"
        ]
    );
    let rows = audit(&pool).await;
    assert!(rows.iter().all(|(_, actor, _)| *actor == id));
    assert_eq!(rows[0].2["operator"], "tester");
    assert_eq!(rows[0].2["reason"], "test");
    assert_eq!(rows[2].2["reason"], "used");
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_token_itself_signs_in_not_its_hash_or_a_variant(pool: PgPool) {
    let router = router(&pool);
    account(&pool, "variant@example.test").await;
    let issued = issue(&pool, "variant@example.test").await;
    let token = issued.token.expose().to_string();
    let stored: Vec<u8> = sqlx::query_scalar("select token_hash from operator_sign_in_link")
        .fetch_one(&pool)
        .await
        .unwrap();
    // What is stored is the SHA-256 of the token and nothing else about it.
    assert_eq!(stored, sha(&token));
    assert_eq!(issued.token_prefix, hex::encode(&stored[..4]));
    let variants = [
        hex::encode(&stored),
        token.to_uppercase(),
        format!("{token} "),
        format!("{token}\n"),
        format!(" {token}"),
        token[..63].to_string(),
        format!("{token}0"),
        String::new(),
        "null".into(),
    ];
    for variant in variants {
        assert_eq!(
            verify(&router, &variant).await.0,
            StatusCode::UNAUTHORIZED,
            "{variant:?}"
        );
    }
    // None of that used the link up.
    assert_eq!(outcome(&pool, &token).await, None);
    assert_eq!(verify(&router, &token).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_expired_link_is_refused_by_the_query_and_by_the_table(pool: PgPool) {
    let router = router(&pool);
    let id = account(&pool, "late@example.test").await;
    let token = link_in_the_past(&pool, id, "2 hours", "15 minutes").await;
    assert_eq!(verify(&router, &token).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        outcome(&pool, &token).await,
        None,
        "refusing does not touch the row"
    );
    let rows = audit(&pool).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].0.as_str(), &rows[0].2["reason"]),
        ("auth.operator_link.refused", &json!("expired"))
    );
    // Even a statement that forgets the time check cannot use an expired link up.
    let error = sqlx::query("update operator_sign_in_link set consumed_at=now(), outcome='used'")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("can only be ended, once"),
        "{error}"
    );
    // A link made now, for the same account, is not affected by the old one.
    let live = issue(&pool, "late@example.test").await;
    assert_eq!(verify(&router, live.token.expose()).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_lifetime_is_bounded_in_the_tool_and_in_the_table(pool: PgPool) {
    account(&pool, "ttl@example.test").await;
    for bad in [
        -1,
        0,
        operator::MIN_TTL_SECONDS - 1,
        operator::MAX_TTL_SECONDS + 1,
        i64::MAX,
    ] {
        let mut r = request("ttl@example.test");
        r.ttl_seconds = bad;
        assert!(
            matches!(
                operator::issue(&pool, &r).await,
                Err(IssueError::Invalid(_))
            ),
            "{bad}"
        );
    }
    assert_eq!(count(&pool, "operator_sign_in_link").await, 0);
    for good in [
        operator::MIN_TTL_SECONDS,
        operator::DEFAULT_TTL_SECONDS,
        operator::MAX_TTL_SECONDS,
    ] {
        let mut r = request("ttl@example.test");
        r.ttl_seconds = good;
        let issued = operator::issue(&pool, &r).await.unwrap();
        let life: f64 = sqlx::query_scalar(
            "select extract(epoch from expires_at - created_at)::float8 from operator_sign_in_link where token_hash=$1",
        )
        .bind(sha(issued.token.expose()))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(life as i64, good);
    }
    // The table refuses a longer life however the row got there.
    let id: Uuid = sqlx::query_scalar("select id from identity where email='ttl@example.test'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let error = sqlx::query("insert into operator_sign_in_link(identity_id,token_hash,session_generation,operator,expires_at) values($1,$2,0,'x',now()+interval '25 hours')")
        .bind(id)
        .bind(sha("long"))
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("operator_sign_in_link_check"),
        "{error}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_link_belongs_to_one_account_and_the_normalised_address(pool: PgPool) {
    let router = router(&pool);
    let a = account(&pool, "ann@example.test").await;
    let b = account(&pool, "bob@example.test").await;
    // The address is trimmed and lower-cased: it finds Ann, it does not make a second Ann.
    let for_ann = issue(&pool, "  ANN@Example.TEST ").await;
    assert_eq!(for_ann.identity_id, a);
    assert_eq!(for_ann.email, "ann@example.test");
    let for_bob = issue(&pool, "bob@example.test").await;
    assert_eq!(for_bob.identity_id, b);
    assert_eq!(count(&pool, "identity").await, 2);
    // Each link signs in its own account and no other.
    let (_, ann) = verify(&router, for_ann.token.expose()).await;
    let (_, bob) = verify(&router, for_bob.token.expose()).await;
    assert_eq!(me(&router, ann["access_token"].as_str().unwrap()).await, a);
    assert_eq!(me(&router, bob["access_token"].as_str().unwrap()).await, b);
    // Addresses that are not addresses, and the reserved agent domain, make nothing.
    for bad in [
        "",
        "no-at-sign",
        "a@b",
        "a b@example.test",
        "x@agents.invalid",
        "x@Sub.Agents.Invalid.",
    ] {
        assert!(
            operator::issue(&pool, &request(bad)).await.is_err(),
            "{bad:?}"
        );
    }
    assert_eq!(count(&pool, "operator_sign_in_link").await, 2);
}

// --- --create and the first administrator ---------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn a_missing_account_needs_create(pool: PgPool) {
    let error = operator::issue(&pool, &request("new@example.test"))
        .await
        .unwrap_err();
    assert!(matches!(error, IssueError::NoAccount), "{error}");
    assert_eq!(count(&pool, "identity").await, 0);
    assert!(audit(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_first_administrator_is_created_without_a_second_confirmation(pool: PgPool) {
    let router = router(&pool);
    let mut r = request("Admin@Example.test");
    r.create = true;
    r.expect = Some(Expect::Admin);
    let issued = operator::issue(&pool, &r).await.unwrap();
    assert!(issued.created_account && issued.administrator);
    assert_eq!(issued.email, "admin@example.test");
    let (status, tokens) = verify(&router, issued.token.expose()).await;
    assert_eq!(status, StatusCode::OK, "{tokens}");
    let access = tokens["access_token"].as_str().unwrap();
    // The existing mechanism makes the administrator: the address is in ADMIN_EMAILS.
    let (status, overview) = send(&router, bearer_get("/admin/overview", access)).await;
    assert_eq!(status, StatusCode::OK, "{overview}");
    assert_eq!(overview["administrator"], "admin@example.test");
    let rows = audit(&pool).await;
    assert_eq!(rows[0].2["created_account"], true);
    assert_eq!(rows[0].2["administrator"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn once_an_administrator_has_an_account_creating_another_needs_the_address_typed_again(
    pool: PgPool,
) {
    account(&pool, "admin@example.test").await;
    // A second administrator address, absent: refused until confirmed.
    let mut r = request("second-admin@example.test");
    r.create = true;
    let error = operator::issue(&pool, &r).await.unwrap_err();
    assert!(
        matches!(
            error,
            IssueError::NeedsConfirmation(NeedsConfirmation::AdministratorExists)
        ),
        "{error}"
    );
    // An ordinary new person, too.
    let mut r = request("new.person@example.test");
    r.create = true;
    let error = operator::issue(&pool, &r).await.unwrap_err();
    assert!(
        matches!(
            error,
            IssueError::NeedsConfirmation(NeedsConfirmation::AdministratorExists)
        ),
        "{error}"
    );
    // Nothing was made by either refusal.
    assert_eq!(count(&pool, "identity").await, 1);
    assert_eq!(count(&pool, "operator_sign_in_link").await, 0);
    assert!(audit(&pool).await.is_empty());
    // A confirmation for another address, or none that matches, does not do it.
    for typed in ["someone.else@example.test", "new.person@example.org", ""] {
        let mut r = request("new.person@example.test");
        r.create = true;
        r.confirm_create = Some(typed);
        assert!(operator::issue(&pool, &r).await.is_err(), "{typed:?}");
    }
    assert_eq!(count(&pool, "identity").await, 1);
    // The address typed again (whatever its case or spaces) does.
    let mut r = request("new.person@example.test");
    r.create = true;
    r.confirm_create = Some(" New.Person@Example.test ");
    let issued = operator::issue(&pool, &r).await.unwrap();
    assert!(issued.created_account && !issued.administrator);
    assert_eq!(count(&pool, "identity").await, 2);
    // --confirm-create without --create creates nothing.
    let mut r = request("never@example.test");
    r.confirm_create = Some("never@example.test");
    assert!(matches!(
        operator::issue(&pool, &r).await,
        Err(IssueError::NoAccount)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_no_administrator_yet_an_address_that_is_not_one_still_needs_the_confirmation(
    pool: PgPool,
) {
    let mut r = request("not.an.admin@example.test");
    r.create = true;
    let error = operator::issue(&pool, &r).await.unwrap_err();
    assert!(
        matches!(
            error,
            IssueError::NeedsConfirmation(NeedsConfirmation::NotAConfiguredAdministrator)
        ),
        "{error}"
    );
    assert_eq!(count(&pool, "identity").await, 0);
    // With ADMIN_EMAILS empty nobody is a configured administrator: the same.
    let mut r = request("admin@example.test");
    r.create = true;
    r.admin_emails = "";
    assert!(matches!(
        operator::issue(&pool, &r).await,
        Err(IssueError::NeedsConfirmation(
            NeedsConfirmation::NotAConfiguredAdministrator
        ))
    ));
    assert_eq!(count(&pool, "identity").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_disabled_administrator_does_not_count_as_one_for_the_bootstrap(pool: PgPool) {
    let id = account(&pool, "admin@example.test").await;
    sqlx::query("update identity set disabled_at=now(), session_generation=session_generation+1 where id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    // The disabled administrator cannot be given a link (restore it first) ...
    let mut r = request("admin@example.test");
    r.create = true;
    assert!(matches!(
        operator::issue(&pool, &r).await,
        Err(IssueError::Disabled)
    ));
    // ... but another configured administrator can be made with the first-administrator rule.
    let mut r = request("second-admin@example.test");
    r.create = true;
    assert!(operator::issue(&pool, &r).await.unwrap().created_account);
}

// --- roles ------------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn a_link_never_makes_anyone_an_administrator(pool: PgPool) {
    let router = router(&pool);
    account(&pool, "plain@example.test").await;
    account(&pool, "admin@example.test").await;

    // An ordinary account stays ordinary, however the link is made.
    let issued = issue(&pool, "plain@example.test").await;
    assert!(!issued.administrator);
    let (_, tokens) = verify(&router, issued.token.expose()).await;
    let access = tokens["access_token"].as_str().unwrap();
    assert_eq!(
        send(&router, bearer_get("/admin/overview", access)).await.0,
        StatusCode::FORBIDDEN
    );

    // --role admin for an address that is not in ADMIN_EMAILS is refused and makes nothing.
    let before = count(&pool, "operator_sign_in_link").await;
    let mut r = request("plain@example.test");
    r.expect = Some(Expect::Admin);
    assert!(matches!(
        operator::issue(&pool, &r).await,
        Err(IssueError::RoleMismatch(_))
    ));
    // --role user for an administrator address is refused too.
    let mut r = request("admin@example.test");
    r.expect = Some(Expect::User);
    assert!(matches!(
        operator::issue(&pool, &r).await,
        Err(IssueError::RoleMismatch(_))
    ));
    assert_eq!(count(&pool, "operator_sign_in_link").await, before);

    // --create plus --role admin cannot make an administrator out of a new address either.
    let mut r = request("hopeful@example.test");
    r.create = true;
    r.confirm_create = Some("hopeful@example.test");
    r.expect = Some(Expect::Admin);
    assert!(matches!(
        operator::issue(&pool, &r).await,
        Err(IssueError::RoleMismatch(_))
    ));
    assert_eq!(
        count(&pool, "identity").await,
        2,
        "the refusal made no account"
    );

    // An address that IS in ADMIN_EMAILS signs in as an administrator because of ADMIN_EMAILS.
    let mut r = request("admin@example.test");
    r.expect = Some(Expect::Admin);
    let issued = operator::issue(&pool, &r).await.unwrap();
    assert!(issued.administrator);
    let (_, tokens) = verify(&router, issued.token.expose()).await;
    let access = tokens["access_token"].as_str().unwrap();
    assert_eq!(
        send(&router, bearer_get("/admin/overview", access)).await.0,
        StatusCode::OK
    );

    // The same session on an installation where ADMIN_EMAILS no longer names the address is not one.
    let mut other = state(pool.clone());
    other.admin_emails = "someone.else@example.test".into();
    let other = app(other);
    assert_eq!(
        send(&other, bearer_get("/admin/overview", access)).await.0,
        StatusCode::FORBIDDEN
    );
}

// --- accounts that cannot sign in --------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn disabled_accounts_get_no_link_and_their_links_stop_working(pool: PgPool) {
    let router = router(&pool);
    let id = account(&pool, "gone@example.test").await;
    let outstanding = issue(&pool, "gone@example.test").await;

    // Disabling the account (the administration screen also bumps the generation) ends the link.
    sqlx::query("update identity set disabled_at=now(), session_generation=session_generation+1 where id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        verify(&router, outstanding.token.expose()).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        outcome(&pool, outstanding.token.expose()).await,
        None,
        "refused, not used up"
    );
    let rows = audit(&pool).await;
    assert_eq!(rows.last().unwrap().2["reason"], "account");
    // No new link for a disabled account, with or without --create.
    for create in [false, true] {
        let mut r = request("gone@example.test");
        r.create = create;
        r.confirm_create = Some("gone@example.test");
        assert!(
            matches!(operator::issue(&pool, &r).await, Err(IssueError::Disabled)),
            "create={create}"
        );
    }
    // Even a disabled flag set WITHOUT the generation bump stops the link (the query checks both).
    let id2 = account(&pool, "flag-only@example.test").await;
    let flag = issue(&pool, "flag-only@example.test").await;
    sqlx::query("update identity set disabled_at=now() where id=$1")
        .bind(id2)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        verify(&router, flag.token.expose()).await.0,
        StatusCode::UNAUTHORIZED
    );
    // The link's own query refused it (a refusal is audited as such), not only the login made after.
    assert_eq!(refusal_reasons(&pool, flag.link_id).await, ["account"]);
    assert_eq!(outcome(&pool, flag.token.expose()).await, None);
}

/// The `reason` of each `auth.operator_link.refused` row for one link.
async fn refusal_reasons(pool: &PgPool, link: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "select detail->>'reason' from operation_audit where action='auth.operator_link.refused' and resource_id=$1 order by id",
    )
    .bind(link)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn signing_out_everywhere_ends_an_outstanding_link(pool: PgPool) {
    let router = router(&pool);
    let id = account(&pool, "everywhere@example.test").await;
    let issued = issue(&pool, "everywhere@example.test").await;
    sqlx::query("update identity set session_generation=session_generation+1 where id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        verify(&router, issued.token.expose()).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Refused by the link's own query (audited as an account change), and not used up.
    assert_eq!(refusal_reasons(&pool, issued.link_id).await, ["account"]);
    assert_eq!(outcome(&pool, issued.token.expose()).await, None);
    // A link made afterwards works.
    let fresh = issue(&pool, "everywhere@example.test").await;
    assert_eq!(
        verify(&router, fresh.token.expose()).await.0,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn erasing_an_account_deletes_its_links(pool: PgPool) {
    let router = router(&pool);
    // The administrator signs in with a link of its own.
    account(&pool, "admin@example.test").await;
    let admin_link = issue(&pool, "admin@example.test").await;
    let (_, tokens) = verify(&router, admin_link.token.expose()).await;
    let admin = tokens["access_token"].as_str().unwrap().to_string();

    let victim = account(&pool, "victim@example.test").await;
    let victim_link = issue(&pool, "victim@example.test").await;
    assert_eq!(count(&pool, "operator_sign_in_link").await, 2);
    let (status, body) = send(
        &router,
        bearer_post(
            "/admin/actions",
            &admin,
            &json!({"kind":"users","id":victim,"removed":true,"confirmation":"victim@example.test"}),
            false,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, preview) = send(
        &router,
        bearer_get(&format!("/admin/impact?kind=users&id={victim}"), &admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    let (status, body) = send(
        &router,
        bearer_post(
            "/admin/erase",
            &admin,
            &json!({"kind":"users","id":victim,"confirmation":"victim@example.test","revision":preview["revision"],"reason":"owner_request","acknowledge_backups":true}),
            false,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let left: Vec<Uuid> = sqlx::query_scalar("select identity_id from operator_sign_in_link")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left.len(), 1);
    assert_ne!(left[0], victim);
    // The erased person's link is gone, and erasure freed the address: there is no account for it,
    // and a new one made on purpose is a different person.
    assert_eq!(
        verify(&router, victim_link.token.expose()).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(matches!(
        operator::issue(&pool, &request("victim@example.test")).await,
        Err(IssueError::NoAccount)
    ));
    let mut r = request("victim@example.test");
    r.create = true;
    r.confirm_create = Some("victim@example.test");
    let again = operator::issue(&pool, &r).await.unwrap();
    assert!(again.created_account);
    assert_ne!(again.identity_id, victim);
}

// --- rate limits and the public ------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn the_publics_limits_and_noise_never_block_the_operator(pool: PgPool) {
    let router = router(&pool);
    account(&pool, "admin@example.test").await;
    // The public exhausts the emailed-link request limit for the address (5 per 15 minutes).
    let mut statuses = Vec::new();
    for _ in 0..7 {
        statuses.push(
            send(
                &router,
                post(
                    "/auth/magic-link",
                    &json!({"email": "admin@example.test"}),
                    true,
                ),
            )
            .await
            .0,
        );
    }
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::FORBIDDEN)
            .count(),
        2,
        "{statuses:?}"
    );
    // ... and hammers the verify route with garbage (no lockout exists to trip).
    for n in 0..60 {
        let junk = format!("{n:064x}");
        assert_eq!(verify(&router, &junk).await.0, StatusCode::UNAUTHORIZED);
    }
    let windows_before = count(&pool, "auth_request_window").await;
    // The operator is not blocked: issuing and using work, and touch no public window.
    let issued = issue(&pool, "admin@example.test").await;
    assert_eq!(
        verify(&router, issued.token.expose()).await.0,
        StatusCode::OK
    );
    assert_eq!(count(&pool, "auth_request_window").await, windows_before);
    // The operator can issue as often as needed; each new link ends the earlier unused one.
    let a = issue(&pool, "admin@example.test").await;
    let b = issue(&pool, "admin@example.test").await;
    assert_eq!(b.superseded, 1);
    assert_eq!(
        verify(&router, a.token.expose()).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(verify(&router, b.token.expose()).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_replayed_link_writes_one_audit_row_a_minute(pool: PgPool) {
    let router = router(&pool);
    account(&pool, "replay@example.test").await;
    let issued = issue(&pool, "replay@example.test").await;
    assert_eq!(
        verify(&router, issued.token.expose()).await.0,
        StatusCode::OK
    );
    for _ in 0..10 {
        assert_eq!(
            verify(&router, issued.token.expose()).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let refused = actions(&pool)
        .await
        .into_iter()
        .filter(|a| a.ends_with("refused"))
        .count();
    assert_eq!(refused, 1);
    // An unknown token has nothing to attribute it to: no row at all.
    let before = audit(&pool).await.len();
    for n in 0..5 {
        verify(&router, &format!("{n:064x}")).await;
    }
    assert_eq!(audit(&pool).await.len(), before);
}

// --- revocation --------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn a_link_can_be_listed_and_revoked_by_a_hash_prefix(pool: PgPool) {
    let router = router(&pool);
    account(&pool, "revoked@example.test").await;
    let issued = issue(&pool, "revoked@example.test").await;
    let listed = operator::list(&pool, false).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].token_prefix, issued.token_prefix);
    assert_eq!(
        (listed[0].state.as_str(), listed[0].email.as_str()),
        ("live", "revoked@example.test")
    );
    // The listing is the prefix of the HASH: nothing in it lets anyone sign in.
    let shown = format!("{:?}", listed[0]);
    assert!(!shown.contains(issued.token.expose()));
    assert_eq!(
        verify(&router, &listed[0].token_prefix).await.0,
        StatusCode::UNAUTHORIZED
    );

    // Too short, not hexadecimal, unknown.
    for bad in ["", "abc", "abcdef0", "zzzzzzzz", &"g".repeat(8)] {
        assert!(
            matches!(
                operator::revoke(&pool, bad, "tester").await,
                Err(RevokeError::BadPrefix)
            ),
            "{bad:?}"
        );
    }
    assert!(matches!(
        operator::revoke(&pool, "ffffffff00", "tester").await,
        Err(RevokeError::NotFound)
    ));
    assert!(matches!(
        operator::revoke(&pool, &issued.token_prefix, "").await,
        Err(RevokeError::BadOperator)
    ));
    assert_eq!(outcome(&pool, issued.token.expose()).await, None);

    operator::revoke(&pool, &issued.token_prefix.to_uppercase(), "tester")
        .await
        .unwrap();
    assert_eq!(
        outcome(&pool, issued.token.expose()).await.as_deref(),
        Some("revoked")
    );
    assert_eq!(
        verify(&router, issued.token.expose()).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Already ended: nothing to revoke any more; it also no longer lists as live.
    assert!(matches!(
        operator::revoke(&pool, &issued.token_prefix, "tester").await,
        Err(RevokeError::NotFound)
    ));
    assert!(operator::list(&pool, false).await.unwrap().is_empty());
    assert_eq!(
        operator::list(&pool, true).await.unwrap()[0].state,
        "revoked"
    );
    assert_eq!(
        actions(&pool).await,
        [
            "auth.operator_link.issued",
            "auth.operator_link.revoked",
            "auth.operator_link.refused"
        ]
    );
    // A used link cannot be revoked.
    let used = issue(&pool, "revoked@example.test").await;
    assert_eq!(verify(&router, used.token.expose()).await.0, StatusCode::OK);
    assert!(matches!(
        operator::revoke(&pool, &used.token_prefix, "tester").await,
        Err(RevokeError::NotFound)
    ));
}

// --- simultaneous use -----------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn simultaneous_uses_of_one_link_have_exactly_one_winner(pool: PgPool) {
    let router = router(&pool);
    let id = account(&pool, "race@example.test").await;
    for round in 0..5 {
        let issued = issue(&pool, "race@example.test").await;
        let token = issued.token.expose().to_string();
        let attempts = (0..8).map(|_| {
            let (router, token) = (router.clone(), token.clone());
            tokio::spawn(async move { verify(&router, &token).await.0 })
        });
        let statuses: Vec<StatusCode> = join_all(attempts)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            statuses.iter().filter(|s| **s == StatusCode::OK).count(),
            1,
            "round {round}: {statuses:?}"
        );
        assert!(statuses
            .iter()
            .filter(|s| **s != StatusCode::OK)
            .all(|s| *s == StatusCode::UNAUTHORIZED));
        assert_eq!(outcome(&pool, &token).await.as_deref(), Some("used"));
        // One login per round, and one `used` row.
        let logins: i64 = sqlx::query_scalar(
            "select count(distinct session_id) from refresh_token where identity_id=$1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(logins, round + 1);
        let used: i64 = sqlx::query_scalar(
            "select count(*) from operation_audit where action='auth.operator_link.used'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(used, round + 1);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn simultaneous_issuing_leaves_one_live_link(pool: PgPool) {
    account(&pool, "twice@example.test").await;
    let attempts = (0..6).map(|_| {
        let pool = pool.clone();
        tokio::spawn(async move {
            operator::issue(&pool, &request("twice@example.test"))
                .await
                .map(|i| i.superseded)
        })
    });
    let results = join_all(attempts).await;
    assert!(results.iter().all(|r| r.as_ref().unwrap().is_ok()));
    let live: i64 =
        sqlx::query_scalar("select count(*) from operator_sign_in_link where consumed_at is null")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(live, 1);
    // Simultaneous --create of one address makes one account.
    let attempts = (0..6).map(|_| {
        let pool = pool.clone();
        tokio::spawn(async move {
            let mut r = request("admin@example.test");
            r.create = true;
            operator::issue(&pool, &r).await.map(|i| i.created_account)
        })
    });
    let created: Vec<bool> = join_all(attempts)
        .await
        .into_iter()
        .filter_map(|r| r.unwrap().ok())
        .collect();
    assert_eq!(created.iter().filter(|c| **c).count(), 1, "{created:?}");
    let admins: i64 =
        sqlx::query_scalar("select count(*) from identity where email='admin@example.test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(admins, 1);
}

// --- no secret anywhere ------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn capture_logs() -> Captured {
    static LOGS: std::sync::OnceLock<Captured> = std::sync::OnceLock::new();
    LOGS.get_or_init(|| {
        let captured = Captured::default();
        tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .with_writer(captured.clone())
                .finish(),
        )
        .expect("no other global subscriber in this test binary");
        captured
    })
    .clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn no_token_reaches_the_table_the_audit_rows_the_debug_output_or_the_log(pool: PgPool) {
    let captured = capture_logs();
    let router = router(&pool);
    account(&pool, "quiet@example.test").await;
    let a = issue(&pool, "quiet@example.test").await;
    let a_token = a.token.expose().to_string();
    let debug = format!("{a:?}");
    assert!(
        !debug.contains(&a_token),
        "Debug of an issued link shows the token"
    );
    let b = issue(&pool, "quiet@example.test").await; // ends `a`
    let b_token = b.token.expose().to_string();
    let c = link_in_the_past(&pool, a.identity_id, "3 hours", "15 minutes").await;
    assert_eq!(verify(&router, &a_token).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(verify(&router, &b_token).await.0, StatusCode::OK);
    assert_eq!(verify(&router, &b_token).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(verify(&router, &c).await.0, StatusCode::UNAUTHORIZED);
    operator::revoke(&pool, &a.token_prefix, "tester")
        .await
        .ok();

    let tokens = [&a_token, &b_token, &c];
    // The table holds hashes: no column, taken as text, shows a token as it travels.
    let rows: Vec<String> = sqlx::query_scalar("select t::text from operator_sign_in_link t")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    for row in &rows {
        for token in tokens {
            assert!(
                !row.contains(token.as_str()),
                "a token is in the table: {row}"
            );
        }
    }
    // The audit rows: ids, a label, a reason, counts, a hash prefix. Never a token or an address.
    let allowed = [
        "operator",
        "reason",
        "ttl_seconds",
        "created_account",
        "administrator",
        "superseded",
        "token_prefix",
        "session_id",
    ];
    for (action, _, detail) in audit(&pool).await {
        let text = format!("{action} {detail}");
        for token in tokens {
            assert!(!text.contains(token.as_str()), "{text}");
        }
        assert!(!text.contains("quiet@example.test"), "{text}");
        for key in detail.as_object().unwrap().keys() {
            assert!(allowed.contains(&key.as_str()) || key == "reason", "{key}");
        }
    }
    // And the log of the whole exchange, request spans included.
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(log.contains("auth/verify"), "the requests were logged");
    for token in tokens {
        assert!(!log.contains(token.as_str()), "a token is in the log");
    }
}

// --- the restricted runtime login ------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn the_restricted_runtime_login_can_use_a_link_but_cannot_make_one(pool: PgPool) {
    // RuntimeDatabase::new runs grant_runtime and check_runtime, which fail for a table that is
    // missing from the allowlists or granted more than its list says.
    let runtime = RuntimeDatabase::new(pool).await;
    let id = account(&runtime.owner, "runtime@example.test").await;
    let router = router(&runtime.pool);
    // The operator (owner) issues; the API (runtime login) verifies.
    let issued = issue(&runtime.owner, "runtime@example.test").await;
    let (status, tokens) = verify(&router, issued.token.expose()).await;
    assert_eq!(status, StatusCode::OK, "{tokens}");
    assert_eq!(
        me(&router, tokens["access_token"].as_str().unwrap()).await,
        id
    );
    assert_eq!(
        verify(&router, issued.token.expose()).await.0,
        StatusCode::UNAUTHORIZED
    );

    // The login cannot insert a row, by any statement.
    for statement in [
        "insert into operator_sign_in_link(identity_id,token_hash,session_generation,operator,expires_at) select id, sha256('forged'), 0, 'x', now()+interval '10 minutes' from identity".to_string(),
        "insert into operator_sign_in_link select * from operator_sign_in_link".to_string(),
    ] {
        let error = sqlx::raw_sql(&statement).execute(&runtime.pool).await.unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("42501"),
            "{statement}: {error}"
        );
    }
    let (table_insert, column_insert, table_update, table_delete, truncate): (
        bool,
        bool,
        bool,
        bool,
        bool,
    ) = sqlx::query_as(
        "select has_table_privilege(current_user,'operator_sign_in_link','INSERT'),
                has_any_column_privilege(current_user,'operator_sign_in_link','INSERT'),
                has_table_privilege(current_user,'operator_sign_in_link','UPDATE'),
                has_table_privilege(current_user,'operator_sign_in_link','DELETE'),
                has_table_privilege(current_user,'operator_sign_in_link','TRUNCATE')",
    )
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    assert_eq!(
        (
            table_insert,
            column_insert,
            table_update,
            table_delete,
            truncate
        ),
        (false, false, true, true, false)
    );

    // So the issuing code, handed the runtime connection, fails and leaves nothing behind.
    let error = operator::issue(&runtime.pool, &request("runtime@example.test"))
        .await
        .unwrap_err();
    match error {
        IssueError::Db(e) => assert_eq!(
            e.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("42501")
        ),
        other => panic!("{other}"),
    }
    let live: i64 =
        sqlx::query_scalar("select count(*) from operator_sign_in_link where consumed_at is null")
            .fetch_one(&runtime.owner)
            .await
            .unwrap();
    assert_eq!(live, 0);

    // It can use a live link up (once) and prune, but not rewrite one.
    let live = issue(&runtime.owner, "runtime@example.test").await;
    for statement in [
        "update operator_sign_in_link set expires_at=expires_at+interval '1 hour'",
        "update operator_sign_in_link set identity_id=identity_id",
        "update operator_sign_in_link set token_hash=sha256('forged')",
        "update operator_sign_in_link set reason='edited'",
        "update operator_sign_in_link set consumed_at=null, outcome=null",
        "update operator_sign_in_link set consumed_at=now()",
        "update operator_sign_in_link set consumed_at=now(), outcome='expired'",
    ] {
        let error = sqlx::raw_sql(statement)
            .execute(&runtime.pool)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("can only be ended, once"),
            "{statement}: {error}"
        );
    }
    let error = sqlx::raw_sql("truncate operator_sign_in_link")
        .execute(&runtime.pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501")
    );
    assert_eq!(verify(&router, live.token.expose()).await.0, StatusCode::OK);
    sqlx::raw_sql("delete from operator_sign_in_link")
        .execute(&runtime.pool)
        .await
        .unwrap();

    // The startup gate notices a login that was given more than the list says.
    sqlx::raw_sql(&format!(
        "grant insert on operator_sign_in_link to {}",
        runtime.role
    ))
    .execute(&runtime.owner)
    .await
    .unwrap();
    let error = engine_core::database::check_runtime(&runtime.pool)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("operator_sign_in_link"),
        "{error}"
    );
    // Column-level insert is noticed as well.
    sqlx::raw_sql(&format!("revoke insert on operator_sign_in_link from {role}; grant insert (reason) on operator_sign_in_link to {role}", role = runtime.role))
        .execute(&runtime.owner)
        .await
        .unwrap();
    assert!(engine_core::database::check_runtime(&runtime.pool)
        .await
        .is_err());
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_table_refuses_rows_that_are_not_born_live(pool: PgPool) {
    let id = account(&pool, "born@example.test").await;
    for tail in [
        "now(), 'used'",
        "now(), 'revoked'",
        "null, 'used'",
        "now(), null",
    ] {
        let (consumed, out) = tail.split_once(", ").unwrap();
        let error = sqlx::query(&format!(
            "insert into operator_sign_in_link(identity_id,token_hash,session_generation,operator,expires_at,consumed_at,outcome)
             values($1,$2,0,'x',now()+interval '5 minutes',{consumed},{out})"
        ))
        .bind(id)
        .bind(sha(tail))
        .execute(&pool)
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("created live") || error.to_string().contains("check"),
            "{tail}: {error}"
        );
    }
    // Operator label and reason have no control characters and are bounded.
    for (operator, reason) in [
        ("", ""),
        ("a\nb", ""),
        ("ok", "line\nbreak"),
        (&"a".repeat(65), ""),
        ("ok", &"r".repeat(201)),
    ] {
        let error = sqlx::query("insert into operator_sign_in_link(identity_id,token_hash,session_generation,operator,reason,expires_at) values($1,$2,0,$3,$4,now()+interval '5 minutes')")
            .bind(id)
            .bind(sha(operator))
            .bind(operator)
            .bind(reason)
            .execute(&pool)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("check"),
            "{operator:?}/{reason:?}: {error}"
        );
    }
}

// --- no way in over the network ----------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn no_http_route_issues_a_link(pool: PgPool) {
    let router = router(&pool);
    account(&pool, "admin@example.test").await;
    let admin = issue(&pool, "admin@example.test").await;
    let (_, tokens) = verify(&router, admin.token.expose()).await;
    let access = tokens["access_token"].as_str().unwrap().to_string();
    let before = count(&pool, "operator_sign_in_link").await;
    for path in [
        "/auth/operator-link",
        "/auth/operator-links",
        "/auth/operator/issue",
        "/auth/operator/issue-link",
        "/auth/issue-link",
        "/auth/admin-link",
        "/auth/sign-in-link",
        "/admin/sign-in-link",
        "/admin/operator-link",
        "/admin/links",
        "/internal/sign-in-link",
    ] {
        for (service, bearer) in [(true, true), (true, false), (false, true)] {
            let body = json!({"email": "admin@example.test", "ttl": 900});
            let req = if bearer {
                bearer_post(path, &access, &body, service)
            } else {
                post(path, &body, service)
            };
            let (status, _) = send(&router, req).await;
            assert!(
                status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED,
                "{path}: {status}"
            );
        }
    }
    assert_eq!(count(&pool, "operator_sign_in_link").await, before);
    // The published API description has no such route either.
    let spec = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../openapi.yaml"))
        .unwrap();
    assert!(
        !spec.to_lowercase().contains("operator"),
        "openapi.yaml names an operator route"
    );
}

/// The issuing function is called from the command line tool and from nowhere in the server: a
/// route that called it would be a network path to the credential. (A source check, because a
/// route can be added under any name.)
#[test]
fn nothing_in_the_server_calls_the_issuing_function() {
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                if name != "operator.rs"
                    && (text.contains("operator::issue(") || text.contains("operator::issue ("))
                {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    let mut hits = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(hits.is_empty(), "{hits:?}");
}

// --- how this fits the rest of sign-in ---------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn emailed_links_are_unchanged_and_an_operator_session_can_start_the_app_hand_over(
    pool: PgPool,
) {
    let router = router(&pool);
    // An emailed link still works once, and its replay is the same 401 with no operator audit row.
    let (status, link) = send(
        &router,
        post(
            "/auth/magic-link",
            &json!({"email": "mail@example.test"}),
            true,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = link["magic_link_token"].as_str().unwrap();
    assert_eq!(verify(&router, token).await.0, StatusCode::OK);
    assert_eq!(verify(&router, token).await.0, StatusCode::UNAUTHORIZED);
    assert!(audit(&pool).await.is_empty());

    // A person who signed in with an operator link can ask for an app hand-over code, as after any login.
    account(&pool, "app@example.test").await;
    let issued = issue(&pool, "app@example.test").await;
    let (_, tokens) = verify(&router, issued.token.expose()).await;
    let access = tokens["access_token"].as_str().unwrap();
    let (status, body) = send(
        &router,
        bearer_post(
            "/auth/app-handover",
            access,
            &json!({"challenge": "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM", "state": "b3JkZXJseS1zdGF0ZS12YWx1ZQ", "platform": "desktop"}),
            true,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The session can be refreshed and signed out like any other.
    let (status, refreshed) = send(
        &router,
        post(
            "/auth/refresh",
            &json!({"refresh_token": tokens["refresh_token"]}),
            false,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");
    let (status, _) = send(
        &router,
        post(
            "/auth/logout",
            &json!({"refresh_token": refreshed["refresh_token"]}),
            false,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        send(&router, bearer_get("/auth/me", access)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_reason_and_label_are_bounded_text(pool: PgPool) {
    account(&pool, "text@example.test").await;
    for (who, why) in [
        ("", "x"),
        ("ok", "line\nbreak"),
        ("esc\u{1b}[31m", ""),
        (&"a".repeat(65), ""),
        ("ok", &"r".repeat(201)),
    ] {
        let mut r = request("text@example.test");
        r.operator = who;
        r.reason = why;
        assert!(
            matches!(
                operator::issue(&pool, &r).await,
                Err(IssueError::Invalid(_))
            ),
            "{who:?}/{why:?}"
        );
    }
    assert_eq!(count(&pool, "operator_sign_in_link").await, 0);
    let mut r = request("text@example.test");
    r.reason = "";
    r.operator = "DOMAIN\\alice";
    assert!(operator::issue(&pool, &r).await.is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn using_a_link_and_the_login_it_makes_succeed_or_fail_together(pool: PgPool) {
    let router = router(&pool);
    let id = account(&pool, "atomic@example.test").await;
    let issued = issue(&pool, "atomic@example.test").await;
    let token = issued.token.expose().to_string();
    let logins = || async {
        sqlx::query_scalar::<_, i64>("select count(*) from refresh_token where identity_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    let used_rows = || async {
        sqlx::query_scalar::<_, i64>(
            "select count(*) from operation_audit where action='auth.operator_link.used'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };

    // The login cannot be made: the link is not used up, and nothing is recorded as used.
    sqlx::raw_sql(
        "create function refuse_login() returns trigger language plpgsql as $$ begin raise exception 'no login'; end $$;
         create trigger refuse_login before insert on refresh_token for each row execute function refuse_login();",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        verify(&router, &token).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(outcome(&pool, &token).await, None, "the link is still live");
    assert_eq!((logins().await, used_rows().await), (0, 0));
    sqlx::raw_sql("drop trigger refuse_login on refresh_token")
        .execute(&pool)
        .await
        .unwrap();

    // The audit row cannot be written: the login that was made is undone with it.
    sqlx::raw_sql(
        "create function refuse_used_audit() returns trigger language plpgsql as $$ begin
             if new.action = 'auth.operator_link.used' then raise exception 'no audit'; end if;
             return new;
         end $$;
         create trigger refuse_used_audit before insert on operation_audit for each row execute function refuse_used_audit();",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        verify(&router, &token).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(outcome(&pool, &token).await, None, "the link is still live");
    assert_eq!(
        (logins().await, used_rows().await),
        (0, 0),
        "no login without its record"
    );
    sqlx::raw_sql("drop trigger refuse_used_audit on operation_audit")
        .execute(&pool)
        .await
        .unwrap();

    // Everything works, once: the link, one login and one audit row together.
    assert_eq!(verify(&router, &token).await.0, StatusCode::OK);
    assert_eq!((logins().await, used_rows().await), (1, 1));
    assert_eq!(outcome(&pool, &token).await.as_deref(), Some("used"));
    assert_eq!(verify(&router, &token).await.0, StatusCode::UNAUTHORIZED);
}
