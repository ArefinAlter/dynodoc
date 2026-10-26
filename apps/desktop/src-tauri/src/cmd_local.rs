//! Commands for the workspace on this computer. Same rules as `commands.rs`: only the bundled
//! launcher page may call them, and each one takes that second lock first.

use crate::commands::require_launcher;
use crate::local::{Profile, Status};
use crate::AppState;
use tauri::{Manager, Webview};

#[cfg(test)]
pub const COMMANDS: [&str; 2] = ["local_status", "local_create_profile"];

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
    local.create_profile(&name, email.as_deref().unwrap_or("")).await
}
