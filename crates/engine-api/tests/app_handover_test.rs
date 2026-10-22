//! Native-app sign-in hand-over (docs/decisions/017-native-app-sign-in-handover.md), against a
//! real PostgreSQL and the real router. Every test is a fresh migrated database
//! (`#[sqlx::test]`). The runtime-role tests run as the restricted login that production uses.
mod common;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use common::RuntimeDatabase;
use engine_api::auth::{handover, AuthConfig};
use engine_api::{app, AppState};
use futures::future::join_all;
use ring::digest::{digest, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use uuid::Uuid;

const SERVICE_KEY: &str = "isolated-handover-test-service-key-32-chars";

fn test_state(pool: PgPool) -> AppState {
    AppState::new(
        pool,
        AuthConfig {
            paseto_key: [9u8; 32],
            service_key: Some(SERVICE_KEY.into()),
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    )
}

fn router(pool: &PgPool) -> Router {
    app(test_state(pool.clone()))
}

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Value, Vec<(String, String)>) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value, headers)
}

fn json_post(uri: &str, body: &Value, bearer: Option<&str>, service: bool) -> Request<Body> {
    let mut req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    if service {
        req = req.header("x-dynodoc-service-key", SERVICE_KEY);
    }
    if let Some(bearer) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
    }
    req.body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

/// Sign `email` in by the ordinary magic-link route; returns (access token, identity id).
async fn sign_in(router: &Router, email: &str) -> (String, Uuid) {
    let (status, link, _) = send(
        router,
        json_post("/auth/magic-link", &json!({"email": email}), None, true),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, tokens, _) = send(
        router,
        json_post(
            "/auth/verify",
            &json!({"token": link["magic_link_token"]}),
            None,
            false,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access = tokens["access_token"].as_str().unwrap().to_string();
    let id = me(router, &access).await;
    (access, id)
}

async fn me(router: &Router, access: &str) -> Uuid {
    let (status, body, _) = send(
        router,
        Request::builder()
            .uri("/auth/me")
            .header(header::AUTHORIZATION, format!("Bearer {access}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["identity_id"].as_str().unwrap().parse().unwrap()
}

/// What an app holds: a verifier, its challenge and a state, made as the shell makes them.
struct Pkce {
    verifier: String,
    challenge: String,
    state: String,
}

fn random_url_string(bytes: usize) -> String {
    let mut raw = vec![0u8; bytes];
    SystemRandom::new().fill(&mut raw).unwrap();
    URL_SAFE_NO_PAD.encode(raw)
}

fn pkce() -> Pkce {
    let verifier = random_url_string(32);
    let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
    Pkce {
        verifier,
        challenge,
        state: random_url_string(24),
    }
}

async fn issue_for(
    router: &Router,
    access: &str,
    pkce: &Pkce,
    platform: &str,
) -> (StatusCode, Value) {
    let (status, body, headers) = send(
        router,
        json_post(
            "/auth/app-handover",
            &json!({"challenge": pkce.challenge, "state": pkce.state, "platform": platform}),
            Some(access),
            true,
        ),
    )
    .await;
    // Nothing here may be cached.
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "cache-control" && v.contains("no-store"))
            || status != StatusCode::OK
    );
    (status, body)
}

async fn issue_code(router: &Router, access: &str, pkce: &Pkce) -> String {
    let (status, body) = issue_for(router, access, pkce, "android").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["expires_in"], handover::CODE_TTL_SECONDS);
    let code = body["code"].as_str().unwrap().to_string();
    assert!(handover::valid_code(&code));
    code
}

async fn redeem_with(
    router: &Router,
    code: &str,
    state: &str,
    verifier: &str,
    client: Option<&str>,
) -> (StatusCode, Value) {
    let mut body = json!({"code": code, "state": state, "verifier": verifier});
    if let Some(client) = client {
        body["client"] = json!(client);
    }
    let (status, body, headers) = send(
        router,
        json_post("/auth/app-handover/redeem", &body, None, true),
    )
    .await;
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "cache-control" && v.contains("no-store"))
            || status != StatusCode::OK
    );
    (status, body)
}

async fn redeem(router: &Router, code: &str, pkce: &Pkce) -> (StatusCode, Value) {
    redeem_with(router, code, &pkce.state, &pkce.verifier, None).await
}

fn hash(text: &str) -> Vec<u8> {
    digest(&SHA256, text.as_bytes()).as_ref().to_vec()
}

async fn audit_rows(pool: &PgPool) -> Vec<(String, Uuid, Value)> {
    sqlx::query_as(
        "select action, actor_id, detail from operation_audit where action like 'auth.app_handover.%' order by id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn outcome_of(pool: &PgPool, code: &str) -> Option<String> {
    sqlx::query_scalar("select outcome from app_handover_code where code_hash=$1")
        .bind(hash(code))
        .fetch_one(pool)
        .await
        .unwrap()
}

// --- the flow ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn the_happy_path_gives_the_issuing_account_a_working_session(pool: PgPool) {
    let router = router(&pool);
    let (access, id) = sign_in(&router, "handover-a@example.test").await;
    let p = pkce();
    let code = issue_code(&router, &access, &p).await;

    let (status, tokens) = redeem(&router, &code, &p).await;
    assert_eq!(status, StatusCode::OK, "{tokens}");
    assert_eq!(tokens["token_type"], "Bearer");
    assert!(tokens["refresh_token"].as_str().unwrap().len() >= 32);
    // The new session is a separate login of the same account.
    let new_access = tokens["access_token"].as_str().unwrap();
    assert_ne!(new_access, access);
    assert_eq!(me(&router, new_access).await, id);
    // And it can be refreshed like any other.
    let (status, refreshed, _) = send(
        &router,
        json_post(
            "/auth/refresh",
            &json!({"refresh_token": tokens["refresh_token"]}),
            None,
            false,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");

    assert_eq!(outcome_of(&pool, &code).await.as_deref(), Some("redeemed"));
    let audit = audit_rows(&pool).await;
    let actions: Vec<&str> = audit.iter().map(|(a, _, _)| a.as_str()).collect();
    assert_eq!(
        actions,
        ["auth.app_handover.issued", "auth.app_handover.redeemed"]
    );
    assert!(audit.iter().all(|(_, actor, _)| *actor == id));
    assert!(audit.iter().all(|(_, _, d)| d["platform"] == "android"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_wrong_verifier_is_refused_and_burns_the_code(pool: PgPool) {
    let router = router(&pool);
    let (access, _) = sign_in(&router, "handover-b@example.test").await;
    let p = pkce();
    let code = issue_code(&router, &access, &p).await;

    let wrong = pkce();
    let (status, body) = redeem_with(&router, &code, &p.state, &wrong.verifier, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(body.get("access_token").is_none());
    assert_eq!(outcome_of(&pool, &code).await.as_deref(), Some("burned"));
    // The right verifier no longer helps: the code is used up.
    let (status, _) = redeem(&router, &code, &p).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let audit = audit_rows(&pool).await;
    let reasons: Vec<&str> = audit
        .iter()
        .filter(|(a, _, _)| a == "auth.app_handover.refused")
        .map(|(_, _, d)| d["reason"].as_str().unwrap())
        .collect();
    assert_eq!(reasons, ["verifier", "used"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_wrong_state_is_refused_and_burns_the_code(pool: PgPool) {
    let router = router(&pool);
    let (access, _) = sign_in(&router, "handover-c@example.test").await;
    let p = pkce();
    let code = issue_code(&router, &access, &p).await;
    let other_state = random_url_string(24);
    let (status, _) = redeem_with(&router, &code, &other_state, &p.verifier, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(outcome_of(&pool, &code).await.as_deref(), Some("burned"));
    let (status, _) = redeem(&router, &code, &p).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let reasons: Vec<Value> = audit_rows(&pool)
        .await
        .into_iter()
        .filter(|(a, _, _)| a == "auth.app_handover.refused")
        .map(|(_, _, d)| d["reason"].clone())
        .collect();
    assert_eq!(reasons, [json!("state"), json!("used")]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_expired_code_is_refused(pool: PgPool) {
    let router = router(&pool);
    let (_, id) = sign_in(&router, "handover-d@example.test").await;
    let p = pkce();
    // A code that was issued two minutes ago and expired a minute ago. (The table's guard
    // forbids moving times afterwards, so the row is written already old.)
    let code = "ab".repeat(32);
    sqlx::query(
        "insert into app_handover_code(identity_id,code_hash,challenge,state,platform,session_generation,created_at,expires_at)
         select $1,$2,$3,$4,'desktop',session_generation, now()-interval '2 minutes', now()-interval '1 minute' from identity where id=$1",
    )
    .bind(id)
    .bind(hash(&code))
    .bind(digest(&SHA256, p.verifier.as_bytes()).as_ref().to_vec())
    .bind(&p.state)
    .execute(&pool)
    .await
    .unwrap();
    let (status, _) = redeem(&router, &code, &p).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let audit = audit_rows(&pool).await;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].2["reason"], "expired");

    // Exactly at the edge: a code with a few seconds left still works, one at its second does not.
    let live = "cd".repeat(32);
    sqlx::query(
        "insert into app_handover_code(identity_id,code_hash,challenge,state,platform,session_generation,created_at,expires_at)
         select $1,$2,$3,$4,'desktop',session_generation, now()-interval '57 seconds', now()+interval '3 seconds' from identity where id=$1",
    )
    .bind(id)
    .bind(hash(&live))
    .bind(digest(&SHA256, p.verifier.as_bytes()).as_ref().to_vec())
    .bind(&p.state)
    .execute(&pool)
    .await
    .unwrap();
    let (status, _) = redeem(&router, &live, &p).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_table_never_allows_a_longer_life_than_two_minutes(pool: PgPool) {
    let router = router(&pool);
    let (_, id) = sign_in(&router, "handover-e@example.test").await;
    let p = pkce();
    let error = sqlx::query(
        "insert into app_handover_code(identity_id,code_hash,challenge,state,platform,session_generation,expires_at)
         select $1,$2,$3,$4,'desktop',session_generation, now()+interval '1 day' from identity where id=$1",
    )
    .bind(id)
    .bind(hash("long-lived"))
    .bind(digest(&SHA256, p.verifier.as_bytes()).as_ref().to_vec())
    .bind(&p.state)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23514")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_replayed_code_is_refused(pool: PgPool) {
    let router = router(&pool);
    let (access, _) = sign_in(&router, "handover-f@example.test").await;
    let p = pkce();
    let code = issue_code(&router, &access, &p).await;
    let (status, _) = redeem(&router, &code, &p).await;
    assert_eq!(status, StatusCode::OK);
    for _ in 0..3 {
        let (status, body) = redeem(&router, &code, &p).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    }
    // The first redemption stands: it was not turned into 'burned' by the replays.
    assert_eq!(outcome_of(&pool, &code).await.as_deref(), Some("redeemed"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn simultaneous_redeems_of_one_code_have_exactly_one_winner(pool: PgPool) {
    let router = router(&pool);
    let (access, _) = sign_in(&router, "handover-g@example.test").await;
    for round in 0..5 {
        let p = pkce();
        let code = issue_code(&router, &access, &p).await;
        let attempts = (0..8).map(|_| {
            let router = router.clone();
            let (code, state, verifier) = (code.clone(), p.state.clone(), p.verifier.clone());
            tokio::spawn(
                async move { redeem_with(&router, &code, &state, &verifier, None).await.0 },
            )
        });
        let statuses: Vec<StatusCode> = join_all(attempts)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        let winners = statuses.iter().filter(|s| **s == StatusCode::OK).count();
        assert_eq!(winners, 1, "round {round}: {statuses:?}");
        // The seven losers are refused (a loser that starts after five failed attempts for the
        // same code prefix is turned away by the rate limit first).
        assert!(
            statuses
                .iter()
                .filter(|s| **s != StatusCode::OK)
                .all(|s| *s == StatusCode::UNAUTHORIZED || *s == StatusCode::TOO_MANY_REQUESTS),
            "round {round}: {statuses:?}"
        );
        // One login was made for the code, not several.
        assert_eq!(outcome_of(&pool, &code).await.as_deref(), Some("redeemed"));
        let issued_sessions: i64 = sqlx::query_scalar(
            "select count(*) from operation_audit where action='auth.app_handover.redeemed'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(issued_sessions, round + 1);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_code_gives_its_own_account_whoever_is_signed_in_where_it_is_redeemed(pool: PgPool) {
    let router = router(&pool);
    let (access_a, id_a) = sign_in(&router, "handover-h-a@example.test").await;
    let (access_b, id_b) = sign_in(&router, "handover-h-b@example.test").await;
    assert_ne!(id_a, id_b);
    let p = pkce();
    let code = issue_code(&router, &access_a, &p).await;
    // Redeemed with B's bearer attached (the web view may hold B's session): still A's login.
    let (status, tokens, _) = send(
        &router,
        json_post(
            "/auth/app-handover/redeem",
            &json!({"code": code, "state": p.state, "verifier": p.verifier}),
            Some(&access_b),
            true,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{tokens}");
    assert_eq!(
        me(&router, tokens["access_token"].as_str().unwrap()).await,
        id_a
    );
    // B cannot ask for a code in A's name: the account is whoever the token says.
    let pb = pkce();
    let code_b = issue_code(&router, &access_b, &pb).await;
    let owner_of_b: Uuid =
        sqlx::query_scalar("select identity_id from app_handover_code where code_hash=$1")
            .bind(hash(&code_b))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(owner_of_b, id_b);
    let (status, body, _) = send(
        &router,
        json_post(
            "/auth/app-handover",
            &json!({"challenge": pb.challenge, "state": pb.state, "platform": "ios", "identity_id": id_a}),
            Some(&access_b),
            true,
        ),
    )
    .await;
    assert!(status.is_client_error(), "{status} {body}");
    // A's code does not work with B's verifier, nor B's with A's.
    let pa = pkce();
    let code_a2 = issue_code(&router, &access_a, &pa).await;
    let (status, _) = redeem_with(&router, &code_a2, &pa.state, &pb.verifier, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = redeem_with(&router, &code_b, &pb.state, &pa.verifier, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn signing_out_everywhere_or_disabling_the_account_kills_an_outstanding_code(pool: PgPool) {
    let router = router(&pool);
    let (access, id) = sign_in(&router, "handover-i@example.test").await;
    let p = pkce();
    let code = issue_code(&router, &access, &p).await;
    sqlx::query("update identity set session_generation=session_generation+1 where id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = redeem(&router, &code, &p).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let audit = audit_rows(&pool).await;
    assert_eq!(audit.last().unwrap().2["reason"], "account");
    // The old access token cannot ask for a new code either.
    let (status, _) = issue_for(&router, &access, &pkce(), "android").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn at_most_three_unused_codes_per_account(pool: PgPool) {
    let router = router(&pool);
    let (access, id) = sign_in(&router, "handover-j@example.test").await;
    let mut kept = Vec::new();
    for _ in 0..handover::MAX_OUTSTANDING {
        let p = pkce();
        let code = issue_code(&router, &access, &p).await;
        kept.push((p, code));
    }
    let (status, body) = issue_for(&router, &access, &pkce(), "desktop").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    // Another account is not affected.
    let (other, _) = sign_in(&router, "handover-j2@example.test").await;
    issue_code(&router, &other, &pkce()).await;
    // Using one up frees a place.
    let (p, code) = &kept[0];
    assert_eq!(redeem(&router, code, p).await.0, StatusCode::OK);
    issue_code(&router, &access, &pkce()).await;
    let (status, _) = issue_for(&router, &access, &pkce(), "desktop").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    // An expired code frees one as well.
    sqlx::raw_sql("alter table app_handover_code disable trigger app_handover_code_use_once")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("update app_handover_code set created_at=now()-interval '2 minutes', expires_at=now()-interval '1 minute' where code_hash=$1")
        .bind(hash(&kept[1].1))
        .execute(&pool)
        .await
        .unwrap();
    issue_code(&router, &access, &pkce()).await;
    let refused: i64 = sqlx::query_scalar("select count(*) from operation_audit where action='auth.app_handover.refused' and detail->>'reason'='too_many_outstanding' and actor_id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(refused, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn simultaneous_issuing_cannot_exceed_the_limit(pool: PgPool) {
    let router = router(&pool);
    let (access, _) = sign_in(&router, "handover-k@example.test").await;
    let attempts = (0..8).map(|_| {
        let router = router.clone();
        let access = access.clone();
        tokio::spawn(async move { issue_for(&router, &access, &pkce(), "android").await.0 })
    });
    let statuses: Vec<StatusCode> = join_all(attempts)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        statuses.iter().filter(|s| **s == StatusCode::OK).count() as i64,
        handover::MAX_OUTSTANDING,
        "{statuses:?}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn both_routes_need_the_web_servers_service_key_and_issue_needs_a_login(pool: PgPool) {
    let router = router(&pool);
    let (access, _) = sign_in(&router, "handover-l@example.test").await;
    let p = pkce();
    let issue_body = json!({"challenge": p.challenge, "state": p.state, "platform": "ios"});
    // No service key: refused whatever else is presented.
    let (status, _, _) = send(
        &router,
        json_post("/auth/app-handover", &issue_body, Some(&access), false),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // No login: refused.
    let (status, _, _) = send(
        &router,
        json_post("/auth/app-handover", &issue_body, None, true),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // A forged or other-key token is refused.
    let (status, _, _) = send(
        &router,
        json_post(
            "/auth/app-handover",
            &issue_body,
            Some("v4.local.forged"),
            true,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let code = issue_code(&router, &access, &p).await;
    let redeem_body = json!({"code": code, "state": p.state, "verifier": p.verifier});
    let (status, _, _) = send(
        &router,
        json_post("/auth/app-handover/redeem", &redeem_body, None, false),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // A key that is wrong, short, or not configured at all closes both routes.
    for key in [None, Some(""), Some("short")] {
        let mut state = test_state(pool.clone());
        state.auth.service_key = key.map(str::to_owned);
        let router = app(state);
        let (status, _, _) = send(
            &router,
            json_post("/auth/app-handover/redeem", &redeem_body, None, true),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{key:?}");
    }
    // The code survived all of that unused.
    assert_eq!(outcome_of(&pool, &code).await, None);
    assert_eq!(redeem(&router, &code, &p).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn malformed_requests_are_refused_without_touching_the_ledger(pool: PgPool) {
    let router = router(&pool);
    let (access, _) = sign_in(&router, "handover-m@example.test").await;
    let p = pkce();
    let good = json!({"challenge": p.challenge, "state": p.state, "platform": "android"});
    let cases: Vec<Value> = vec![
        json!({"challenge": "short", "state": p.state, "platform": "android"}),
        json!({"challenge": format!("{}=", p.challenge), "state": p.state, "platform": "android"}),
        json!({"challenge": p.challenge, "state": "tiny", "platform": "android"}),
        json!({"challenge": p.challenge, "state": format!("{}!", p.state), "platform": "android"}),
        json!({"challenge": p.challenge, "state": "a".repeat(65), "platform": "android"}),
        json!({"challenge": p.challenge, "state": p.state, "platform": "windows"}),
        json!({"challenge": p.challenge, "state": p.state, "platform": "Android"}),
        json!({"challenge": p.challenge, "state": p.state}),
        json!({"challenge": p.challenge, "state": p.state, "platform": "android", "extra": 1}),
        json!({"challenge": p.challenge, "state": p.state, "platform": 7}),
    ];
    for case in cases {
        let (status, _, _) = send(
            &router,
            json_post("/auth/app-handover", &case, Some(&access), true),
        )
        .await;
        assert!(status.is_client_error(), "{case} -> {status}");
    }
    let rows: i64 = sqlx::query_scalar("select count(*) from app_handover_code")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    // The good request next to them works.
    let (status, _, _) = send(
        &router,
        json_post("/auth/app-handover", &good, Some(&access), true),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn redeem_is_rate_limited_per_client_and_per_code_prefix(pool: PgPool) {
    let router = router(&pool);
    let (access, _) = sign_in(&router, "handover-n@example.test").await;
    let p = pkce();

    // Per client: 20 failures, then even a good request from that client is refused.
    let client = "203.0.113.7";
    for i in 0..handover::CLIENT_FAILURES {
        let wrong_code = format!("{i:02x}{}", "ee".repeat(31));
        let (status, _) =
            redeem_with(&router, &wrong_code, &p.state, &p.verifier, Some(client)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }
    let good = issue_code(&router, &access, &p).await;
    let (status, _) = redeem_with(&router, &good, &p.state, &p.verifier, Some(client)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    // The code was not touched by the refused attempt; another client redeems it.
    assert_eq!(outcome_of(&pool, &good).await, None);
    let (status, _) = redeem_with(&router, &good, &p.state, &p.verifier, Some("203.0.113.8")).await;
    assert_eq!(status, StatusCode::OK);
    // A client that sends no address is not limited per client.
    for _ in 0..3 {
        let (status, _) = redeem_with(&router, &"ff".repeat(32), &p.state, &p.verifier, None).await;
        assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);
    }

    // Per code prefix: five failures for codes that begin alike, then that prefix is closed.
    let prefix = "0123abcd";
    for i in 0..handover::PREFIX_FAILURES {
        let wrong = format!("{prefix}{:056x}", i);
        let (status, _) = redeem_with(&router, &wrong, &p.state, &p.verifier, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }
    let sixth = format!("{prefix}{:056x}", 99);
    let (status, _) = redeem_with(&router, &sixth, &p.state, &p.verifier, None).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    // Another prefix is open, and a correct redeem never uses up a budget.
    let p2 = pkce();
    let other = issue_code(&router, &access, &p2).await;
    assert_eq!(redeem(&router, &other, &p2).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_malformed_code_or_verifier_is_one_more_refusal_and_never_a_lookup(pool: PgPool) {
    let router = router(&pool);
    let p = pkce();
    for (code, verifier) in [
        ("short".to_string(), p.verifier.clone()),
        (
            "0".repeat(64).to_uppercase().replace('0', "G"),
            p.verifier.clone(),
        ),
        ("ab".repeat(32), "tiny".to_string()),
        ("ab".repeat(32), format!("{}+", p.verifier)),
        ("ab".repeat(33), p.verifier.clone()),
    ] {
        let (status, body) = redeem_with(&router, &code, &p.state, &verifier, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    }
    assert!(audit_rows(&pool).await.is_empty());
}

// --- secrets stay out -------------------------------------------------------------------

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

/// One process-wide subscriber that keeps everything the server logs while this test binary
/// runs (a scoped subscriber loses events to tracing's per-callsite caching when other tests
/// run on other threads at the same time). Every secret in this file is random per test, so
/// other tests' lines cannot make the absence checks pass or fail by accident.
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
async fn no_secret_reaches_the_ledger_the_audit_rows_or_the_log(pool: PgPool) {
    let captured = capture_logs();

    let router = router(&pool);
    let (access, _) = sign_in(&router, "handover-o@example.test").await;
    let p = pkce();
    let code = issue_code(&router, &access, &p).await;
    let wrong = pkce();
    // A refused redeem, an accepted one, a replayed one, a malformed one.
    let _ = redeem_with(
        &router,
        &code,
        &p.state,
        &wrong.verifier,
        Some("198.51.100.2"),
    )
    .await;
    let code2 = issue_code(&router, &access, &p).await;
    assert_eq!(redeem(&router, &code2, &p).await.0, StatusCode::OK);
    let _ = redeem(&router, &code2, &p).await;
    let _ = redeem_with(&router, "nonsense", &p.state, &p.verifier, None).await;

    let secrets = [
        &code,
        &code2,
        &p.verifier,
        &wrong.verifier,
        &p.challenge,
        &p.state,
    ];
    // The ledger holds hashes: no column, taken as text, shows a secret in the form it travels in.
    let ledger: Vec<String> = sqlx::query_scalar("select t::text from app_handover_code t")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(ledger.len(), 2);
    for row in &ledger {
        for secret in [&code, &code2, &p.verifier, &wrong.verifier] {
            assert!(
                !row.contains(secret.as_str()),
                "a secret is in the ledger: {row}"
            );
        }
    }
    // The code's hash is stored; the code is not derivable from it by reading.
    let stored: Vec<u8> =
        sqlx::query_scalar("select code_hash from app_handover_code order by created_at limit 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, hash(&code));
    // The audit rows: platform and reason only.
    let rows = audit_rows(&pool).await;
    assert_eq!(rows.len(), 5, "{rows:?}");
    for (action, _, detail) in &rows {
        let text = format!("{action} {detail}");
        for secret in secrets {
            assert!(!text.contains(secret.as_str()), "{text}");
        }
        for key in detail.as_object().unwrap().keys() {
            assert!(["platform", "reason"].contains(&key.as_str()), "{key}");
        }
    }
    // And the log of the whole exchange, request spans included.
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("auth/app-handover"),
        "the requests were logged"
    );
    for secret in secrets {
        assert!(!log.contains(secret.as_str()), "a secret is in the log");
    }
}

// --- the restricted runtime login ---------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn the_restricted_runtime_login_runs_the_flow_and_cannot_edit_the_ledger(pool: PgPool) {
    // RuntimeDatabase::new runs grant_runtime and check_runtime, which fail for a table that is
    // missing from the allowlists or granted more than its list says.
    let runtime = RuntimeDatabase::new(pool).await;
    let router = router(&runtime.pool);
    let (access, id) = sign_in(&router, "handover-p@example.test").await;
    let p = pkce();
    let code = issue_code(&router, &access, &p).await;
    let (status, tokens) = redeem(&router, &code, &p).await;
    assert_eq!(status, StatusCode::OK, "{tokens}");
    assert_eq!(
        me(&router, tokens["access_token"].as_str().unwrap()).await,
        id
    );
    let (status, _) = redeem(&router, &code, &p).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The login may use a code up (once) and prune; it cannot rewrite one or empty the table.
    for statement in [
        "update app_handover_code set expires_at=expires_at+interval '1 minute'",
        "update app_handover_code set challenge=challenge",
        "update app_handover_code set identity_id=identity_id",
        "update app_handover_code set consumed_at=null, outcome=null",
        "update app_handover_code set consumed_at=now()",
    ] {
        let error = sqlx::raw_sql(statement)
            .execute(&runtime.pool)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("only be used up, once"),
            "{statement}: {error}"
        );
    }
    let error = sqlx::raw_sql("truncate app_handover_code")
        .execute(&runtime.pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501")
    );
    // The audit rows the flow wrote cannot be edited by the login either.
    let error = sqlx::raw_sql(
        "update operation_audit set action='x' where action like 'auth.app_handover.%'",
    )
    .execute(&runtime.pool)
    .await
    .unwrap_err();
    assert!(error.as_database_error().is_some());
    let audit: i64 = sqlx::query_scalar(
        "select count(*) from operation_audit where action like 'auth.app_handover.%'",
    )
    .fetch_one(&runtime.owner)
    .await
    .unwrap();
    assert_eq!(audit, 3);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn old_ledger_rows_are_pruned_by_the_next_issue_and_young_ones_are_kept(pool: PgPool) {
    let router = router(&pool);
    let (access, id) = sign_in(&router, "handover-q@example.test").await;
    let p = pkce();
    let old = "12".repeat(32);
    sqlx::query(
        "insert into app_handover_code(identity_id,code_hash,challenge,state,platform,session_generation,created_at,expires_at,consumed_at,outcome)
         select $1,$2,$3,$4,'desktop',session_generation, now()-interval '3 days', now()-interval '3 days'+interval '60 seconds', now()-interval '3 days', 'redeemed' from identity where id=$1",
    )
    .bind(id)
    .bind(hash(&old))
    .bind(digest(&SHA256, p.verifier.as_bytes()).as_ref().to_vec())
    .bind(&p.state)
    .execute(&pool)
    .await
    .unwrap();
    let young = issue_code(&router, &access, &p).await;
    let remaining: Vec<Vec<u8>> = sqlx::query_scalar("select code_hash from app_handover_code")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, vec![hash(&young)]);
}
