//! Dynodoc desktop shell: a native window around the configured Dynodoc server.
//!
//! The web app is not bundled. This crate adds the pieces a browser tab cannot: a launcher
//! page to choose the server, a hardened navigation policy, a `dynodoc://` link handler and
//! a fixed set of local tools (a device signing key kept by the operating system). Read
//! docs/decisions/016-devices-processors-desktop-and-mobile-apps.md and
//! apps/desktop/README.md before changing the security-relevant modules (`nav`, `server`,
//! `deeplink`, `sidecar`, `commands`, the capability file and tauri.conf.json).

mod commands;
mod config;
mod deeplink;
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
use tauri::webview::PermissionResponse;
use tauri::Manager;

/// What the running app knows. The server is read by the navigation hooks on every
/// navigation, so changing it takes effect at once.
pub struct AppState {
    pub server: Mutex<Option<server::ServerOrigin>>,
    pub config_dir: PathBuf,
    /// Local (not roamed) data: the device key file lives here.
    pub data_dir: PathBuf,
    /// Debug builds only: where the web view keeps its data when a test home is set.
    pub webview_dir: Option<PathBuf>,
}

impl AppState {
    pub fn server(&self) -> Option<server::ServerOrigin> {
        self.server.lock().ok().and_then(|s| s.clone())
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
    let server = config::load(&config_dir);
    Ok(AppState {
        server: Mutex::new(server),
        config_dir,
        data_dir,
        webview_dir,
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
        // rest: refused for every page, always.
        .on_permission_request(|_, _| PermissionResponse::Deny)
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
