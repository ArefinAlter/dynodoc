//! Where the main window may go. Pure functions over `url::Url`; the Tauri hooks in
//! `window.rs` only call them.
//!
//! The window shows the local launcher, or the configured server. Everything else is a
//! link out: it opens in the system browser and never in the window. Anything that is not
//! a plain web link is dropped.

use crate::server::{same_origin, ServerOrigin, MAX_URL_LEN};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Load it in the window.
    Allow,
    /// Hand it to the system browser (or mail program) and keep the window where it is.
    OpenExternal,
    /// Drop it silently.
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewWindow {
    /// A same-server link that wanted a new window: show it in the one window instead.
    NavigateHere(Url),
    OpenExternal(Url),
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

pub fn decide(target: &Url, server: Option<&ServerOrigin>, launcher: &Url) -> Decision {
    if target.as_str().len() > MAX_URL_LEN {
        return Decision::Deny;
    }
    // https://trusted.example@evil.example: never loaded, never handed on.
    if !target.username().is_empty() || target.password().is_some() {
        return Decision::Deny;
    }
    if is_launcher(target, launcher) {
        return Decision::Allow;
    }
    match target.scheme() {
        "about" if target.as_str() == "about:blank" => Decision::Allow,
        "http" | "https" => {
            if server.is_some_and(|s| s.matches(target)) {
                Decision::Allow
            } else if target.host().is_some() {
                Decision::OpenExternal
            } else {
                Decision::Deny
            }
        }
        "mailto" => Decision::OpenExternal,
        // javascript:, data:, file:, blob:, ftp:, other apps' schemes, other origins' tauri://
        _ => Decision::Deny,
    }
}

/// `window.open` and `target="_blank"` follow the same rule as navigation, and never
/// create a second window.
pub fn new_window(target: &Url, server: Option<&ServerOrigin>, launcher: &Url) -> NewWindow {
    match decide(target, server, launcher) {
        Decision::OpenExternal => NewWindow::OpenExternal(target.clone()),
        Decision::Allow if server.is_some_and(|s| s.matches(target)) => {
            NewWindow::NavigateHere(target.clone())
        }
        _ => NewWindow::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::parse_server_address;

    fn server(text: &str) -> ServerOrigin {
        parse_server_address(text).unwrap()
    }
    fn decide_for(url: &str, configured: Option<&ServerOrigin>) -> Decision {
        decide(&Url::parse(url).unwrap(), configured, &launcher_origin())
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
    fn other_web_origins_leave_for_the_system_browser() {
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
            "https://[::1]/",
            "https://127.0.0.1/",
            "https://xn--pp-8ma.dynodoc.online/",
            "https://аpp.dynodoc.online/",
            "mailto:support@dynodoc.online",
        ] {
            assert_eq!(decide_for(url, Some(&s)), Decision::OpenExternal, "{url}");
        }
        // Nothing configured yet: every web link is external.
        assert_eq!(
            decide_for("https://app.dynodoc.online/", None),
            Decision::OpenExternal
        );
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
    fn non_web_schemes_are_dropped() {
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
            "about:srcdoc",
            "about:blank#x",
            "view-source:https://app.dynodoc.online/",
            "chrome://settings",
            "ipc://localhost/set_server",
        ] {
            assert_eq!(decide_for(url, Some(&s)), Decision::Deny, "{url}");
        }
        assert_eq!(decide_for("about:blank", Some(&s)), Decision::Allow);
    }

    #[test]
    fn only_the_launcher_origin_counts_as_the_launcher() {
        let launcher = launcher_origin();
        let s = server("https://app.dynodoc.online");
        assert_eq!(
            decide(&launcher_url(None), Some(&s), &launcher),
            Decision::Allow
        );
        assert_eq!(
            decide(&launcher_url(Some("key")), None, &launcher),
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
                decide(&target, Some(&s), &launcher),
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

    #[test]
    fn new_windows_follow_the_same_rule_and_never_open() {
        let launcher = launcher_origin();
        let s = server("https://app.dynodoc.online");
        let go = |u: &str| new_window(&Url::parse(u).unwrap(), Some(&s), &launcher);
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
        ] {
            let result = go(dropped);
            assert!(
                matches!(result, NewWindow::Ignore | NewWindow::OpenExternal(_)),
                "{dropped}: {result:?}"
            );
            assert!(!matches!(result, NewWindow::NavigateHere(_)), "{dropped}");
        }
        assert_eq!(go("javascript:alert(1)"), NewWindow::Ignore);
        assert_eq!(go("about:blank"), NewWindow::Ignore);
    }

    #[test]
    fn launcher_url_accepts_only_known_sections() {
        assert!(launcher_url(None).fragment().is_none());
        assert_eq!(launcher_url(Some("signin")).fragment(), Some("signin"));
        assert_eq!(launcher_url(Some("key")).fragment(), Some("key"));
        assert!(launcher_url(Some("../../x")).fragment().is_none());
        assert!(launcher_url(Some("<script>")).fragment().is_none());
    }
}
