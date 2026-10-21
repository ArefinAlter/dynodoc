//! Dynodoc desktop shell: a native window around the configured Dynodoc server.
//!
//! The web app is not bundled. This crate adds the pieces a browser tab cannot: a launcher
//! page to choose the server, a hardened navigation policy, a `dynodoc://` link handler and
//! a fixed set of local tools (a device signing key kept by the operating system). Read
//! docs/decisions/016-devices-processors-desktop-and-mobile-apps.md and
//! apps/desktop/README.md before changing the security-relevant modules (`nav`, `handoff`,
//! `server`, `deeplink`, `sidecar`, `commands`, the capability file and tauri.conf.json).

mod commands;
mod config;
mod deeplink;
mod handoff;
#[cfg(desktop)]
mod menu;
mod nav;
mod server;
mod sidecar;
mod window;

#[cfg(test)]
mod policy_tests;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;
use tauri::Manager;

/// What the running app knows. The server is read by the navigation hooks on every
/// navigation, so changing it takes effect at once.
pub struct AppState {
    pub server: Mutex<Option<server::ServerOrigin>>,
    /// A saved address that did not match its seal at start-up: shown, not opened.
    pub unconfirmed: Mutex<Option<server::ServerOrigin>>,
    pub config_dir: PathBuf,
    /// Local (not roamed) data: the device key file and the config seal live here.
    pub data_dir: PathBuf,
    /// Debug builds only: where the web view keeps its data when a test home is set.
    pub webview_dir: Option<PathBuf>,
    /// Lets the launcher load, once, for a navigation the shell started.
    pub launcher_grant: Mutex<nav::LauncherGrant>,
    /// Limits how often a page can make the system browser open.
    pub handoffs: Mutex<handoff::HandOffLimiter>,
    /// Proves a click to the new-window hook; lives in the injected script and here only.
    pub click_secret: String,
}

impl AppState {
    pub fn server(&self) -> Option<server::ServerOrigin> {
        self.server.lock().ok().and_then(|s| s.clone())
    }

    pub fn unconfirmed(&self) -> Option<server::ServerOrigin> {
        self.unconfirmed.lock().ok().and_then(|s| s.clone())
    }

    /// The person chose (or confirmed) this server.
    pub fn set_server(&self, server: server::ServerOrigin) {
        if let Ok(mut current) = self.server.lock() {
            *current = Some(server);
        }
        if let Ok(mut pending) = self.unconfirmed.lock() {
            *pending = None;
        }
    }

    pub fn arm_launcher_grant(&self) {
        if let Ok(mut grant) = self.launcher_grant.lock() {
            grant.arm(Instant::now());
        }
    }

    /// True once per grant, and only while it is fresh.
    pub fn take_launcher_grant(&self) -> bool {
        self.launcher_grant
            .lock()
            .is_ok_and(|mut grant| grant.take(Instant::now()))
    }
}

/// Debug builds only: `DYNODOC_DESKTOP_HOME=<dir>` keeps the config, key file and web view
/// data under that folder instead of the real per-user ones. Used by the smoke test so it
/// never touches a real profile. Release builds do not read it.
fn home_override() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let home = PathBuf::from(std::env::var_os("DYNODOC_DESKTOP_HOME")?);
    home.is_absolute().then_some(home)
}

/// The server to open at start-up, and the saved one that is held back. A saved address is
/// opened only when it matches its seal.
fn split_loaded(
    loaded: config::Loaded,
) -> (Option<server::ServerOrigin>, Option<server::ServerOrigin>) {
    match loaded {
        config::Loaded::Confirmed(server) => (Some(server), None),
        config::Loaded::Unconfirmed(server) => (None, Some(server)),
        config::Loaded::Nothing => (None, None),
    }
}

fn resolve_state(app: &tauri::AppHandle) -> tauri::Result<AppState> {
    let (config_dir, data_dir, webview_dir) = match home_override() {
        Some(home) => (
            home.join("config"),
            home.join("data"),
            Some(home.join("webview")),
        ),
        None => (
            app.path().app_config_dir()?,
            app.path().app_local_data_dir()?,
            None,
        ),
    };
    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(&data_dir)?;
    let (server, unconfirmed) = split_loaded(config::load(&config_dir, &data_dir));
    Ok(AppState {
        server: Mutex::new(server),
        unconfirmed: Mutex::new(unconfirmed),
        config_dir,
        data_dir,
        webview_dir,
        launcher_grant: Mutex::new(nav::LauncherGrant::default()),
        handoffs: Mutex::new(handoff::HandOffLimiter::default()),
        click_secret: handoff::new_secret(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // Must be first: a second launch (or a dynodoc:// link) is detected before anything else
    // starts, handed to the running copy and this process exits.
    #[cfg(desktop)]
    {
        builder = builder.plugin(
            tauri_plugin_single_instance::Builder::new()
                .callback(|app, _args, _cwd| window::focus_main(app))
                .build(),
        );
        // A test home keeps its own folders; the plugin would write to the real profile.
        if home_override().is_none() {
            builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());
        }
    }

    builder = builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        // Camera, microphone, location, notifications, clipboard reading, sensors and the
        // rest: refused for every page, always (window::permission_response).
        .on_permission_request(|_, kind| window::permission_response(kind))
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_server,
            commands::open_sign_in_link,
            commands::tools_version,
            commands::device_key_status,
            commands::device_key_create,
            commands::device_public_key,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(resolve_state(&handle)?);
            window::create_main(&handle)?;
            // Links that arrived while this was still starting are handled here.
            window::watch_deep_links(&handle);
            Ok(())
        });

    #[cfg(desktop)]
    {
        builder = builder
            .menu(menu::build)
            .on_menu_event(|app, event| menu::handle(app, event.id().as_ref()));
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running Dynodoc");
}

#[cfg(test)]
mod tests {
    use super::*;
    use server::parse_server_address;

    #[test]
    fn only_a_sealed_address_is_opened_at_start_up() {
        let s = parse_server_address("https://dynodoc.example.org").unwrap();
        assert_eq!(
            split_loaded(config::Loaded::Confirmed(s.clone())),
            (Some(s.clone()), None)
        );
        assert_eq!(
            split_loaded(config::Loaded::Unconfirmed(s.clone())),
            (None, Some(s))
        );
        assert_eq!(split_loaded(config::Loaded::Nothing), (None, None));
    }
}
