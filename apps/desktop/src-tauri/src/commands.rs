//! Everything the launcher page may ask the app to do. Nothing else is reachable from a page.
//!
//! Three layers keep it that way: the capability file grants these commands to the main
//! window's LOCAL content only (a remote page gets no native access at all); build.rs lists
//! them so Tauri checks every call against that file; and each command re-checks, through
//! `require_launcher`, that the page asking is the bundled launcher. Arguments are plain
//! strings that are validated here.

use crate::envs::{self, Env};
use crate::nav;
use crate::remote::{self, Method, Reply, Target};
use crate::server::{parse_server_address, sign_in_url, ServerOrigin, CLOUD_ORIGIN, SUPPORT_URL};
use crate::sidecar::{self, KeyPath, Operation, Recovery, SidecarError};
use crate::signin;
use crate::{config, window, AppState};
use serde::Serialize;
use serde_json::Value;
use std::io::Read;
use tauri::{AppHandle, Manager, Webview};
use url::Url;

/// Every command, in the order of `generate_handler!` in lib.rs and of build.rs. A test
/// compares this list with the capability files and with build.rs.
#[cfg(test)]
pub const COMMANDS: [&str; 17] = [
    "get_state",
    "set_server",
    "open_sign_in_link",
    "start_browser_sign_in",
    "tools_version",
    "device_key_status",
    "device_key_create",
    "device_public_key",
    "session_info",
    "visitor_start",
    "sign_out",
    "api_get",
    "api_send",
    "svc_get",
    "svc_send",
    "prefs_load",
    "prefs_save",
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
    /// Whether this build can sign in with the system browser: only where its web view names the
    /// app to the server (`signin::APP_TOKEN`); elsewhere the button is not shown.
    browser_signin: bool,
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
        browser_signin: signin::APP_TOKEN,
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
    window::navigate_main(app, origin.entry());
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

/// Sign in with the system browser: open the server's sign-in page there and wait for it to
/// hand the app a one-time code (see `signin.rs`). The launcher's button; also a menu item on
/// desktop. Takes no argument: the page cannot choose what is opened. Returns the four-character
/// pairing code the launcher shows while the request waits.
#[tauri::command]
pub fn start_browser_sign_in(webview: Webview) -> Result<String, String> {
    require_launcher(webview.url())?;
    window::begin_browser_sign_in(webview.app_handle()).map_err(|e| e.to_string())
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

// ---- the client's way to the configured server (remote.rs) ------------------------------------

#[derive(Serialize)]
pub struct SessionView {
    /// The configured server, or None.
    server: Option<String>,
    /// Whether this window holds a session for it that the server accepts.
    signed_in: bool,
    identity: Option<String>,
}

#[derive(Serialize)]
pub struct VisitorView {
    display_name: String,
    expires_at: String,
}

const NO_SERVER: &str = "Choose a server first.";

fn configured(webview: &Webview) -> Result<ServerOrigin, String> {
    state_of(webview.app_handle())
        .server()
        .ok_or_else(|| NO_SERVER.to_string())
}

/// The web view's session cookie for the server, read off the main thread (WebView2 answers
/// cookie questions on its own thread and would wait forever for a blocked one).
async fn session_cookie(webview: &Webview, server: &ServerOrigin) -> Option<String> {
    let view = webview.clone();
    let root = server.root();
    tauri::async_runtime::spawn_blocking(move || view.cookies_for_url(root).ok())
        .await
        .ok()
        .flatten()
        .and_then(|cookies| remote::session_value(&cookies))
}

/// Store (or clear) a session cookie the server sent back, for this server only.
async fn keep_cookie(webview: &Webview, server: &ServerOrigin, set_cookie: Option<String>) {
    let Some((cookie, cleared)) = set_cookie.and_then(|v| remote::web_view_cookie(server, &v))
    else {
        return;
    };
    let view = webview.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        if cleared {
            view.delete_cookie(cookie)
        } else {
            view.set_cookie(cookie)
        }
    })
    .await;
}

/// One request on a fixed route of the configured server (see remote.rs for the tables).
async fn call(
    webview: &Webview,
    env: Option<&str>,
    target: Target,
    method: Method,
    path: &str,
    body: Option<Value>,
    if_match: Option<&str>,
) -> Result<Reply, String> {
    let place = Env::parse(env)?;
    if place != Env::Server {
        return envs::call(webview.app_handle(), &place, target, method, path, body, if_match).await;
    }
    let server = configured(webview)?;
    remote::allowed(target, method, path).map_err(str::to_string)?;
    if let Some(value) = &body {
        if serde_json::to_vec(value).map_or(true, |b| b.len() > remote::MAX_BODY_BYTES) {
            return Err("That change is too large to send at once.".to_string());
        }
    }
    let url = remote::address(&server, target, path)
        .ok_or_else(|| "That request is not available from the client.".to_string())?;
    let cookie = session_cookie(webview, &server).await;
    let (reply, renewed) = remote::send(
        &server,
        url,
        target,
        method,
        cookie.as_deref(),
        body.as_ref(),
        if_match,
    )
    .await
    .map_err(|e| e.to_string())?;
    keep_cookie(webview, &server, renewed).await;
    Ok(reply)
}

/// `If-Match` for a settings save: the revision it was made from, `N`, `"N"` or `W/"N"`.
fn if_match_ok(tag: &str) -> bool {
    let inner = tag.strip_prefix("W/").unwrap_or(tag);
    let inner = inner
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(inner);
    !inner.is_empty() && inner.len() <= 12 && inner.bytes().all(|b| b.is_ascii_digit())
}

fn write_method(method: &str) -> Result<Method, String> {
    Method::parse(method)
        .filter(|m| *m != Method::Get)
        .ok_or_else(|| "That request is not available from the client.".to_string())
}

/// Whether this window is signed in to the configured server.
#[tauri::command]
pub async fn session_info(webview: Webview) -> Result<SessionView, String> {
    require_launcher(webview.url())?;
    let Some(server) = state_of(webview.app_handle()).server() else {
        return Ok(SessionView {
            server: None,
            signed_in: false,
            identity: None,
        });
    };
    let cookie = session_cookie(&webview, &server).await;
    let mut view = SessionView {
        server: Some(server.as_text()),
        signed_in: false,
        identity: None,
    };
    let (Some(cookie), Some(url)) = (cookie, server.join("/auth/session")) else {
        return Ok(view);
    };
    let (reply, renewed) = remote::send(
        &server,
        url,
        Target::Api,
        Method::Get,
        Some(&cookie),
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    keep_cookie(&webview, &server, renewed).await;
    if reply.status == 200 && reply.body["authenticated"] == Value::Bool(true) {
        view.signed_in = true;
        view.identity = reply.body["identity_id"].as_str().map(str::to_string);
    }
    Ok(view)
}

/// Start a visitor sandbox on the configured server (the hosted pre-alpha server when none is
/// chosen yet) and sign this window in to it. The one-use sign-in token goes from the server's
/// pre-alpha service to the server's own sign-in route without reaching the page.
#[tauri::command]
pub async fn visitor_start(webview: Webview) -> Result<VisitorView, String> {
    require_launcher(webview.url())?;
    let app = webview.app_handle();
    let state = state_of(app);
    let server = match state.server() {
        Some(server) => server,
        None => {
            let cloud = parse_server_address(CLOUD_ORIGIN).map_err(|e| e.to_string())?;
            config::save(&state.config_dir, &state.data_dir, &cloud)
                .map_err(|_| "Your choice could not be saved on this computer.".to_string())?;
            state.set_server(cloud.clone());
            cloud
        }
    };
    let claim_url = server
        .join("/svc/v1/visitor")
        .ok_or_else(|| NO_SERVER.to_string())?;
    let (claim, _) = remote::send(
        &server,
        claim_url,
        Target::Svc,
        Method::Post,
        None,
        Some(&serde_json::json!({})),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    if claim.status != 200 {
        return Err(claim.body["error"]
            .as_str()
            .unwrap_or("This server does not offer sample workspaces.")
            .chars()
            .take(200)
            .collect());
    }
    let token = claim.body["token"].as_str().unwrap_or_default();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("This server does not offer sample workspaces.".to_string());
    }
    let verify_url = server
        .join("/auth/verify")
        .ok_or_else(|| NO_SERVER.to_string())?;
    let (verified, cookie) = remote::send(
        &server,
        verify_url,
        Target::Api,
        Method::Post,
        None,
        Some(&serde_json::json!({ "token": token })),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    if verified.status != 200 || cookie.is_none() {
        return Err("The sample workspace could not be opened. Please try again.".to_string());
    }
    keep_cookie(&webview, &server, cookie).await;
    Ok(VisitorView {
        display_name: claim.body["display_name"]
            .as_str()
            .unwrap_or("Visitor")
            .chars()
            .take(80)
            .collect(),
        expires_at: claim.body["expires_at"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(40)
            .collect(),
    })
}

/// Sign this window out of the configured server (the server ends the login; the cookie goes).
#[tauri::command]
pub async fn sign_out(webview: Webview) -> Result<(), String> {
    require_launcher(webview.url())?;
    let server = configured(&webview)?;
    let cookie = session_cookie(&webview, &server).await;
    let url = server
        .join("/auth/logout")
        .ok_or_else(|| NO_SERVER.to_string())?;
    let (_, cleared) = remote::send(
        &server,
        url,
        Target::Api,
        Method::Post,
        cookie.as_deref(),
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    let clear =
        cleared.unwrap_or_else(|| format!("{}=; Path=/; Max-Age=0", remote::SESSION_COOKIE));
    keep_cookie(&webview, &server, Some(clear)).await;
    Ok(())
}

/// Read from the server's engine proxy (fixed routes; see remote.rs).
#[tauri::command]
pub async fn api_get(webview: Webview, path: String, env: Option<String>) -> Result<Reply, String> {
    require_launcher(webview.url())?;
    call(&webview, env.as_deref(), Target::Api, Method::Get, &path, None, None).await
}

/// Change something through the server's engine proxy (fixed routes and methods).
#[tauri::command]
pub async fn api_send(
    webview: Webview,
    method: String,
    path: String,
    body: Option<Value>,
    if_match: Option<String>,
    env: Option<String>,
) -> Result<Reply, String> {
    require_launcher(webview.url())?;
    let method = write_method(&method)?;
    if let Some(tag) = if_match.as_deref() {
        if !if_match_ok(tag) {
            return Err("That request is not available from the client.".to_string());
        }
    }
    call(
        &webview,
        env.as_deref(),
        Target::Api,
        method,
        &path,
        body,
        if_match.as_deref(),
    )
    .await
}

/// Read from the server's pre-alpha service.
#[tauri::command]
pub async fn svc_get(webview: Webview, path: String, env: Option<String>) -> Result<Reply, String> {
    require_launcher(webview.url())?;
    call(&webview, env.as_deref(), Target::Svc, Method::Get, &path, None, None).await
}

/// Write to the server's pre-alpha service.
#[tauri::command]
pub async fn svc_send(
    webview: Webview,
    method: String,
    path: String,
    body: Option<Value>,
    env: Option<String>,
) -> Result<Reply, String> {
    require_launcher(webview.url())?;
    let method = write_method(&method)?;
    call(&webview, env.as_deref(), Target::Svc, method, &path, body, None).await
}

/// The client's preferences (theme, keybindings, providers without keys, ...), from the app's
/// config folder. Null when none were saved.
#[tauri::command]
pub fn prefs_load(webview: Webview) -> Result<Value, String> {
    require_launcher(webview.url())?;
    Ok(remote::load_prefs(
        &state_of(webview.app_handle()).config_dir,
    ))
}

/// Save the client's preferences. A value that looks like a provider key is refused: keys stay
/// in the page's memory only.
#[tauri::command]
pub fn prefs_save(webview: Webview, prefs: Value) -> Result<(), String> {
    require_launcher(webview.url())?;
    if remote::looks_like_a_key(&prefs) {
        return Err("Preferences never hold a key.".to_string());
    }
    remote::save_prefs(&state_of(webview.app_handle()).config_dir, &prefs).map_err(str::to_string)
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
    fn settings_revisions_and_methods_are_checked() {
        for ok in ["0", "12", "\"7\"", "W/\"7\""] {
            assert!(if_match_ok(ok), "{ok}");
        }
        for bad in ["", "\"\"", "abc", "7, 8", "\"7", "1234567890123", "W/abc"] {
            assert!(!if_match_ok(bad), "{bad}");
        }
        assert!(write_method("GET").is_err());
        assert!(write_method("post").is_err());
        assert!(write_method("CONNECT").is_err());
        assert_eq!(write_method("PATCH"), Ok(Method::Patch));
    }

    #[test]
    fn every_command_takes_the_second_lock_first() {
        // Source check: each `#[tauri::command]` function begins with require_launcher, so
        // removing the call from one of them is caught here. Every command module is checked.
        let modules: [(&str, &str, &[&str]); 3] = [
            ("commands.rs", include_str!("commands.rs"), &COMMANDS),
            ("cmd_local.rs", include_str!("cmd_local.rs"), &crate::cmd_local::COMMANDS),
            ("cmd_files.rs", include_str!("cmd_files.rs"), &crate::cmd_files::COMMANDS),
        ];
        for (file, source, names) in modules {
            let body = source.split("#[cfg(test)]
mod tests").next().unwrap();
            let functions: Vec<&str> = body.split("#[tauri::command]").skip(1).collect();
            assert_eq!(functions.len(), names.len(), "{file}");
            for (function, name) in functions.iter().zip(names) {
                let head = function.split("
}
").next().unwrap();
                let after_signature = head.split(") -> ").nth(1).expect(name);
                let first_statement = after_signature.split('{').nth(1).unwrap().trim_start();
                assert!(
                    first_statement.starts_with("require_launcher(webview.url())?;"),
                    "{name} must check the caller first: {first_statement:.60}"
                );
            }
        }
    }
}
