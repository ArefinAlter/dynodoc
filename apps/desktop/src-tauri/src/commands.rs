//! Everything the launcher page may ask the app to do. Nothing else is reachable from a page.
//!
//! Three layers keep it that way: the capability file grants these commands to the main
//! window's LOCAL content only (a remote page gets no native access at all); build.rs lists
//! them so Tauri checks every call against that file; and each command re-checks that the
//! page asking is the bundled launcher. Arguments are plain strings that are validated here.

use crate::nav;
use crate::server::{parse_server_address, sign_in_url, CLOUD_ORIGIN, SUPPORT_URL};
use crate::sidecar::{self, KeyPath, Operation, SidecarError};
use crate::{window, AppState};
use serde::Serialize;
use tauri::{AppHandle, Manager, Webview};

/// Every command, in the order of `generate_handler!` in lib.rs and of build.rs. A test
/// compares this list with the capability files and with build.rs.
#[cfg(test)]
pub const COMMANDS: [&str; 7] = [
    "get_state",
    "set_server",
    "open_sign_in_link",
    "tools_version",
    "device_key_status",
    "device_key_create",
    "device_public_key",
];

#[derive(Serialize)]
pub struct StateView {
    version: &'static str,
    platform: &'static str,
    cloud: &'static str,
    support: &'static str,
    /// The configured server, e.g. `https://app.dynodoc.online`.
    server: Option<String>,
    /// Whether this build can run the bundled tools (desktop only).
    tools: bool,
}

#[derive(Serialize)]
pub struct KeyStatus {
    exists: bool,
    /// Where the key file is, shown to the person. Not a secret.
    path: String,
}

/// Only the bundled launcher page may call a command. The ACL already says so; this is the
/// second lock, on the page's current address.
fn from_launcher(webview: &Webview) -> Result<(), String> {
    let url = webview
        .url()
        .map_err(|_| "Not available from this page.".to_string())?;
    if nav::is_launcher(&url, &nav::launcher_origin()) {
        Ok(())
    } else {
        Err("Not available from this page.".to_string())
    }
}

fn state_of(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

#[tauri::command]
pub fn get_state(webview: Webview) -> Result<StateView, String> {
    from_launcher(&webview)?;
    let state = state_of(webview.app_handle());
    Ok(StateView {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        cloud: CLOUD_ORIGIN,
        support: SUPPORT_URL,
        server: state.server().map(|s| s.as_text()),
        tools: cfg!(desktop),
    })
}

/// Validate, remember and open a server. Returns the normalised address.
#[tauri::command]
pub fn set_server(webview: Webview, address: String) -> Result<String, String> {
    from_launcher(&webview)?;
    let origin = parse_server_address(&address).map_err(|e| e.to_string())?;
    let app = webview.app_handle();
    let state = state_of(app);
    crate::config::save(&state.config_dir, &origin)
        .map_err(|_| "Your choice could not be saved on this computer.".to_string())?;
    if let Ok(mut server) = state.server.lock() {
        *server = Some(origin.clone());
    }
    window::navigate_main(app, origin.root());
    Ok(origin.as_text())
}

/// Open a pasted sign-in link in this window (see `server::sign_in_url`).
#[tauri::command]
pub fn open_sign_in_link(webview: Webview, link: String) -> Result<(), String> {
    from_launcher(&webview)?;
    let app = webview.app_handle();
    let server = state_of(app)
        .server()
        .ok_or_else(|| "Choose a server first.".to_string())?;
    let target = sign_in_url(&server, &link).map_err(|e| e.to_string())?;
    window::navigate_main(app, target);
    Ok(())
}

fn key_path(app: &AppHandle) -> Result<KeyPath, String> {
    KeyPath::in_dir(&state_of(app).data_dir).map_err(|e| e.to_string())
}

fn tools_unavailable() -> Result<(), String> {
    if cfg!(desktop) {
        Ok(())
    } else {
        Err("The local tools are not available in this app yet.".to_string())
    }
}

/// Run a sidecar operation off the main thread and map its answer.
async fn tool<T: Send + 'static>(
    op: Operation,
    key: KeyPath,
    parse: fn(&str) -> Result<T, SidecarError>,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sidecar::run(op, &key).and_then(|o| parse(&o.stdout))
    })
    .await
    .map_err(|_| "The tools stopped unexpectedly.".to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tools_version(webview: Webview) -> Result<String, String> {
    from_launcher(&webview)?;
    tools_unavailable()?;
    let key = key_path(webview.app_handle())?;
    tool(Operation::Version, key, sidecar::parse_version).await
}

#[tauri::command]
pub fn device_key_status(webview: Webview) -> Result<KeyStatus, String> {
    from_launcher(&webview)?;
    tools_unavailable()?;
    let key = key_path(webview.app_handle())?;
    let exists = std::fs::symlink_metadata(key.as_path())
        .map(|m| m.is_file())
        .unwrap_or(false);
    Ok(KeyStatus {
        exists,
        path: key.as_path().display().to_string(),
    })
}

/// Create the key with `engine-cli device keygen --encrypt --keystore`. Never overwrites.
#[tauri::command]
pub async fn device_key_create(webview: Webview) -> Result<String, String> {
    from_launcher(&webview)?;
    tools_unavailable()?;
    let key = key_path(webview.app_handle())?;
    if std::fs::symlink_metadata(key.as_path()).is_ok() {
        return Err("This computer already has a key. It is never replaced from here.".to_string());
    }
    tool(Operation::CreateKey, key, sidecar::parse_public_key).await
}

#[tauri::command]
pub async fn device_public_key(webview: Webview) -> Result<String, String> {
    from_launcher(&webview)?;
    tools_unavailable()?;
    let key = key_path(webview.app_handle())?;
    if std::fs::symlink_metadata(key.as_path()).is_err() {
        return Err("This computer has no key yet.".to_string());
    }
    tool(Operation::PublicKey, key, sidecar::parse_public_key).await
}
