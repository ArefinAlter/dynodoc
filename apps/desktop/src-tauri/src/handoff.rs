//! Handing a link to the system browser or mail program, and the proof that a person asked.
//!
//! The web view tells the shell where a page wants to go but, in WebView2, not whether a
//! person clicked (wry ignores `IsUserInitiated`). So a hostile page can loop
//! `location = 'https://evil/' + n` or `window.open('mailto:...')` to flood the browser or
//! mail program, or aim at `http://localhost` and the local network. Two defences, both
//! pure functions here:
//!
//! * A page-initiated hand-off opens only a PUBLIC `http(s)` address (never a mail link,
//!   loopback, private, link-local or similar host), and at most once every 3 seconds and
//!   five times a minute; the rest are dropped silently.
//! * A person's click is recognised by a script the shell injects (`link_targets.js`): on a
//!   trusted click (`event.isTrusted`) it calls `window.open` with a marker address that
//!   carries a secret only that script knows. Such a request is a "person" hand-off and may
//!   open mail links and local addresses. The secret is made at start-up, lives in the
//!   script's closure and is never given to a page.
//!
//! Limit: a page that gets a person to click something it controls (an overlay) counts as
//! that person's click, as in any browser; and a public name that resolves to a private
//! address (DNS rebinding) cannot be seen here.

use crate::server::MAX_URL_LEN;
use std::collections::VecDeque;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use url::{Host, Url};

/// Who asked for the hand-off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A page's script or a link without a recognised click.
    Page,
    /// The person: a native menu item or a trusted click seen by the injected script.
    Person,
}

/// May `url` be handed to the system at all, for this source?
pub fn external_allowed(url: &Url, source: Source) -> bool {
    if url.as_str().len() > MAX_URL_LEN {
        return false;
    }
    match url.scheme() {
        "http" | "https" => {
            if !url.username().is_empty() || url.password().is_some() {
                return false;
            }
            match source {
                Source::Person => url.host().is_some(),
                Source::Page => url.host().is_some_and(|host| public_host(&host)),
            }
        }
        // A mail link starts the user's mail program: only a person may do that.
        "mailto" => source == Source::Person,
        _ => false,
    }
}

/// Is this a host on the public internet (as far as its spelling shows)? Rejects loopback,
/// private, link-local, shared, reserved and multicast addresses, IPv6 forms that embed an
/// IPv4 address, `localhost` and names under local-network suffixes, and single-label names.
pub fn public_host(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(name) => public_name(name),
        Host::Ipv4(ip) => public_v4(*ip),
        Host::Ipv6(ip) => public_v6(*ip),
    }
}

fn public_name(name: &str) -> bool {
    // "localhost." and "localhost" are the same host.
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if name.is_empty() || !name.contains('.') {
        return false; // a single label is a local-network or intranet name
    }
    const LOCAL_SUFFIXES: [&str; 9] = [
        "localhost",
        "local",
        "internal",
        "lan",
        "intranet",
        "corp",
        "home",
        "private",
        "home.arpa",
    ];
    !LOCAL_SUFFIXES
        .iter()
        .any(|suffix| name == *suffix || name.ends_with(&format!(".{suffix}")))
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0 // 0.0.0.0/8, "this network"
        || (a == 100 && (64..=127).contains(&b)) // 100.64.0.0/10, carrier-grade NAT
        || (a == 192 && b == 0 && c == 0) // 192.0.0.0/24, protocol assignments
        || (a == 198 && (b == 18 || b == 19)) // 198.18.0.0/15, benchmarking
        || a >= 240) // 240.0.0.0/4, reserved
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
        return false;
    }
    // ::ffff:a.b.c.d and ::a.b.c.d carry an IPv4 address.
    if let Some(v4) = ip.to_ipv4() {
        return public_v4(v4);
    }
    let s = ip.segments();
    let octets = ip.octets();
    let embedded = |from: usize| {
        Ipv4Addr::new(
            octets[from],
            octets[from + 1],
            octets[from + 2],
            octets[from + 3],
        )
    };
    if s[0] & 0xfe00 == 0xfc00 || s[0] & 0xffc0 == 0xfe80 || s[0] & 0xffc0 == 0xfec0 {
        return false; // unique local, link-local, deprecated site-local
    }
    if s[0] == 0x2002 {
        return public_v4(embedded(2)); // 6to4
    }
    if s[0] == 0x0064 && s[1] == 0xff9b && s[2..6].iter().all(|&x| x == 0) {
        return public_v4(embedded(12)); // NAT64
    }
    if s[0] == 0x2001 && (s[1] == 0 || s[1] == 0x0db8) {
        return false; // Teredo, documentation
    }
    if s[0] == 0x0100 && s[1..4].iter().all(|&x| x == 0) {
        return false; // discard-only 100::/64
    }
    true
}

/// At most one hand-off every 3 seconds and five in any minute. `now` is passed in so the
/// rule is testable; only accepted hand-offs are recorded.
#[derive(Debug, Default)]
pub struct HandOffLimiter {
    accepted: VecDeque<Instant>,
}

impl HandOffLimiter {
    pub const MIN_GAP: Duration = Duration::from_secs(3);
    pub const WINDOW: Duration = Duration::from_secs(60);
    pub const MAX_IN_WINDOW: usize = 5;

    pub fn allow(&mut self, now: Instant) -> bool {
        while self
            .accepted
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= Self::WINDOW)
        {
            self.accepted.pop_front();
        }
        if let Some(last) = self.accepted.back() {
            if now.saturating_duration_since(*last) < Self::MIN_GAP {
                return false;
            }
        }
        if self.accepted.len() >= Self::MAX_IN_WINDOW {
            return false;
        }
        self.accepted.push_back(now);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandOff {
    Opened,
    /// The address or scheme is not one this source may open.
    Refused,
    /// Allowed in kind, but too soon after the last one.
    RateLimited,
}

/// The whole decision, with the actual opening passed in so tests can watch it. Nothing is
/// opened unless this returns `Opened`.
pub fn hand_off(
    url: &Url,
    source: Source,
    limiter: &Mutex<HandOffLimiter>,
    now: Instant,
    open: impl FnOnce(String),
) -> HandOff {
    if !external_allowed(url, source) {
        return HandOff::Refused;
    }
    if source == Source::Page && !limiter.lock().is_ok_and(|mut l| l.allow(now)) {
        return HandOff::RateLimited;
    }
    open(url.to_string());
    HandOff::Opened
}

/// The marker the injected script opens for a person's click. `.invalid` never resolves.
pub const MARKER_HOST: &str = "gesture.dynodoc.invalid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    /// Not a marker address.
    No,
    /// A marker address that is wrong in any way (bad secret, bad target): drop it.
    Bad,
    /// A valid marker: the person asked to open this.
    Open(Url),
}

/// Read a marker address: `https://gesture.dynodoc.invalid/<secret>?u=<address>`.
pub fn read_marker(url: &Url, secret: &str) -> Marker {
    if url.host_str() != Some(MARKER_HOST) {
        return Marker::No;
    }
    let path_ok = url
        .path()
        .strip_prefix('/')
        .is_some_and(|given| same_text(given, secret));
    if secret.len() < 16 || url.scheme() != "https" || !path_ok || url.fragment().is_some() {
        return Marker::Bad;
    }
    let mut pairs = url.query_pairs();
    let (Some((key, target)), None) = (pairs.next(), pairs.next()) else {
        return Marker::Bad;
    };
    if key != "u" {
        return Marker::Bad;
    }
    match Url::parse(&target) {
        Ok(target) if external_allowed(&target, Source::Person) => Marker::Open(target),
        _ => Marker::Bad,
    }
}

/// Equal text, compared without stopping at the first difference.
fn same_text(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// 128 bits from the operating system's randomness (each `RandomState` is seeded by it).
pub fn new_secret() -> String {
    use std::hash::{BuildHasher, Hasher};
    let part = || {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as u64),
        );
        hasher.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }
    fn page(text: &str) -> bool {
        external_allowed(&url(text), Source::Page)
    }
    fn person(text: &str) -> bool {
        external_allowed(&url(text), Source::Person)
    }

    #[test]
    fn a_page_may_open_only_public_web_addresses() {
        for ok in [
            "https://example.org/",
            "http://example.org:8080/x?y=1#z",
            "https://8.8.8.8/",
            "https://[2606:4700:4700::1111]/",
            "https://sub.dynodoc.online/support",
            "https://xn--bcher-kva.example/",
            "https://flood1.invalid/",
        ] {
            assert!(page(ok), "{ok}");
        }
    }

    #[test]
    fn a_page_may_not_reach_this_machine_or_the_local_network() {
        for bad in [
            "http://localhost/",
            "http://LOCALHOST./",
            "http://localhost:8080/admin",
            "http://foo.localhost/",
            "http://127.0.0.1/",
            "http://127.1/",
            "http://0x7f.0.0.1/",
            "http://2130706433/",
            "http://127.255.255.254/",
            "http://0.0.0.0/",
            "http://0/",
            "http://[::1]/",
            "http://[::]/",
            "http://[::ffff:127.0.0.1]/",
            "http://[::ffff:7f00:1]/",
            "http://[::ffff:10.0.0.1]/",
            "http://[::127.0.0.1]/",
            "http://10.0.0.5/",
            "http://172.16.0.1/",
            "http://172.31.255.255/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://100.64.0.1/",
            "http://192.0.0.8/",
            "http://198.18.0.1/",
            "http://224.0.0.1/",
            "http://255.255.255.255/",
            "http://240.0.0.1/",
            "http://192.0.2.1/",
            "http://[fe80::1]/",
            "http://[fd00::1]/",
            "http://[fc00::1]/",
            "http://[fec0::1]/",
            "http://[ff02::1]/",
            "http://[64:ff9b::7f00:1]/",
            "http://[64:ff9b::a00:1]/",
            "http://[2002:7f00:1::]/",
            "http://[2002:c0a8:101::1]/",
            "http://[2001:db8::1]/",
            "http://[2001::1]/",
            "http://printer/",
            "http://intranet/",
            "http://router.local/",
            "http://nas.lan/",
            "http://wiki.corp/",
            "http://a.b.internal/",
            "http://x.home.arpa/",
            "http://localhost.localdomain.local/",
        ] {
            assert!(!page(bad), "{bad}");
            assert!(person(bad), "a person may open {bad}");
        }
        // Public addresses that merely look close.
        for ok in [
            "https://8.8.4.4/",
            "https://[2a00:1450::1]/",
            "https://localhost.example.org/",
            "https://example.local.org/",
        ] {
            assert!(page(ok), "{ok}");
        }
    }

    #[test]
    fn only_a_person_may_start_the_mail_program() {
        for mail in [
            "mailto:support@dynodoc.online",
            "mailto:a@b?subject=x&body=y",
        ] {
            assert!(!page(mail), "{mail}");
            assert!(person(mail), "{mail}");
        }
    }

    #[test]
    fn nothing_else_is_ever_handed_over() {
        for bad in [
            "javascript:alert(1)",
            "data:text/html,x",
            "file:///C:/Windows/System32/calc.exe",
            "ftp://example.org/",
            "ssh://example.org/",
            "ms-msdt:/id",
            "vscode://file/x",
            "dynodoc://open/workspace",
            "tel:+4420",
            "sms:+4420",
            "blob:https://example.org/x",
            "about:blank",
            "tauri://localhost/",
            "steam://run/1",
        ] {
            assert!(!page(bad), "{bad}");
            assert!(!person(bad), "{bad}");
        }
    }

    #[test]
    fn credentials_and_very_long_addresses_are_refused_for_everyone() {
        for bad in [
            "https://user@example.org/",
            "https://user:pw@example.org/",
            "https://example.org@evil.example/",
            "http://user@localhost/",
        ] {
            assert!(!page(bad) && !person(bad), "{bad}");
        }
        let long = format!("https://example.org/{}", "a".repeat(MAX_URL_LEN));
        assert!(!page(&long) && !person(&long));
    }

    #[test]
    fn the_limiter_allows_one_every_three_seconds_and_five_a_minute() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut limiter = HandOffLimiter::default();
        assert!(limiter.allow(at(0)));
        assert!(!limiter.allow(at(1)));
        assert!(!limiter.allow(at(1_000)));
        assert!(!limiter.allow(at(2_999)));
        assert!(
            limiter.allow(at(3_000)),
            "exactly 3 s after the last one is allowed"
        );
        assert!(limiter.allow(at(6_000)));
        assert!(limiter.allow(at(9_000)));
        assert!(limiter.allow(at(12_000)));
        // Five accepted in the minute: the sixth is refused however long since the last.
        assert!(!limiter.allow(at(15_000)));
        assert!(!limiter.allow(at(59_999)));
        // The first one has left the window at 60 s.
        assert!(limiter.allow(at(60_000)));
        assert!(!limiter.allow(at(60_001)));
        assert!(!limiter.allow(at(62_999)));
        assert!(limiter.allow(at(63_000)));
    }

    #[test]
    fn a_flood_gets_one_hand_off_and_a_slow_trickle_gets_five_a_minute() {
        let t0 = Instant::now();
        let mut limiter = HandOffLimiter::default();
        let flood = (0..10_000u64)
            .filter(|i| limiter.allow(t0 + Duration::from_micros(i * 100)))
            .count();
        assert_eq!(flood, 1, "a loop that runs for a second gets one");

        let mut limiter = HandOffLimiter::default();
        let trickle = (0..600u64)
            .filter(|i| limiter.allow(t0 + Duration::from_millis(i * 100)))
            .count();
        assert_eq!(
            trickle, 5,
            "sixty seconds of attempts every 100 ms gets five"
        );

        // A clock that jumps backwards never opens the gate.
        let mut limiter = HandOffLimiter::default();
        assert!(limiter.allow(t0 + Duration::from_secs(100)));
        assert!(!limiter.allow(t0));
    }

    #[test]
    fn hand_off_opens_nothing_unless_every_rule_passes() {
        let limiter = Mutex::new(HandOffLimiter::default());
        let t0 = Instant::now();
        let mut opened: Vec<String> = Vec::new();
        let mut try_open = |text: &str, source, at: u64| {
            hand_off(
                &url(text),
                source,
                &limiter,
                t0 + Duration::from_secs(at),
                |u| opened.push(u),
            )
        };
        assert_eq!(
            try_open("https://example.org/a", Source::Page, 0),
            HandOff::Opened
        );
        assert_eq!(
            try_open("https://example.org/b", Source::Page, 1),
            HandOff::RateLimited
        );
        assert_eq!(try_open("mailto:x@y.z", Source::Page, 10), HandOff::Refused);
        assert_eq!(
            try_open("http://localhost/", Source::Page, 10),
            HandOff::Refused
        );
        assert_eq!(
            try_open("javascript:alert(1)", Source::Person, 10),
            HandOff::Refused
        );
        // A refused one does not use up the allowance.
        assert_eq!(
            try_open("https://example.org/c", Source::Page, 10),
            HandOff::Opened
        );
        // A person is not rate limited and may open mail and local addresses.
        assert_eq!(
            try_open("mailto:x@y.z", Source::Person, 10),
            HandOff::Opened
        );
        assert_eq!(
            try_open("http://localhost:3000/", Source::Person, 10),
            HandOff::Opened
        );
        assert_eq!(
            try_open("https://example.org/d", Source::Person, 10),
            HandOff::Opened
        );
        assert_eq!(
            opened,
            [
                "https://example.org/a",
                "https://example.org/c",
                "mailto:x@y.z",
                "http://localhost:3000/",
                "https://example.org/d"
            ]
        );
    }

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn marker(secret: &str, target: &str) -> Url {
        let mut u = Url::parse(&format!("https://{MARKER_HOST}/{secret}")).unwrap();
        u.query_pairs_mut().append_pair("u", target);
        u
    }

    #[test]
    fn a_marker_with_the_secret_opens_what_a_person_clicked() {
        for target in [
            "https://example.org/x?y=1",
            "mailto:support@dynodoc.online",
            "http://localhost:8080/",
            "http://192.168.1.10/",
        ] {
            assert_eq!(
                read_marker(&marker(SECRET, target), SECRET),
                Marker::Open(url(target)),
                "{target}"
            );
        }
        assert_eq!(
            read_marker(&url("https://example.org/"), SECRET),
            Marker::No
        );
        assert_eq!(
            read_marker(&url("http://tauri.localhost/"), SECRET),
            Marker::No
        );
    }

    #[test]
    fn a_marker_without_the_secret_or_with_a_bad_target_is_dropped() {
        let bad: Vec<Url> = vec![
            marker("wrong-secret-wrong-secret-wrong!", "https://example.org/"),
            marker("", "https://example.org/"),
            marker(&SECRET[..15], "https://example.org/"),
            marker(&format!("{SECRET}x"), "https://example.org/"),
            marker(SECRET, "javascript:alert(1)"),
            marker(SECRET, "file:///C:/Windows/win.ini"),
            marker(SECRET, "https://user@example.org/"),
            marker(SECRET, "data:text/html,x"),
            marker(SECRET, "not a url"),
            marker(SECRET, ""),
            url(&format!(
                "http://{MARKER_HOST}/{SECRET}?u=https%3A%2F%2Fexample.org%2F"
            )),
            url(&format!("https://{MARKER_HOST}/{SECRET}")),
            url(&format!(
                "https://{MARKER_HOST}/{SECRET}?u=https%3A%2F%2Fexample.org%2F&u=x"
            )),
            url(&format!(
                "https://{MARKER_HOST}/{SECRET}?v=https%3A%2F%2Fexample.org%2F"
            )),
            url(&format!(
                "https://{MARKER_HOST}/{SECRET}/extra?u=https%3A%2F%2Fexample.org%2F"
            )),
            url(&format!(
                "https://{MARKER_HOST}/{SECRET}?u=https%3A%2F%2Fexample.org%2F#f"
            )),
        ];
        for u in bad {
            assert_eq!(read_marker(&u, SECRET), Marker::Bad, "{u}");
        }
        // An empty or short configured secret never validates anything.
        assert_eq!(
            read_marker(&marker("", "https://example.org/"), ""),
            Marker::Bad
        );
        assert_eq!(
            read_marker(&marker("abc", "https://example.org/"), "abc"),
            Marker::Bad
        );
    }

    #[test]
    fn secrets_are_long_and_do_not_repeat() {
        let a = new_secret();
        let b = new_secret();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
