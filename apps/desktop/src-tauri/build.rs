// Every command the launcher may call is listed here. That makes Tauri generate one
// `allow-<command>` permission each and stops it treating app commands as open to all.
// Keep in step with COMMANDS in src/commands.rs (a test compares them).
const COMMANDS: &[&str] = &[
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
    "local_status",
    "local_create_profile",
    "local_retry",
    "local_open_folder",
    "backup_export",
    "backup_import",
    "files_pick",
    "file_read",
    "file_export",
    "folder_bind",
    "folder_unbind",
    "folder_status",
    "watch_poll",
    "watch_seen",
    "agent_config",
    "agent_config_set",
    "agent_consent",
    "agent_key_set",
    "agent_key_clear",
    "agent_key_status",
    "agent_keys_unlock",
    "agent_preview",
    "agent_run",
    "agent_tool_result",
    "agent_cancel",
    "agent_usage",
    "agent_installed_status",
    "accounts_list",
    "account_add",
    "account_remove",
    "account_signin_start",
    "account_signin_status",
    "account_signin_cancel",
    "account_signout",
    "account_check",
    "window_reload",
    "devtools_open",
];

fn main() {
    // The sidecar's file name carries the target triple; the sidecar tests need it.
    if let Ok(target) = std::env::var("TARGET") {
        println!("cargo:rustc-env=DYNODOC_TARGET={target}");
    }
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build failed");
}
