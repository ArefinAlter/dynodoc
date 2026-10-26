//! The Dynodoc server the window talks to: address validation, normalisation and origin
//! comparison. Pure functions, no Tauri types, so the hostile cases are unit-tested.
//!
//! A server address is an ORIGIN: scheme, host and optional port. Nothing else is kept.

use std::fmt;
use std::net::IpAddr;
use url::{Host, Url};

/// The hosted workspace.
pub const CLOUD_ORIGIN: &str = "https://app.dynodoc.online";
/// Where the Help menu item and the launcher's help link go.
pub const SUPPORT_URL: &str = "https://dynodoc.online/support";
/// The path the window opens on a server (see `ServerOrigin::entry`).
pub const ENTRY_PATH: &str = "/workspace";
/// The page that asks the signed-in person to let the app in (docs/decisions/017).
pub const APP_SIGNIN_PATH: &str = "/app-signin";
/// The page the app's own window finishes signing in on, with the secrets in the fragment.
pub const APP_SIGNIN_FINISH_PATH: &str = "/app-signin/finish";
/// Longest address text accepted: `https://` + a 253 character name + `:65535` is 267.
pub const MAX_ADDRESS_LEN: usize = 300;
/// Longest URL the navigation policy looks at; anything longer is refused outright.
pub const MAX_URL_LEN: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressError {
    Empty,
    TooLong,
    Characters,
    Credentials,
    Unreadable,
    Scheme,
    Insecure,
    Path,
    Port,
    Host,
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "Enter the address of your Dynodoc server.",
            Self::TooLong => "That address is too long.",
            Self::Characters => {
                "The address has characters that are not allowed. Type it like https://dynodoc.example.org."
            }
            Self::Credentials => {
                "Remove the user name or password from the address. Sign in on the server's own page instead."
            }
            Self::Unreadable => "That does not look like a web address.",
            Self::Scheme => "Only https:// addresses can be used (http://localhost works for testing).",
            Self::Insecure => "http:// is only allowed for localhost, for testing. Use an https:// address.",
            Self::Path => {
                "Enter only the server address, for example https://dynodoc.example.org, with nothing after it."
            }
            Self::Port => "The port number is not valid.",
            Self::Host => "The server name is not valid.",
        })
    }
}

/// A validated server origin. Always `scheme://host[:port]/`, host lowercase and in its
/// ASCII (punycode) form, no credentials, path, query or fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOrigin(Url);

impl ServerOrigin {
    /// `https://host[:port]` without a trailing slash; the form that is stored and shown.
    pub fn as_text(&self) -> String {
        self.0.origin().ascii_serialization()
    }

    pub fn is_cloud(&self) -> bool {
        self.as_text() == CLOUD_ORIGIN
    }

    /// The URL of the server's root page.
    pub fn root(&self) -> Url {
        self.0.clone()
    }

    /// The page the window opens on the server: the web app's workspace, which sends a person
    /// who is not signed in to the sign-in page. NOT the root: the hosted cloud's root answers
    /// with a redirect to the marketing site (hooks.server.ts of the web app), which the shell
    /// would hand to the system browser, leaving the window empty. Found on the first Android run.
    pub fn entry(&self) -> Url {
        self.0
            .join(ENTRY_PATH)
            .expect("a constant absolute path joins any origin")
    }

    /// `path` must come from a validated source (deep link, sign-in link) and start with `/`.
    pub fn join(&self, path: &str) -> Option<Url> {
        if !path.starts_with('/') || path.starts_with("//") || path.contains('\\') {
            return None;
        }
        let url = self.0.join(path).ok()?;
        self.matches(&url).then_some(url)
    }

    /// The page the system browser opens to sign the app in:
    /// `<server>/app-signin?challenge=..&state=..&platform=..`. A fixed path of the configured
    /// server; the three values are made by `signin.rs` (URL-safe characters only) and are
    /// encoded anyway. The page never learns the verifier.
    pub fn app_signin_url(&self, challenge: &str, state: &str, platform: &str) -> Url {
        let mut url = self.0.clone();
        url.set_path(APP_SIGNIN_PATH);
        url.query_pairs_mut()
            .append_pair("challenge", challenge)
            .append_pair("state", state)
            .append_pair("platform", platform);
        url
    }

    /// The same page for the standalone desktop app (decision 020): one more value, the port of
    /// the app's own listener on this computer. The page hands the code to
    /// `http://127.0.0.1:<port>/signin` instead of a `dynodoc://` link.
    pub fn app_signin_url_loopback(
        &self,
        challenge: &str,
        state: &str,
        platform: &str,
        port: u16,
    ) -> Url {
        let mut url = self.app_signin_url(challenge, state, platform);
        url.query_pairs_mut()
            .append_pair("redirect_port", &port.to_string());
        url
    }

    /// The page the app's own window opens to finish signing in: `<server>/app-signin/finish`,
    /// with nothing after the path. The code, state and verifier are handed to it once it has
    /// loaded (see `signin::Outbox`), as its address fragment; they are in no address the window
    /// requests.
    pub fn finish_url(&self) -> Url {
        let mut url = self.0.clone();
        url.set_path(APP_SIGNIN_FINISH_PATH);
        url
    }

    /// True when `url` has exactly this scheme, host and port and carries no credentials.
    pub fn matches(&self, url: &Url) -> bool {
        same_origin(&self.0, url) && url.username().is_empty() && url.password().is_none()
    }
}

/// Scheme, host and (default-aware) port are all equal. The `url` crate has already
/// lowercased hosts and converted names to punycode, so equality is exact.
pub fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host().is_some()
        && a.host() == b.host()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Validate what the user typed in the launcher (or what a saved config holds).
pub fn parse_server_address(input: &str) -> Result<ServerOrigin, AddressError> {
    let text = input.trim();
    if text.is_empty() {
        return Err(AddressError::Empty);
    }
    if text.len() > MAX_ADDRESS_LEN {
        return Err(AddressError::TooLong);
    }
    // '@' is how https://trusted.example@evil.example hides the real host.
    if text.contains('@') {
        return Err(AddressError::Credentials);
    }
    // Backslashes and percent-encoding are read differently by different parsers; control
    // characters and spaces never belong in a host name.
    if text
        .chars()
        .any(|c| c == '\\' || c == '%' || c.is_control() || c.is_whitespace() || is_invisible(c))
    {
        return Err(AddressError::Characters);
    }

    let with_scheme = if text.contains("://") {
        text.to_string()
    } else if looks_like_other_scheme(text) {
        return Err(AddressError::Scheme);
    } else {
        format!("https://{text}")
    };

    // The part after the authority must be empty or a single slash. Checked on the text,
    // before the parser resolves dot segments, so "/.." and "//" are not quietly accepted.
    let after_scheme = &with_scheme[with_scheme.find("://").map_or(0, |i| i + 3)..];
    let rest = after_scheme
        .find(['/', '?', '#'])
        .map_or("", |i| &after_scheme[i..]);
    if !(rest.is_empty() || rest == "/") {
        return Err(AddressError::Path);
    }

    let url = Url::parse(&with_scheme).map_err(|_| AddressError::Unreadable)?;
    let secure = match url.scheme() {
        "https" => true,
        "http" => false,
        _ => return Err(AddressError::Scheme),
    };
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AddressError::Credentials);
    }
    if url.query().is_some() || url.fragment().is_some() || url.path() != "/" {
        return Err(AddressError::Path);
    }
    if url.port() == Some(0) {
        return Err(AddressError::Port);
    }
    let loopback = match url.host() {
        Some(Host::Domain(name)) => {
            check_domain(name)?;
            name == "localhost"
        }
        Some(Host::Ipv4(ip)) => {
            if ip.is_unspecified() {
                return Err(AddressError::Host);
            }
            ip.is_loopback()
        }
        Some(Host::Ipv6(ip)) => {
            if ip.is_unspecified() {
                return Err(AddressError::Host);
            }
            IpAddr::V6(ip).is_loopback()
        }
        None => return Err(AddressError::Host),
    };
    if !secure && !loopback {
        return Err(AddressError::Insecure);
    }
    Ok(ServerOrigin(url))
}

/// Zero-width, soft-hyphen and bidirectional-control characters: invisible in a host name,
/// so they could hide a look-alike. (Category Cf is not covered by `is_control`.)
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00ad}'
            | '\u{061c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
    )
}

/// "javascript:alert(1)", "data:text/html,x", "mailto:a", "file:c:/x" have no "://" but are
/// not "host:port" either.
fn looks_like_other_scheme(text: &str) -> bool {
    let Some((head, tail)) = text.split_once(':') else {
        return false;
    };
    let scheme_like = head.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && head
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    let port_only = tail
        .split('/')
        .next()
        .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    scheme_like && !port_only
}

fn check_domain(name: &str) -> Result<(), AddressError> {
    // A final dot is a different cookie host from the same name without it. Refuse it so
    // there is one spelling of every server.
    if name.is_empty() || name.len() > 253 || name.ends_with('.') {
        return Err(AddressError::Host);
    }
    if name
        .split('.')
        .any(|label| label.is_empty() || label.len() > 63)
    {
        return Err(AddressError::Host);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInError {
    NotALink,
    WrongServer,
    NotASignInLink,
}

impl fmt::Display for SignInError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotALink => "That is not a link. Copy the whole sign-in link from the email.",
            Self::WrongServer => "That link is for a different server than the one chosen here.",
            Self::NotASignInLink => "That does not look like a Dynodoc sign-in link.",
        })
    }
}

/// The emailed sign-in link opens in the system browser, which does not share a session
/// with this window. The user can paste it here instead. Only a link to the configured
/// server's sign-in page, carrying one token and nothing else, is accepted, and the URL
/// that is opened is rebuilt from its parts.
pub fn sign_in_url(server: &ServerOrigin, link: &str) -> Result<Url, SignInError> {
    let text = link.trim();
    if text.is_empty()
        || text.len() > 2048
        || text
            .chars()
            .any(|c| c == '\\' || c.is_control() || c.is_whitespace())
    {
        return Err(SignInError::NotALink);
    }
    let url = Url::parse(text).map_err(|_| SignInError::NotALink)?;
    if !server.matches(&url) {
        return Err(SignInError::WrongServer);
    }
    if !matches!(url.path(), "/auth/verify" | "/login") || url.fragment().is_some() {
        return Err(SignInError::NotASignInLink);
    }
    let mut pairs = url.query_pairs();
    let (Some((key, token)), None) = (pairs.next(), pairs.next()) else {
        return Err(SignInError::NotASignInLink);
    };
    let token_ok = (32..=256).contains(&token.len())
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if key != "token" || !token_ok {
        return Err(SignInError::NotASignInLink);
    }
    let mut target = server.root();
    target.set_path("/login");
    target.query_pairs_mut().append_pair("token", &token);
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(input: &str) -> String {
        parse_server_address(input)
            .unwrap_or_else(|e| panic!("{input:?} should be accepted, got {e:?}"))
            .as_text()
    }

    fn err(input: &str) -> AddressError {
        match parse_server_address(input) {
            Ok(v) => panic!("{input:?} should be refused, got {}", v.as_text()),
            Err(e) => e,
        }
    }

    #[test]
    fn the_window_opens_the_workspace_never_the_root() {
        // The hosted cloud's root redirects to the marketing site, which the shell would hand to
        // the browser (found on the first Android run); the workspace is the app's entry.
        for (address, expected) in [
            (
                "https://app.dynodoc.online",
                "https://app.dynodoc.online/workspace",
            ),
            (
                "dynodoc.example.org:8443",
                "https://dynodoc.example.org:8443/workspace",
            ),
            ("http://localhost:5173", "http://localhost:5173/workspace"),
            ("https://[2001:db8::1]", "https://[2001:db8::1]/workspace"),
        ] {
            let server = parse_server_address(address).unwrap();
            let entry = server.entry();
            assert_eq!(entry.as_str(), expected);
            assert!(
                server.matches(&entry),
                "{address}: the entry stays on the server"
            );
            assert_eq!(
                Some(entry),
                server.join("/workspace"),
                "{address}: the same page the dynodoc://open/workspace link opens"
            );
        }
    }

    #[test]
    fn accepts_and_normalises_ordinary_addresses() {
        assert_eq!(ok("https://app.dynodoc.online"), CLOUD_ORIGIN);
        assert_eq!(ok("  https://app.dynodoc.online/  "), CLOUD_ORIGIN);
        assert_eq!(ok("HTTPS://APP.Dynodoc.Online"), CLOUD_ORIGIN);
        assert_eq!(ok("app.dynodoc.online"), CLOUD_ORIGIN);
        assert_eq!(ok("https://app.dynodoc.online:443"), CLOUD_ORIGIN);
        assert_eq!(
            ok("https://dynodoc.example.org:8443"),
            "https://dynodoc.example.org:8443"
        );
        assert_eq!(
            ok("dynodoc.example.org:8443"),
            "https://dynodoc.example.org:8443"
        );
        assert_eq!(ok("https://[2001:db8::1]"), "https://[2001:db8::1]");
        assert_eq!(ok("https://192.168.1.10"), "https://192.168.1.10");
        assert!(parse_server_address(CLOUD_ORIGIN).unwrap().is_cloud());
        assert!(!parse_server_address("https://dynodoc.example.org")
            .unwrap()
            .is_cloud());
    }

    #[test]
    fn plain_http_is_for_loopback_only() {
        assert_eq!(ok("http://localhost"), "http://localhost");
        assert_eq!(ok("http://localhost:5173"), "http://localhost:5173");
        assert_eq!(ok("http://127.0.0.1:8080"), "http://127.0.0.1:8080");
        assert_eq!(ok("http://[::1]:8080"), "http://[::1]:8080");
        assert_eq!(ok("http://127.1"), "http://127.0.0.1");
        for bad in [
            "http://app.dynodoc.online",
            "http://192.168.1.10",
            "http://10.0.0.1",
            "http://localhost.evil.example",
            "http://127.0.0.1.evil.example",
            "http://foo.localhost",
            "http://0.0.0.0",
            "http://[::]",
            "http://[2001:db8::1]",
        ] {
            assert!(
                matches!(err(bad), AddressError::Insecure | AddressError::Host),
                "{bad}"
            );
        }
    }

    #[test]
    fn userinfo_tricks_are_refused() {
        for bad in [
            "https://app.dynodoc.online@evil.example",
            "https://app.dynodoc.online:443@evil.example",
            "https://user:pass@dynodoc.example.org",
            "https://@dynodoc.example.org",
            "app.dynodoc.online@evil.example",
            "https://evil.example\\@app.dynodoc.online",
            "https://evil.example#@app.dynodoc.online",
        ] {
            assert!(
                matches!(
                    err(bad),
                    AddressError::Credentials | AddressError::Characters
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn other_schemes_are_refused() {
        for bad in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "file:c:/windows",
            "ftp://dynodoc.example.org",
            "ws://dynodoc.example.org",
            "mailto:someone",
            "tauri://localhost",
            "dynodoc://open/workspace",
            "blob:https://app.dynodoc.online/1",
            "about:blank",
        ] {
            let e = err(bad);
            assert!(
                matches!(
                    e,
                    AddressError::Scheme | AddressError::Characters | AddressError::Path
                ),
                "{bad}: {e:?}"
            );
        }
        assert_eq!(err("javascript://%0aalert(1)"), AddressError::Characters);
    }

    #[test]
    fn paths_queries_fragments_and_traversal_are_refused() {
        for bad in [
            "https://dynodoc.example.org/workspace",
            "https://dynodoc.example.org/../..",
            "https://dynodoc.example.org/..",
            "https://dynodoc.example.org/.",
            "https://dynodoc.example.org//",
            "https://dynodoc.example.org/%2e%2e/",
            "https://dynodoc.example.org?x=1",
            "https://dynodoc.example.org?",
            "https://dynodoc.example.org/?x=1",
            "https://dynodoc.example.org#frag",
            "https://dynodoc.example.org/#",
            "dynodoc.example.org/path",
        ] {
            assert!(
                matches!(err(bad), AddressError::Path | AddressError::Characters),
                "{bad}"
            );
        }
    }

    #[test]
    fn backslashes_percent_whitespace_and_controls_are_refused() {
        for bad in [
            "https:\\\\dynodoc.example.org",
            "https://dynodoc.example.org\\",
            "https://dynodoc%2eexample.org",
            "https://app%2Edynodoc.online",
            "https://dynodoc.example.org\nfoo",
            "https://dynodoc .example.org",
            "https://dynodoc\u{200b}.example.org",
            "https://dynodoc\u{00ad}.example.org",
            "https://dynodoc\t.example.org",
            "https://dynodoc\u{0}.example.org",
            "https://dynodoc\u{202e}.example.org",
        ] {
            assert!(
                matches!(err(bad), AddressError::Characters | AddressError::Empty),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn trailing_dots_and_odd_hosts_are_refused() {
        for bad in [
            "https://app.dynodoc.online.",
            "https://app.dynodoc.online..",
            "https://app..dynodoc.online",
            "https://.dynodoc.online",
            "https://",
            "https:///",
            "https://:443",
            "https://0.0.0.0",
            "https://[::]",
        ] {
            let e = err(bad);
            assert!(
                matches!(
                    e,
                    AddressError::Host | AddressError::Unreadable | AddressError::Path
                ),
                "{bad}: {e:?}"
            );
        }
        assert_eq!(
            err(&format!("https://{}.example.org", "a".repeat(64))),
            AddressError::Host
        );
        assert_eq!(err("https://dynodoc.example.org:0"), AddressError::Port);
        assert_eq!(
            err("https://dynodoc.example.org:65536"),
            AddressError::Unreadable
        );
        assert_eq!(
            err("https://dynodoc.example.org:http"),
            AddressError::Unreadable
        );
    }

    #[test]
    fn length_and_emptiness_are_bounded() {
        assert_eq!(err(""), AddressError::Empty);
        assert_eq!(err("   \t "), AddressError::Empty);
        assert_eq!(
            err(&format!("https://{}.example.org", "a".repeat(400))),
            AddressError::TooLong
        );
        assert_eq!(err(&"x".repeat(MAX_ADDRESS_LEN + 1)), AddressError::TooLong);
        let long_path = format!("https://dynodoc.example.org/{}", "a".repeat(100_000));
        assert_eq!(err(&long_path), AddressError::TooLong);
        let label = "a".repeat(60);
        let long_host = format!("https://{}.org", [label.as_str(); 6].join("."));
        assert!(matches!(
            err(&long_host),
            AddressError::TooLong | AddressError::Host
        ));
    }

    #[test]
    fn lookalike_hosts_normalise_to_a_different_origin() {
        // Cyrillic "а" and "о" look like Latin "a" and "o".
        let cloud = parse_server_address(CLOUD_ORIGIN).unwrap();
        for lookalike in ["https://аpp.dynodoc.online", "https://app.dynodoc.оnline"] {
            let origin = parse_server_address(lookalike)
                .unwrap_or_else(|e| panic!("{lookalike} should parse to punycode, got {e:?}"));
            assert_ne!(origin, cloud, "{lookalike}");
            assert!(!origin.is_cloud(), "{lookalike}");
            assert!(origin.as_text().contains("xn--"), "{}", origin.as_text());
            // The lookalike is never treated as the cloud server by the navigation check.
            let cloud_url = Url::parse(CLOUD_ORIGIN).unwrap();
            assert!(!origin.matches(&cloud_url), "{lookalike}");
            assert!(!cloud.matches(&origin.root()), "{lookalike}");
        }
        // The punycode spelling is the same thing as the Unicode spelling.
        assert_eq!(
            ok("https://bücher.example"),
            ok("https://xn--bcher-kva.example")
        );
        // Full-width and mixed-case forms collapse to the real host, never to another one.
        assert_eq!(ok("https://ＡＰＰ.dynodoc.online"), CLOUD_ORIGIN);
    }

    #[test]
    fn origin_matching_is_exact() {
        let server = parse_server_address("https://app.dynodoc.online").unwrap();
        let yes = |u: &str| server.matches(&Url::parse(u).unwrap());
        assert!(yes("https://app.dynodoc.online/"));
        assert!(yes("https://APP.dynodoc.online/workspace?x=1#y"));
        assert!(yes("https://app.dynodoc.online:443/workspace"));
        assert!(!yes("http://app.dynodoc.online/"));
        assert!(!yes("https://app.dynodoc.online:8443/"));
        assert!(!yes("https://app.dynodoc.online./"));
        assert!(!yes("https://app.dynodoc.online.evil.example/"));
        assert!(!yes("https://evil.example/app.dynodoc.online"));
        assert!(!yes("https://sub.app.dynodoc.online/"));
        assert!(!yes("https://dynodoc.online/"));
        assert!(!yes("https://app.dynodoc.online@evil.example/"));
        assert!(!yes("https://user@app.dynodoc.online/"));
        assert!(!yes("https://user:pw@app.dynodoc.online/"));
        assert!(!yes("https://[::1]/"));
        assert!(!yes("blob:https://app.dynodoc.online/id"));
        assert!(!yes("tauri://localhost/"));
        let with_port = parse_server_address("https://dynodoc.example.org:8443").unwrap();
        assert!(with_port.matches(&Url::parse("https://dynodoc.example.org:8443/x").unwrap()));
        assert!(!with_port.matches(&Url::parse("https://dynodoc.example.org/x").unwrap()));
    }

    #[test]
    fn join_stays_on_the_server() {
        let server = parse_server_address("https://app.dynodoc.online").unwrap();
        assert_eq!(
            server.join("/projects/abc").unwrap().as_str(),
            "https://app.dynodoc.online/projects/abc"
        );
        for bad in [
            "workspace",
            "//evil.example/x",
            "/\\evil.example",
            "https://evil.example/",
            "",
        ] {
            assert!(server.join(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_browser_sign_in_pages_are_fixed_paths_of_the_configured_server() {
        for address in [
            "https://app.dynodoc.online",
            "dynodoc.example.org:8443",
            "http://localhost:5173",
            "https://[2001:db8::1]",
        ] {
            let server = parse_server_address(address).unwrap();
            let start = server.app_signin_url("CHALLENGE-43-chars", "state_value", "android");
            assert!(server.matches(&start), "{address}: {start}");
            assert_eq!(start.path(), "/app-signin");
            assert_eq!(
                start.query(),
                Some("challenge=CHALLENGE-43-chars&state=state_value&platform=android")
            );
            assert_eq!(start.fragment(), None);

            let finish = server.finish_url();
            assert!(server.matches(&finish), "{address}: {finish}");
            assert_eq!(finish.path(), "/app-signin/finish");
            // Nothing is in the address the window requests: not the path, the query or the
            // fragment (the secrets come later, by `signin::Outbox`).
            assert_eq!(finish.query(), None);
            assert_eq!(finish.fragment(), None);
            assert_eq!(
                finish.as_str(),
                format!("{}app-signin/finish", server.root())
            );
            // The same host, scheme and port as the server, never another.
            assert_eq!(finish.origin(), server.root().origin());
            assert_eq!(start.origin(), server.root().origin());
        }
    }

    #[test]
    fn nothing_but_the_three_values_ever_reaches_the_sign_in_address() {
        let server = parse_server_address("https://app.dynodoc.online").unwrap();
        // Values are encoded, so even a value with separators in it stays one value.
        let start = server.app_signin_url("a&evil=1#x", "s t", "p/../q");
        assert_eq!(start.fragment(), None);
        let pairs: Vec<(String, String)> = start
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("challenge".to_string(), "a&evil=1#x".to_string()),
                ("state".to_string(), "s t".to_string()),
                ("platform".to_string(), "p/../q".to_string()),
            ]
        );
        assert_eq!(start.path(), "/app-signin");
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn sign_in_links_are_rebuilt_from_their_parts() {
        let server = parse_server_address("https://app.dynodoc.online").unwrap();
        let want = format!("https://app.dynodoc.online/login?token={TOKEN}");
        for link in [
            format!("https://app.dynodoc.online/auth/verify?token={TOKEN}"),
            format!("https://app.dynodoc.online/login?token={TOKEN}"),
            format!("  https://APP.dynodoc.online/auth/verify?token={TOKEN}\r\n"),
        ] {
            assert_eq!(
                sign_in_url(&server, &link).unwrap().as_str(),
                want,
                "{link}"
            );
        }
    }

    #[test]
    fn hostile_sign_in_links_are_refused() {
        let server = parse_server_address("https://app.dynodoc.online").unwrap();
        let t = TOKEN;
        let cases: Vec<(String, SignInError)> = vec![
            (
                format!("https://evil.example/auth/verify?token={t}"),
                SignInError::WrongServer,
            ),
            (
                format!("https://app.dynodoc.online@evil.example/login?token={t}"),
                SignInError::WrongServer,
            ),
            (
                format!("https://user@app.dynodoc.online/login?token={t}"),
                SignInError::WrongServer,
            ),
            (
                format!("http://app.dynodoc.online/login?token={t}"),
                SignInError::WrongServer,
            ),
            (
                format!("https://app.dynodoc.online:8443/login?token={t}"),
                SignInError::WrongServer,
            ),
            (
                format!("https://app.dynodoc.online/admin?token={t}"),
                SignInError::NotASignInLink,
            ),
            (
                format!("https://app.dynodoc.online/auth/verify/../x?token={t}"),
                SignInError::NotASignInLink,
            ),
            (
                format!("https://app.dynodoc.online/login?token={t}&next=//evil.example"),
                SignInError::NotASignInLink,
            ),
            (
                format!("https://app.dynodoc.online/login?next=/x&token={t}"),
                SignInError::NotASignInLink,
            ),
            (
                format!("https://app.dynodoc.online/login?token={t}#x"),
                SignInError::NotASignInLink,
            ),
            (
                "https://app.dynodoc.online/login?token=short".to_string(),
                SignInError::NotASignInLink,
            ),
            (
                "https://app.dynodoc.online/login".to_string(),
                SignInError::NotASignInLink,
            ),
            (
                format!("https://app.dynodoc.online/login?token={}", "a".repeat(257)),
                SignInError::NotASignInLink,
            ),
            (
                format!("https://app.dynodoc.online/login?token={}%0a", &t[..40]),
                SignInError::NotASignInLink,
            ),
            (
                format!(
                    "https://app.dynodoc.online/login?token={}<script>",
                    &t[..40]
                ),
                SignInError::NotASignInLink,
            ),
            ("javascript:alert(1)".to_string(), SignInError::WrongServer),
            (
                format!("https://app.dynodoc.online/login?token={t}\nhttps://evil.example"),
                SignInError::NotALink,
            ),
            (
                format!("https://app.dynodoc.online\\@evil.example/login?token={t}"),
                SignInError::NotALink,
            ),
            ("".to_string(), SignInError::NotALink),
            ("not a link".to_string(), SignInError::NotALink),
            (
                format!(
                    "https://app.dynodoc.online/login?token={}",
                    "a".repeat(10_000)
                ),
                SignInError::NotALink,
            ),
        ];
        for (link, want) in cases {
            assert_eq!(sign_in_url(&server, &link), Err(want), "{link}");
        }
    }
}
