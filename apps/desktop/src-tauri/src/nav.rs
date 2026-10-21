//! Where the main window may go. Pure functions over `url::Url`; the Tauri hooks in
//! `window.rs` only call them.
//!
//! The window shows the local launcher, or the configured server. The launcher is reached
//! only when the shell itself asked for it. Every other web link is a link out: a page may
//! hand a PUBLIC address to the system browser at a limited rate (see `handoff`), a person's
//! click may open more, and nothing else is ever loaded or opened.

use crate::handoff::{self, Marker, Source};
use crate::server::{same_origin, ServerOrigin, MAX_URL_LEN};
use std::time::{Duration, Instant};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Load it in the window.
    Allow,
    /// Hand it to the system browser (a public address, at a limited rate) and keep the
    /// window where it is.
    OpenExternal,
    /// Drop it silently.
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewWindow {
    /// A same-server link that wanted a new window: show it in the one window instead.
    NavigateHere(Url),
    /// A page's own request for a public address: rate limited.
    OpenExternal(Url),
    /// A person's click, recognised by the injected script's secret marker.
    PersonOpen(Url),
    Ignore,
}

/// The origin the bundled launcher page is served from. Tauri serves local files at
/// `http://tauri.localhost` on Windows and Android and at `tauri://localhost` elsewhere
/// (tauri 2.12 `protocol::origin`, with `useHttpsScheme` off, which is the default).
pub fn launcher_origin() -> Url {
    let text = if cfg!(any(windows, target_os = "android")) {
        "http://tauri.localhost/"
    } else {
        "tauri://localhost/"
    };
    Url::parse(text).expect("constant URL")
}

/// The launcher page, optionally with a section to scroll to (`signin`, `key`).
pub fn launcher_url(section: Option<&str>) -> Url {
    let mut url = launcher_origin().join("index.html").expect("constant join");
    if let Some(section) = section {
        if matches!(section, "signin" | "key") {
            url.set_fragment(Some(section));
        }
    }
    url
}

pub fn is_launcher(url: &Url, launcher: &Url) -> bool {
    same_origin(url, launcher) && url.username().is_empty() && url.password().is_none()
}

/// Permission to load the launcher, given by the shell for one navigation it started. A
/// server page that navigates to the launcher's address gets nothing: the launcher can run
/// the key operations, so a page must not be able to bring it up on its own.
#[derive(Debug, Default)]
pub struct LauncherGrant {
    until: Option<Instant>,
}

impl LauncherGrant {
    pub const TTL: Duration = Duration::from_secs(5);

    pub fn arm(&mut self, now: Instant) {
        self.until = Some(now + Self::TTL);
    }

    /// True once per `arm`, and only before it expires.
    pub fn take(&mut self, now: Instant) -> bool {
        self.until.take().is_some_and(|until| now < until)
    }
}

/// `launcher_allowed` is the result of `LauncherGrant::take` for this navigation.
pub fn decide(
    target: &Url,
    server: Option<&ServerOrigin>,
    launcher: &Url,
    launcher_allowed: bool,
) -> Decision {
    if target.as_str().len() > MAX_URL_LEN {
        return Decision::Deny;
    }
    // https://trusted.example@evil.example: never loaded, never handed on.
    if !target.username().is_empty() || target.password().is_some() {
        return Decision::Deny;
    }
    if is_launcher(target, launcher) {
        return if launcher_allowed {
            Decision::Allow
        } else {
            Decision::Deny
        };
    }
    match target.scheme() {
        "http" | "https" => {
            if server.is_some_and(|s| s.matches(target)) {
                Decision::Allow
            } else if target.host_str() == Some(handoff::MARKER_HOST) {
                Decision::Deny // the click marker is only meaningful as a new-window request
            } else if handoff::external_allowed(target, Source::Page) {
                Decision::OpenExternal
            } else {
                Decision::Deny
            }
        }
        // javascript:, data:, file:, blob:, about:, ftp:, mailto: from a page, other apps'
        // schemes, other origins' tauri://
        _ => Decision::Deny,
    }
}

/// `window.open` and `target="_blank"` follow the same rule as navigation, and never
/// create a second window. `secret` is the injected script's click-marker secret.
pub fn new_window(
    target: &Url,
    server: Option<&ServerOrigin>,
    launcher: &Url,
    secret: &str,
) -> NewWindow {
    match handoff::read_marker(target, secret) {
        Marker::Open(url) => return NewWindow::PersonOpen(url),
        Marker::Bad => return NewWindow::Ignore,
        Marker::No => {}
    }
    match decide(target, server, launcher, false) {
        Decision::OpenExternal => NewWindow::OpenExternal(target.clone()),
        Decision::Allow if server.is_some_and(|s| s.matches(target)) => {
            NewWindow::NavigateHere(target.clone())
        }
        _ => NewWindow::Ignore,
    }
}

/// The window title: the app name, and for a page of the configured server its host (ASCII
/// form, with the port when there is one), so the person can always see where they are.
pub fn window_title(url: &Url, server: Option<&ServerOrigin>) -> String {
    match (server, url.host_str()) {
        (Some(server), Some(host)) if server.matches(url) => match url.port() {
            Some(port) => format!("Dynodoc - {host}:{port}"),
            None => format!("Dynodoc - {host}"),
        },
        _ => "Dynodoc".to_string(),
    }
}

/// A URL for the debug log: path only, with a query shown as `?...` and no fragment, so a
/// sign-in token never reaches a log. A click marker's path IS the click secret, so it is
/// never printed. (Only debug builds log, so a release build never calls it.)
#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub fn redacted(url: &Url) -> String {
    if url.host_str() == Some(handoff::MARKER_HOST) {
        return "(click marker)".to_string();
    }
    let mut text = format!("{}://", url.scheme());
    if let Some(host) = url.host_str() {
        text.push_str(host);
        if let Some(port) = url.port() {
            text.push_str(&format!(":{port}"));
        }
    }
    text.push_str(url.path());
    if url.query().is_some() {
        text.push_str("?...");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::parse_server_address;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn server(text: &str) -> ServerOrigin {
        parse_server_address(text).unwrap()
    }
    fn decide_for(url: &str, configured: Option<&ServerOrigin>) -> Decision {
        decide(
            &Url::parse(url).unwrap(),
            configured,
            &launcher_origin(),
            false,
        )
    }

    #[test]
    fn the_configured_server_loads_in_the_window() {
        let s = server("https://app.dynodoc.online");
        for url in [
            "https://app.dynodoc.online/",
            "https://app.dynodoc.online/workspace",
            "https://APP.DYNODOC.ONLINE/projects/1?x=y#z",
            "https://app.dynodoc.online:443/login?token=abc",
        ] {
            assert_eq!(decide_for(url, Some(&s)), Decision::Allow, "{url}");
        }
    }

    #[test]
    fn other_public_web_origins_leave_for_the_system_browser() {
        let s = server("https://app.dynodoc.online");
        for url in [
            "https://dynodoc.online/support",
            "https://accounts.google.com/o/oauth2/auth",
            "http://app.dynodoc.online/",
            "https://app.dynodoc.online:8443/",
            "https://app.dynodoc.online.evil.example/",
            "https://evil.example/https://app.dynodoc.online/",
            "https://sub.app.dynodoc.online/",
            "https://app.dynodoc.online./",
            "https://xn--pp-8ma.dynodoc.online/",
            "https://аpp.dynodoc.online/",
        ] {
            assert_eq!(decide_for(url, Some(&s)), Decision::OpenExternal, "{url}");
        }
        // Nothing configured yet: every public web link is external.
        assert_eq!(
            decide_for("https://app.dynodoc.online/", None),
            Decision::OpenExternal
        );
    }

    #[test]
    fn a_page_cannot_hand_over_mail_links_or_local_addresses() {
        let s = server("https://app.dynodoc.online");
        for url in [
            "mailto:support@dynodoc.online",
            "mailto:a@b?subject=x",
            "https://127.0.0.1/",
            "https://[::1]/",
            "http://localhost:8080/admin",
            "http://192.168.0.1/",
            "http://10.1.2.3/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::ffff:127.0.0.1]/",
            "http://printer/",
            "http://router.local/",
        ] {
            assert_eq!(decide_for(url, Some(&s)), Decision::Deny, "{url}");
        }
    }

    #[test]
    fn the_click_marker_address_is_never_loaded_or_opened_as_a_navigation() {
        let s = server("https://app.dynodoc.online");
        let marker = format!(
            "https://{}/{SECRET}?u=https%3A%2F%2Fexample.org%2F",
            handoff::MARKER_HOST
        );
        assert_eq!(decide_for(&marker, Some(&s)), Decision::Deny);
        assert_eq!(decide_for(&marker, None), Decision::Deny);
    }

    #[test]
    fn credentials_in_a_link_drop_it_even_for_the_server() {
        let s = server("https://app.dynodoc.online");
        for url in [
            "https://app.dynodoc.online@evil.example/",
            "https://user@app.dynodoc.online/",
            "https://user:pw@app.dynodoc.online/",
            "https://:pw@app.dynodoc.online/",
            "http://user@localhost/",
        ] {
            assert_eq!(decide_for(url, Some(&s)), Decision::Deny, "{url}");
        }
    }

    #[test]
    fn non_web_schemes_are_dropped_and_about_blank_is_not_special() {
        let s = server("https://app.dynodoc.online");
        for url in [
            "javascript:alert(1)",
            "JAVASCRIPT:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///C:/Windows/System32/calc.exe",
            "file://evil/share/x",
            "blob:https://app.dynodoc.online/6f1c",
            "ftp://app.dynodoc.online/",
            "ssh://app.dynodoc.online/",
            "ms-msdt:/id",
            "vscode://file/x",
            "dynodoc://open/workspace",
            "tauri://evil/index.html",
            "about:blank",
            "about:srcdoc",
            "about:blank#x",
            "view-source:https://app.dynodoc.online/",
            "chrome://settings",
            "ipc://localhost/set_server",
        ] {
            assert_eq!(decide_for(url, Some(&s)), Decision::Deny, "{url}");
        }
    }

    #[test]
    fn only_the_launcher_origin_counts_as_the_launcher() {
        let launcher = launcher_origin();
        let s = server("https://app.dynodoc.online");
        // With the shell's permission the launcher loads, with or without a section.
        assert_eq!(
            decide(&launcher_url(None), Some(&s), &launcher, true),
            Decision::Allow
        );
        assert_eq!(
            decide(&launcher_url(Some("key")), None, &launcher, true),
            Decision::Allow
        );
        let hosts = if cfg!(any(windows, target_os = "android")) {
            [
                "http://tauri.localhost.evil.example/",
                "https://tauri.localhost/",
                "http://tauri.localhost:8080/",
            ]
        } else {
            [
                "tauri://localhost.evil.example/",
                "tauri://localhost:8080/",
                "tauri://other/",
            ]
        };
        for url in hosts {
            let target = Url::parse(url).unwrap();
            assert!(!is_launcher(&target, &launcher), "{url}");
            assert_ne!(
                decide(&target, Some(&s), &launcher, true),
                Decision::Allow,
                "{url}"
            );
        }
        let other = if cfg!(any(windows, target_os = "android")) {
            "tauri://localhost/"
        } else {
            "http://tauri.localhost/"
        };
        assert!(!is_launcher(&Url::parse(other).unwrap(), &launcher));
    }

    #[test]
    fn a_page_cannot_bring_up_the_launcher_by_navigating_to_it() {
        let launcher = launcher_origin();
        let s = server("https://app.dynodoc.online");
        for target in [
            launcher_url(None),
            launcher_url(Some("key")),
            launcher_url(Some("signin")),
            launcher.clone(),
            launcher.join("other.html").unwrap(),
        ] {
            for configured in [Some(&s), None] {
                assert_eq!(
                    decide(&target, configured, &launcher, false),
                    Decision::Deny,
                    "{target}"
                );
            }
        }
    }

    #[test]
    fn the_launcher_grant_is_one_shot_and_expires() {
        let t0 = Instant::now();
        let mut grant = LauncherGrant::default();
        assert!(!grant.take(t0), "nothing armed");
        grant.arm(t0);
        assert!(grant.take(t0 + Duration::from_secs(1)));
        assert!(!grant.take(t0 + Duration::from_secs(1)), "used once");
        grant.arm(t0);
        assert!(
            !grant.take(t0 + LauncherGrant::TTL),
            "expired exactly at the limit"
        );
        assert!(!grant.take(t0), "an expired grant is gone");
        // Arming again replaces an old one.
        grant.arm(t0);
        grant.arm(t0 + Duration::from_secs(10));
        assert!(grant.take(t0 + Duration::from_secs(12)));
    }

    #[test]
    fn the_server_may_never_be_the_launcher_origin_by_configuration() {
        // http://tauri.localhost is not loopback, so it cannot be configured on Windows.
        assert!(parse_server_address("http://tauri.localhost").is_err());
        assert!(parse_server_address("tauri://localhost").is_err());
    }

    #[test]
    fn very_long_urls_are_dropped() {
        let s = server("https://app.dynodoc.online");
        let long = format!("https://app.dynodoc.online/{}", "a".repeat(MAX_URL_LEN));
        assert_eq!(decide_for(&long, Some(&s)), Decision::Deny);
        let long_external = format!("https://evil.example/?{}", "a".repeat(MAX_URL_LEN));
        assert_eq!(decide_for(&long_external, Some(&s)), Decision::Deny);
    }

    fn marker(target: &str) -> Url {
        let mut u = Url::parse(&format!("https://{}/{SECRET}", handoff::MARKER_HOST)).unwrap();
        u.query_pairs_mut().append_pair("u", target);
        u
    }

    #[test]
    fn new_windows_follow_the_same_rule_and_never_open() {
        let launcher = launcher_origin();
        let s = server("https://app.dynodoc.online");
        let go = |u: &str| new_window(&Url::parse(u).unwrap(), Some(&s), &launcher, SECRET);
        assert_eq!(
            go("https://app.dynodoc.online/print/1"),
            NewWindow::NavigateHere(Url::parse("https://app.dynodoc.online/print/1").unwrap())
        );
        assert_eq!(
            go("https://dynodoc.online/support"),
            NewWindow::OpenExternal(Url::parse("https://dynodoc.online/support").unwrap())
        );
        for dropped in [
            "about:blank",
            "javascript:alert(1)",
            "data:text/html,x",
            "file:///etc/passwd",
            "https://app.dynodoc.online@evil.example/",
            "tauri://localhost/index.html",
            "http://tauri.localhost/index.html",
            "mailto:a@b.c",
            "http://127.0.0.1:8080/",
            "http://[::1]/",
            "http://192.168.1.1/",
            "http://localhost/",
        ] {
            assert_eq!(go(dropped), NewWindow::Ignore, "{dropped}");
        }
    }

    #[test]
    fn a_marked_new_window_is_a_persons_click_and_only_with_the_secret() {
        let launcher = launcher_origin();
        let s = server("https://app.dynodoc.online");
        let go = |u: &Url, secret: &str| new_window(u, Some(&s), &launcher, secret);
        for target in [
            "mailto:support@dynodoc.online",
            "http://localhost:3000/",
            "https://example.org/",
        ] {
            assert_eq!(
                go(&marker(target), SECRET),
                NewWindow::PersonOpen(Url::parse(target).unwrap()),
                "{target}"
            );
            // A page that guesses the format but not the secret gets nothing, not even an
            // external open of the marker address itself.
            assert_eq!(
                go(&marker(target), "ffffffffffffffffffffffffffffffff"),
                NewWindow::Ignore
            );
        }
        assert_eq!(
            go(&marker("javascript:alert(1)"), SECRET),
            NewWindow::Ignore
        );
        assert_eq!(
            go(&marker("file:///C:/Windows/win.ini"), SECRET),
            NewWindow::Ignore
        );
    }

    #[test]
    fn launcher_url_accepts_only_known_sections() {
        assert!(launcher_url(None).fragment().is_none());
        assert_eq!(launcher_url(Some("signin")).fragment(), Some("signin"));
        assert_eq!(launcher_url(Some("key")).fragment(), Some("key"));
        assert!(launcher_url(Some("../../x")).fragment().is_none());
        assert!(launcher_url(Some("<script>")).fragment().is_none());
    }

    #[test]
    fn the_title_shows_where_the_window_is() {
        let s = server("https://app.dynodoc.online");
        let t = |u: &str| window_title(&Url::parse(u).unwrap(), Some(&s));
        assert_eq!(
            t("https://app.dynodoc.online/workspace"),
            "Dynodoc - app.dynodoc.online"
        );
        assert_eq!(
            t("https://APP.dynodoc.online:443/x"),
            "Dynodoc - app.dynodoc.online"
        );
        assert_eq!(window_title(&launcher_url(None), Some(&s)), "Dynodoc");
        assert_eq!(window_title(&launcher_url(None), None), "Dynodoc");
        // A page that is not the configured server never gets a server's name in the title.
        assert_eq!(t("https://evil.example/"), "Dynodoc");
        assert_eq!(t("https://app.dynodoc.online.evil.example/"), "Dynodoc");
        let local = server("http://127.0.0.1:8080");
        assert_eq!(
            window_title(
                &Url::parse("http://127.0.0.1:8080/x").unwrap(),
                Some(&local)
            ),
            "Dynodoc - 127.0.0.1:8080"
        );
        let idn = server("https://bücher.example");
        assert_eq!(
            window_title(
                &Url::parse("https://xn--bcher-kva.example/").unwrap(),
                Some(&idn)
            ),
            "Dynodoc - xn--bcher-kva.example",
            "the ASCII form shows, so a look-alike is visible"
        );
    }

    #[test]
    fn the_debug_log_never_shows_a_query_or_fragment() {
        let url = |u: &str| redacted(&Url::parse(u).unwrap());
        assert_eq!(
            url("https://app.dynodoc.online/login?token=SECRET123"),
            "https://app.dynodoc.online/login?..."
        );
        assert_eq!(
            url("https://app.dynodoc.online:8443/a/b#frag-secret"),
            "https://app.dynodoc.online:8443/a/b"
        );
        assert_eq!(
            url("https://x.example/p?a=1&token=SECRET#f"),
            "https://x.example/p?..."
        );
        assert_eq!(url("https://x.example/"), "https://x.example/");
        for text in [
            "https://x.example/?token=SECRET",
            "mailto:a@b.c?body=SECRET",
        ] {
            assert!(!url(text).contains("SECRET"), "{text}");
        }
        // The click marker's path IS the click secret.
        let marked = marker("https://example.org/");
        assert_eq!(redacted(&marked), "(click marker)");
        assert!(!redacted(&marked).contains(SECRET));
    }
}
