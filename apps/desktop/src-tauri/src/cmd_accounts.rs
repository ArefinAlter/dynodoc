//! Commands for the servers the person signs in to (decision 020): the list, signing in with the
//! system browser, and signing out. Same rules as `commands.rs`: only the bundled launcher page,
//! and each command takes that second lock first. A session never reaches the page: it is told
//! only whether it has one, and (from the server) a name and an email.

use crate::accounts::{AccountView, FlowView};
use crate::commands::require_launcher;
use crate::server::CLOUD_ORIGIN;
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, Manager, Webview};

#[cfg(test)]
pub const COMMANDS: [&str; 8] = [
    "accounts_list",
    "account_add",
    "account_remove",
    "account_signin_start",
    "account_signin_status",
    "account_signin_cancel",
    "account_signout",
    "account_check",
];

fn state_of(webview: &Webview) -> tauri::State<'_, AppState> {
    webview.app_handle().state::<AppState>()
}

#[derive(Serialize)]
pub struct AccountList {
    cloud: &'static str,
    accounts: Vec<AccountView>,
    /// Where the sessions are kept, in words.
    store: &'static str,
}

/// The servers added to the app and whether each has a session.
#[tauri::command]
pub fn accounts_list(webview: Webview) -> Result<AccountList, String> {
    require_launcher(webview.url())?;
    let accounts = &state_of(&webview).accounts;
    Ok(AccountList {
        cloud: CLOUD_ORIGIN,
        accounts: accounts.views(),
        store: accounts.store_describe(),
    })
}

/// Adds a server by its address. Nothing is sent to it.
#[tauri::command]
pub fn account_add(webview: Webview, address: String) -> Result<AccountView, String> {
    require_launcher(webview.url())?;
    state_of(&webview).accounts.add(&address)
}

/// Forgets a server on this computer: its session leaves the credential store.
#[tauri::command]
pub fn account_remove(webview: Webview, id: String) -> Result<(), String> {
    require_launcher(webview.url())?;
    if !crate::accounts::id_ok(&id) {
        return Err("That server is not in your list.".to_string());
    }
    state_of(&webview).accounts.remove(&id)
}

/// Opens the server's sign-in page in the system browser, and waits (in the app) for the browser
/// to hand the code back on this computer. Returns the four-character pairing code the page shows
/// too.
#[tauri::command]
pub async fn account_signin_start(
    webview: Webview,
    id: String,
    via_link: Option<bool>,
) -> Result<String, String> {
    require_launcher(webview.url())?;
    if !crate::accounts::id_ok(&id) {
        return Err("That server is not in your list.".to_string());
    }
    let app = webview.app_handle().clone();
    let accounts = state_of(&webview).accounts.clone();
    accounts
        .begin(&id, via_link.unwrap_or(false), move |url| {
            open_browser(&app, url)
        })
        .await
}

/// Where the sign-in is: waiting for the browser, done, or failed.
#[tauri::command]
pub fn account_signin_status(webview: Webview) -> Result<FlowView, String> {
    require_launcher(webview.url())?;
    Ok(state_of(&webview).accounts.flow())
}

#[tauri::command]
pub fn account_signin_cancel(webview: Webview) -> Result<(), String> {
    require_launcher(webview.url())?;
    state_of(&webview).accounts.cancel_flow();
    Ok(())
}

#[derive(Serialize)]
pub struct SignedOut {
    account: AccountView,
    /// Whether the server confirmed that it ended the session (it may be unreachable; the session
    /// is gone from this computer either way).
    server_confirmed: bool,
}

/// Ends the session at the server and removes it from this computer.
#[tauri::command]
pub async fn account_signout(webview: Webview, id: String) -> Result<SignedOut, String> {
    require_launcher(webview.url())?;
    if !crate::accounts::id_ok(&id) {
        return Err("That server is not in your list.".to_string());
    }
    let accounts = state_of(&webview).accounts.clone();
    let (account, server_confirmed) = accounts.sign_out(&id).await?;
    Ok(SignedOut {
        account,
        server_confirmed,
    })
}

/// Asks the server who the session is, and whether it still is one.
#[tauri::command]
pub async fn account_check(webview: Webview, id: String) -> Result<AccountView, String> {
    require_launcher(webview.url())?;
    if !crate::accounts::id_ok(&id) {
        return Err("That server is not in your list.".to_string());
    }
    let accounts = state_of(&webview).accounts.clone();
    accounts.check(&id).await
}

/// The system browser, at an address this app built from a server the person added. In a debug
/// build a test can name a file in `DYNODOC_TEST_BROWSER`: the address is written there instead,
/// for the test to open in a browser it drives.
fn open_browser(app: &AppHandle, url: &str) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if let Ok(file) = std::env::var("DYNODOC_TEST_BROWSER") {
        return std::fs::write(file, url)
            .map_err(|_| "The test browser file could not be written.".to_string());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "The system browser could not be opened. Open the server's address yourself and sign in there, then try again.".to_string())
}
