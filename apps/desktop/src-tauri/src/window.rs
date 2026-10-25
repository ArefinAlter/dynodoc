//! The one window, and the hooks that apply `nav`, `handoff`, `deeplink` and `server` to it.

use crate::handoff::{self, Source};
use crate::nav::{self, Decision, NewWindow};
use crate::{deeplink, signin, AppState};
use std::sync::Mutex;
use std::time::Instant;
use tauri::webview::{NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_opener::OpenerExt;
use url::Url;

pub const MAIN: &str = "main";

/// Recognises a person's click and makes `target="_blank"` reach the new-window rule (see
/// the file). `__GESTURE_SECRET__` is replaced by a secret made at start-up.
///
/// DESKTOP ONLY. On Android the web view has no new-window event: `window.open` loads in the one
/// web view and reaches the navigation rule like any other link, so a marker address would arrive
/// there as a navigation and be dropped (every link out would do nothing), and the secret would sit
/// in the page's world for no gain. Android gets `blank_targets.js` instead: no secret, no marker.
#[cfg(desktop)]
const LINK_TARGETS: &str = include_str!("link_targets.js");
#[cfg(desktop)]
const SECRET_PLACEHOLDER: &str = "__GESTURE_SECRET__";

/// What Android injects: only makes a tap on target="_blank" load in the one window (the web view
/// drops it otherwise). Nothing secret in it.
#[cfg(mobile)]
const BLANK_TARGETS: &str = include_str!("blank_targets.js");

/// The injected script for this run of the app.
#[cfg(desktop)]
pub fn link_script(secret: &str) -> String {
    LINK_TARGETS.replace(SECRET_PLACEHOLDER, secret)
}

/// The inspector exists only in debug builds (the `devtools` cargo feature is off, so a
/// release build has none whatever this says).
pub const fn devtools_enabled() -> bool {
    cfg!(debug_assertions)
}

/// What every web view permission request gets: no. Camera, microphone, location,
/// notifications, clipboard reading, sensors, MIDI, screen capture and the rest.
pub fn permission_response(_kind: PermissionKind) -> PermissionResponse {
    PermissionResponse::Deny
}

/// Debug builds print each decision to standard error (the security probe reads it), with
/// any query or fragment cut off so a token never reaches a log. Release builds print nothing.
fn trace(what: &str, url: &Url, decision: &dyn std::fmt::Debug) {
    #[cfg(debug_assertions)]
    eprintln!("dynodoc: {what} {} -> {decision:?}", nav::redacted(url));
    #[cfg(not(debug_assertions))]
    let _ = (what, url, decision);
}

/// A debug-build note with no address and nothing secret in it.
fn trace_note(what: &str, note: &dyn std::fmt::Debug) {
    #[cfg(debug_assertions)]
    eprintln!("dynodoc: {what} -> {note:?}");
    #[cfg(not(debug_assertions))]
    let _ = (what, note);
}

/// Only one thread creates the window at a time (start-up and a second launch meanwhile).
static CREATE: Mutex<()> = Mutex::new(());

/// Create the window. On desktop it opens on the launcher page, which is also the client: with a
/// server chosen and a session for it the client shows that server's workspace (remote.rs), and
/// View > Web workspace opens the server's own pages in this window. On phones it opens on the
/// configured server when there is one, otherwise on the launcher.
/// Returns the existing window if there already is one.
pub fn create_main(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let _creating = CREATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = main_window(app) {
        return Ok(existing);
    }
    let state = app.state::<AppState>();
    let start = match state.server() {
        Some(server) if cfg!(mobile) => WebviewUrl::External(server.entry()),
        _ => {
            // The shell itself starts at the launcher: the one navigation it may make there.
            state.arm_launcher_grant();
            WebviewUrl::App("index.html".into())
        }
    };

    let navigation_app = app.clone();
    let new_window_app = app.clone();
    let mut builder = WebviewWindowBuilder::new(app, MAIN, start)
        .title("Dynodoc")
        .devtools(devtools_enabled())
        .on_navigation(move |url| {
            let state = navigation_app.state::<AppState>();
            let launcher = nav::launcher_origin();
            // The launcher loads only for a navigation the shell started (one-shot).
            // Only the launcher's own page can use the grant: nothing else on that origin does.
            let launcher_allowed =
                nav::launcher_load_allowed(url, &launcher, state.server().is_some(), || {
                    state.take_launcher_grant()
                });
            let decision = nav::decide(
                url,
                state.server().as_ref(),
                &launcher,
                launcher_allowed,
                nav::frames_are_ambiguous(),
            );
            trace("navigation", url, &decision);
            match decision {
                Decision::Allow => true,
                Decision::OpenExternal => {
                    open_external(&navigation_app, url, Source::Page);
                    false
                }
                Decision::Deny => {
                    // Phones only: a refused launcher load (the system re-created the web view,
                    // which reloaded the window's first address) must not leave a blank window.
                    #[cfg(mobile)]
                    if let Some(entry) =
                        nav::launcher_denied_goes_to(url, &launcher, state.server().as_ref())
                    {
                        trace(
                            "navigation (launcher refused, server saved)",
                            &entry,
                            &"entry",
                        );
                        navigate_main(&navigation_app, entry);
                    }
                    false
                }
            }
        })
        .on_new_window(move |url, _features| {
            let state = new_window_app.state::<AppState>();
            let action = nav::new_window(
                &url,
                state.server().as_ref(),
                &nav::launcher_origin(),
                &state.click_secret,
            );
            match action {
                NewWindow::NavigateHere(target) => {
                    trace("new window", &url, &"navigate here");
                    navigate_main(&new_window_app, target)
                }
                NewWindow::OpenExternal(target) => {
                    trace("new window", &url, &"page asks to open");
                    open_external(&new_window_app, &target, Source::Page)
                }
                NewWindow::ClickOpen(target) => {
                    // The marker's secret is not logged; the target is the clicked link.
                    trace("new window (click)", &target, &"click asks to open");
                    open_external(&new_window_app, &target, Source::Click)
                }
                NewWindow::Ignore => trace("new window", &url, &"ignored"),
            }
            NewWindowResponse::Deny
        })
        .on_page_load(|window, payload| {
            // The title always names the server the page is from.
            if let Some(state) = window.app_handle().try_state::<AppState>() {
                let title = nav::window_title(payload.url(), state.server().as_ref());
                let _ = window.set_title(&title);
                // The browser sign-in's last step: the finish page has loaded, so hand it the
                // fragment the shell is holding for it (once, only to that page; see signin.rs).
                if matches!(payload.event(), PageLoadEvent::Finished)
                    && nav::is_finish_page(payload.url(), state.server().as_ref())
                {
                    // The fragment is only for the server it was made for (the outbox compares), and
                    // the script itself checks the page's origin and path again before it acts.
                    let origin = state.server().map(|server| server.as_text());
                    if let Some(script) = origin.as_deref().and_then(|origin| {
                        state
                            .outbox
                            .take(signin::Moment::now(), origin)
                            .and_then(|fragment| signin::finish_script(fragment.expose(), origin))
                    }) {
                        trace(
                            "page load",
                            payload.url(),
                            &"finish page, handing it the fragment",
                        );
                        let _ = window.eval(script.expose());
                    }
                }
                // Phones only (no menu to reach the launcher from): when the window has landed
                // on the server's sign-in page, which cannot sign this window in, show the
                // launcher and its "Sign in with your browser" button instead. Decided from the
                // address alone; see `nav::needs_sign_in`.
                #[cfg(mobile)]
                if matches!(payload.event(), PageLoadEvent::Finished)
                    && nav::needs_sign_in(payload.url(), state.server().as_ref())
                {
                    trace(
                        "page load",
                        payload.url(),
                        &"sign-in page, showing the launcher",
                    );
                    show_launcher(window.app_handle(), Some("signin"));
                }
            }
        });

    #[cfg(mobile)]
    {
        builder = builder.initialization_script(BLANK_TARGETS);
    }
    #[cfg(desktop)]
    {
        builder = builder
            .initialization_script(link_script(&state.click_secret))
            .inner_size(1280.0, 820.0)
            .min_inner_size(800.0, 560.0)
            .center()
            // Shown below, after the window-state plugin has put it back where it was.
            .visible(false);
    }
    if let Some(dir) = &state.webview_dir {
        builder = builder.data_directory(dir.clone());
    }

    let window = builder.build()?;
    // Names the app's own window to the server (decision 017). Where this build cannot do that
    // the browser sign-in is not offered (`signin::APP_TOKEN`).
    #[cfg(windows)]
    append_app_token(&window);
    #[cfg(desktop)]
    {
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(window)
}

/// Windows: add `DynodocApp/<version> (desktop)` to the end of WebView2's own User-Agent, for
/// every request this window makes from now on (the finish page's loads and its redeem call among
/// them). The server's app sign-in finish page and routes work only for a window that carries it;
/// a browser does not. (The first page loaded at start-up may precede it, which matters to none of
/// them.) Android adds its token in Kotlin (`MainActivity`); other systems add none, and the
/// browser sign-in is not offered there (`signin::APP_TOKEN`).
#[cfg(windows)]
fn append_app_token(window: &WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings2;
    use windows_core::{Interface, HSTRING, PWSTR};

    let token = signin::app_token();
    let _ = window.with_webview(move |webview| {
        // SAFETY: calls on the WebView2 objects Tauri hands this closure, on the thread that owns
        // them; the string WebView2 returns is freed by `take_pwstr`.
        unsafe {
            let Ok(core) = webview.controller().CoreWebView2() else {
                return;
            };
            let Ok(settings) = core.Settings() else {
                return;
            };
            let Ok(settings2) = settings.cast::<ICoreWebView2Settings2>() else {
                return;
            };
            let mut current = PWSTR::null();
            if settings2.UserAgent(&mut current).is_err() {
                return;
            }
            let agent = webview2_com::take_pwstr(current);
            if agent.contains("DynodocApp/") {
                return;
            }
            let _ = settings2.SetUserAgent(&HSTRING::from(format!("{agent} {token}")));
        }
    });
}

pub fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN)
}

/// Bring the window forward (a second launch, a dynodoc:// link). Does nothing before the
/// app has finished starting (its state does not exist yet).
pub fn focus_main(app: &AppHandle) {
    if app.try_state::<AppState>().is_none() {
        return;
    }
    let window = match main_window(app) {
        Some(window) => window,
        None => match create_main(app) {
            Ok(window) => window,
            Err(_) => return,
        },
    };
    #[cfg(desktop)]
    {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    #[cfg(mobile)]
    let _ = window;
}

/// Load `url` in the window. It goes through `on_navigation` like any other navigation; the
/// launcher is allowed through it only because this call said so. Runs on a worker thread so
/// it can be called from inside a webview callback.
pub fn navigate_main(app: &AppHandle, url: Url) {
    if nav::is_launcher(&url, &nav::launcher_origin()) {
        if let Some(state) = app.try_state::<AppState>() {
            state.arm_launcher_grant();
        }
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(window) = main_window(&app) {
            let _ = window.navigate(url);
        }
    });
}

/// Reload the page. The launcher reloads only because this call said so.
#[cfg(desktop)]
pub fn reload_main(app: &AppHandle) {
    let Some(window) = main_window(app) else {
        return;
    };
    if window
        .url()
        .is_ok_and(|url| nav::is_launcher(&url, &nav::launcher_origin()))
    {
        if let Some(state) = app.try_state::<AppState>() {
            state.arm_launcher_grant();
        }
    }
    let _ = window.reload();
}

pub fn show_launcher(app: &AppHandle, section: Option<&str>) {
    focus_main(app);
    navigate_main(app, nav::launcher_url(section));
}

/// Hand a link to the system's own browser or mail program, if `handoff` allows it for this
/// source. A page's hand-offs are limited to public web addresses at a limited rate; the
/// rest are dropped silently (and logged in a debug build).
pub fn open_external(app: &AppHandle, url: &Url, source: Source) {
    open_external_then(app, url, source, || {});
}

/// Like [`open_external`], and `before` runs just before the system is asked to open the link,
/// and only if it will be opened. Returns what `handoff` decided.
fn open_external_then(
    app: &AppHandle,
    url: &Url,
    source: Source,
    before: impl FnOnce(),
) -> Option<handoff::HandOff> {
    let state = app.try_state::<AppState>()?;
    let opener = app.clone();
    let outcome = handoff::hand_off(
        url,
        source,
        &state.handoffs,
        Instant::now(),
        move |target| {
            before();
            std::thread::spawn(move || {
                let _ = opener.opener().open_url(target, None::<&str>);
            });
        },
    );
    // Every refusal is silent: the person sees nothing, a page learns nothing.
    trace("hand-off", url, &outcome);
    Some(outcome)
}

/// Why the browser sign-in could not be started; the words are the person's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInStart {
    NoServer,
    NoRandomness,
    TooSoon,
    /// This system's web view does not carry the app's token (see `signin::APP_TOKEN`), so the
    /// server would refuse the sign-in; it is not started.
    Unsupported,
}

impl std::fmt::Display for SignInStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoServer => "Choose a server first.",
            Self::NoRandomness => "This device could not make a safe sign-in request.",
            Self::TooSoon => "Please wait a moment, then try again.",
            Self::Unsupported => {
                "Signing in with the browser is not available on this system yet. Use an emailed sign-in link instead."
            }
        })
    }
}

/// Start signing in with the system browser (docs/decisions/017; `signin.rs` has the rules):
/// make the secrets, remember them as THE pending attempt (replacing any older one) and open
/// `<server>/app-signin?challenge=..&state=..&platform=..` in the browser. The attempt is only
/// kept if the browser is really asked to open (a page that was refused or rate limited leaves
/// the older attempt alone). Called by the launcher's button and, on desktop, a menu item;
/// never by a page. Returns the four-character pairing code the launcher shows while the attempt
/// waits (the server's page shows the same four).
pub fn begin_browser_sign_in(app: &AppHandle) -> Result<String, SignInStart> {
    if !signin::APP_TOKEN {
        return Err(SignInStart::Unsupported);
    }
    let state = app.try_state::<AppState>().ok_or(SignInStart::NoServer)?;
    let server = state.server().ok_or(SignInStart::NoServer)?;
    let now = signin::Moment::now();
    let (attempt, request) = signin::Attempt::new(&server.as_text(), now, signin::os_random)
        .ok_or(SignInStart::NoRandomness)?;
    let url = server.app_signin_url(&request.challenge, &request.state, signin::PLATFORM);
    let pairing = signin::pairing_code(&request.state);
    let outcome = open_external_then(app, &url, Source::SignIn, || {
        state.outbox.clear();
        state.signin.start(attempt);
    });
    match outcome {
        Some(handoff::HandOff::Opened) => Ok(pairing),
        _ => Err(SignInStart::TooSoon),
    }
}

/// The browser has come back with a sign-in link: finish in this window. `signin.rs` decides
/// whether the link belongs to the attempt this app started; if it does, the window goes to the
/// server's finish page with the code, the state and the verifier in the address FRAGMENT. A
/// link that is refused changes nothing (the window is only brought forward) and says nothing.
fn finish_browser_sign_in(app: &AppHandle, state: &AppState, code: &str, link_state: &str) {
    let Some(server) = state.server() else {
        focus_main(app);
        return;
    };
    match state
        .signin
        .complete(signin::Moment::now(), &server.as_text(), code, link_state)
    {
        Ok(finish) => {
            // The secrets wait here, for this server's finish page only, and reach the page after
            // it has loaded; the window is sent to the plain address.
            state
                .outbox
                .put(finish.fragment(), &server.as_text(), signin::Moment::now());
            focus_main(app);
            navigate_main(app, server.finish_url());
        }
        Err(refusal) => {
            trace_note("sign-in link refused", &refusal);
            focus_main(app);
        }
    }
}

/// Route `dynodoc://` links (at start-up and while running) through the parser. Called at the
/// end of setup, after the app's state exists. A second launch that starts while the first is
/// still setting up reaches the deep-link plugin before this listener exists: the event is
/// lost, but the plugin keeps the most recent link and `get_current` below reads it (earlier
/// ones in that window are dropped).
pub fn watch_deep_links(app: &AppHandle) {
    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            open_deep_link(&handle, url.as_str());
        }
    });
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            open_deep_link(app, url.as_str());
        }
    }
}

/// Validate a link, then open it on the configured server only. A link that is refused, or
/// that arrives before a server has been chosen, changes nothing except showing the window.
/// Both callers run after the app's state exists, so `try_state` always succeeds here; it is
/// a guard, not a code path (a panic would abort the whole app).
pub fn open_deep_link(app: &AppHandle, raw: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(target) = deeplink::parse(raw) else {
        return;
    };
    // The browser sign-in coming back is not a page to open: it finishes the attempt this app
    // started, or does nothing.
    if let deeplink::Target::SignIn {
        code,
        state: link_state,
    } = &target
    {
        finish_browser_sign_in(app, &state, code, link_state);
        return;
    }
    match target
        .path()
        .and_then(|path| state.server().and_then(|s| s.join(&path)))
    {
        Some(url) => {
            focus_main(app);
            navigate_main(app, url);
        }
        None => show_launcher(app, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_permission_request_is_refused() {
        for kind in [
            PermissionKind::Microphone,
            PermissionKind::Camera,
            PermissionKind::Geolocation,
            PermissionKind::Notifications,
            PermissionKind::ClipboardRead,
            PermissionKind::DisplayCapture,
            PermissionKind::Midi,
            PermissionKind::Sensors,
            PermissionKind::MediaKeySystemAccess,
            PermissionKind::LocalFonts,
            PermissionKind::WindowManagement,
            PermissionKind::PointerLock,
            PermissionKind::AutomaticDownloads,
            PermissionKind::FileSystemAccess,
            PermissionKind::Autoplay,
            PermissionKind::Other,
        ] {
            assert_eq!(
                permission_response(kind),
                PermissionResponse::Deny,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn the_inspector_is_a_debug_build_thing() {
        assert_eq!(devtools_enabled(), cfg!(debug_assertions));
        // Source check: the builder takes the value from the function, not a literal.
        let source = include_str!("window.rs");
        let production = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        assert!(production.contains(".devtools(devtools_enabled())"));
        assert!(!production.contains(".devtools(true)"));
    }

    #[test]
    fn the_injected_script_gets_this_runs_secret_and_nothing_else_changes() {
        let secret = handoff::new_secret().unwrap();
        let script = link_script(&secret);
        assert!(script.contains(&secret));
        assert!(!script.contains(SECRET_PLACEHOLDER));
        assert_eq!(
            script.len(),
            LINK_TARGETS.len() - SECRET_PLACEHOLDER.len() + secret.len()
        );
        assert!(LINK_TARGETS.contains(SECRET_PLACEHOLDER));
    }

    #[test]
    fn the_click_script_is_injected_on_desktop_only() {
        // On Android nothing of ours runs in the page: the marker would come back as a
        // navigation and be dropped (links out would do nothing) and its secret would sit in
        // the page's world for no gain. See the comment on LINK_TARGETS.
        let source = include_str!("window.rs");
        let production = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        // Two calls: the secret-carrying script (desktop block) and Android's own (below).
        assert_eq!(production.matches(".initialization_script(").count(), 2);
        let call = production
            .find(".initialization_script(link_script(")
            .unwrap();
        let gate = production[..call].rfind("#[cfg(").unwrap();
        assert!(production[gate..].starts_with("#[cfg(desktop)]"));
        assert!(
            !production[gate..call].contains("\n    }\n"),
            "the call must sit inside the desktop block"
        );
        assert!(production.contains("#[cfg(desktop)]\nconst LINK_TARGETS"));
        // Android injects its own script, in a mobile-only block, and nothing else.
        assert!(production.contains("#[cfg(mobile)]\nconst BLANK_TARGETS"));
        assert!(production.contains(
            "#[cfg(mobile)]\n    {\n        builder = builder.initialization_script(BLANK_TARGETS);\n    }"
        ));
    }

    #[test]
    fn the_android_script_has_no_secret_no_window_open_and_no_page_reachable_state() {
        let script = include_str!("blank_targets.js");
        assert!(!script.contains(SECRET_PLACEHOLDER));
        assert!(!script.contains("secret =") && !script.contains("var secret"));
        let code: String = script
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for banned in [
            "window.open",
            "open(",
            "invoke",
            "__TAURI",
            "fetch(",
            "XMLHttpRequest",
            "postMessage",
            "location",
            "eval(",
            "Function(",
        ] {
            assert!(!code.contains(banned), "blank_targets.js uses {banned}");
        }
        // It only ever turns _blank/_new into _self for a moment and puts it back.
        assert!(code.contains("'_self'"));
        assert!(code.contains("setTimeout"));
    }

    #[test]
    fn a_refused_launcher_load_is_redirected_on_phones_only() {
        // Desktop keeps refusing the launcher outright (a server page must not bring it up).
        let source = include_str!("window.rs");
        let production = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        let call = production.find("nav::launcher_denied_goes_to(").unwrap();
        let gate = production[..call].rfind("#[cfg(").unwrap();
        assert!(production[gate..].starts_with("#[cfg(mobile)]"));
        assert_eq!(production.matches("launcher_denied_goes_to(").count(), 1);
    }

    #[test]
    fn the_hand_off_always_goes_through_the_rules() {
        // Source check: the only call that opens anything is inside the closure given to
        // hand_off, and the opener is not used anywhere else in this file.
        let source = include_str!("window.rs");
        let production = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        assert_eq!(production.matches(".opener()").count(), 1);
        let call = production.find("handoff::hand_off(").unwrap();
        assert!(production.find(".opener()").unwrap() > call);
    }

    fn production() -> &'static str {
        let source = include_str!("window.rs");
        &source[..source.find("#[cfg(test)]\nmod tests").unwrap()]
    }

    /// The text with every space and line break removed, so a check does not depend on how the
    /// formatter happened to lay a call out.
    fn flat(text: &str) -> String {
        text.split_whitespace().collect()
    }

    /// The body of the function that starts at `signature`, up to the next top-level item.
    fn body(signature: &str) -> &'static str {
        let production = production();
        let start = production
            .find(signature)
            .unwrap_or_else(|| panic!("{signature}"));
        let rest = &production[start..];
        &rest[..rest.find("\n}\n").unwrap()]
    }

    #[test]
    fn the_browser_sign_in_is_kept_only_when_the_browser_is_really_asked() {
        // The attempt (and with it the verifier) is stored inside the closure that runs just
        // before the system is asked to open the link, so a refused or rate-limited hand-off
        // leaves the older attempt alone, and it is stored BEFORE the browser can answer.
        let begin = flat(body("pub fn begin_browser_sign_in("));
        assert_eq!(flat(production()).matches(".signin.start(").count(), 1);
        assert!(begin.contains(
            "open_external_then(app,&url,Source::SignIn,||{state.outbox.clear();state.signin.start(attempt);})"
        ));
        // It opens the page of the configured server, built from the server and the request.
        assert!(begin
            .contains("server.app_signin_url(&request.challenge,&request.state,signin::PLATFORM)"));
        // And randomness comes from the operating system or the attempt is not made.
        assert!(begin.contains("signin::os_random"));
        // Not at all where the web view cannot carry the app's token (the server would refuse it).
        assert!(begin.starts_with(
            "pubfnbegin_browser_sign_in(app:&AppHandle)->Result<String,SignInStart>{if!signin::APP_TOKEN{returnErr(SignInStart::Unsupported);}"
        ));
        let then = body("fn open_external_then(");
        let before = then.find("before();").unwrap();
        assert!(before > then.find("move |target|").unwrap());
        assert!(
            before
                < then
                    .find("opener.opener()")
                    .unwrap_or(then.find(".opener()").unwrap())
        );
    }

    #[test]
    fn only_the_sign_in_button_uses_the_sign_in_budget() {
        // Source::SignIn exists for begin_browser_sign_in alone: a page-driven hand-off (a click
        // marker, a navigation, a new window) must never spend the budget a person needs to sign
        // in, so it is not made anywhere else, the menu included (the menu calls begin).
        let all = flat(production());
        assert_eq!(all.matches("Source::SignIn").count(), 1);
        assert_eq!(
            all.matches("Source::Click").count(),
            1,
            "the click marker only"
        );
        assert!(flat(body("pub fn begin_browser_sign_in(")).contains("Source::SignIn"));
    }

    #[test]
    fn a_returning_sign_in_link_goes_through_the_pending_attempt_and_nothing_else() {
        // One place decides, and it is the pending attempt.
        assert_eq!(flat(production()).matches(".signin.complete(").count(), 1);
        let finish = body("fn finish_browser_sign_in(");
        // The window is sent to the plain finish address; the secrets wait in the outbox, for the
        // server they were made for.
        assert!(flat(finish).contains(
            "state.outbox.put(finish.fragment(),&server.as_text(),signin::Moment::now());"
        ));
        assert!(finish.contains("navigate_main(app, server.finish_url());"));
        // The secrets reach no address the window requests.
        for banned in ["eprintln", "println", "format!", "trace("] {
            assert!(
                !finish.contains(banned),
                "finish_browser_sign_in uses {banned}"
            );
        }
        assert!(finish.contains("trace_note(\"sign-in link refused\", &refusal)"));
        // The deep-link handler hands the link to it and does nothing else with it.
        let open = body("pub fn open_deep_link(");
        assert!(open
            .contains("finish_browser_sign_in(app, &state, code, link_state);\n        return;"));
        // The debug note prints a note and no address.
        let note = body("fn trace_note(");
        assert!(!note.contains("url"));
    }

    #[test]
    fn the_fragment_is_handed_over_once_and_only_to_the_finish_page_after_it_loaded() {
        let production = production();
        let all = flat(production);
        // The only place the outbox is emptied, the only place a script is run in the page.
        assert_eq!(all.matches(".outbox.take(").count(), 1);
        assert_eq!(all.matches("window.eval(").count(), 1);
        let call = production.find("window.eval(script.expose())").unwrap();
        let before = &production[..call];
        let gate = before
            .rfind("if matches!(payload.event(), PageLoadEvent::Finished)")
            .unwrap();
        let block = flat(&production[gate..call]);
        assert!(block.contains("nav::is_finish_page(payload.url(),state.server().as_ref())"));
        // For the server configured NOW: what the outbox holds was made for one server, and the
        // script checks the page's origin again.
        assert!(block.contains("state.server().map(|server|server.as_text())"));
        assert!(block.contains(".take(signin::Moment::now(),origin)"));
        assert!(block.contains("signin::finish_script(fragment.expose(),origin)"));
        // The script is made by finish_script from the fragment, never from anything a page sent.
        assert!(!block.contains("payload.url().as_str()"));
        // A new attempt drops what an older one left behind.
        assert!(body("pub fn begin_browser_sign_in(").contains("state.outbox.clear();"));
        // The debug note names no secret.
        assert!(block.contains("finishpage,handingitthefragment"));
    }

    #[test]
    fn the_app_names_itself_in_the_user_agent_where_it_can_and_nowhere_else() {
        let production = production();
        // Windows appends to WebView2's own agent once the window exists; the function is Windows
        // only and called once.
        assert!(
            production.contains("#[cfg(windows)]\nfn append_app_token(window: &WebviewWindow) {")
        );
        assert!(production.contains("#[cfg(windows)]\n    append_app_token(&window);"));
        assert_eq!(production.matches("append_app_token(&window)").count(), 1);
        let function = body("fn append_app_token(");
        assert!(function.contains("signin::app_token()"));
        assert!(
            function.contains("agent.contains(\"DynodocApp/\")"),
            "never twice"
        );
        assert!(
            function.contains("format!(\"{agent} {token}\")"),
            "appended, not replaced"
        );
        assert!(
            !production.contains(".user_agent("),
            "the builder's call replaces the whole agent"
        );
        // Android adds its token in Kotlin, on the web view's own settings.
        let activity =
            include_str!("../gen/android/app/src/main/java/online/dynodoc/client/MainActivity.kt");
        assert!(activity
            .contains("userAgentString = AppToken.appended(userAgentString, appVersionName())"));
    }

    #[test]
    fn only_a_phone_hands_the_sign_in_page_to_the_launcher() {
        // Desktop has a menu; a phone has none, so there the server's sign-in page (which cannot
        // sign this window in) is replaced by the launcher with its browser sign-in button.
        let production = production();
        let call = production
            .find("show_launcher(window.app_handle(), Some(\"signin\"))")
            .unwrap();
        let gate = production[..call].rfind("#[cfg(").unwrap();
        assert!(production[gate..].starts_with("#[cfg(mobile)]"));
        assert_eq!(production.matches("nav::needs_sign_in(").count(), 1);
        // Decided from the finished page's address alone.
        assert!(production[gate..call].contains("PageLoadEvent::Finished"));
    }
}
