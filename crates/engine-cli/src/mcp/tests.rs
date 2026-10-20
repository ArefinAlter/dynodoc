//! The device-session client of `mcp` against a loopback server that plays the two
//! issuance routes. Most tests ask for `history:read`, which needs no agent key; the last
//! ones ask for the real scopes of an agent key, read and write.
use super::*;
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use ring::rand::SystemRandom;
use std::sync::{
    atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
};

const PROJECT: Uuid = Uuid::from_u128(0x1111_1111_2222_4333_8444_5555_5555_5555);
const AUTHORITY: Uuid = Uuid::from_u128(0x7777_7777_2222_4333_8444_5555_5555_5555);

struct Mock {
    challenges: AtomicUsize,
    sessions: AtomicUsize,
    bad_signatures: AtomicUsize,
    bearer_seen: AtomicBool,
    challenge_status: AtomicU16,
    expires_in: AtomicU64,
    /// The installation the challenge claims to come from.
    authority: Mutex<Uuid>,
    /// Refuse (`403`) every challenge that asks for a write scope, as the service does for a
    /// key that was not granted them.
    refuse_writes: AtomicBool,
    /// The scopes of each challenge asked for, in order.
    asked: Mutex<Vec<Vec<String>>>,
}

async fn challenge(
    State(mock): State<Arc<Mock>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::response::Response {
    mock.challenges.fetch_add(1, Ordering::SeqCst);
    if headers.contains_key(header::AUTHORIZATION) {
        mock.bearer_seen.store(true, Ordering::SeqCst);
    }
    let asked: Vec<String> = serde_json::from_value(body["scopes"].clone()).unwrap();
    let writes = asked
        .iter()
        .any(|s| s == "content:propose" || s == "content:submit");
    // The service signs the sorted set (`normalize_scopes`); the client compares against it.
    let mut sorted = asked.clone();
    sorted.sort();
    mock.asked.lock().unwrap().push(asked);
    let status = mock.challenge_status.load(Ordering::SeqCst);
    if status != 200 {
        return StatusCode::from_u16(status).unwrap().into_response();
    }
    if writes && mock.refuse_writes.load(Ordering::SeqCst) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let challenge = SessionChallenge {
        version: 1,
        id: Uuid::new_v4(),
        authority: *mock.authority.lock().unwrap(),
        public_key: body["public_key"].as_str().unwrap().to_owned(),
        project: PROJECT,
        scopes: sorted,
        nonce: "ab".repeat(32),
        issued_at_ms: 1_000_000,
        expires_at_ms: 1_100_000,
        token_ttl_seconds: 3600,
    };
    Json(json!({ "challenge": challenge, "mac": "cd".repeat(32) })).into_response()
}

async fn session(
    State(mock): State<Arc<Mock>>,
    headers: HeaderMap,
    Json(proof): Json<SessionProof>,
) -> axum::response::Response {
    let n = mock.sessions.fetch_add(1, Ordering::SeqCst) + 1;
    if headers.contains_key(header::AUTHORIZATION) {
        mock.bearer_seen.store(true, Ordering::SeqCst);
    }
    if proof.verify_signature().is_err() {
        mock.bad_signatures.fetch_add(1, Ordering::SeqCst);
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({
        "access_token": format!("v4.local.mock-{n}"),
        "expires_in": mock.expires_in.load(Ordering::SeqCst),
    }))
    .into_response()
}

async fn serve() -> (Arc<Mock>, DeviceSession) {
    let mock = Arc::new(Mock {
        challenges: AtomicUsize::new(0),
        sessions: AtomicUsize::new(0),
        bad_signatures: AtomicUsize::new(0),
        bearer_seen: AtomicBool::new(false),
        challenge_status: AtomicU16::new(200),
        expires_in: AtomicU64::new(3600),
        authority: Mutex::new(AUTHORITY),
        refuse_writes: AtomicBool::new(false),
        asked: Mutex::new(Vec::new()),
    });
    let app = Router::new()
        .route("/projects/{id}/device-sessions/challenge", post(challenge))
        .route("/projects/{id}/device-sessions", post(session))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = parse_base(&format!("http://{}/", listener.local_addr().unwrap()), true).unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let session = DeviceSession::new(
        base,
        PROJECT,
        AUTHORITY,
        key,
        vec!["history:read".to_owned()],
    )
    .unwrap();
    (mock, session)
}

#[tokio::test]
async fn one_challenge_and_one_session_serve_many_calls() {
    let (mock, session) = serve().await;
    let first = session.token().await.unwrap();
    let second = session.token().await.unwrap();
    assert_eq!((first.generation(), second.generation()), (1, 1));
    assert_eq!(mock.challenges.load(Ordering::SeqCst), 1);
    assert_eq!(mock.sessions.load(Ordering::SeqCst), 1);
    // The service saw a valid signature, and no bearer on either issuance request.
    assert_eq!(mock.bad_signatures.load(Ordering::SeqCst), 0);
    assert!(!mock.bearer_seen.load(Ordering::SeqCst));
}

#[tokio::test]
async fn a_token_is_replaced_shortly_before_it_expires() {
    let (mock, session) = serve().await;
    // A one-second lifetime renews after 750 ms (a quarter of the lifetime is the margin).
    mock.expires_in.store(1, Ordering::SeqCst);
    assert_eq!(session.token().await.unwrap().generation(), 1);
    assert_eq!(session.token().await.unwrap().generation(), 1);
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(session.token().await.unwrap().generation(), 2);
    assert_eq!(mock.sessions.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_rejected_token_is_replaced_once_for_every_caller() {
    let (mock, mut session) = serve().await;
    session.renew_gap = Duration::ZERO;
    let stale = session.token().await.unwrap();
    let (a, b) = tokio::join!(session.rejected(&stale), session.rejected(&stale));
    assert!(a.is_ok() && b.is_ok());
    assert_eq!(
        mock.sessions.load(Ordering::SeqCst),
        2,
        "one renewal, not two"
    );
    assert_eq!(session.token().await.unwrap().generation(), 2);
    // A rejection of the old token now changes nothing.
    session.rejected(&stale).await.unwrap();
    assert_eq!(mock.sessions.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_token_rejected_the_moment_it_was_issued_is_not_replaced() {
    let (mock, session) = serve().await;
    let fresh = session.token().await.unwrap();
    assert_eq!(session.rejected(&fresh).await, Err(SessionError::Refused));
    assert_eq!(
        mock.challenges.load(Ordering::SeqCst),
        1,
        "no second challenge"
    );
}

#[tokio::test]
async fn the_service_refusing_the_key_is_final_and_a_failing_service_is_not() {
    let (mock, session) = serve().await;
    for (status, expected) in [
        (401, SessionError::Refused),
        (403, SessionError::Refused),
        (404, SessionError::Refused),
        (429, SessionError::Unavailable),
        (408, SessionError::Unavailable),
        (500, SessionError::Unavailable),
        (503, SessionError::Unavailable),
    ] {
        mock.challenge_status.store(status, Ordering::SeqCst);
        assert_eq!(session.token().await.err(), Some(expected), "{status}");
    }
    // It recovers when the service does.
    mock.challenge_status.store(200, Ordering::SeqCst);
    assert!(session.token().await.is_ok());
}

#[tokio::test]
async fn a_challenge_from_another_installation_is_not_signed() {
    let (mock, session) = serve().await;
    *mock.authority.lock().unwrap() = Uuid::from_u128(9);
    assert_eq!(session.token().await.err(), Some(SessionError::Refused));
    assert_eq!(
        mock.sessions.load(Ordering::SeqCst),
        0,
        "nothing was signed or sent"
    );
}

#[tokio::test]
async fn a_lifetime_the_protocol_does_not_allow_is_not_accepted() {
    let (mock, session) = serve().await;
    for expires_in in [0, 3601, 86_400] {
        mock.expires_in.store(expires_in, Ordering::SeqCst);
        assert_eq!(session.token().await.err(), Some(SessionError::Unavailable));
    }
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_is_unavailable() {
    let (_mock, mut session) = serve().await;
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    session.base = parse_base(&format!("http://{}/", closed.local_addr().unwrap()), true).unwrap();
    drop(closed);
    assert_eq!(session.token().await.err(), Some(SessionError::Unavailable));
}

/// The scopes the real command asks for, with a session the way `run` builds it.
fn agent_session(session: DeviceSession, write: bool) -> DeviceSession {
    let owned = |scopes: &[&str]| scopes.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let mut session = session;
    session.scopes = owned(if write { &WRITE_SCOPES } else { &SCOPES });
    if write {
        session = session.with_read_fallback(owned(&SCOPES));
    }
    session
}

#[tokio::test]
async fn without_write_the_session_asks_for_the_read_scope_and_nothing_else() {
    let (mock, session) = serve().await;
    let session = agent_session(session, false);
    let token = session.token().await.unwrap();
    assert_eq!(*mock.asked.lock().unwrap(), [["content:read"]]);
    assert!(
        !token.writes(),
        "a read session does not carry the write scopes"
    );
    // A renewal asks for the same.
    session.rejected(&token).await.ok();
    assert!(mock
        .asked
        .lock()
        .unwrap()
        .iter()
        .all(|asked| asked == &["content:read"]));
}

#[tokio::test]
async fn with_write_the_session_asks_for_the_read_and_both_write_scopes() {
    let (mock, session) = serve().await;
    let session = agent_session(session, true);
    let token = session.token().await.unwrap();
    assert_eq!(
        *mock.asked.lock().unwrap(),
        [["content:read", "content:propose", "content:submit"]]
    );
    assert!(token.writes());
    assert_eq!(mock.sessions.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_refusal_of_the_write_scopes_falls_back_to_the_read_scope_and_stays_there() {
    let (mock, mut session) = serve().await;
    mock.refuse_writes.store(true, Ordering::SeqCst);
    session.renew_gap = Duration::ZERO;
    let session = agent_session(session, true);
    let token = session.token().await.unwrap();
    assert!(!token.writes(), "the write tools must be refused");
    assert_eq!(
        *mock.asked.lock().unwrap(),
        [
            vec!["content:read", "content:propose", "content:submit"],
            vec!["content:read"]
        ]
    );
    // Later calls reuse the token; a renewal asks for the read scope straight away.
    assert_eq!(session.token().await.unwrap().generation(), 1);
    session.rejected(&token).await.unwrap();
    let renewed = session.token().await.unwrap();
    assert_eq!(renewed.generation(), 2);
    assert!(!renewed.writes());
    assert_eq!(mock.asked.lock().unwrap().len(), 3);
    assert_eq!(mock.asked.lock().unwrap()[2], ["content:read"]);
}

#[tokio::test]
async fn a_key_refused_the_read_scope_too_stays_refused() {
    let (mock, session) = serve().await;
    mock.challenge_status.store(403, Ordering::SeqCst);
    let session = agent_session(session, true);
    assert_eq!(session.token().await.err(), Some(SessionError::Refused));
    // A service that merely fails is not a refusal of the write scopes: no fallback.
    mock.challenge_status.store(503, Ordering::SeqCst);
    mock.asked.lock().unwrap().clear();
    assert_eq!(session.token().await.err(), Some(SessionError::Unavailable));
    assert_eq!(mock.asked.lock().unwrap().len(), 1);
}

#[test]
fn the_environment_switches_writing_on_only_for_a_clear_yes() {
    assert!(!write_wanted(None));
    for on in ["1", "on", "ON", "yes", "true", " True "] {
        assert!(write_wanted(Some(on)), "{on}");
    }
    for off in ["", "0", "off", "no", "false", "maybe", "2", "enabled"] {
        assert!(!write_wanted(Some(off)), "{off}");
    }
}
