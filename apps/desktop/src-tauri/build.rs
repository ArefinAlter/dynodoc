// Every command the launcher may call is listed here. That makes Tauri generate one
// `allow-<command>` permission each and stops it treating app commands as open to all.
// Keep in step with COMMANDS in src/commands.rs (a test compares them).
const COMMANDS: &[&str] = &[
    "get_state",
    "set_server",
    "open_sign_in_link",
    "tools_version",
    "device_key_status",
    "device_key_create",
    "device_public_key",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build failed");
}
