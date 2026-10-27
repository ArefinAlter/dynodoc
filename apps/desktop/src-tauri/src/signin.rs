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
//!    replaces the old), and it is good for [`ATTEMPT_TTL`] (ten minutes) by BOTH clocks: the
//!    monotonic one, which does not run while a phone sleeps, and the wall clock, which does
//!    (see [`Moment`]). Nothing is written to disk, so an app the system ends meanwhile simply has
//!    to start again. [`pairing_code`] gives four characters of the state that the launcher shows
//!    while the attempt waits and the server's page shows too, so a person can tell the request
//!    they started from one another program on the device started.
//! 3. The browser comes back with `dynodoc://signin?code=..&state=..` (see `deeplink.rs`).
//!    [`Pending::complete`] accepts it only when an attempt is pending, fresh, for the server
//!    now configured, and the link's state equals the attempt's state (compared without
//!    stopping at the first difference). An accepted link USES THE ATTEMPT UP: a second copy of
//!    the same link finds nothing pending and does nothing. A link with the wrong state changes
//!    nothing (it cannot be told apart from noise, and it must not be able to cancel the
//!    person's own attempt).
//! 4. The result, [`Finish::fragment`], reaches the server's `/app-signin/finish` page as an
//!    address FRAGMENT (which no server or proxy ever receives), but not in the address the window
//!    is sent to: the window loads the plain page first, and once it has finished loading the
//!    shell runs [`finish_script`] (`finish_hash.js`), which sets `location.hash` from an
//!    [`Outbox`]. The script first checks that the page really is the finish page of the server
//!    the attempt was started for, and waits for the page to say it is ready (the page ignores any
//!    fragment that was there when it loaded). So the secrets are in no address the web view
//!    requests, keeps in its history or writes to a log (Android's web view logs the address of
//!    the page next to every console message, and that log showed a verifier when the address held
//!    it). The fragment is handed over once, to that page only, for the server it was made for,
//!    within [`DELIVERY_TTL`].
//!
//! Nothing here prints a secret: [`Secret`] has no `Display`, no `Clone`, and a `Debug` that shows
//! nothing, no type holding one derives `Debug`, and every buffer that held one is overwritten
//! when it is dropped ([`wipe`]; best effort: the copies the web view makes of a script it runs
//! cannot be reached, and a write cannot be forced to stay without `unsafe`, which this crate does
//! not use, so `black_box` is what asks the optimiser to keep it).

use crate::handoff::same_text;
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// How long a started attempt waits for the browser to come back.
pub const ATTEMPT_TTL: Duration = Duration::from_secs(10 * 60);
/// How long a finished attempt's fragment waits for the finish page to load.
pub const DELIVERY_TTL: Duration = Duration::from_secs(30);
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

/// Whether this build puts the `DynodocApp/<version> (<kind>)` token into its web view's
/// User-Agent, which the server's finish page and sign-in routes require (decision 017). Only
/// where that is built AND has been run (Windows' WebView2 and Android's WebView); elsewhere the
/// browser sign-in is not offered, because it would only be refused.
pub const APP_TOKEN: bool = cfg!(any(windows, target_os = "android"));

/// The version as the token writes it. ONE grammar for the shell (this function), the Android
/// layer (`AppToken.kt`) and the server (`appKind` in `frontend/researcher/src/lib/app-signin.ts`),
/// with fixtures all three run (`apps/desktop/fixtures/app-token.json`): `<major>.<minor>.<patch>`,
/// each one to nine ASCII digits. The leading dotted triple of `raw` is kept and anything after it
/// is dropped on purpose (`0.1.0-beta.1` and `10.0.0+build5` give `0.1.0` and `10.0.0`); with no
/// such triple the answer is `0.0.0`. So a token is never one the server silently refuses.
pub fn normalise_version(raw: &str) -> String {
    const NONE: &str = "0.0.0";
    let mut rest = raw;
    let mut parts = [""; 3];
    for (i, slot) in parts.iter_mut().enumerate() {
        let end = rest
            .bytes()
            .position(|b| !b.is_ascii_digit())
            .unwrap_or(rest.len());
        if end == 0 || end > 9 {
            return NONE.to_string();
        }
        *slot = &rest[..end];
        rest = &rest[end..];
        if i < 2 {
            match rest.strip_prefix('.') {
                Some(after) => rest = after,
                None => return NONE.to_string(),
            }
        }
    }
    format!("{}.{}.{}", parts[0], parts[1], parts[2])
}

/// The token appended to the web view's own User-Agent: `DynodocApp/<version> (<kind>)`.
pub fn app_token() -> String {
    format!(
        "DynodocApp/{} ({PLATFORM})",
        normalise_version(env!("CARGO_PKG_VERSION"))
    )
}

/// A moment read from both clocks. The monotonic clock cannot be set, but on a phone it stops
/// while the device sleeps (Android's `CLOCK_MONOTONIC` does not count suspend), so a ten minute
/// attempt could be much older than that. The wall clock keeps running, but can be set. What has
/// passed between two moments is the LARGER of what the two clocks say; a clock that appears to
/// run backwards counts as no time having passed.
#[derive(Clone, Copy)]
pub struct Moment {
    mono: Instant,
    wall: SystemTime,
}

impl Moment {
    pub fn now() -> Self {
        Self {
            mono: Instant::now(),
            wall: SystemTime::now(),
        }
    }

    /// Time since `earlier`, by whichever clock says more.
    pub fn since(self, earlier: Moment) -> Duration {
        let mono = self.mono.saturating_duration_since(earlier.mono);
        let wall = self
            .wall
            .duration_since(earlier.wall)
            .unwrap_or(Duration::ZERO);
        mono.max(wall)
    }

    #[cfg(test)]
    pub fn from_parts(mono: Instant, wall: SystemTime) -> Self {
        Self { mono, wall }
    }

    #[cfg(test)]
    pub fn plus(self, by: Duration) -> Self {
        Self {
            mono: self.mono + by,
            wall: self.wall + by,
        }
    }
}

impl fmt::Debug for Moment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Moment(..)")
    }
}

/// Overwrite `bytes` with zeros in a way the optimiser is asked to keep.
pub fn wipe_bytes(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        *b = 0;
    }
    std::hint::black_box(&*bytes);
}

/// Overwrite the text's bytes with zeros, and leave it empty.
pub fn wipe(text: &mut String) {
    let mut bytes = std::mem::take(text).into_bytes();
    wipe_bytes(&mut bytes);
}

/// Text that must not be printed. No `Display`, no `Clone`; overwritten when dropped.
pub struct Secret(String);

impl Secret {
    pub fn new(text: String) -> Self {
        Self(text)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        wipe(&mut self.0);
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
        wipe_bytes(&mut group);
    }
    out
}

/// 32 symbols without 0, 1, I and O, so that a person can read four of them out and compare.
const PAIRING_ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// Four characters derived from the state (the low five bits of the first four bytes of its
/// SHA-256): shown in the launcher while the attempt waits, and by the server's confirmation page
/// (`pairingCode` in `frontend/researcher/src/lib/server/app-signin.ts`, the same function with the
/// same test vectors). They tell the person's own request from one another program on the device
/// started; they are not a secret and prove nothing to a server.
pub fn pairing_code(state: &str) -> String {
    let digest = Sha256::digest(state.as_bytes());
    digest[..4]
        .iter()
        .map(|b| char::from(PAIRING_ALPHABET[usize::from(b & 31)]))
        .collect()
}

/// What the browser is told: the public half of an attempt.
pub struct Request {
    pub challenge: String,
    /// Sent to the server and echoed back in the app link. Not a credential on its own, but kept
    /// out of every log and overwritten when dropped.
    pub state: String,
}

impl Drop for Request {
    fn drop(&mut self) {
        wipe(&mut self.state);
    }
}

/// A started sign-in: the secrets of one trip to the browser.
pub struct Attempt {
    verifier: Secret,
    state: Secret,
    /// The server (its origin text) the attempt was started for. The verifier is only ever
    /// shown to that server.
    origin: String,
    started: Moment,
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
        now: Moment,
        mut fill: impl FnMut(&mut [u8]) -> bool,
    ) -> Option<(Self, Request)> {
        let mut verifier = [0u8; VERIFIER_BYTES];
        let mut state = [0u8; STATE_BYTES];
        let filled = fill(&mut verifier) && fill(&mut state);
        let made = filled.then(|| (base64url(&verifier), base64url(&state)));
        wipe_bytes(&mut verifier);
        wipe_bytes(&mut state);
        let (verifier, state) = made?;
        let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
        let request = Request {
            challenge,
            state: state.clone(),
        };
        let attempt = Self {
            verifier: Secret::new(verifier),
            state: Secret::new(state),
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
    /// The JSON body of the server's redeem route (`POST /auth/app-signin/redeem`): exactly the
    /// code, the state and the verifier, all URL-safe characters (the code was checked by the
    /// listener, the other two were made here), so nothing needs escaping.
    pub fn redeem_body(&self) -> Secret {
        Secret::new(format!(
            "{{\"code\":\"{}\",\"state\":\"{}\",\"verifier\":\"{}\"}}",
            self.code.expose(),
            self.state.expose(),
            self.verifier.expose()
        ))
    }

    /// `code=..&state=..&verifier=..`: the three values are URL-safe characters only, so
    /// nothing needs encoding.
    pub fn fragment(&self) -> Secret {
        Secret::new(format!(
            "code={}&state={}&verifier={}",
            self.code.expose(),
            self.state.expose(),
            self.verifier.expose()
        ))
    }
}

/// What the outbox holds: the fragment, the server it was made for and when.
struct Parcel {
    fragment: Secret,
    origin: String,
    put: Moment,
}

/// A fragment waiting for the finish page to load: handed over once, fresh, to that page only,
/// and only if the server now configured is still the one it was made for.
#[derive(Default)]
pub struct Outbox(Mutex<Option<Parcel>>);

impl fmt::Debug for Outbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Outbox(..)")
    }
}

impl Outbox {
    /// Keep `fragment` for the finish page of `origin`, replacing anything older.
    pub fn put(&self, fragment: Secret, origin: &str, now: Moment) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(Parcel {
                fragment,
                origin: origin.to_string(),
                put: now,
            });
        }
    }

    /// The fragment, once, if it is still fresh and `origin` (the server configured now) is the
    /// one it was made for. Whatever is there is gone afterwards, whatever the answer.
    pub fn take(&self, now: Moment, origin: &str) -> Option<Secret> {
        let parcel = self.0.lock().ok()?.take()?;
        let fresh = now.since(parcel.put) < DELIVERY_TTL;
        (fresh && same_text(&parcel.origin, origin)).then_some(parcel.fragment)
    }

    /// Drop anything waiting (a new attempt, or a change of server, makes it meaningless).
    pub fn clear(&self) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = None;
        }
    }
}

/// The script the shell runs in the finish page: the template in `finish_hash.js`.
const FINISH_HASH: &str = include_str!("finish_hash.js");
const ORIGIN_PLACEHOLDER: &str = "__ORIGIN__";
const FRAGMENT_PLACEHOLDER: &str = "__FRAGMENT__";

/// An origin as the shell writes it (`https://host`, `http://host:port`, `https://[::1]:8443`):
/// only characters that cannot end a script string or start anything else.
fn plain_origin(origin: &str) -> bool {
    (origin.starts_with("https://") || origin.starts_with("http://"))
        && origin.len() <= 300
        && origin.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'/' | b'[' | b']')
        })
}

/// The script that hands the fragment to the finish page of `origin`. `None` unless `fragment` is
/// exactly `code=<64 hex>&state=<16 to 64 base64url>&verifier=<43 to 128 base64url>` (the shapes
/// the web app and the API accept) and `origin` is a plain origin, so nothing but those
/// characters can ever be put into a script. The script itself (`finish_hash.js`) checks the
/// page's origin and path before it touches anything, and waits for the page to be ready.
pub fn finish_script(fragment: &str, origin: &str) -> Option<Secret> {
    if !plain_origin(origin) {
        return None;
    }
    let mut parts = fragment.split('&');
    let (code, state, verifier) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let value = |part: &str, name: &str, min: usize, max: usize, hex_only: bool| {
        let text = part.strip_prefix(name)?.strip_prefix('=')?;
        let fits = (min..=max).contains(&text.len())
            && text.bytes().all(|b| {
                if hex_only {
                    matches!(b, b'0'..=b'9' | b'a'..=b'f')
                } else {
                    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
                }
            });
        fits.then_some(())
    };
    value(code, "code", 64, 64, true)?;
    value(state, "state", 16, 64, false)?;
    value(verifier, "verifier", 43, 128, false)?;
    Some(Secret::new(
        FINISH_HASH
            .replace(ORIGIN_PLACEHOLDER, origin)
            .replace(FRAGMENT_PLACEHOLDER, fragment),
    ))
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

    /// Forget the attempt (the server was changed: its verifier is not kept for another one).
    pub fn clear(&self) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = None;
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
        now: Moment,
        origin: &str,
        code: &str,
        state: &str,
    ) -> Result<Finish, Refusal> {
        let mut slot = self.0.lock().map_err(|_| Refusal::NothingPending)?;
        let Some(attempt) = slot.as_ref() else {
            return Err(Refusal::NothingPending);
        };
        if now.since(attempt.started) >= ATTEMPT_TTL {
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
        let Attempt {
            verifier, state, ..
        } = attempt;
        Ok(Finish {
            code: Secret::new(code.to_string()),
            state,
            verifier,
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

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn started(now: Moment) -> (Pending, Request) {
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
        let (attempt, request) = Attempt::new(ORIGIN, Moment::now(), counting()).unwrap();
        assert_eq!(
            request.challenge,
            base64url(&Sha256::digest(attempt.verifier.expose().as_bytes()))
        );
    }

    #[test]
    fn the_secrets_have_the_agreed_sizes_and_characters() {
        let (attempt, request) = Attempt::new(ORIGIN, Moment::now(), counting()).unwrap();
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
        let now = Moment::now();
        let a = Attempt::new(ORIGIN, now, os_random).unwrap();
        let b = Attempt::new(ORIGIN, now, os_random).unwrap();
        assert_ne!(a.0.verifier.expose(), b.0.verifier.expose());
        assert_ne!(a.1.state, b.1.state);
        assert_ne!(a.1.challenge, b.1.challenge);
        assert_ne!(a.0.verifier.expose(), a.0.state.expose());
    }

    #[test]
    fn no_randomness_means_no_attempt() {
        assert!(Attempt::new(ORIGIN, Moment::now(), |_| false).is_none());
        // Even if only the second request for randomness fails.
        let mut calls = 0;
        assert!(Attempt::new(ORIGIN, Moment::now(), |buffer| {
            calls += 1;
            buffer.fill(1);
            calls < 2
        })
        .is_none());
    }

    #[test]
    fn a_returning_link_finishes_the_attempt_once() {
        let t0 = Moment::now();
        let (pending, request) = started(t0);
        assert!(pending.is_waiting());
        let finish = pending
            .complete(t0.plus(secs(30)), ORIGIN, CODE, &request.state)
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
                .complete(t0.plus(secs(31)), ORIGIN, CODE, &request.state)
                .unwrap_err(),
            Refusal::NothingPending
        );
    }

    #[test]
    fn nothing_pending_means_nothing_happens() {
        let pending = Pending::default();
        assert_eq!(
            pending
                .complete(Moment::now(), ORIGIN, CODE, "anything")
                .unwrap_err(),
            Refusal::NothingPending
        );
    }

    #[test]
    fn a_wrong_state_is_refused_and_leaves_the_attempt_alone() {
        let t0 = Moment::now();
        let (pending, request) = started(t0);
        let now = t0.plus(secs(5));
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
        let t0 = Moment::now();
        // One tick before the limit: still good.
        let (pending, request) = started(t0);
        assert!(pending
            .complete(
                t0.plus(ATTEMPT_TTL - Duration::from_nanos(1)),
                ORIGIN,
                CODE,
                &request.state
            )
            .is_ok());
        // At the limit: gone, and gone for good (the right state does not bring it back).
        let (pending, request) = started(t0);
        assert_eq!(
            pending
                .complete(t0.plus(ATTEMPT_TTL), ORIGIN, CODE, &request.state)
                .unwrap_err(),
            Refusal::Expired
        );
        assert!(!pending.is_waiting());
        assert_eq!(
            pending
                .complete(t0.plus(secs(1)), ORIGIN, CODE, &request.state)
                .unwrap_err(),
            Refusal::NothingPending
        );
        // Long after, with a wrong state: expiry is judged first, so nothing is learned.
        let (pending, _) = started(t0);
        assert_eq!(
            pending
                .complete(t0.plus(secs(86_400)), ORIGIN, CODE, "wrong")
                .unwrap_err(),
            Refusal::Expired
        );
        assert_eq!(ATTEMPT_TTL, secs(600));
    }

    #[test]
    fn a_phone_that_slept_does_not_extend_an_attempt() {
        // The monotonic clock stopped while the device slept (it barely moved), the wall clock did
        // not: the attempt is as old as the larger of the two says.
        let mono0 = Instant::now();
        let wall0 = SystemTime::now();
        let t0 = Moment::from_parts(mono0, wall0);
        let (pending, request) = started(t0);
        let slept = Moment::from_parts(mono0 + secs(30), wall0 + ATTEMPT_TTL + secs(1));
        assert_eq!(
            pending
                .complete(slept, ORIGIN, CODE, &request.state)
                .unwrap_err(),
            Refusal::Expired,
            "ten minutes of wall clock with half a minute of monotonic clock"
        );
        // And the other way round: the wall clock set back does not shorten it, the monotonic
        // clock still counts.
        let (pending, request) = started(t0);
        let set_back = Moment::from_parts(mono0 + ATTEMPT_TTL, wall0 - secs(3600));
        assert_eq!(
            pending
                .complete(set_back, ORIGIN, CODE, &request.state)
                .unwrap_err(),
            Refusal::Expired
        );
        // Each clock alone, just inside the limit, is still good.
        let (pending, request) = started(t0);
        let inside = Moment::from_parts(mono0 + ATTEMPT_TTL - secs(1), wall0 + secs(5));
        assert!(pending
            .complete(inside, ORIGIN, CODE, &request.state)
            .is_ok());
    }

    #[test]
    fn a_clock_that_runs_backwards_is_not_an_expiry() {
        let t0 = Moment::now().plus(secs(1000));
        let (pending, request) = started(t0);
        let before = Moment::from_parts(t0.mono - secs(500), t0.wall - secs(500));
        assert!(pending
            .complete(before, ORIGIN, CODE, &request.state)
            .is_ok());
        assert_eq!(before.since(t0), Duration::ZERO);
    }

    #[test]
    fn a_new_attempt_replaces_the_old_one() {
        let t0 = Moment::now();
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
        let t0 = Moment::now();
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
    fn clearing_forgets_the_attempt() {
        let t0 = Moment::now();
        let (pending, request) = started(t0);
        pending.clear();
        assert!(!pending.is_waiting());
        assert_eq!(
            pending
                .complete(t0, ORIGIN, CODE, &request.state)
                .unwrap_err(),
            Refusal::NothingPending
        );
    }

    #[test]
    fn nothing_prints_a_secret() {
        let t0 = Moment::now();
        let (attempt, request) = Attempt::new(ORIGIN, t0, os_random).unwrap();
        let verifier = attempt.verifier.expose().to_string();
        let state = request.state.clone();
        let pending = Pending::default();
        let shown = format!("{attempt:?} {:?} {:#?}", pending, attempt.verifier);
        pending.start(attempt);
        let finish = pending.complete(t0, ORIGIN, CODE, &state).unwrap();
        let fragment = finish.fragment();
        let outbox = Outbox::default();
        outbox.put(finish.fragment(), ORIGIN, t0);
        let shown = format!(
            "{shown} {pending:?} {finish:?} {fragment:?} {:#?} {:?} {outbox:?} {t0:?}",
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
            "pub struct Outbox",
            "struct Parcel",
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
        // No unsafe anywhere: the overwrite is plain stores that `black_box` asks to keep.
        assert!(!production.contains("unsafe {") && !production.contains("unsafe fn"));
    }

    #[test]
    fn every_buffer_that_held_a_secret_is_overwritten() {
        let mut bytes = *b"verifier-bytes-verifier-bytes!!!";
        wipe_bytes(&mut bytes);
        assert_eq!(bytes, [0u8; 32]);
        let mut text = String::from("a-secret-state-value");
        wipe(&mut text);
        assert!(text.is_empty());
        // Source check: the holders are wiped when dropped, and the randomness buffers after use.
        let source = include_str!("signin.rs");
        let production = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        let drop_of = |holder: &str| {
            let at = production
                .find(&format!("impl Drop for {holder}"))
                .unwrap_or_else(|| panic!("no Drop for {holder}"));
            &production[at..at + 140]
        };
        for holder in ["Secret", "Request"] {
            assert!(drop_of(holder).contains("wipe("), "{holder}");
        }
        let new = &production[production.find("pub fn new(\n        origin").unwrap()..];
        let new = &new[..new.find("Some((attempt, request))").unwrap()];
        assert!(new.contains("wipe_bytes(&mut verifier);"));
        assert!(new.contains("wipe_bytes(&mut state);"));
    }

    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const STATE: &str = "b3JkZXJseS1zdGF0ZS12YWx1ZQ";

    fn fragment() -> String {
        format!("code={CODE}&state={STATE}&verifier={VERIFIER}")
    }

    #[test]
    fn the_pairing_code_is_a_fixed_function_of_the_state() {
        // The same vectors as the server's confirmation page (src/lib/server/app-signin.test.ts).
        for (state, want) in [
            ("b3JkZXJseS1zdGF0ZS12YWx1ZQ", "4K77"),
            (&"a".repeat(16), "EDCG"),
            ("K8710daSV_AjCvmPElN6z6L-DQ2YusRS", "6CR6"),
            ("0123456789abcdef0123456789abcdef", "YKX5"),
            (&"g".repeat(64), "GLJA"),
        ] {
            assert_eq!(pairing_code(state), want, "{state}");
        }
        // Only symbols a person cannot confuse, and the app's own attempts follow it.
        for i in 0..500 {
            let code = pairing_code(&format!("state-{i}-{}", "x".repeat(16)));
            assert_eq!(code.len(), 4);
            assert!(
                code.bytes()
                    .all(|b| b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ".contains(&b)),
                "{code}"
            );
        }
        let (_, request) = Attempt::new(ORIGIN, Moment::now(), os_random).unwrap();
        assert_eq!(pairing_code(&request.state).len(), 4);
    }

    #[test]
    fn the_outbox_hands_the_fragment_over_once_and_only_while_fresh() {
        let t0 = Moment::now();
        let outbox = Outbox::default();
        assert!(
            outbox.take(t0, ORIGIN).is_none(),
            "nothing put, nothing taken"
        );
        outbox.put(Secret::new(fragment()), ORIGIN, t0);
        let taken = outbox.take(t0.plus(secs(1)), ORIGIN).expect("fresh");
        assert_eq!(taken.expose(), fragment());
        assert!(outbox.take(t0.plus(secs(1)), ORIGIN).is_none(), "once");
        // One tick before the limit; at the limit; and an expired one is gone for good.
        outbox.put(Secret::new(fragment()), ORIGIN, t0);
        assert!(outbox
            .take(t0.plus(DELIVERY_TTL - Duration::from_nanos(1)), ORIGIN)
            .is_some());
        outbox.put(Secret::new(fragment()), ORIGIN, t0);
        assert!(outbox.take(t0.plus(DELIVERY_TTL), ORIGIN).is_none());
        assert!(
            outbox.take(t0, ORIGIN).is_none(),
            "an expired fragment is not kept"
        );
        // A newer put replaces an older one; clear drops it; a backwards clock is no expiry.
        outbox.put(Secret::new("old".into()), ORIGIN, t0);
        outbox.put(Secret::new(fragment()), ORIGIN, t0);
        assert_eq!(outbox.take(t0, ORIGIN).unwrap().expose(), fragment());
        outbox.put(Secret::new(fragment()), ORIGIN, t0);
        outbox.clear();
        assert!(outbox.take(t0, ORIGIN).is_none());
        let later = t0.plus(secs(100));
        outbox.put(Secret::new(fragment()), ORIGIN, later);
        assert!(outbox.take(t0, ORIGIN).is_some());
        assert_eq!(DELIVERY_TTL, secs(30));
        // Nothing prints it.
        outbox.put(Secret::new(fragment()), ORIGIN, t0);
        assert!(!format!("{outbox:?}").contains(CODE));
    }

    #[test]
    fn a_fragment_made_for_one_server_is_never_handed_to_another() {
        let t0 = Moment::now();
        let outbox = Outbox::default();
        for other in [
            "https://dynodoc.example.org",
            "https://app.dynodoc.online:8443",
            "http://app.dynodoc.online",
            "https://APP.dynodoc.online",
            "https://app.dynodoc.online/",
            "",
        ] {
            outbox.put(Secret::new(fragment()), ORIGIN, t0);
            assert!(outbox.take(t0, other).is_none(), "{other:?}");
            // Asking was enough: it is gone, even for the right server.
            assert!(outbox.take(t0, ORIGIN).is_none(), "{other:?}");
        }
        outbox.put(Secret::new(fragment()), ORIGIN, t0);
        assert!(outbox.take(t0, ORIGIN).is_some());
    }

    #[test]
    fn the_delivery_window_counts_the_larger_clock_too() {
        let mono0 = Instant::now();
        let wall0 = SystemTime::now();
        let outbox = Outbox::default();
        outbox.put(
            Secret::new(fragment()),
            ORIGIN,
            Moment::from_parts(mono0, wall0),
        );
        // The device slept: half a second of monotonic time, a minute of wall time.
        let slept = Moment::from_parts(mono0 + Duration::from_millis(500), wall0 + secs(60));
        assert!(outbox.take(slept, ORIGIN).is_none());
    }

    #[test]
    fn the_script_that_hands_over_the_fragment_is_made_only_of_the_template_and_the_three_values() {
        let script = finish_script(&fragment(), ORIGIN).unwrap();
        let script = script.expose();
        assert_eq!(
            script,
            FINISH_HASH
                .replace(ORIGIN_PLACEHOLDER, ORIGIN)
                .replace(FRAGMENT_PLACEHOLDER, &fragment())
        );
        assert!(!script.contains(ORIGIN_PLACEHOLDER));
        assert!(!script.contains(FRAGMENT_PLACEHOLDER));
        assert!(script.contains(&format!("var origin = '{ORIGIN}';")));
        assert!(script.contains(&format!("var fragment = '{}';", fragment())));
        // The template has each placeholder exactly once, as a whole single-quoted string.
        assert_eq!(FINISH_HASH.matches(ORIGIN_PLACEHOLDER).count(), 1);
        assert_eq!(FINISH_HASH.matches(FRAGMENT_PLACEHOLDER).count(), 1);
        assert!(FINISH_HASH.contains("var origin = '__ORIGIN__';"));
        assert!(FINISH_HASH.contains("var fragment = '__FRAGMENT__';"));
        // Whatever the values are made of, they are in no way executable: only the characters
        // the three values use can be in the script, and every other shape is refused.
        for bad in [
            String::new(),
            format!("code={CODE}"),
            format!("code={CODE}&state={STATE}"),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}&x=1"),
            format!("state={STATE}&code={CODE}&verifier={VERIFIER}"),
            format!("code={CODE}&verifier={VERIFIER}&state={STATE}"),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}\";alert(1);\""),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}'"),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}\n"),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}+"),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}%22"),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}<"),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}\\"),
            format!("code={}\"&state={STATE}&verifier={VERIFIER}", &CODE[1..]),
            format!(
                "code={}&state={STATE}&verifier={VERIFIER}",
                CODE.to_uppercase()
            ),
            format!("code={}0&state={STATE}&verifier={VERIFIER}", CODE),
            format!("code={CODE}&state=short&verifier={VERIFIER}"),
            format!("code={CODE}&state={}&verifier={VERIFIER}", "s".repeat(65)),
            format!("code={CODE}&state={STATE}&verifier={}", "v".repeat(42)),
            format!("code={CODE}&state={STATE}&verifier={}", "v".repeat(129)),
            format!("Code={CODE}&state={STATE}&verifier={VERIFIER}"),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}\u{2028}"),
            format!("code={CODE}&state={STATE}&verifier={VERIFIER}\u{e9}"),
        ] {
            assert!(finish_script(&bad, ORIGIN).is_none(), "{bad:?}");
        }
        // The longest accepted values still fit; the result has no quote but the template's own.
        let longest = format!(
            "code={CODE}&state={}&verifier={}",
            "s".repeat(64),
            "v".repeat(128)
        );
        let script = finish_script(&longest, ORIGIN).unwrap();
        assert_eq!(
            script.expose().matches('\'').count(),
            FINISH_HASH.matches('\'').count()
        );
        assert!(script.expose().is_ascii());
    }

    #[test]
    fn a_verifier_of_valid_characters_that_looks_like_script_is_refused_by_the_charset_check_alone()
    {
        // 43 characters, the minimum length, so the length rule passes; what is wrong is only the
        // characters: a quote, a semicolon and the letter x that would close the string and start
        // a statement. Every position is tried, so a check that only looked at the first or last
        // character would be caught.
        let pieces = [
            "\";x;\"",
            "';x;'",
            "\\\";x;",
            "`;x;`",
            "\u{0}\";x;\"",
            "</script>",
            "${x}",
        ];
        for piece in pieces {
            for at in 0..=(43 - piece.chars().count().min(43)) {
                let mut verifier: Vec<char> = "A".repeat(43).chars().collect();
                for (i, c) in piece.chars().enumerate() {
                    if at + i < verifier.len() {
                        verifier[at + i] = c;
                    }
                }
                let verifier: String = verifier.into_iter().collect();
                let fragment = format!("code={CODE}&state={STATE}&verifier={verifier}");
                assert!(
                    finish_script(&fragment, ORIGIN).is_none(),
                    "{piece:?} at {at}"
                );
            }
        }
        // The same checks on the other two values, and on the origin.
        let bad_state = format!(
            "code={CODE}&state={}&verifier={VERIFIER}",
            "A\";x;\"A".repeat(3)
        );
        assert!(finish_script(&bad_state, ORIGIN).is_none());
        for origin in [
            "https://app.dynodoc.online\";x;\"",
            "https://app.dynodoc.online';x;'",
            "https://app.dynodoc.online\n",
            "https://app.dynodoc.online\\",
            "https://app dynodoc.online",
            "ftp://app.dynodoc.online",
            "javascript:alert(1)",
            "app.dynodoc.online",
            "",
        ] {
            assert!(finish_script(&fragment(), origin).is_none(), "{origin:?}");
        }
        // And the plain forms of an origin the app writes all pass.
        for origin in [
            "https://app.dynodoc.online",
            "http://127.0.0.1:8080",
            "https://[2001:db8::1]:8443",
            "http://localhost:5173",
        ] {
            assert!(finish_script(&fragment(), origin).is_some(), "{origin:?}");
        }
    }

    #[test]
    fn the_template_checks_the_page_before_it_touches_the_address() {
        // The behaviour is run in a Node realm by scripts/finish-hash.test.mjs; this keeps the
        // pieces that must never be dropped from the text.
        let code: String = FINISH_HASH
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for needed in [
            "window.top !== window",
            "location.origin !== origin",
            "location.pathname !== '/app-signin/finish'",
            "location.search !== ''",
            "location.hash !== ''",
            "data-finish-ready",
            "location.hash = fragment;",
        ] {
            assert!(code.contains(needed), "finish_hash.js lost: {needed}");
        }
        // Nothing but the hash is written, and nothing is read back from the page or sent out.
        assert_eq!(code.matches("location.hash =").count(), 1);
        for banned in [
            "fetch(",
            "XMLHttpRequest",
            "postMessage",
            "eval(",
            "Function(",
            "innerHTML",
            "__TAURI",
            "invoke",
            "document.cookie",
            "localStorage",
            "console.",
            "location.href",
            "location.assign",
            "location.replace",
            "window.open",
        ] {
            assert!(!code.contains(banned), "finish_hash.js uses {banned}");
        }
    }

    #[test]
    fn the_finished_attempts_own_fragment_is_a_valid_script() {
        let t0 = Moment::now();
        let (pending, request) = started(t0);
        let finish = pending.complete(t0, ORIGIN, CODE, &request.state).unwrap();
        assert!(finish_script(finish.fragment().expose(), ORIGIN).is_some());
    }

    #[test]
    fn the_platform_is_one_of_three_words() {
        assert!(["desktop", "android", "ios"].contains(&PLATFORM));
        if cfg!(not(any(target_os = "android", target_os = "ios"))) {
            assert_eq!(PLATFORM, "desktop");
        }
    }

    #[test]
    fn the_version_follows_the_one_grammar_the_fixtures_pin() {
        // The same cases the Kotlin layer and the web app run (apps/desktop/fixtures/app-token.json).
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../fixtures/app-token.json")).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() > 15);
        for case in cases {
            let raw = case["raw"].as_str().unwrap();
            assert_eq!(
                normalise_version(raw),
                case["version"].as_str().unwrap(),
                "{raw:?}"
            );
        }
        // The two long cases every language generates for itself.
        assert_eq!(
            normalise_version(&format!("1.2.3-{}", "x".repeat(5000))),
            "1.2.3"
        );
        assert_eq!(
            normalise_version(&format!("{}.0.0", "9".repeat(400))),
            "0.0.0"
        );
        // Whatever comes out is in the grammar the server reads.
        for case in cases {
            let version = normalise_version(case["raw"].as_str().unwrap());
            let parts: Vec<&str> = version.split('.').collect();
            assert_eq!(parts.len(), 3, "{version}");
            assert!(parts
                .iter()
                .all(|p| (1..=9).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit())));
        }
    }

    #[test]
    fn the_app_token_is_what_the_server_reads() {
        // `DynodocApp/<major.minor.patch> (<kind>)`, exactly what the web tier's `appKind` accepts.
        let token = app_token();
        let (name, rest) = token.split_once('/').unwrap();
        assert_eq!(name, "DynodocApp");
        let (version, kind) = rest.split_once(" (").unwrap();
        let numbers: Vec<&str> = version.split('.').collect();
        assert_eq!(numbers.len(), 3, "{token}");
        assert!(numbers
            .iter()
            .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())));
        assert_eq!(kind, format!("{PLATFORM})"));
        assert_eq!(version, normalise_version(env!("CARGO_PKG_VERSION")));
        // The leading triple of this build's version is what the token carries; a pre-release
        // suffix such as `-beta` is dropped on purpose (see `normalise_version`).
        let triple = env!("CARGO_PKG_VERSION").split(['-', '+']).next().unwrap();
        assert_eq!(normalise_version(env!("CARGO_PKG_VERSION")), triple);
        // Only where it is built and has been run is the browser sign-in offered.
        assert_eq!(APP_TOKEN, cfg!(any(windows, target_os = "android")));
    }
}
