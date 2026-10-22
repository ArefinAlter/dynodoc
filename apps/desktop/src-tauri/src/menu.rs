//! The native menu. Desktop only.

use crate::handoff::Source;
use crate::server::SUPPORT_URL;
use crate::{window, AppState};
use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime};
use url::Url;

const CHANGE_SERVER: &str = "change-server";
const SIGN_IN_BROWSER: &str = "sign-in-browser";
const SIGN_IN: &str = "sign-in-link";
const DEVICE_KEY: &str = "device-key";
const OPEN_IN_BROWSER: &str = "open-in-browser";
const RELOAD: &str = "reload";
const SUPPORT: &str = "support";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let about = PredefinedMenuItem::about(
        app,
        Some("About Dynodoc"),
        Some(AboutMetadata {
            name: Some("Dynodoc".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            website: Some("https://dynodoc.online".into()),
            website_label: Some("dynodoc.online".into()),
            copyright: Some("Copyright Verne Ltd.".into()),
            comments: Some(
                "Opens your Dynodoc server in its own window. Not signed or notarised yet.".into(),
            ),
            ..Default::default()
        }),
    )?;
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
    // On macOS Quit and About live in the application menu below, not here.
    #[cfg(not(target_os = "macos"))]
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &change,
            &sign_in_browser,
            &sign_in,
            &key,
            &browser,
            &separator()?,
            &quit,
        ],
    )?;
    #[cfg(target_os = "macos")]
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[&change, &sign_in_browser, &sign_in, &key, &browser],
    )?;

    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &separator()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let reload = MenuItem::with_id(app, RELOAD, "Reload", true, Some("CmdOrCtrl+R"))?;
    let view = Submenu::with_items(app, "View", true, &[&reload])?;

    let support = MenuItem::with_id(app, SUPPORT, "Dynodoc support", true, None::<&str>)?;
    #[cfg(not(target_os = "macos"))]
    let help = Submenu::with_items(
        app,
        "Help",
        true,
        &[&support, &PredefinedMenuItem::separator(app)?, &about],
    )?;
    #[cfg(target_os = "macos")]
    let help = Submenu::with_items(app, "Help", true, &[&support])?;

    // macOS shows the first submenu under the application's name; About and Quit belong
    // there by convention.
    #[cfg(target_os = "macos")]
    {
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
        Menu::with_items(app, &[&app_menu, &file, &edit, &view, &help])
    }
    #[cfg(not(target_os = "macos"))]
    Menu::with_items(app, &[&file, &edit, &view, &help])
}

pub fn handle(app: &AppHandle, id: &str) {
    match id {
        CHANGE_SERVER => window::show_launcher(app, None),
        // A menu item is the person's choice, like the launcher's button. Without a server
        // there is nothing to sign in to: the launcher is where one is chosen.
        SIGN_IN_BROWSER => {
            match window::begin_browser_sign_in(app) {
                Err(window::SignInStart::NoServer) => window::show_launcher(app, None),
                // Where this system's web view cannot name the app to the server the launcher
                // shows the paste-a-link fallback (and says nothing of a button it does not have).
                Err(window::SignInStart::Unsupported) => window::show_launcher(app, Some("signin")),
                _ => {}
            }
        }
        SIGN_IN => window::show_launcher(app, Some("signin")),
        DEVICE_KEY => window::show_launcher(app, Some("key")),
        // A menu item is a person's choice: it may open a local server in the browser.
        OPEN_IN_BROWSER => match app.try_state::<AppState>().and_then(|s| s.server()) {
            Some(server) => window::open_external(app, &server.root(), Source::Menu),
            None => window::show_launcher(app, None),
        },
        RELOAD => window::reload_main(app),
        SUPPORT => {
            if let Ok(url) = Url::parse(SUPPORT_URL) {
                window::open_external(app, &url, Source::Menu);
            }
        }
        _ => {}
    }
}
