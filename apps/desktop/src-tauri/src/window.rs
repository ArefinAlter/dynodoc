//! The one window, and the hooks that apply `nav`, `deeplink` and `server` to it.

use crate::nav::{self, Decision, NewWindow};
use crate::{deeplink, AppState};
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_opener::OpenerExt;
use url::Url;

pub const MAIN: &str = "main";

/// Makes a click on `<a target="_blank">` reach the new-window rule (see the file).
const LINK_TARGETS: &str = include_str!("link_targets.js");

/// Debug builds print each navigation decision to standard error (the security probe reads
/// it). Release builds print nothing.
fn trace(what: &str, url: &Url, decision: &dyn std::fmt::Debug) {
    #[cfg(debug_assertions)]
    eprintln!("dynodoc: {what} {url} -> {decision:?}");
    #[cfg(not(debug_assertions))]
    let _ = (what, url, decision);
}

/// Create the window: on the configured server when there is one, otherwise on the launcher.
pub fn create_main(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let state = app.state::<AppState>();
    let start = match state.server() {
        Some(server) => WebviewUrl::External(server.root()),
        None => WebviewUrl::App("index.html".into()),
    };

    let navigation_app = app.clone();
    let new_window_app = app.clone();
    let mut builder = WebviewWindowBuilder::new(app, MAIN, start)
        .title("Dynodoc")
        // The inspector exists only in debug builds (the `devtools` cargo feature is off).
        .devtools(cfg!(debug_assertions))
        .initialization_script(LINK_TARGETS)
        .on_navigation(move |url| {
            let state = navigation_app.state::<AppState>();
            let decision = nav::decide(url, state.server().as_ref(), &nav::launcher_origin());
            trace("navigation", url, &decision);
            match decision {
                Decision::Allow => true,
                Decision::OpenExternal => {
                    open_external(&navigation_app, url);
                    false
                }
                Decision::Deny => false,
            }
        })
        .on_new_window(move |url, _features| {
            let state = new_window_app.state::<AppState>();
            let action = nav::new_window(&url, state.server().as_ref(), &nav::launcher_origin());
            trace("new window", &url, &action);
            match action {
                NewWindow::NavigateHere(target) => navigate_main(&new_window_app, target),
                NewWindow::OpenExternal(target) => open_external(&new_window_app, &target),
                NewWindow::Ignore => {}
            }
            NewWindowResponse::Deny
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

/// Bring the window forward (a second launch, a dynodoc:// link).
pub fn focus_main(app: &AppHandle) {
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

/// Load `url` in the window. It goes through `on_navigation` like any other navigation.
/// Runs on a worker thread so it can be called from inside a webview callback.
pub fn navigate_main(app: &AppHandle, url: Url) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(window) = main_window(&app) {
            let _ = window.navigate(url);
        }
    });
}

pub fn show_launcher(app: &AppHandle, section: Option<&str>) {
    focus_main(app);
    navigate_main(app, nav::launcher_url(section));
}

/// Open a web or mail link in the system's own program. Refuses anything else.
pub fn open_external(app: &AppHandle, url: &Url) {
    if !matches!(url.scheme(), "http" | "https" | "mailto") {
        return;
    }
    let app = app.clone();
    let target = url.to_string();
    std::thread::spawn(move || {
        let _ = app.opener().open_url(target, None::<&str>);
    });
}

/// Route `dynodoc://` links (at start-up and while running) through the parser.
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
pub fn open_deep_link(app: &AppHandle, raw: &str) {
    let Ok(target) = deeplink::parse(raw) else {
        return;
    };
    let server = app.state::<AppState>().server();
    match server.and_then(|s| s.join(&target.path())) {
        Some(url) => {
            focus_main(app);
            navigate_main(app, url);
        }
        None => show_launcher(app, None),
    }
}
