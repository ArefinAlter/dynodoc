//! Everything the launcher page may ask the app to do. Nothing else is reachable from a page.
//!
//! Three layers keep it that way: the capability file grants these commands to the main
//! window's LOCAL content only (a remote page gets no native access at all); build.rs lists
//! them so Tauri checks every call against that file; and each command re-checks, through
//! `require_launcher`, that the page asking is the bundled launcher. Arguments are plain
//! strings that are validated here.

use crate::nav;
use crate::server::{parse_server_address, sign_in_url, CLOUD_ORIGIN, SUPPORT_URL};
use crate::sidecar::{self, KeyPath, Operation, Recovery, SidecarError};
use crate::{config, window, AppState};
use serde::Serialize;
use std::io::Read;
use tauri::{AppHandle, Manager, Webview};
use url::Url;

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

const NOT_HERE: &str = "Not available from this page.";

#[derive(Serialize)]
pub struct StateView {
    version: &'static str,
    platform: &'static str,
    cloud: &'static str,
    support: &'static str,
    /// The configured server, e.g. `https://app.dynodoc.online`.
    server: Option<String>,
    /// A saved address that was NOT opened because the saved file no longer matches what the
    /// app wrote (see config.rs). The launcher shows it and asks the person to confirm.
    unconfirmed: Option<String>,
    /// Whether this build can run the bundled tools (desktop only).
    tools: bool,
}

#[derive(Serialize)]
pub struct KeyStatus {
    exists: bool,
    /// A file is there but cannot be a finished key (see `sidecar::key_file_damaged`).
    damaged: bool,
    /// Where the key file is, shown to the person. Not a secret.
    path: String,
    /// What to do about a damaged file; never an action the app takes itself.
    recovery: Option<String>,
}

/// Only the bundled launcher page may call a command. The ACL already says so; this is the
/// second lock, on the page's current address. Takes the result of `Webview::url()` so that
/// the rule is testable without a window.
pub fn require_launcher(url: tauri::Result<Url>) -> Result<(), String> {
    match url {
        Ok(url) if nav::is_launcher(&url, &nav::launcher_origin()) => Ok(()),
        _ => Err(NOT_HERE.to_string()),
    }
}

fn state_of(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

#[tauri::command]
pub fn get_state(webview: Webview) -> Result<StateView, String> {
    require_launcher(webview.url())?;
    let state = state_of(webview.app_handle());
    Ok(StateView {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        cloud: CLOUD_ORIGIN,
        support: SUPPORT_URL,
        server: state.server().map(|s| s.as_text()),
        unconfirmed: state.unconfirmed().map(|s| s.as_text()),
        tools: cfg!(desktop),
    })
}

/// Validate, remember and open a server. Returns the normalised address.
#[tauri::command]
pub fn set_server(webview: Webview, address: String) -> Result<String, String> {
    require_launcher(webview.url())?;
    let origin = parse_server_address(&address).map_err(|e| e.to_string())?;
    let app = webview.app_handle();
    let state = state_of(app);
    config::save(&state.config_dir, &state.data_dir, &origin)
        .map_err(|_| "Your choice could not be saved on this computer.".to_string())?;
    state.set_server(origin.clone());
    window::navigate_main(app, origin.root());
    Ok(origin.as_text())
}

/// Open a pasted sign-in link in this window (see `server::sign_in_url`).
#[tauri::command]
pub fn open_sign_in_link(webview: Webview, link: String) -> Result<(), String> {
    require_launcher(webview.url())?;
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

/// Something (a file, a folder, a link) is at the key location.
fn occupied(key: &KeyPath) -> bool {
    std::fs::symlink_metadata(key.as_path()).is_ok()
}

/// The start of the key file, to judge whether it is finished. None when it cannot be read.
fn read_key_bytes(key: &KeyPath) -> Option<Vec<u8>> {
    let file = std::fs::File::open(key.as_path()).ok()?;
    let mut bytes = Vec::new();
    file.take(sidecar::MAX_KEY_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

/// `message` followed by what to do next.
fn with_recovery(message: String, key: &KeyPath, why: Recovery) -> String {
    format!("{message} {}", sidecar::recovery_text(key, why))
}

#[tauri::command]
pub async fn tools_version(webview: Webview) -> Result<String, String> {
    require_launcher(webview.url())?;
    tools_unavailable()?;
    let key = key_path(webview.app_handle())?;
    tool(Operation::Version, key, sidecar::parse_version).await
}

/// Whether a key file is there, and whether it can be a finished one. Reads no key.
#[tauri::command]
pub fn device_key_status(webview: Webview) -> Result<KeyStatus, String> {
    require_launcher(webview.url())?;
    tools_unavailable()?;
    let key = key_path(webview.app_handle())?;
    let exists = std::fs::symlink_metadata(key.as_path())
        .map(|m| m.is_file())
        .unwrap_or(false);
    let damaged = exists && read_key_bytes(&key).is_some_and(|b| sidecar::key_file_damaged(&b));
    Ok(KeyStatus {
        exists,
        damaged,
        path: key.as_path().display().to_string(),
        recovery: damaged.then(|| sidecar::recovery_text(&key, Recovery::Damaged)),
    })
}

/// Create the key with `engine-cli device keygen --encrypt --keystore`. Never overwrites.
#[tauri::command]
pub async fn device_key_create(webview: Webview) -> Result<String, String> {
    require_launcher(webview.url())?;
    tools_unavailable()?;
    let key = key_path(webview.app_handle())?;
    if occupied(&key) {
        let damaged = read_key_bytes(&key).is_some_and(|b| sidecar::key_file_damaged(&b));
        let why = if damaged {
            Recovery::Damaged
        } else {
            Recovery::CannotOpen
        };
        return Err(with_recovery(
            "This computer already has a key file. It is never replaced from here.".to_string(),
            &key,
            why,
        ));
    }
    match tool(Operation::CreateKey, key.clone(), sidecar::parse_public_key).await {
        Ok(public) => Ok(public),
        // A killed or timed-out creation may have left a cut-off file behind.
        Err(message) if occupied(&key) => Err(with_recovery(message, &key, Recovery::CreateFailed)),
        Err(message) => Err(message),
    }
}

/// Read the public half back (this decrypts the key through the operating system's key
/// store, which may ask the person): only on request, never when the page loads.
#[tauri::command]
pub async fn device_public_key(webview: Webview) -> Result<String, String> {
    require_launcher(webview.url())?;
    tools_unavailable()?;
    let key = key_path(webview.app_handle())?;
    if !occupied(&key) {
        return Err("This computer has no key yet.".to_string());
    }
    if read_key_bytes(&key).is_some_and(|b| sidecar::key_file_damaged(&b)) {
        return Err(sidecar::recovery_text(&key, Recovery::Damaged));
    }
    tool(Operation::PublicKey, key.clone(), sidecar::parse_public_key)
        .await
        .map_err(|message| with_recovery(message, &key, Recovery::CannotOpen))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_launcher_page_passes_the_second_lock() {
        let launcher = nav::launcher_url(None);
        assert_eq!(require_launcher(Ok(launcher)), Ok(()));
        assert_eq!(require_launcher(Ok(nav::launcher_url(Some("key")))), Ok(()));
        let refused = Err(NOT_HERE.to_string());
        for page in [
            "https://app.dynodoc.online/",
            "http://127.0.0.1:8080/",
            "http://tauri.localhost.evil.example/index.html",
            "tauri://evil/index.html",
            "https://user@app.dynodoc.online/",
            "about:blank",
            "file:///C:/index.html",
        ] {
            assert_eq!(
                require_launcher(Ok(Url::parse(page).unwrap())),
                refused,
                "{page}"
            );
        }
        // A page whose address cannot be read is refused, not trusted.
        assert_eq!(
            require_launcher(Err(tauri::Error::WebviewNotFound)),
            refused
        );
    }

    #[test]
    fn every_command_takes_the_second_lock_first() {
        // Source check: each `#[tauri::command]` function begins with require_launcher, so
        // removing the call from one of them is caught here.
        let source = include_str!("commands.rs");
        let body = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        let functions: Vec<&str> = body.split("#[tauri::command]").skip(1).collect();
        assert_eq!(functions.len(), COMMANDS.len());
        for (function, name) in functions.iter().zip(COMMANDS) {
            let head = function.split("\n}\n").next().unwrap();
            let after_signature = head.split(") -> ").nth(1).expect(name);
            let first_statement = after_signature.split('{').nth(1).unwrap().trim_start();
            assert!(
                first_statement.starts_with("require_launcher(webview.url())?;"),
                "{name} must check the caller first: {first_statement:.60}"
            );
        }
    }
}
