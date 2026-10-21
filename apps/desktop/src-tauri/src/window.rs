//! The one window, and the hooks that apply `nav`, `handoff`, `deeplink` and `server` to it.

use crate::handoff::{self, Source};
use crate::nav::{self, Decision, NewWindow};
use crate::{deeplink, AppState};
use std::sync::Mutex;
use std::time::Instant;
use tauri::webview::{NewWindowResponse, PermissionKind, PermissionResponse};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_opener::OpenerExt;
use url::Url;

pub const MAIN: &str = "main";

/// Recognises a person's click and makes `target="_blank"` reach the new-window rule (see
/// the file). `__GESTURE_SECRET__` is replaced by a secret made at start-up.
const LINK_TARGETS: &str = include_str!("link_targets.js");
const SECRET_PLACEHOLDER: &str = "__GESTURE_SECRET__";

/// The injected script for this run of the app.
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

/// Only one thread creates the window at a time (start-up and a link that arrives meanwhile).
static CREATE: Mutex<()> = Mutex::new(());

/// Links that arrived before the app finished starting.
static EARLY_LINKS: Mutex<deeplink::LinkQueue> = Mutex::new(deeplink::LinkQueue::new());

/// Create the window: on the configured server when there is one, otherwise on the launcher.
/// Returns the existing window if there already is one.
pub fn create_main(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let _creating = CREATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = main_window(app) {
        return Ok(existing);
    }
    let state = app.state::<AppState>();
    let start = match state.server() {
        Some(server) => WebviewUrl::External(server.root()),
        None => {
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
        .initialization_script(link_script(&state.click_secret))
        .on_navigation(move |url| {
            let state = navigation_app.state::<AppState>();
            let launcher = nav::launcher_origin();
            // The launcher loads only for a navigation the shell started (one-shot).
            let launcher_allowed = nav::is_launcher(url, &launcher) && state.take_launcher_grant();
            let decision = nav::decide(url, state.server().as_ref(), &launcher, launcher_allowed);
            trace("navigation", url, &decision);
            match decision {
                Decision::Allow => true,
                Decision::OpenExternal => {
                    open_external(&navigation_app, url, Source::Page);
                    false
                }
                Decision::Deny => false,
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
                NewWindow::PersonOpen(target) => {
                    // The marker's secret is not logged; the target is the person's click.
                    trace("new window (click)", &target, &"person asks to open");
                    open_external(&new_window_app, &target, Source::Person)
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
            }
        });

    #[cfg(desktop)]
    {
        builder = builder
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
    #[cfg(desktop)]
    {
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(window)
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
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let opener = app.clone();
    let outcome = handoff::hand_off(
        url,
        source,
        &state.handoffs,
        Instant::now(),
        move |target| {
            std::thread::spawn(move || {
                let _ = opener.opener().open_url(target, None::<&str>);
            });
        },
    );
    // Every refusal is silent: the person sees nothing, a page learns nothing.
    trace("hand-off", url, &outcome);
}

/// Route `dynodoc://` links (at start-up and while running) through the parser, and handle
/// the ones that arrived before the app was ready.
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
    let early = EARLY_LINKS
        .lock()
        .map(|mut q| q.drain())
        .unwrap_or_default();
    for raw in early {
        open_deep_link(app, &raw);
    }
}

/// Validate a link, then open it on the configured server only. A link that is refused, or
/// that arrives before a server has been chosen, changes nothing except showing the window.
/// One that arrives before the app has finished starting is kept and handled at the end of
/// start-up (a few at most).
pub fn open_deep_link(app: &AppHandle, raw: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        if let Ok(mut queue) = EARLY_LINKS.lock() {
            queue.push(raw);
        }
        return;
    };
    let Ok(target) = deeplink::parse(raw) else {
        return;
    };
    match state.server().and_then(|s| s.join(&target.path())) {
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
        let secret = handoff::new_secret();
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
    fn the_hand_off_always_goes_through_the_rules() {
        // Source check: the only call that opens anything is inside the closure given to
        // hand_off, and the opener is not used anywhere else in this file.
        let source = include_str!("window.rs");
        let production = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        assert_eq!(production.matches(".opener()").count(), 1);
        let call = production.find("handoff::hand_off(").unwrap();
        assert!(production.find(".opener()").unwrap() > call);
    }
}
