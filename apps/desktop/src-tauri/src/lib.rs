//! Dynodoc desktop shell: a native window around the configured Dynodoc server.
//!
//! The web app is not bundled. This crate adds the pieces a browser tab cannot: a launcher
//! page to choose the server, a hardened navigation policy, a `dynodoc://` link handler and
//! a fixed set of local tools (a device signing key kept by the operating system). Read
//! docs/decisions/016-devices-processors-desktop-and-mobile-apps.md and
//! apps/desktop/README.md before changing the security-relevant modules (`nav`, `handoff`,
//! `server`, `deeplink`, `signin`, `sidecar`, `commands`, the capability file and tauri.conf.json).

mod accounts;
#[cfg(desktop)]
mod agent;
#[cfg(desktop)]
mod cmd_accounts;
#[cfg(desktop)]
mod cmd_agent;
#[cfg(desktop)]
mod cmd_files;
#[cfg(desktop)]
mod cmd_local;
mod commands;
mod config;
mod deeplink;
mod envs;
mod files;
mod handoff;
#[cfg(desktop)]
mod local;
#[cfg(desktop)]
mod menu;
mod nav;
mod remote;
mod server;
mod sidecar;
mod signin;
mod watch;
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
    pub handoffs: handoff::Limiters,
    /// Proves a click to the new-window hook; lives in the injected script and here only.
    pub click_secret: String,
    /// The one browser sign-in this running app is waiting for (memory only, see signin.rs).
    pub signin: signin::Pending,
    /// A finished attempt's fragment, waiting for the finish page to load (see signin.rs).
    pub outbox: signin::Outbox,
    /// The workspace on this computer: its own database and engine (decision 019).
    #[cfg(desktop)]
    pub local: local::Local,
    /// Files and folders the person chose: the page holds tokens, the app holds paths.
    #[cfg(desktop)]
    pub picks: files::Picks,
    /// The agent: provider keys, consent, caps and the runs in progress.
    #[cfg(desktop)]
    pub agent: agent::Agent,
    /// Servers the person signed in to from the app, with their sessions (decision 020).
    #[cfg(desktop)]
    pub accounts: accounts::Accounts,
    /// Folder watch: what the bound folders showed since the page last asked.
    #[cfg(desktop)]
    pub watcher: watch::Watcher,
}

impl AppState {
    pub fn server(&self) -> Option<server::ServerOrigin> {
        self.server.lock().ok().and_then(|s| s.clone())
    }

    pub fn unconfirmed(&self) -> Option<server::ServerOrigin> {
        self.unconfirmed.lock().ok().and_then(|s| s.clone())
    }

    /// The person chose (or confirmed) this server. A different server forgets the browser
    /// sign-in in flight: its verifier was made for the old one, and a finished attempt's fragment
    /// waiting for a page is only for the old one's finish page (the outbox and the attempt each
    /// compare the server again, so this is the second lock, not the only one).
    pub fn set_server(&self, server: server::ServerOrigin) {
        if let Ok(mut current) = self.server.lock() {
            if current.as_ref() != Some(&server) {
                self.signin.clear();
                self.outbox.clear();
            }
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

/// Portable mode: a folder named `dynodoc-data` beside the program keeps the config, the local
/// workspace and the web view's data there instead of in the per-user folders (the portable zip
/// ships it empty; delete it to go back). The secrets of such a copy get a credential-store name of
/// their own, so a portable copy and an installed one never share or overwrite each other's.
fn portable_home() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.join("dynodoc-data");
    dir.is_dir().then_some(dir)
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
    let test_home = home_override();
    let portable = test_home.is_none() && portable_home().is_some();
    let home = test_home.or_else(portable_home);
    let ordinary = home.is_none();
    let (config_dir, data_dir, webview_dir) = match home {
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
    #[cfg(desktop)]
    let local = {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_default();
        local::Local::new(
            &data_dir,
            app.path().resource_dir().ok().as_deref(),
            &exe_dir,
            ordinary,
            portable,
        )
    };
    #[cfg(desktop)]
    let agent = agent::Agent::new(
        &config_dir,
        &local::secrets::service_for(&data_dir, ordinary),
    );
    #[cfg(desktop)]
    let accounts = {
        let service = local::secrets::service_for(&data_dir, ordinary);
        accounts::Accounts::new(
            &config_dir,
            local::secrets::Preferred::open(&config_dir.join("sessions"), &service),
        )
    };
    let (server, unconfirmed) = split_loaded(config::load(&config_dir, &data_dir));
    Ok(AppState {
        server: Mutex::new(server),
        unconfirmed: Mutex::new(unconfirmed),
        config_dir,
        data_dir,
        webview_dir,
        launcher_grant: Mutex::new(nav::LauncherGrant::default()),
        handoffs: handoff::Limiters::new(),
        click_secret: handoff::new_secret()
            .map_err(|e| std::io::Error::other(format!("no operating-system randomness: {e}")))?,
        signin: signin::Pending::default(),
        outbox: signin::Outbox::default(),
        #[cfg(desktop)]
        local,
        #[cfg(desktop)]
        picks: files::Picks::default(),
        #[cfg(desktop)]
        watcher: watch::Watcher::default(),
        #[cfg(desktop)]
        agent,
        #[cfg(desktop)]
        accounts,
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
        if home_override().is_none() && portable_home().is_none() {
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
            commands::start_browser_sign_in,
            commands::tools_version,
            commands::device_key_status,
            commands::device_key_create,
            commands::device_public_key,
            commands::session_info,
            commands::visitor_start,
            commands::sign_out,
            commands::api_get,
            commands::api_send,
            commands::svc_get,
            commands::svc_send,
            commands::prefs_load,
            commands::prefs_save,
            #[cfg(desktop)]
            cmd_local::local_status,
            #[cfg(desktop)]
            cmd_local::local_create_profile,
            #[cfg(desktop)]
            cmd_local::local_retry,
            #[cfg(desktop)]
            cmd_local::local_open_folder,
            #[cfg(desktop)]
            cmd_local::backup_export,
            #[cfg(desktop)]
            cmd_local::backup_import,
            #[cfg(desktop)]
            cmd_files::files_pick,
            #[cfg(desktop)]
            cmd_files::file_read,
            #[cfg(desktop)]
            cmd_files::file_export,
            #[cfg(desktop)]
            cmd_files::folder_bind,
            #[cfg(desktop)]
            cmd_files::folder_unbind,
            #[cfg(desktop)]
            cmd_files::folder_status,
            #[cfg(desktop)]
            cmd_files::watch_poll,
            #[cfg(desktop)]
            cmd_files::watch_seen,
            #[cfg(desktop)]
            cmd_agent::agent_config,
            #[cfg(desktop)]
            cmd_agent::agent_config_set,
            #[cfg(desktop)]
            cmd_agent::agent_consent,
            #[cfg(desktop)]
            cmd_agent::agent_key_set,
            #[cfg(desktop)]
            cmd_agent::agent_key_clear,
            #[cfg(desktop)]
            cmd_agent::agent_key_status,
            #[cfg(desktop)]
            cmd_agent::agent_keys_unlock,
            #[cfg(desktop)]
            cmd_agent::agent_preview,
            #[cfg(desktop)]
            cmd_agent::agent_run,
            #[cfg(desktop)]
            cmd_agent::agent_tool_result,
            #[cfg(desktop)]
            cmd_agent::agent_cancel,
            #[cfg(desktop)]
            cmd_agent::agent_usage,
            #[cfg(desktop)]
            cmd_accounts::accounts_list,
            #[cfg(desktop)]
            cmd_accounts::account_add,
            #[cfg(desktop)]
            cmd_accounts::account_remove,
            #[cfg(desktop)]
            cmd_accounts::account_signin_start,
            #[cfg(desktop)]
            cmd_accounts::account_signin_status,
            #[cfg(desktop)]
            cmd_accounts::account_signin_cancel,
            #[cfg(desktop)]
            cmd_accounts::account_signout,
            #[cfg(desktop)]
            cmd_accounts::account_check,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(resolve_state(&handle)?);
            // The workspace on this computer starts in the background while the window opens.
            #[cfg(desktop)]
            {
                app.state::<AppState>().local.start();
                cmd_files::spawn_watch_loop(&handle);
            }
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

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building Dynodoc");
    app.run(|handle, event| {
        // Closing the app closes the engine and the database first, cleanly.
        #[cfg(desktop)]
        if let tauri::RunEvent::ExitRequested { api, .. } = &event {
            let state = handle.state::<AppState>();
            if !state.local.is_closing() {
                api.prevent_exit();
                let local = state.local.clone();
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    local.shutdown().await;
                    handle.exit(0);
                });
            }
        }
        #[cfg(not(desktop))]
        let _ = (handle, &event);
    });
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

    fn bare_state() -> AppState {
        AppState {
            server: Mutex::new(None),
            unconfirmed: Mutex::new(None),
            config_dir: PathBuf::new(),
            data_dir: PathBuf::new(),
            webview_dir: None,
            launcher_grant: Mutex::new(nav::LauncherGrant::default()),
            handoffs: handoff::Limiters::new(),
            click_secret: String::new(),
            signin: signin::Pending::default(),
            outbox: signin::Outbox::default(),
            #[cfg(desktop)]
            local: local::Local::new(
                std::path::Path::new("."),
                None,
                std::path::Path::new("."),
                false,
                false,
            ),
            picks: files::Picks::default(),
            watcher: watch::Watcher::default(),
            agent: agent::Agent::new(std::path::Path::new("."), "online.dynodoc.client.test"),
            accounts: accounts::Accounts::new(
                std::path::Path::new("."),
                local::secrets::FileStore::new(std::path::Path::new("./test-sessions")),
            ),
        }
    }

    #[test]
    fn choosing_another_server_forgets_the_sign_in_in_flight() {
        let a = parse_server_address("https://a.example.org").unwrap();
        let b = parse_server_address("https://b.example.org").unwrap();
        let state = bare_state();
        state.set_server(a.clone());
        let now = signin::Moment::now();
        let start = |state: &AppState| {
            let (attempt, request) =
                signin::Attempt::new(&a.as_text(), now, signin::os_random).unwrap();
            state.signin.start(attempt);
            state
                .outbox
                .put(signin::Secret::new("fragment".into()), &a.as_text(), now);
            request
        };
        // Choosing the same server again changes nothing.
        let request = start(&state);
        state.set_server(a.clone());
        assert!(state.signin.is_waiting());
        assert!(state.outbox.take(now, &a.as_text()).is_some());
        // Another server: the attempt and the fragment are gone, and stay gone.
        let request2 = start(&state);
        state.set_server(b.clone());
        assert!(!state.signin.is_waiting());
        assert!(state.outbox.take(now, &a.as_text()).is_none());
        assert!(state.outbox.take(now, &b.as_text()).is_none());
        for state_text in [&request.state, &request2.state] {
            assert_eq!(
                state
                    .signin
                    .complete(now, &b.as_text(), &"ab".repeat(32), state_text)
                    .unwrap_err(),
                signin::Refusal::NothingPending
            );
        }
        // And the source says so: the clearing is in set_server itself.
        let source = include_str!("lib.rs");
        let body = &source[source.find("pub fn set_server(").unwrap()..];
        let body = &body[..body.find("\n    }\n").unwrap()];
        assert!(body.contains("self.signin.clear();") && body.contains("self.outbox.clear();"));
    }
}
