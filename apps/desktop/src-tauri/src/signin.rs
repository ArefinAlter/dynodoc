//! Signing the app in with the system browser (docs/decisions/017-native-app-sign-in-handover.md).
//!
//! The emailed link and Google both finish in the system browser, which shares no session with
//! this window. So the app asks the browser to sign in and to hand back a one-time code, as RFC
//! 8252 describes for native apps, with PKCE (RFC 7636) so that a code taken from the link by any
//! other program is useless:
//!
//! 1. [`Attempt::new`] makes a random VERIFIER (32 bytes from the operating system, 43
//!    characters) and a random STATE (24 bytes), and the CHALLENGE `base64url(SHA-256(verifier))`.
//!    Only the challenge and the state go to the browser, in the address of the server's
//!    `/app-signin` page. The verifier is kept here, in memory, and nowhere else.
//! 2. [`Pending::start`] keeps the attempt. There is at most ONE per running app (a new one
//!    replaces the old), and it is good for [`ATTEMPT_TTL`] (ten minutes) of monotonic time.
//!    Nothing is written to disk, so an app the system ends meanwhile simply has to start again.
//! 3. The browser comes back with `dynodoc://signin?code=..&state=..` (see `deeplink.rs`).
//!    [`Pending::complete`] accepts it only when an attempt is pending, fresh, for the server
//!    now configured, and the link's state equals the attempt's state (compared without
//!    stopping at the first difference). An accepted link USES THE ATTEMPT UP: a second copy of
//!    the same link finds nothing pending and does nothing. A link with the wrong state changes
//!    nothing (it cannot be told apart from noise, and it must not be able to cancel the
//!    person's own attempt).
//! 4. The result, [`Finish::fragment`], goes on the server's `/app-signin/finish` page as an
//!    address FRAGMENT, which no server, proxy or log ever receives.
//!
//! Nothing here prints a secret: [`Secret`] has no `Display`, no `Clone`, and a `Debug` that shows
//! nothing, and no type holding one derives `Debug`.

use crate::handoff::same_text;
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a started attempt waits for the browser to come back.
pub const ATTEMPT_TTL: Duration = Duration::from_secs(10 * 60);
/// Bytes of operating-system randomness behind the verifier (RFC 7636 asks for 32).
const VERIFIER_BYTES: usize = 32;
/// Bytes behind the state.
const STATE_BYTES: usize = 24;

/// The kind of device this build runs on, as the server's page names it to the person.
pub const PLATFORM: &str = if cfg!(target_os = "android") {
    "android"
} else if cfg!(target_os = "ios") {
    "ios"
} else {
    "desktop"
};

/// Text that must not be printed. No `Display`, no `Clone`.
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

/// Fills `buffer` from the operating system's random source; false if it cannot.
pub fn os_random(buffer: &mut [u8]) -> bool {
    getrandom::fill(buffer).is_ok()
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url without padding (RFC 4648 section 5).
pub fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let mut group = [0u8; 3];
        group[..chunk.len()].copy_from_slice(chunk);
        let n = (u32::from(group[0]) << 16) | (u32::from(group[1]) << 8) | u32::from(group[2]);
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
        }
    }
    out
}

/// What the browser is told: the public half of an attempt.
pub struct Request {
    pub challenge: String,
    /// Sent to the server and echoed back in the app link. Not a credential on its own, but kept
    /// out of every log all the same.
    pub state: String,
}

/// A started sign-in: the secrets of one trip to the browser.
pub struct Attempt {
    verifier: Secret,
    state: Secret,
    /// The server (its origin text) the attempt was started for. The verifier is only ever
    /// shown to that server.
    origin: String,
    started: Instant,
}

impl fmt::Debug for Attempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Attempt(..)")
    }
}

impl Attempt {
    /// Make the secrets. `fill` is the source of randomness (the operating system's in the app,
    /// a fixed one in a test). `None` if it fails: the app never falls back to anything weaker.
    pub fn new(
        origin: &str,
        now: Instant,
        mut fill: impl FnMut(&mut [u8]) -> bool,
    ) -> Option<(Self, Request)> {
        let mut verifier = [0u8; VERIFIER_BYTES];
        let mut state = [0u8; STATE_BYTES];
        if !fill(&mut verifier) || !fill(&mut state) {
            return None;
        }
        let verifier = base64url(&verifier);
        let state = base64url(&state);
        let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
        let request = Request {
            challenge,
            state: state.clone(),
        };
        let attempt = Self {
            verifier: Secret(verifier),
            state: Secret(state),
            origin: origin.to_string(),
            started: now,
        };
        Some((attempt, request))
    }
}

/// Why a returning link did nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No attempt is waiting (none was started, it was used, or the app was started afresh).
    NothingPending,
    /// The attempt waited longer than [`ATTEMPT_TTL`]; it is gone.
    Expired,
    /// The configured server is no longer the one the attempt was started for; it is gone.
    OtherServer,
    /// The link's state is not the attempt's. The attempt stays.
    StateMismatch,
}

/// The code, the state and the verifier, to be put on the finish page's address fragment.
pub struct Finish {
    code: Secret,
    state: Secret,
    verifier: Secret,
}

impl fmt::Debug for Finish {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Finish(..)")
    }
}

impl Finish {
    /// `code=..&state=..&verifier=..`: the three values are URL-safe characters only, so
    /// nothing needs encoding.
    pub fn fragment(&self) -> Secret {
        Secret(format!(
            "code={}&state={}&verifier={}",
            self.code.expose(),
            self.state.expose(),
            self.verifier.expose()
        ))
    }
}

/// The one attempt this running app may have waiting.
#[derive(Default)]
pub struct Pending(Mutex<Option<Attempt>>);

impl fmt::Debug for Pending {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pending(..)")
    }
}

impl Pending {
    /// Keep `attempt`, replacing any older one.
    pub fn start(&self, attempt: Attempt) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(attempt);
        }
    }

    /// Is an attempt waiting? (For tests and the debug log; it says nothing about its secrets.)
    #[cfg(test)]
    pub fn is_waiting(&self) -> bool {
        self.0.lock().is_ok_and(|slot| slot.is_some())
    }

    /// A link came back with `code` and `state`, while the server configured is `origin`. See
    /// the module documentation for the rules; a poisoned lock refuses everything.
    pub fn complete(
        &self,
        now: Instant,
        origin: &str,
        code: &str,
        state: &str,
    ) -> Result<Finish, Refusal> {
        let mut slot = self.0.lock().map_err(|_| Refusal::NothingPending)?;
        let Some(attempt) = slot.as_ref() else {
            return Err(Refusal::NothingPending);
        };
        // A clock that appears to run backwards counts as no time having passed.
        if now.saturating_duration_since(attempt.started) >= ATTEMPT_TTL {
            *slot = None;
            return Err(Refusal::Expired);
        }
        if !same_text(&attempt.origin, origin) {
            *slot = None;
            return Err(Refusal::OtherServer);
        }
        if !same_text(attempt.state.expose(), state) {
            return Err(Refusal::StateMismatch);
        }
        // Used up here, before anything else happens.
        let attempt = slot.take().ok_or(Refusal::NothingPending)?;
        Ok(Finish {
            code: Secret(code.to_string()),
            state: attempt.state,
            verifier: attempt.verifier,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "https://app.dynodoc.online";
    const CODE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// Randomness that counts up: different for each call, the same for each run.
    fn counting() -> impl FnMut(&mut [u8]) -> bool {
        let mut next = 1u8;
        move |buffer| {
            for b in buffer.iter_mut() {
                *b = next;
                next = next.wrapping_add(7);
            }
            true
        }
    }

    fn started(now: Instant) -> (Pending, Request) {
        let (attempt, request) = Attempt::new(ORIGIN, now, counting()).unwrap();
        let pending = Pending::default();
        pending.start(attempt);
        (pending, request)
    }

    #[test]
    fn base64url_matches_the_rfc_vectors() {
        // RFC 4648 section 10, with the padding removed.
        for (input, want) in [
            ("", ""),
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg"),
            ("fooba", "Zm9vYmE"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64url(input.as_bytes()), want, "{input:?}");
        }
        // The two characters that differ from the plain alphabet.
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(base64url(&[0xff; 3]), "____");
        assert_eq!(base64url(&[0; 32]).len(), 43);
    }

    #[test]
    fn the_challenge_is_the_rfc_7636_function_of_the_verifier() {
        // RFC 7636 appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        // And the app's own attempt follows it.
        let (attempt, request) = Attempt::new(ORIGIN, Instant::now(), counting()).unwrap();
        assert_eq!(
            request.challenge,
            base64url(&Sha256::digest(attempt.verifier.expose().as_bytes()))
        );
    }

    #[test]
    fn the_secrets_have_the_agreed_sizes_and_characters() {
        let (attempt, request) = Attempt::new(ORIGIN, Instant::now(), counting()).unwrap();
        let url_safe = |s: &str| {
            s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        // 32 bytes of verifier: 43 characters (RFC 7636 allows 43 to 128).
        assert_eq!(attempt.verifier.expose().len(), 43);
        assert_eq!(request.challenge.len(), 43);
        assert_eq!(attempt.state.expose().len(), 32);
        assert_eq!(request.state, attempt.state.expose());
        for text in [
            attempt.verifier.expose(),
            &request.challenge,
            &request.state,
        ] {
            assert!(url_safe(text), "{text}");
        }
        // The browser is told neither the verifier nor anything it could be derived from.
        assert_ne!(request.challenge, attempt.verifier.expose());
        assert!(!request.state.contains(attempt.verifier.expose()));
    }

    #[test]
    fn the_operating_system_gives_different_secrets_every_time() {
        let now = Instant::now();
        let a = Attempt::new(ORIGIN, now, os_random).unwrap();
        let b = Attempt::new(ORIGIN, now, os_random).unwrap();
        assert_ne!(a.0.verifier.expose(), b.0.verifier.expose());
        assert_ne!(a.1.state, b.1.state);
        assert_ne!(a.1.challenge, b.1.challenge);
        assert_ne!(a.0.verifier.expose(), a.0.state.expose());
    }

    #[test]
    fn no_randomness_means_no_attempt() {
        assert!(Attempt::new(ORIGIN, Instant::now(), |_| false).is_none());
        // Even if only the second request for randomness fails.
        let mut calls = 0;
        assert!(Attempt::new(ORIGIN, Instant::now(), |buffer| {
            calls += 1;
            buffer.fill(1);
            calls < 2
        })
        .is_none());
    }

    #[test]
    fn a_returning_link_finishes_the_attempt_once() {
        let t0 = Instant::now();
        let (pending, request) = started(t0);
        assert!(pending.is_waiting());
        let finish = pending
            .complete(t0 + Duration::from_secs(30), ORIGIN, CODE, &request.state)
            .expect("the right state, in time");
        // The attempt is used up at once.
        assert!(!pending.is_waiting());
        let fragment = finish.fragment();
        let expected_verifier = fragment
            .expose()
            .rsplit_once("&verifier=")
            .map(|(_, v)| v.to_string())
            .unwrap();
        assert_eq!(expected_verifier.len(), 43);
        assert_eq!(
            fragment.expose(),
            format!(
                "code={CODE}&state={}&verifier={expected_verifier}",
                request.state
            )
        );
        // The challenge the browser was given belongs to this verifier.
        assert_eq!(
            request.challenge,
            base64url(&Sha256::digest(expected_verifier.as_bytes()))
        );
        // The very same link again does nothing.
        assert_eq!(
            pending
                .complete(t0 + Duration::from_secs(31), ORIGIN, CODE, &request.state)
                .unwrap_err(),
            Refusal::NothingPending
        );
    }

    #[test]
    fn nothing_pending_means_nothing_happens() {
        let pending = Pending::default();
        assert_eq!(
            pending
                .complete(Instant::now(), ORIGIN, CODE, "anything")
                .unwrap_err(),
            Refusal::NothingPending
        );
    }

    #[test]
    fn a_wrong_state_is_refused_and_leaves_the_attempt_alone() {
        let t0 = Instant::now();
        let (pending, request) = started(t0);
        let now = t0 + Duration::from_secs(5);
        let mut wrong_ones = vec![
            String::new(),
            "x".repeat(32),
            request.state.to_uppercase(),
            request.state.to_lowercase(),
            format!("{}x", request.state),
            request.state[1..].to_string(),
            request.state[..31].to_string(),
        ];
        let mut flipped = request.state.clone().into_bytes();
        flipped[31] = if flipped[31] == b'A' { b'B' } else { b'A' };
        wrong_ones.push(String::from_utf8(flipped).unwrap());
        for wrong in wrong_ones {
            if wrong == request.state {
                continue;
            }
            assert_eq!(
                pending.complete(now, ORIGIN, CODE, &wrong).unwrap_err(),
                Refusal::StateMismatch,
                "{wrong:?}"
            );
            assert!(
                pending.is_waiting(),
                "a wrong state must not cancel the attempt"
            );
        }
        // The person's own return still works.
        assert!(pending.complete(now, ORIGIN, CODE, &request.state).is_ok());
    }

    #[test]
    fn an_attempt_expires_after_ten_minutes_exactly() {
        let t0 = Instant::now();
        // One tick before the limit: still good.
        let (pending, request) = started(t0);
        assert!(pending
            .complete(
                t0 + ATTEMPT_TTL - Duration::from_nanos(1),
                ORIGIN,
                CODE,
                &request.state
            )
            .is_ok());
        // At the limit: gone, and gone for good (the right state does not bring it back).
        let (pending, request) = started(t0);
        assert_eq!(
            pending
                .complete(t0 + ATTEMPT_TTL, ORIGIN, CODE, &request.state)
                .unwrap_err(),
            Refusal::Expired
        );
        assert!(!pending.is_waiting());
        assert_eq!(
            pending
                .complete(t0 + Duration::from_secs(1), ORIGIN, CODE, &request.state)
                .unwrap_err(),
            Refusal::NothingPending
        );
        // Long after, with a wrong state: expiry is judged first, so nothing is learned.
        let (pending, _) = started(t0);
        assert_eq!(
            pending
                .complete(t0 + Duration::from_secs(86_400), ORIGIN, CODE, "wrong")
                .unwrap_err(),
            Refusal::Expired
        );
        assert_eq!(ATTEMPT_TTL, Duration::from_secs(600));
    }

    #[test]
    fn a_clock_that_runs_backwards_is_not_an_expiry() {
        let t0 = Instant::now() + Duration::from_secs(1000);
        let (pending, request) = started(t0);
        let before = t0 - Duration::from_secs(500);
        assert!(pending
            .complete(before, ORIGIN, CODE, &request.state)
            .is_ok());
    }

    #[test]
    fn a_new_attempt_replaces_the_old_one() {
        let t0 = Instant::now();
        let (pending, first) = started(t0);
        let (second_attempt, second) = Attempt::new(ORIGIN, t0, os_random).unwrap();
        pending.start(second_attempt);
        // The first link no longer finds its attempt.
        assert_eq!(
            pending
                .complete(t0, ORIGIN, CODE, &first.state)
                .unwrap_err(),
            Refusal::StateMismatch
        );
        assert!(pending.complete(t0, ORIGIN, CODE, &second.state).is_ok());
    }

    #[test]
    fn the_attempt_belongs_to_the_server_it_was_started_for() {
        let t0 = Instant::now();
        for other in [
            "https://dynodoc.example.org",
            "https://app.dynodoc.online:8443",
            "http://app.dynodoc.online",
            "",
        ] {
            let (pending, request) = started(t0);
            assert_eq!(
                pending
                    .complete(t0, other, CODE, &request.state)
                    .unwrap_err(),
                Refusal::OtherServer,
                "{other:?}"
            );
            // The verifier is not kept for a server it was not made for.
            assert!(!pending.is_waiting());
        }
    }

    #[test]
    fn nothing_prints_a_secret() {
        let t0 = Instant::now();
        let (attempt, request) = Attempt::new(ORIGIN, t0, os_random).unwrap();
        let verifier = attempt.verifier.expose().to_string();
        let state = request.state.clone();
        let pending = Pending::default();
        let shown = format!("{attempt:?} {:?} {:#?}", pending, attempt.verifier);
        pending.start(attempt);
        let finish = pending.complete(t0, ORIGIN, CODE, &state).unwrap();
        let fragment = finish.fragment();
        let shown = format!(
            "{shown} {pending:?} {finish:?} {fragment:?} {:#?} {:?}",
            finish, fragment
        );
        for secret in [
            verifier.as_str(),
            state.as_str(),
            CODE,
            &CODE[..16],
            &state[..12],
            &verifier[..12],
        ] {
            assert!(!shown.contains(secret), "a secret was printed: {shown}");
        }
    }

    #[test]
    fn a_secret_cannot_be_copied_or_printed_by_accident() {
        // Source check: no derive of Debug or Clone on a type that holds a secret, and no
        // Display for any of them.
        let source = include_str!("signin.rs");
        let production = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        for holder in [
            "pub struct Secret",
            "pub struct Request",
            "pub struct Attempt",
            "pub struct Finish",
            "pub struct Pending",
        ] {
            let at = production
                .find(holder)
                .unwrap_or_else(|| panic!("{holder}"));
            let above = &production[at.saturating_sub(80)..at];
            assert!(
                !above.contains("derive(Debug") && !above.contains("derive(Clone"),
                "{holder} derives something that would copy or print it"
            );
        }
        assert!(!production.contains("impl fmt::Display"));
        // The state is compared without stopping at the first difference, never with ==.
        let complete = &production[production.find("pub fn complete(").unwrap()..];
        assert!(complete.contains("same_text(attempt.state.expose(), state)"));
        assert!(!complete.contains("attempt.state.expose() == "));
        assert!(!complete.contains("== attempt.state"));
    }

    #[test]
    fn the_platform_is_one_of_three_words() {
        assert!(["desktop", "android", "ios"].contains(&PLATFORM));
        if cfg!(not(any(target_os = "android", target_os = "ios"))) {
            assert_eq!(PLATFORM, "desktop");
        }
    }
}
