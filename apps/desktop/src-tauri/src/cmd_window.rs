//! Two window commands for the client's application menu (the client window has no native menu bar):
//! reload the client, and open the inspector in a debug build. Same rules as `commands.rs`: only
//! the bundled launcher page may call them, and each one takes that second lock first. (Minimise,
//! maximise, close and moving the window are not commands of ours: they are Tauri's window
//! permissions, granted to the client window alone by `capabilities/window-controls.json`.)

use crate::commands::require_launcher;
use crate::window;
use tauri::{Manager, Webview};

#[cfg(test)]
pub const COMMANDS: [&str; 2] = ["window_reload", "devtools_open"];

/// Reload the client. The launcher reloads only because this call said so (`window::reload_main`).
#[tauri::command]
pub fn window_reload(webview: Webview) -> Result<(), String> {
    require_launcher(webview.url())?;
    window::reload_main(webview.app_handle());
    Ok(())
}

/// Open the web view's inspector. Debug builds only: a release build has none (the `devtools` cargo
/// feature is off), and says so.
#[tauri::command]
pub fn devtools_open(webview: Webview) -> Result<(), String> {
    require_launcher(webview.url())?;
    if !window::devtools_enabled() {
        return Err("The inspector is only in debug builds.".to_string());
    }
    #[cfg(debug_assertions)]
    webview.open_devtools();
    Ok(())
}
