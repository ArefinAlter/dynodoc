//! Handing a link to the system browser or mail program, and the proof that a person asked.
//!
//! The web view tells the shell where a page wants to go but, in WebView2, not whether a
//! person clicked (wry ignores `IsUserInitiated`). So a hostile page can loop
//! `location = 'https://evil/' + n` or `window.open('mailto:...')` to flood the browser or
//! mail program, or aim at `http://localhost` and the local network. Defences, all pure
//! functions here:
//!
//! * A page-initiated hand-off opens only a PUBLIC `http(s)` address (never a mail link,
//!   loopback, private, link-local or similar host), at most once every 3 seconds and five
//!   times a minute; the rest are dropped silently. This is its own budget (`Limiters::page`).
//! * A person's click is recognised by a script the shell injects (`link_targets.js`). On a
//!   trusted click (`event.isTrusted`) on a link that leaves the page, or on a mail link, it
//!   calls `window.open` with a marker address that carries a secret. Such a request is a
//!   "click" hand-off and may open mail links and local addresses. It has its own budget
//!   (`Limiters::click`, one per second and ten a minute), so a page flooding the page budget
//!   cannot starve a person's click and a leaked secret could not flood. Menu items are not
//!   limited.
//!
//! What protects the secret: it is made from the operating system's randomness once per run
//! of the app and written into the script, where it lives in one closure variable. The script
//! runs before any page script and captures every built-in it will ever call (the
//! `Function.prototype.call`-free "uncurry" trick, the accessor functions of `Event`, `Node`
//! and `HTMLAnchorElement`, `window.open`), so a page that later patches prototypes, globals
//! or getters receives nothing: it never holds the marker address, because only the captured
//! native `window.open` and the Rust navigation handler see it (`scripts/link-targets.test.mjs`
//! runs the real script against such a page). The same script is injected again on every page
//! load with the SAME secret: rotating it per load would need a channel from the page to this
//! program, which pages must not have.
//!
//! What a hostile page can still do: get a person to really click something it controls (an
//! overlay, a link that moves under the pointer). That is a click hand-off, the same power
//! as a click in a browser, bounded by the click budget. A public name that resolves to a
//! private address (DNS rebinding, `localtest.me`, `*.nip.io`) cannot be seen from the
//! spelling; the well-known wildcard-DNS services are refused as a best effort.

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
    /// A trusted click, vouched for by the injected script's secret marker.
    Click,
    /// A native menu item (desktop only: phones have no menu).
    #[cfg_attr(mobile, allow(dead_code))]
    Menu,
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
                Source::Click | Source::Menu => url.host().is_some(),
                Source::Page => url.host().is_some_and(|host| public_host(&host)),
            }
        }
        // A mail link starts the user's mail program: only a person may do that.
        "mailto" => source != Source::Page,
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
    let under = |suffix: &&str| name == *suffix || name.ends_with(&format!(".{suffix}"));
    !LOCAL_SUFFIXES.iter().any(under) && !WILDCARD_DNS.iter().any(under) && !embeds_ipv4(&name)
}

/// Services whose names resolve to the address written in them or to 127.0.0.1, so a
/// "public" name can point at this machine or the local network. BEST EFFORT: a list of the
/// well-known ones, which cannot be complete (any domain can do the same, and DNS can be
/// rebound after this check).
const WILDCARD_DNS: [&str; 11] = [
    "nip.io",
    "sslip.io",
    "xip.io",
    "localtest.me",
    "lvh.me",
    "vcap.me",
    "lacolhost.com",
    "localhost.direct",
    "local.gd",
    "traefik.me",
    "127-0-0-1.org.uk",
];

/// Does the name spell an IPv4 address: four labels in a row that are numbers up to 255
/// (`10.0.0.5.example.com`), or four dash-separated numbers in one label (`10-0-0-5.example.com`)?
/// Such names are how wildcard-DNS services work; they are refused whatever the domain
/// (best effort, and it can refuse a harmless name).
fn embeds_ipv4(name: &str) -> bool {
    let octet = |part: &str| {
        !part.is_empty()
            && part.len() <= 3
            && part.bytes().all(|b| b.is_ascii_digit())
            && part.parse::<u16>().is_ok_and(|n| n <= 255)
    };
    let labels: Vec<&str> = name.split('.').collect();
    labels.windows(4).any(|w| w.iter().all(|l| octet(l)))
        || labels.iter().any(|label| {
            let parts: Vec<&str> = label.split('-').collect();
            parts.windows(4).any(|w| w.iter().all(|p| octet(p)))
        })
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

/// At most one hand-off every `min_gap` and `max_in_window` in any `window`. `now` is passed
/// in so the rule is testable; only accepted hand-offs are recorded.
#[derive(Debug)]
pub struct HandOffLimiter {
    min_gap: Duration,
    window: Duration,
    max_in_window: usize,
    accepted: VecDeque<Instant>,
}

impl Default for HandOffLimiter {
    fn default() -> Self {
        Self::page()
    }
}

impl HandOffLimiter {
    pub const MIN_GAP: Duration = Duration::from_secs(3);
    pub const WINDOW: Duration = Duration::from_secs(60);
    pub const MAX_IN_WINDOW: usize = 5;

    pub fn new(min_gap: Duration, max_in_window: usize, window: Duration) -> Self {
        Self {
            min_gap,
            window,
            max_in_window,
            accepted: VecDeque::new(),
        }
    }

    /// A page's own requests: one per 3 seconds, five a minute.
    pub fn page() -> Self {
        Self::new(Self::MIN_GAP, Self::MAX_IN_WINDOW, Self::WINDOW)
    }

    /// Requests vouched for by a click marker: one per second, ten a minute. A person cannot
    /// click faster; this only bounds what a leaked secret could do.
    pub fn click() -> Self {
        Self::new(Duration::from_secs(1), 10, Duration::from_secs(60))
    }

    pub fn allow(&mut self, now: Instant) -> bool {
        while self
            .accepted
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= self.window)
        {
            self.accepted.pop_front();
        }
        if let Some(last) = self.accepted.back() {
            if now.saturating_duration_since(*last) < self.min_gap {
                return false;
            }
        }
        if self.accepted.len() >= self.max_in_window {
            return false;
        }
        self.accepted.push_back(now);
        true
    }
}

/// The two budgets. They are separate so that a page that spends its own cannot take a
/// person's click budget (and the reverse). Menu items have none.
#[derive(Debug, Default)]
pub struct Limiters {
    pub page: Mutex<HandOffLimiter>,
    pub click: Mutex<HandOffLimiter>,
}

impl Limiters {
    pub fn new() -> Self {
        Self {
            page: Mutex::new(HandOffLimiter::page()),
            click: Mutex::new(HandOffLimiter::click()),
        }
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
    limiters: &Limiters,
    now: Instant,
    open: impl FnOnce(String),
) -> HandOff {
    if !external_allowed(url, source) {
        return HandOff::Refused;
    }
    let budget = match source {
        Source::Page => Some(&limiters.page),
        Source::Click => Some(&limiters.click),
        Source::Menu => None,
    };
    if budget.is_some_and(|b| !b.lock().is_ok_and(|mut l| l.allow(now))) {
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
    /// A valid marker: a trusted click asked to open this.
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
        Ok(target) if external_allowed(&target, Source::Click) => Marker::Open(target),
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

/// 256 bits straight from the operating system's random source (`getrandom`, already part
/// of Tauri's dependency tree). Hex, 64 characters. Fails (and the app does not start)
/// rather than falling back to anything weaker.
pub fn new_secret() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
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
        let click = external_allowed(&url(text), Source::Click);
        assert_eq!(click, external_allowed(&url(text), Source::Menu), "{text}");
        click
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
    fn well_known_wildcard_dns_names_and_names_spelling_an_address_are_not_public() {
        // Names that resolve to the address written in them or to 127.0.0.1. Best effort.
        for bad in [
            "http://localtest.me/",
            "http://foo.localtest.me/",
            "http://lvh.me:3000/",
            "http://app.lvh.me/",
            "http://vcap.me/",
            "http://lacolhost.com/",
            "http://anything.localhost.direct/",
            "http://local.gd/",
            "http://x.traefik.me/",
            "http://127.0.0.1.nip.io/",
            "http://192.168.1.10.nip.io/",
            "http://app.10.0.0.5.sslip.io/",
            "http://10-0-0-5.sslip.io/",
            "http://7f000001.nip.io/",
            "http://127-0-0-1.org.uk/",
            "http://NIP.IO./",
            "http://1.2.3.4.example.com/",
            "http://ten-10-0-0-5.example.com/",
        ] {
            assert!(!page(bad), "{bad}");
            assert!(person(bad), "a click may open {bad}");
        }
        // Close, but not those.
        for ok in [
            "https://example.org/",
            "https://nip.io.example.org/",
            "https://notnip.io/",
            "https://a.b.example.com/",
            "https://v1.2.example.com/",
            "https://app-1-2.example.com/",
            "https://x.300.1.1.1.example.com/",
        ] {
            assert!(page(ok), "{ok}");
        }
    }

    #[test]
    fn only_a_click_or_a_menu_item_may_start_the_mail_program() {
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
        let limiters = Limiters::new();
        let t0 = Instant::now();
        let mut opened: Vec<String> = Vec::new();
        let mut try_open = |text: &str, source, at: u64| {
            hand_off(
                &url(text),
                source,
                &limiters,
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
        for source in [Source::Click, Source::Menu] {
            assert_eq!(
                try_open("javascript:alert(1)", source, 10),
                HandOff::Refused
            );
        }
        // A refused one does not use up the allowance.
        assert_eq!(
            try_open("https://example.org/c", Source::Page, 10),
            HandOff::Opened
        );
        // A click may open mail and local addresses, within its own budget.
        assert_eq!(try_open("mailto:x@y.z", Source::Click, 10), HandOff::Opened);
        assert_eq!(
            try_open("http://localhost:3000/", Source::Click, 12),
            HandOff::Opened
        );
        // A menu item has no budget at all.
        for i in 0..20 {
            assert_eq!(
                try_open(&format!("http://localhost:3000/{i}"), Source::Menu, 12),
                HandOff::Opened
            );
        }
        assert_eq!(opened.len(), 4 + 20);
        assert_eq!(
            opened[..4],
            [
                "https://example.org/a",
                "https://example.org/c",
                "mailto:x@y.z",
                "http://localhost:3000/"
            ]
        );
    }

    #[test]
    fn a_click_gets_one_per_second_and_ten_a_minute_in_its_own_bucket() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut limiter = HandOffLimiter::click();
        assert!(limiter.allow(at(0)));
        assert!(!limiter.allow(at(999)));
        assert!(limiter.allow(at(1_000)));
        let mut accepted = 2;
        for i in 2..40u64 {
            if limiter.allow(at(i * 1_000)) {
                accepted += 1;
            }
        }
        assert_eq!(accepted, 10, "ten in the first minute");
        assert!(limiter.allow(at(60_000)), "the first has left the window");
        // A flood of 10,000 attempts in a second gets one.
        let mut flood = HandOffLimiter::click();
        let got = (0..10_000u64)
            .filter(|i| flood.allow(t0 + Duration::from_micros(i * 100)))
            .count();
        assert_eq!(got, 1);
    }

    #[test]
    fn a_page_cannot_use_up_a_persons_budget_and_the_reverse() {
        let limiters = Limiters::new();
        let t0 = Instant::now();
        let try_open = |text: &str, source, at_ms: u64| {
            hand_off(
                &url(text),
                source,
                &limiters,
                t0 + Duration::from_millis(at_ms),
                |_| {},
            )
        };
        // A page floods: its own budget is spent...
        let page: Vec<HandOff> = (0..50)
            .map(|i| try_open(&format!("https://flood{i}.example/"), Source::Page, i * 100))
            .collect();
        assert_eq!(page.iter().filter(|r| **r == HandOff::Opened).count(), 2);
        // ...and a person's click right after is still let through.
        assert_eq!(
            try_open("https://example.org/", Source::Click, 5_000),
            HandOff::Opened
        );
        // Click floods do not take the page budget either.
        let limiters = Limiters::new();
        let try_open = |text: &str, source, at_ms: u64| {
            hand_off(
                &url(text),
                source,
                &limiters,
                t0 + Duration::from_millis(at_ms),
                |_| {},
            )
        };
        for i in 0..30 {
            try_open(&format!("https://c{i}.example/"), Source::Click, i * 200);
        }
        assert_eq!(
            try_open("https://example.org/", Source::Page, 6_000),
            HandOff::Opened
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
        let a = new_secret().unwrap();
        let b = new_secret().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
