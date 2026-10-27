//! The native menus. Desktop only.
//!
//! On Windows and Linux the client window has no native menu bar: the client draws its own title
//! bar and application menu (and the command palette, Ctrl+K). The web window, which shows the
//! configured server's own pages in the system's frame, has the small menu below. On macOS the
//! one menu bar serves both windows and has the whole set.

use crate::handoff::Source;
use crate::server::SUPPORT_URL;
use crate::{window, AppState};
use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime};
use url::Url;

#[cfg(target_os = "macos")]
const CHANGE_SERVER: &str = "change-server";
#[cfg(target_os = "macos")]
const SIGN_IN_BROWSER: &str = "sign-in-browser";
#[cfg(target_os = "macos")]
const SIGN_IN: &str = "sign-in-link";
#[cfg(target_os = "macos")]
const DEVICE_KEY: &str = "device-key";
const OPEN_IN_BROWSER: &str = "open-in-browser";
const RELOAD: &str = "reload";
const CLIENT: &str = "client";
#[cfg(target_os = "macos")]
const WORKSPACE: &str = "web-workspace";
const SUPPORT: &str = "support";

fn about<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<PredefinedMenuItem<R>> {
    PredefinedMenuItem::about(
        app,
        Some("About Dynodoc"),
        Some(AboutMetadata {
            name: Some("Dynodoc".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            website: Some("https://dynodoc.online".into()),
            website_label: Some("dynodoc.online".into()),
            copyright: Some("Copyright Verne Ltd.".into()),
            comments: Some(
                "Dynodoc keeps your documents under version control on this computer. Not signed or notarised yet."
                    .into(),
            ),
            ..Default::default()
        }),
    )
}

fn edit_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )
}

/// The web window's menu (Windows and Linux): back to the client, the server in the browser, the
/// usual edit commands, reload, and help. Reload and the client shortcut work as they always did.
#[cfg(not(target_os = "macos"))]
pub fn build_web<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let client = MenuItem::with_id(
        app,
        CLIENT,
        "Dynodoc client",
        true,
        Some("CmdOrCtrl+Shift+D"),
    )?;
    let browser = MenuItem::with_id(
        app,
        OPEN_IN_BROWSER,
        "Open server in browser",
        true,
        None::<&str>,
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &client,
            &browser,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some("Close window"))?,
        ],
    )?;
    let reload = MenuItem::with_id(app, RELOAD, "Reload", true, Some("CmdOrCtrl+R"))?;
    let view = Submenu::with_items(app, "View", true, &[&reload])?;
    let support = MenuItem::with_id(app, SUPPORT, "Dynodoc support", true, None::<&str>)?;
    let help = Submenu::with_items(
        app,
        "Help",
        true,
        &[&support, &PredefinedMenuItem::separator(app)?, &about(app)?],
    )?;
    Menu::with_items(app, &[&file, &edit_menu(app)?, &view, &help])
}

/// The menu bar on macOS, which has the system's frame and one menu for both windows.
#[cfg(target_os = "macos")]
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let about = about(app)?;
    let separator = || PredefinedMenuItem::separator(app);

    let change = MenuItem::with_id(app, CHANGE_SERVER, "Change server...", true, None::<&str>)?;
    let sign_in_browser = MenuItem::with_id(
        app,
        SIGN_IN_BROWSER,
        "Sign in with your browser...",
        true,
        None::<&str>,
    )?;
    let sign_in = MenuItem::with_id(
        app,
        SIGN_IN,
        "Sign in with an emailed link...",
        true,
        None::<&str>,
    )?;
    let key = MenuItem::with_id(
        app,
        DEVICE_KEY,
        "This computer's key...",
        true,
        None::<&str>,
    )?;
    let browser = MenuItem::with_id(
        app,
        OPEN_IN_BROWSER,
        "Open server in browser",
        true,
        None::<&str>,
    )?;
    let quit = PredefinedMenuItem::quit(app, Some("Quit Dynodoc"))?;
    // Quit and About live in the application menu below, by convention.
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[&change, &sign_in_browser, &sign_in, &key, &browser],
    )?;
    let reload = MenuItem::with_id(app, RELOAD, "Reload", true, Some("CmdOrCtrl+R"))?;
    let client = MenuItem::with_id(
        app,
        CLIENT,
        "Dynodoc client",
        true,
        Some("CmdOrCtrl+Shift+D"),
    )?;
    let workspace = MenuItem::with_id(app, WORKSPACE, "Web workspace", true, None::<&str>)?;
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[&client, &workspace, &separator()?, &reload],
    )?;
    let support = MenuItem::with_id(app, SUPPORT, "Dynodoc support", true, None::<&str>)?;
    let help = Submenu::with_items(app, "Help", true, &[&support])?;
    // macOS shows the first submenu under the application's name.
    let app_menu = Submenu::with_items(
        app,
        "Dynodoc",
        true,
        &[
            &about,
            &separator()?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &separator()?,
            &quit,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &file, &edit_menu(app)?, &view, &help])
}

pub fn handle(app: &AppHandle, id: &str) {
    match id {
        #[cfg(target_os = "macos")]
        CHANGE_SERVER => window::show_launcher(app, None),
        // A menu item is the person's choice, like the launcher's button. Without a server
        // there is nothing to sign in to: the launcher is where one is chosen.
        #[cfg(target_os = "macos")]
        SIGN_IN_BROWSER => {
            match window::begin_browser_sign_in(app) {
                Err(window::SignInStart::NoServer) => window::show_launcher(app, None),
                // Where this system's web view cannot name the app to the server the launcher
                // shows the paste-a-link fallback (and says nothing of a button it does not have).
                Err(window::SignInStart::Unsupported) => window::show_launcher(app, Some("signin")),
                _ => {}
            }
        }
        #[cfg(target_os = "macos")]
        SIGN_IN => window::show_launcher(app, Some("signin")),
        #[cfg(target_os = "macos")]
        DEVICE_KEY => window::show_launcher(app, Some("key")),
        // A menu item is a person's choice: it may open a local server in the browser.
        OPEN_IN_BROWSER => match app.try_state::<AppState>().and_then(|s| s.server()) {
            Some(server) => window::open_external(app, &server.root(), Source::Menu),
            None => window::focus_main(app),
        },
        // The window that has the focus: the client, or the web window.
        RELOAD => window::reload_focused(app),
        // Back to the client window, as it is (nothing is reloaded).
        CLIENT => window::focus_main(app),
        // The configured server's own pages, in the web window (the client if none).
        #[cfg(target_os = "macos")]
        WORKSPACE => match app.try_state::<AppState>().and_then(|s| s.server()) {
            Some(server) => window::navigate_web(app, server.entry()),
            None => window::show_launcher(app, None),
        },
        SUPPORT => {
            if let Ok(url) = Url::parse(SUPPORT_URL) {
                window::open_external(app, &url, Source::Menu);
            }
        }
        _ => {}
    }
}
