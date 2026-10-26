//! Commands for the workspace on this computer. Same rules as `commands.rs`: only the bundled
//! launcher page may call them, and each one takes that second lock first.

use crate::commands::require_launcher;
use crate::local::{Profile, Status};
use crate::AppState;
use tauri::{Manager, Webview};

#[cfg(test)]
pub const COMMANDS: [&str; 6] = [
    "local_status",
    "local_create_profile",
    "local_retry",
    "local_open_folder",
    "backup_export",
    "backup_import",
];

fn state_of(webview: &Webview) -> tauri::State<'_, AppState> {
    webview.app_handle().state::<AppState>()
}

/// How the local workspace is doing: starting (and what step), waiting for a name, ready, or failed.
#[tauri::command]
pub fn local_status(webview: Webview) -> Result<Status, String> {
    require_launcher(webview.url())?;
    Ok(state_of(&webview).local.status())
}

/// The first thing a person does in a new workspace: say who they are. Nothing leaves the computer.
#[tauri::command]
pub async fn local_create_profile(
    webview: Webview,
    name: String,
    email: Option<String>,
) -> Result<Profile, String> {
    require_launcher(webview.url())?;
    let local = state_of(&webview).local.clone();
    local
        .create_profile(&name, email.as_deref().unwrap_or(""))
        .await
}

/// After a failed start: try again.
#[tauri::command]
pub fn local_retry(webview: Webview) -> Result<(), String> {
    require_launcher(webview.url())?;
    if state_of(&webview).local.retry() {
        Ok(())
    } else {
        Err("The workspace is not in a failed state.".to_string())
    }
}

/// Shows the workspace's logs in the system's file manager.
#[tauri::command]
pub fn local_open_folder(webview: Webview) -> Result<(), String> {
    require_launcher(webview.url())?;
    use tauri_plugin_opener::OpenerExt;
    let logs = state_of(&webview).local.layout().logs.clone();
    webview
        .app_handle()
        .opener()
        .open_path(logs.to_string_lossy().to_string(), None::<&str>)
        .map_err(|_| "The folder could not be opened.".to_string())
}

/// Writes one archive of the whole workspace to a place the person chose (a token from
/// `files_pick("backup_save")`). The database is closed for a moment while it is written.
#[tauri::command]
pub async fn backup_export(
    webview: Webview,
    token: String,
) -> Result<crate::local::backup::BackupInfo, String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let mut path = state
        .picks
        .path(&token)
        .ok_or_else(|| "Choose where to save the backup again.".to_string())?;
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        != Some("zip".to_string())
    {
        path.set_extension("zip");
    }
    state.local.backup(path).await
}

/// Replaces the workspace with the one in a backup the person chose (a token from
/// `files_pick("backup_open")`). The data it replaces is kept beside it, not deleted.
#[tauri::command]
pub async fn backup_import(
    webview: Webview,
    token: String,
) -> Result<crate::local::backup::Manifest, String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let path = state
        .picks
        .path(&token)
        .ok_or_else(|| "Choose the backup again.".to_string())?;
    state.local.restore(path).await
}
