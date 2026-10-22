//! Native-app sign-in hand-over (docs/decisions/017-native-app-sign-in-handover.md).
//!
//! The Dynodoc app cannot be signed in by an emailed link or by Google: both finish in the
//! system browser, which shares no session with the app's web view. So the app asks the
//! browser to do the sign-in and receive a **one-time hand-over code**, and redeems it from
//! inside its own web view (RFC 8252 for native apps, with PKCE as in RFC 7636):
//!
//! 1. The app makes a random `verifier` (kept in memory only) and a random `state`, and opens
//!    the browser at the web app's `/app-signin` with `challenge = base64url(SHA-256(verifier))`.
//! 2. The person, signed in in the browser, confirms on a page. The web server then calls
//!    `POST /auth/app-handover` with the person's own access token and the trusted web
//!    server's service key. A code is made: 256 random bits, stored **only as its SHA-256**,
//!    bound to the account, the challenge, the state and the platform, valid for 60 seconds,
//!    usable once. At most [`MAX_OUTSTANDING`] unused codes exist per account.
//! 3. The browser hands the code and the state to the app by a `dynodoc://signin` link. A
//!    program that intercepts that link gets a code that is useless without the verifier,
//!    which never leaves the app.
//! 4. The app opens the web app's `/app-signin/finish` with the code, the state and the verifier
//!    in the URL fragment; the page posts them to its own server, which calls
//!    `POST /auth/app-handover/redeem` (service key). The row is locked, checked (live,
//!    unused, the account still signed in at the same generation, `SHA-256(verifier)` equal to
//!    the challenge and the state equal to the one issued, both in constant time) and used up in ONE transaction, so two simultaneous
//!    redeems cannot both win. A redeem that fails while the code is live **burns** it: an
//!    interceptor gets one guess at a 256-bit verifier, and costs the person one retry.
//!    A session is then made exactly as a magic-link sign-in makes one.
//!
//! Every refusal of a redeem is the same `401` (it says nothing about which check failed;
//! the reason is in the audit row). `auth.app_handover.issued`, `.redeemed` and `.refused`
//! rows in `operation_audit` carry the platform and the reason and never a code, a verifier,
//! a challenge or a state. Time is judged only by the database clock. No part of this module
//! logs a request body.

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroize;

use super::routes::{mint_session_with, require_service, TokenResponse};
use super::{random_token, sha256, AuthContext};
use crate::error::ApiError;
use crate::AppState;

/// A code lives this long. The database clock alone decides.
pub const CODE_TTL_SECONDS: i64 = 60;
/// Unused, unexpired codes one account may hold at once.
pub const MAX_OUTSTANDING: i64 = 3;
/// Failed redeems one client address may make in [`WINDOW_MINUTES`].
pub const CLIENT_FAILURES: i32 = 20;
/// Failed redeems one code prefix (the first eight characters) may see in [`WINDOW_MINUTES`].
pub const PREFIX_FAILURES: i32 = 5;
/// The length of both rate windows.
pub const WINDOW_MINUTES: i32 = 15;
/// How much of a code the per-prefix limit looks at.
const PREFIX_LEN: usize = 8;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/app-handover", post(issue))
        .route("/auth/app-handover/redeem", post(redeem))
}

// --- shapes ---------------------------------------------------------------------------

fn is_in(text: &str, min: usize, max: usize, allowed: impl Fn(u8) -> bool) -> bool {
    (min..=max).contains(&text.len()) && text.bytes().all(allowed)
}

fn base64url(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// The app's anti-forgery value: 16 to 64 characters of the URL-safe alphabet.
pub fn valid_state(state: &str) -> bool {
    is_in(state, 16, 64, base64url)
}

/// A hand-over code as this module makes it: 64 lower-case hexadecimal digits (256 bits).
pub fn valid_code(code: &str) -> bool {
    code.len() == 64 && code.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// An RFC 7636 verifier: 43 to 128 characters of the unreserved set.
pub fn valid_verifier(verifier: &str) -> bool {
    is_in(verifier, 43, 128, |b| {
        b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
    })
}

/// `base64url(SHA-256(verifier))`, 43 characters with no padding, in its one canonical
/// spelling. Returns the 32 raw bytes.
pub fn parse_challenge(challenge: &str) -> Option<[u8; 32]> {
    if challenge.len() != 43 || !challenge.bytes().all(base64url) {
        return None;
    }
    let raw: [u8; 32] = URL_SAFE_NO_PAD
        .decode(challenge.as_bytes())
        .ok()?
        .try_into()
        .ok()?;
    // Strict decoding already refuses stray bits; the re-encoding makes "one spelling" explicit.
    (URL_SAFE_NO_PAD.encode(raw) == challenge).then_some(raw)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    Desktop,
    Android,
    Ios,
}

impl Platform {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "desktop" => Some(Self::Desktop),
            "android" => Some(Self::Android),
            "ios" => Some(Self::Ios),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }
}

fn no_store<T: IntoResponse>(body: T) -> Response {
    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn bad_request() -> ApiError {
    ApiError::BadRequest {
        reason: "invalid hand-over request".into(),
    }
}

/// One row of `operation_audit`. `detail` never holds a secret.
async fn audit(
    connection: &mut PgConnection,
    actor: Uuid,
    action: &str,
    resource: Uuid,
    detail: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into operation_audit(actor_id,action,resource_id,detail) values($1,$2,$3,$4)",
    )
    .bind(actor)
    .bind(action)
    .bind(resource)
    .bind(detail)
    .execute(connection)
    .await?;
    Ok(())
}

// --- issue ----------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueRequest {
    /// `base64url(SHA-256(verifier))`, 43 characters.
    challenge: String,
    /// The app's anti-forgery value, 16 to 64 characters of `A-Za-z0-9_-`.
    state: String,
    /// `desktop`, `android` or `ios`.
    platform: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct IssueResponse {
    /// The one-time code: 64 lower-case hexadecimal digits. Shown once, never stored.
    code: String,
    /// Seconds the code stays valid.
    expires_in: i64,
}

/// Make a one-time hand-over code for the signed-in account. `[web server only: service key
/// and the person's own account token]`. The code is bound to the PKCE challenge and the
/// state, valid for 60 seconds, usable once; an account holds at most three unused codes.
#[utoipa::path(
    operation_id = "app_handover_issue",
    post, path = "/auth/app-handover", request_body = IssueRequest, security(("paseto" = [])),
    responses(
        (status = 200, description = "A code, valid for 60 seconds, usable once", body = IssueResponse),
        (status = 400, description = "Challenge, state or platform malformed"),
        (status = 401, description = "No valid account token"),
        (status = 403, description = "Not the trusted web server"),
        (status = 429, description = "The account already holds three unused codes")
    ),
    tag = "auth"
)]
pub async fn issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth: AuthContext,
    Json(req): Json<IssueRequest>,
) -> Result<Response, ApiError> {
    require_service(&state, &headers)?;
    let challenge = parse_challenge(&req.challenge).ok_or_else(bad_request)?;
    let platform = Platform::parse(&req.platform).ok_or_else(bad_request)?;
    if !valid_state(&req.state) {
        return Err(bad_request());
    }
    let (code, hash) = random_token()?;

    let mut tx = state.pool.begin().await?;
    // Serialise this account's issuance (and a sign-out, which locks the same row), so the
    // outstanding count cannot be raced and a code cannot outlive a sign-out-everywhere.
    let generation: Option<i64> = sqlx::query_scalar(
        "select session_generation from identity where id=$1 and disabled_at is null and session_generation=$2 for update",
    )
    .bind(auth.identity_id)
    .bind(auth.session_generation)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(generation) = generation else {
        return Err(ApiError::Unauthorized);
    };
    let outstanding: i64 = sqlx::query_scalar(
        "select count(*) from app_handover_code where identity_id=$1 and consumed_at is null and expires_at > now()",
    )
    .bind(auth.identity_id)
    .fetch_one(&mut *tx)
    .await?;
    if outstanding >= MAX_OUTSTANDING {
        // At most one such row per account per minute, so a script pressing the button cannot
        // fill the audit trail (it is an append-only table).
        let recent: bool = sqlx::query_scalar(
            "select exists(select 1 from operation_audit where actor_id=$1 and action='auth.app_handover.refused'
               and created_at > now()-interval '1 minute' and detail->>'reason'='too_many_outstanding')",
        )
        .bind(auth.identity_id)
        .fetch_one(&mut *tx)
        .await?;
        if !recent {
            audit(
                &mut tx,
                auth.identity_id,
                "auth.app_handover.refused",
                auth.identity_id,
                json!({"platform": platform.as_str(), "reason": "too_many_outstanding"}),
            )
            .await?;
        }
        tx.commit().await?;
        return Err(ApiError::RateLimited {
            retry_after: CODE_TTL_SECONDS as u64,
        });
    }
    let id: Uuid = sqlx::query_scalar(
        "insert into app_handover_code(identity_id,code_hash,challenge,state,platform,session_generation,issued_session,expires_at)
         values($1,$2,$3,$4,$5,$6,$7, now()+make_interval(secs => $8)) returning id",
    )
    .bind(auth.identity_id)
    .bind(&hash)
    .bind(&challenge[..])
    .bind(&req.state)
    .bind(platform.as_str())
    .bind(generation)
    .bind(auth.session_id)
    .bind(CODE_TTL_SECONDS as f64)
    .fetch_one(&mut *tx)
    .await?;
    audit(
        &mut tx,
        auth.identity_id,
        "auth.app_handover.issued",
        id,
        json!({"platform": platform.as_str()}),
    )
    .await?;
    tx.commit().await?;
    Ok(no_store(Json(IssueResponse {
        code,
        expires_in: CODE_TTL_SECONDS,
    })))
}

// --- redeem ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RedeemRequest {
    /// The code from the browser, 64 lower-case hexadecimal digits.
    code: String,
    /// The state the app sent when it started and received back with the code.
    state: String,
    /// The app's PKCE verifier, 43 to 128 characters of `A-Za-z0-9-._~`.
    verifier: String,
    /// The kind of app the web server saw the request come from, read from the app's
    /// `User-Agent` token (`desktop`, `android` or `ios`). It must equal the platform the code
    /// was asked for; a request with no such token never reaches this route.
    app: String,
    /// The visitor's network address as the web server sees it, when that tells visitors
    /// apart (behind a proxy that hides it, the web server leaves it out). Only its hash is
    /// kept, in a rate window.
    #[serde(default)]
    client: Option<String>,
}

// The three values of a redeem are secrets in memory for as long as the request lives; they are
// wiped when it is dropped (best effort: the framework's own buffers are not ours to wipe).
impl Drop for RedeemRequest {
    fn drop(&mut self) {
        self.code.zeroize();
        self.state.zeroize();
        self.verifier.zeroize();
    }
}

/// Equality of two byte strings: different lengths are different (lengths here are public:
/// 32-byte hashes, or the bounded state), and equal lengths are compared by `subtle`, which does
/// not stop at the first difference. (An HMAC-key comparison would zero-pad, and make
/// `b"abc"` equal `b"abc\0"`.)
fn constant_time_equal(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

/// A rate-window key: the hash of a namespace and a value, so the two kinds cannot collide
/// with each other or with the magic-link windows (which hash an email address).
fn window_key(kind: &str, value: &str) -> Vec<u8> {
    sha256(format!("dynodoc/app-handover/{kind}/{value}").as_bytes())
}

async fn over_budget(state: &AppState, key: &[u8], limit: i32) -> Result<bool, sqlx::Error> {
    let attempts: Option<i32> = sqlx::query_scalar(
        "select attempts from auth_request_window where key_hash=$1 and started_at >= now()-make_interval(mins => $2)",
    )
    .bind(key)
    .bind(WINDOW_MINUTES)
    .fetch_optional(&state.pool)
    .await?;
    Ok(attempts.unwrap_or(0) >= limit)
}

async fn record_failure(state: &AppState, key: &[u8]) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into auth_request_window(key_hash) values($1)
         on conflict(key_hash) do update set
           attempts=case when auth_request_window.started_at < now()-make_interval(mins => $2) then 1 else auth_request_window.attempts+1 end,
           started_at=case when auth_request_window.started_at < now()-make_interval(mins => $2) then now() else auth_request_window.started_at end",
    )
    .bind(key)
    .bind(WINDOW_MINUTES)
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// What a redeem needs to know about the code it was given.
#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    identity_id: Uuid,
    challenge: Vec<u8>,
    state: String,
    platform: String,
    /// Not used up yet.
    unused: bool,
    /// Not expired, by the database clock.
    fresh: bool,
    /// The account is active and still at the generation the code was issued at.
    account: bool,
    generation: i64,
}

enum Redeemed {
    Session(Box<TokenResponse>),
    Refused,
}

/// Check and use up a code, and make the session, in ONE transaction: the code is used up, the
/// login exists and the audit row names it, or none of the three happened.
async fn consume(
    state: &AppState,
    code: &str,
    app_state: &str,
    verifier: &str,
    app: &str,
) -> Result<Redeemed, ApiError> {
    let mut tx = state.pool.begin().await?;
    // The row lock makes simultaneous redeems of one code take turns: the second sees it used.
    let row: Option<Row> = sqlx::query_as(
        "select h.id, h.identity_id, h.challenge, h.state, h.platform,
                h.consumed_at is null as unused, h.expires_at > now() as fresh,
                exists(select 1 from identity i where i.id=h.identity_id and i.disabled_at is null and i.session_generation=h.session_generation) as account,
                h.session_generation as generation
         from app_handover_code h where h.code_hash=$1 for update of h",
    )
    .bind(sha256(code.as_bytes()))
    .fetch_optional(&mut *tx)
    .await?;
    // Nothing to attribute an unknown code to, so no audit row; the rate windows count it.
    let Some(Row {
        id,
        identity_id: identity,
        challenge,
        state: stored_state,
        platform,
        unused,
        fresh,
        account,
        generation,
    }) = row
    else {
        return Ok(Redeemed::Refused);
    };
    // Always computed, in constant time, whatever the other checks said.
    let verifier_ok = constant_time_equal(&sha256(verifier.as_bytes()), &challenge);
    let state_ok = constant_time_equal(stored_state.as_bytes(), app_state.as_bytes());
    let app_ok = constant_time_equal(platform.as_bytes(), app.as_bytes());
    if unused && fresh && account && verifier_ok && state_ok && app_ok {
        sqlx::query(
            "update app_handover_code set consumed_at=now(), outcome='redeemed' where id=$1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let (tokens, session) = mint_session_with(state, &mut *tx, identity, generation).await?;
        audit(
            &mut tx,
            identity,
            "auth.app_handover.redeemed",
            id,
            json!({"platform": platform, "session_id": session}),
        )
        .await?;
        tx.commit().await?;
        return Ok(Redeemed::Session(Box::new(tokens)));
    }
    let reason = if !unused {
        "used"
    } else if !fresh {
        "expired"
    } else if !account {
        "account"
    } else if !verifier_ok {
        "verifier"
    } else if !state_ok {
        "state"
    } else {
        "app"
    };
    if unused {
        // A failed redeem uses the code up (committed, not rolled back).
        sqlx::query("update app_handover_code set consumed_at=now(), outcome='burned' where id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    audit(
        &mut tx,
        identity,
        "auth.app_handover.refused",
        id,
        json!({"platform": platform, "reason": reason}),
    )
    .await?;
    tx.commit().await?;
    Ok(Redeemed::Refused)
}

/// Trade a hand-over code and its PKCE verifier for a session, exactly as a magic-link
/// sign-in makes one. `[web server only: service key]`; the app's own credentials are the
/// code and the verifier, so no account token is read. Every refusal is the same `401`.
#[utoipa::path(
    operation_id = "app_handover_redeem",
    post, path = "/auth/app-handover/redeem", request_body = RedeemRequest,
    responses(
        (status = 200, description = "A new login for the account that issued the code", body = super::routes::TokenResponse),
        (status = 401, description = "Refused; the same answer for every reason, a malformed code included"),
        (status = 403, description = "Not the trusted web server"),
        (status = 429, description = "Too many failed attempts from this address or for this code prefix")
    ),
    tag = "auth"
)]
pub async fn redeem(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RedeemRequest>,
) -> Result<Response, ApiError> {
    require_service(&state, &headers)?;
    let client = req
        .client
        .as_deref()
        .filter(|c| !c.is_empty() && c.len() <= 64)
        .map(|c| window_key("client", c));
    let shaped = valid_code(&req.code)
        && valid_state(&req.state)
        && valid_verifier(&req.verifier)
        && Platform::parse(&req.app).is_some();
    let prefix = shaped.then(|| window_key("prefix", &req.code[..PREFIX_LEN]));
    let budgets = [
        (client.as_deref(), CLIENT_FAILURES),
        (prefix.as_deref(), PREFIX_FAILURES),
    ];
    for (key, limit) in budgets {
        if let Some(key) = key {
            if over_budget(&state, key, limit).await? {
                return Err(ApiError::RateLimited {
                    retry_after: (WINDOW_MINUTES * 60) as u64,
                });
            }
        }
    }
    let outcome = if shaped {
        consume(&state, &req.code, &req.state, &req.verifier, &req.app).await?
    } else {
        Redeemed::Refused
    };
    match outcome {
        Redeemed::Session(tokens) => Ok(no_store(Json(*tokens))),
        Redeemed::Refused => {
            for (key, _) in budgets {
                if let Some(key) = key {
                    record_failure(&state, key).await?;
                }
            }
            Err(ApiError::Unauthorized)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 7636 appendix B.
    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    #[test]
    fn the_rfc_example_pair_matches() {
        let raw = parse_challenge(CHALLENGE).expect("canonical challenge");
        assert_eq!(sha256(VERIFIER.as_bytes()), raw.to_vec());
        assert!(valid_verifier(VERIFIER));
    }

    #[test]
    fn challenges_have_one_spelling() {
        assert!(parse_challenge(CHALLENGE).is_some());
        // The last character of a 43-character base64url string carries only four bits.
        let tweaked = format!("{}{}", &CHALLENGE[..42], "N");
        assert!(parse_challenge(&tweaked).is_none());
        for bad in [
            "",
            "short",
            &CHALLENGE[..42],
            &format!("{CHALLENGE}="),
            &format!("{CHALLENGE}A"),
            &CHALLENGE.replace('-', "+"),
            &CHALLENGE.replace('M', " "),
            &CHALLENGE.replace('M', "\n"),
        ] {
            assert!(parse_challenge(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn states_codes_and_verifiers_are_bounded() {
        assert!(valid_state(&"a".repeat(16)) && valid_state(&"a-_Z9".repeat(12)));
        for bad in [
            "a".repeat(15),
            "a".repeat(65),
            format!("{}!", "a".repeat(20)),
            format!("{}/", "a".repeat(20)),
            format!("{}\n", "a".repeat(20)),
            format!("{}é", "a".repeat(20)),
        ] {
            assert!(!valid_state(&bad), "{bad:?}");
        }
        assert!(valid_code(&"0f".repeat(32)));
        for bad in [
            "0F".repeat(32),
            "0f".repeat(31),
            "0f".repeat(33),
            format!("{}g", "0f".repeat(31) + "0"),
            String::new(),
        ] {
            assert!(!valid_code(&bad), "{bad:?}");
        }
        assert!(valid_verifier(&"A.~_-".repeat(9)));
        for bad in [
            "a".repeat(42),
            "a".repeat(129),
            format!("{}+", "a".repeat(43)),
            format!("{}=", "a".repeat(43)),
            format!("{} ", "a".repeat(43)),
        ] {
            assert!(!valid_verifier(&bad), "{bad:?}");
        }
    }

    #[test]
    fn platforms_are_exactly_three() {
        for (text, platform) in [
            ("desktop", Platform::Desktop),
            ("android", Platform::Android),
            ("ios", Platform::Ios),
        ] {
            assert_eq!(Platform::parse(text), Some(platform));
            assert_eq!(platform.as_str(), text);
        }
        for bad in ["", "Desktop", "iOS", "windows", "android ", "web"] {
            assert_eq!(Platform::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn comparison_is_exact() {
        let a = sha256(b"a");
        assert!(constant_time_equal(&a, &a.clone()));
        assert!(!constant_time_equal(&a, &sha256(b"b")));
        let mut flipped = a.clone();
        flipped[31] ^= 1;
        assert!(!constant_time_equal(&a, &flipped));
        assert!(!constant_time_equal(&a, &a[..31]));
        assert!(!constant_time_equal(&a, &[]));
        // Zero-padding must not make two different values equal (an HMAC key comparison did).
        assert!(!constant_time_equal(b"abc", b"abc\0"));
        assert!(!constant_time_equal(b"abc\0", b"abc"));
        assert!(!constant_time_equal(b"", b"\0"));
        assert!(!constant_time_equal(b"abc\0\0", b"abc\0"));
        assert!(constant_time_equal(b"", b""));
        assert!(constant_time_equal(b"abc\0", b"abc\0"));
    }

    #[test]
    fn window_keys_do_not_collide_across_kinds() {
        assert_ne!(window_key("client", "x"), window_key("prefix", "x"));
        assert_ne!(window_key("client", "x"), sha256(b"x"));
    }
}
