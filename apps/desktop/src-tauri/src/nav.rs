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
    /// A trusted click, vouched for by the injected script's secret marker.
    ClickOpen(Url),
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

/// The one page the launcher origin serves as a window's document. (Tauri reports the first
/// load as `/` and a later one as `/index.html`.) Any other path on that origin is never loaded
/// as a window's page, and never uses up a grant.
pub fn is_launcher_page(url: &Url, launcher: &Url) -> bool {
    is_launcher(url, launcher) && matches!(url.path(), "/" | "/index.html")
}

/// WebView2 reports only main-frame navigations to the navigation handler. WebKitGTK and
/// WKWebView (and perhaps Android) report sub-frame navigations too, and wry passes no
/// "is main frame" flag on those platforms, so there the handler cannot tell an `<iframe>`
/// from the window. UNVERIFIED ON WEBKIT AND ANDROID: this is an assumption written down, not
/// something observed. See `frame_like` for what is done about it.
pub const fn frames_are_ambiguous() -> bool {
    !cfg!(windows)
}

/// Addresses that are how pages normally fill a sub-frame: `about:srcdoc`, `about:blank`, a
/// `blob:` made by the configured server's own page, and `data:`. Where the handler cannot
/// tell frames from the window, refusing them would break pages (a PDF preview in a `blob:`
/// frame, a `srcdoc` frame), so they are let through. What that gives a page that really does
/// navigate the WINDOW to one of them: its own content, with no server session beyond what the
/// page already has (a `blob:` of the server inherits its origin, which is the page's own power;
/// `data:` gets an opaque origin), no native access (none of these is a local origin) and a title
/// that no longer names the server. None of them is ever the launcher, so none can use a grant.
fn frame_like(target: &Url, server: Option<&ServerOrigin>) -> bool {
    match target.scheme() {
        "about" => matches!(target.as_str(), "about:srcdoc" | "about:blank"),
        "data" => true,
        "blob" => {
            server.is_some_and(|s| Url::parse(target.path()).is_ok_and(|inner| s.matches(&inner)))
        }
        _ => false,
    }
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

/// May the launcher page load now? Yes for a navigation the shell started (the grant, taken
/// only for the launcher's own page), and also while NO server is configured: then the window
/// can only ever show the launcher (every other page is refused or sent to the browser), so a
/// load of it is the shell building its own window again and not a page asking for it. That
/// happens on Android, where the system re-creates the activity, and with it the web view, on a
/// font-size or display-size change or when the app is updated under it; the second load of
/// the launcher would otherwise find the one-shot grant spent and leave a blank window. With a
/// server configured the grant is still the only way in. `take_grant` is called only for the
/// launcher's page.
pub fn launcher_load_allowed(
    target: &Url,
    launcher: &Url,
    server_configured: bool,
    take_grant: impl FnOnce() -> bool,
) -> bool {
    if !is_launcher_page(target, launcher) {
        return false;
    }
    let granted = take_grant();
    granted || !server_configured
}

/// Where to go instead when the launcher's page was refused: on a phone, a configured server's
/// entry page. Android re-creates the web view on a font-size or display-size change (or after a
/// dead renderer) and reloads the address the window was FIRST created with, which is the
/// launcher whenever the app was first run without a saved server; with a server saved that load
/// is refused, and the window would stay blank. `None` when there is no server (the launcher is
/// allowed then) or the address is not the launcher's page.
#[cfg_attr(not(mobile), allow(dead_code))]
pub fn launcher_denied_goes_to(
    target: &Url,
    launcher: &Url,
    server: Option<&ServerOrigin>,
) -> Option<Url> {
    if !is_launcher_page(target, launcher) {
        return None;
    }
    server.map(ServerOrigin::entry)
}

/// Is `url`, a page the window has finished loading, the configured server's own sign-in page?
///
/// On a phone, where there is no menu and the launcher is otherwise out of reach once a server
/// is saved, the shell shows its launcher (with the browser sign-in button) when the window
/// lands on the server's sign-in page: the emailed link and Google both finish in the system
/// browser, so that page cannot sign this window in. The test is on the address alone (the
/// shell never reads a page, a cookie or a header) and is only used to choose what to show: the
/// launcher is a page of the app itself, and nothing a page does gives it any power over the
/// app. `/login?token=...` is the pasted-link route and is left alone.
#[cfg_attr(not(mobile), allow(dead_code))]
pub fn needs_sign_in(url: &Url, server: Option<&ServerOrigin>) -> bool {
    server.is_some_and(|s| s.matches(url))
        && url.path() == "/login"
        && !url.query_pairs().any(|(name, _)| name == "token")
}

/// Is `url` the configured server's finish page, exactly as the shell opens it (no query, no
/// fragment)? The one page the browser sign-in's fragment is handed to.
pub fn is_finish_page(url: &Url, server: Option<&ServerOrigin>) -> bool {
    server.is_some_and(|s| s.matches(url))
        && url.path() == crate::server::APP_SIGNIN_FINISH_PATH
        && url.query().is_none()
        && url.fragment().is_none()
}

/// `launcher_allowed` is the result of `launcher_load_allowed` for this navigation.
/// `frames_ambiguous` is `frames_are_ambiguous()`.
pub fn decide(
    target: &Url,
    server: Option<&ServerOrigin>,
    launcher: &Url,
    launcher_allowed: bool,
    frames_ambiguous: bool,
) -> Decision {
    if target.as_str().len() > MAX_URL_LEN {
        return Decision::Deny;
    }
    // https://trusted.example@evil.example: never loaded, never handed on.
    if !target.username().is_empty() || target.password().is_some() {
        return Decision::Deny;
    }
    if is_launcher(target, launcher) {
        return if launcher_allowed && is_launcher_page(target, launcher) {
            Decision::Allow
        } else {
            Decision::Deny
        };
    }
    if frames_ambiguous && frame_like(target, server) {
        return Decision::Allow;
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
        Marker::Open(url) => return NewWindow::ClickOpen(url),
        Marker::Bad => return NewWindow::Ignore,
        Marker::No => {}
    }
    // A request for a new window is never a sub-frame: no ambiguity.
    match decide(target, server, launcher, false, false) {
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
            false,
        )
    }
    fn decide_ambiguous(url: &str, configured: Option<&ServerOrigin>, grant: bool) -> Decision {
        decide(
            &Url::parse(url).unwrap(),
            configured,
            &launcher_origin(),
            grant,
            true,
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
            decide(&launcher_url(None), Some(&s), &launcher, true, false),
            Decision::Allow
        );
        assert_eq!(
            decide(&launcher_url(Some("key")), None, &launcher, true, false),
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
                decide(&target, Some(&s), &launcher, true, false),
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
                    decide(&target, configured, &launcher, false, false),
                    Decision::Deny,
                    "{target}"
                );
            }
        }
    }

    #[test]
    fn only_the_launchers_page_can_be_loaded_even_with_the_shells_permission() {
        let launcher = launcher_origin();
        let s = server("https://app.dynodoc.online");
        let origin = launcher.as_str().trim_end_matches('/').to_string();
        for path in ["/", "/index.html", "/index.html#key"] {
            let target = Url::parse(&format!("{origin}{path}")).unwrap();
            assert!(is_launcher_page(&target, &launcher), "{path}");
            assert_eq!(
                decide(&target, Some(&s), &launcher, true, false),
                Decision::Allow
            );
            assert_eq!(
                decide(&target, Some(&s), &launcher, true, true),
                Decision::Allow
            );
        }
        for path in [
            "/other.html",
            "/launcher.js",
            "/index.html/x",
            "/..%2f",
            "/index.htm",
        ] {
            let target = Url::parse(&format!("{origin}{path}")).unwrap();
            for ambiguous in [false, true] {
                assert_ne!(
                    decide(&target, Some(&s), &launcher, true, ambiguous),
                    Decision::Allow,
                    "{path} (ambiguous {ambiguous})"
                );
            }
        }
    }

    #[test]
    fn where_frames_cannot_be_told_from_the_window_frame_like_addresses_are_let_through() {
        let s = server("https://app.dynodoc.online");
        // Let through when the handler cannot tell (WebKit): these are how pages fill frames.
        for url in [
            "about:srcdoc",
            "about:blank",
            "data:text/html,<p>preview</p>",
            "data:application/pdf;base64,AAAA",
            "blob:https://app.dynodoc.online/6f1c-aa",
        ] {
            assert_eq!(
                decide_ambiguous(url, Some(&s), false),
                Decision::Allow,
                "{url}"
            );
            // Never because of a grant, and never the other way round: not the launcher.
            assert_eq!(
                decide_ambiguous(url, Some(&s), true),
                Decision::Allow,
                "{url}"
            );
            // On WebView2 (main frame only) the same addresses stay refused.
            assert_eq!(decide_for(url, Some(&s)), Decision::Deny, "{url}");
        }
        // A blob of another origin, or with nothing configured, is not the server's own.
        assert_eq!(
            decide_ambiguous("blob:https://evil.example/x", Some(&s), false),
            Decision::Deny
        );
        assert_eq!(
            decide_ambiguous("blob:https://app.dynodoc.online/x", None, false),
            Decision::Deny
        );
        assert_eq!(
            decide_ambiguous("blob:null/abc", Some(&s), false),
            Decision::Deny
        );
        // Everything else the strict rule refuses is still refused when ambiguous.
        for url in [
            "javascript:alert(1)",
            "file:///C:/Windows/win.ini",
            "about:srcdoc#x",
            "about:config",
            "view-source:https://app.dynodoc.online/",
            "https://user@app.dynodoc.online/",
            "dynodoc://open/workspace",
        ] {
            assert_eq!(
                decide_ambiguous(url, Some(&s), false),
                Decision::Deny,
                "{url}"
            );
        }
        // A frame-like address does not touch the launcher: the launcher still needs the
        // shell's permission, and a frame request for it without one is refused.
        let launcher = launcher_origin();
        assert_eq!(
            decide(&launcher_url(None), Some(&s), &launcher, false, true),
            Decision::Deny
        );
        // Public links still go out, mail and local addresses still do not, when ambiguous.
        assert_eq!(
            decide_ambiguous("https://dynodoc.online/", Some(&s), false),
            Decision::OpenExternal
        );
        assert_eq!(
            decide_ambiguous("mailto:a@b.c", Some(&s), false),
            Decision::Deny
        );
        assert_eq!(
            decide_ambiguous("http://127.0.0.1/", Some(&s), false),
            Decision::Deny
        );
        assert_eq!(frames_are_ambiguous(), !cfg!(windows));
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
    fn the_launcher_loads_again_while_no_server_is_configured_but_never_for_a_page() {
        let launcher = launcher_origin();
        let page = launcher_url(None);
        // Nothing configured: the shell's own window may load its launcher any number of
        // times (Android re-creates the web view), with or without a grant.
        for _ in 0..3 {
            assert!(launcher_load_allowed(&page, &launcher, false, || false));
        }
        // A server is configured: only the shell's grant opens it, once.
        let mut grants = vec![true, false];
        assert!(launcher_load_allowed(&page, &launcher, true, || grants.remove(0)));
        assert!(!launcher_load_allowed(&page, &launcher, true, || grants.remove(0)));
        assert!(!launcher_load_allowed(&page, &launcher, true, || false));
        // Other addresses on the launcher's origin never load, and never spend a grant.
        for other in ["/other.html", "/launcher.js", "/index.html/x"] {
            let target = launcher.join(other).unwrap();
            for configured in [false, true] {
                assert!(!launcher_load_allowed(
                    &target,
                    &launcher,
                    configured,
                    || { panic!("the grant is only taken for the launcher's page") }
                ));
            }
        }
        // Another origin is not the launcher whatever is configured.
        let elsewhere = Url::parse("https://app.dynodoc.online/").unwrap();
        assert!(!launcher_load_allowed(&elsewhere, &launcher, false, || {
            panic!("not the launcher")
        }));
        // And the decision that follows still denies it when the answer was no.
        let s = server("https://app.dynodoc.online");
        assert_eq!(
            decide(&page, Some(&s), &launcher, false, false),
            Decision::Deny
        );
    }

    #[test]
    fn a_refused_launcher_load_goes_to_the_servers_entry_when_there_is_a_server() {
        let launcher = launcher_origin();
        let s = server("https://app.dynodoc.online");
        for target in [
            launcher_url(None),
            launcher.clone(),
            launcher_url(Some("key")),
        ] {
            assert_eq!(
                launcher_denied_goes_to(&target, &launcher, Some(&s)),
                Some(s.entry()),
                "{target}"
            );
            assert_eq!(launcher_denied_goes_to(&target, &launcher, None), None);
        }
        for other in ["/other.html", "/launcher.js", "/index.html/x"] {
            let target = launcher.join(other).unwrap();
            assert_eq!(launcher_denied_goes_to(&target, &launcher, Some(&s)), None);
        }
        let elsewhere = Url::parse("https://evil.example/index.html").unwrap();
        assert_eq!(
            launcher_denied_goes_to(&elsewhere, &launcher, Some(&s)),
            None
        );
    }

    #[test]
    fn only_the_servers_own_sign_in_page_asks_for_the_browser_sign_in() {
        let s = server("https://app.dynodoc.online");
        let yes = |u: &str| needs_sign_in(&Url::parse(u).unwrap(), Some(&s));
        for url in [
            "https://app.dynodoc.online/login",
            "https://app.dynodoc.online/login?next=%2Fworkspace",
            "https://APP.dynodoc.online:443/login?next=%2Fprojects#x",
        ] {
            assert!(yes(url), "{url}");
        }
        for url in [
            // The pasted-link route finishes the sign-in itself.
            "https://app.dynodoc.online/login?token=abc",
            "https://app.dynodoc.online/login?next=%2Fworkspace&token=abc",
            // Every other page of the server, and anything look-alike.
            "https://app.dynodoc.online/workspace",
            "https://app.dynodoc.online/",
            "https://app.dynodoc.online/login/",
            "https://app.dynodoc.online/login.html",
            "https://app.dynodoc.online/loginx",
            "https://app.dynodoc.online/auth/login",
            "https://app.dynodoc.online/app-signin",
            "https://app.dynodoc.online/app-signin/finish",
            // Another server, scheme or port, or credentials, is not this server's page.
            "https://evil.example/login",
            "https://app.dynodoc.online.evil.example/login",
            "http://app.dynodoc.online/login",
            "https://app.dynodoc.online:8443/login",
            "https://user@app.dynodoc.online/login",
            "tauri://localhost/login",
            "http://tauri.localhost/login",
        ] {
            assert!(!yes(url), "{url}");
        }
        // With no server configured nothing is "the server's sign-in page".
        assert!(!needs_sign_in(
            &Url::parse("https://app.dynodoc.online/login").unwrap(),
            None
        ));
    }

    #[test]
    fn only_the_servers_own_finish_page_receives_the_sign_in_fragment() {
        let s = server("https://app.dynodoc.online");
        let yes = |u: &str| is_finish_page(&Url::parse(u).unwrap(), Some(&s));
        assert!(yes("https://app.dynodoc.online/app-signin/finish"));
        assert!(yes("https://APP.dynodoc.online:443/app-signin/finish"));
        for url in [
            // Not as the shell opens it: a query or a fragment already there.
            "https://app.dynodoc.online/app-signin/finish?x=1",
            "https://app.dynodoc.online/app-signin/finish#code=a",
            "https://app.dynodoc.online/app-signin/finish#",
            // Other pages of the server, and look-alikes.
            "https://app.dynodoc.online/app-signin",
            "https://app.dynodoc.online/app-signin/finish/",
            "https://app.dynodoc.online/app-signin/finish/x",
            "https://app.dynodoc.online/app-signin/finish.html",
            "https://app.dynodoc.online/login",
            "https://app.dynodoc.online/workspace",
            "https://app.dynodoc.online/",
            // Another origin, scheme, port, or credentials.
            "https://evil.example/app-signin/finish",
            "https://app.dynodoc.online.evil.example/app-signin/finish",
            "http://app.dynodoc.online/app-signin/finish",
            "https://app.dynodoc.online:8443/app-signin/finish",
            "https://user@app.dynodoc.online/app-signin/finish",
            "tauri://localhost/app-signin/finish",
            "http://tauri.localhost/app-signin/finish",
        ] {
            assert!(!yes(url), "{url}");
        }
        assert!(!is_finish_page(
            &Url::parse("https://app.dynodoc.online/app-signin/finish").unwrap(),
            None
        ));
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
                NewWindow::ClickOpen(Url::parse(target).unwrap()),
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
