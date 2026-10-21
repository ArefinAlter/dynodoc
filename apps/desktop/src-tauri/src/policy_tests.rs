//! Tests of the configuration files that carry the security model, so a careless edit to
//! a capability, the CSP or a dependency fails `cargo test` instead of shipping.

use crate::commands::COMMANDS;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn read(relative: &str) -> String {
    let path = root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
fn json(relative: &str) -> Value {
    serde_json::from_str(&read(relative)).unwrap_or_else(|e| panic!("{relative}: {e}"))
}
fn permission(command: &str) -> String {
    format!("allow-{}", command.replace('_', "-"))
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("not an array: {value}"))
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn remote_pages_get_no_capability_at_all() {
    let dir = root().join("capabilities");
    let mut files = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        files += 1;
        let capability: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let name = path.display().to_string();
        let object = capability.as_object().unwrap();
        // `remote` is the only way to give a server's pages access; it must never appear.
        assert!(
            !object.contains_key("remote"),
            "{name}: has a remote section"
        );
        assert_ne!(
            object.get("local"),
            Some(&Value::Bool(false)),
            "{name}: not local"
        );
        assert_eq!(strings(&object["windows"]), ["main"], "{name}");
        assert!(!object.contains_key("webviews"), "{name}");
        let allowed: Vec<String> = COMMANDS.iter().map(|c| permission(c)).collect();
        for item in strings(&object["permissions"]) {
            assert!(
                allowed.contains(&item),
                "{name}: {item} is not one of the launcher commands"
            );
        }
        // Every key must be one of the known ones (no `scope`, `deny`, `webviews` surprises).
        for key in object.keys() {
            assert!(
                [
                    "$schema",
                    "identifier",
                    "description",
                    "local",
                    "windows",
                    "platforms",
                    "permissions"
                ]
                .contains(&key.as_str()),
                "{name}: unexpected key {key}"
            );
        }
    }
    assert_eq!(files, 2, "capability files");
}

#[test]
fn desktop_gets_all_commands_and_phones_get_no_tools() {
    let desktop = json("capabilities/launcher.json");
    let mobile = json("capabilities/launcher-mobile.json");
    let all: Vec<String> = COMMANDS.iter().map(|c| permission(c)).collect();
    let mut got = strings(&desktop["permissions"]);
    got.sort();
    let mut want = all.clone();
    want.sort();
    assert_eq!(got, want);
    assert_eq!(
        strings(&desktop["platforms"]),
        ["linux", "macOS", "windows"]
    );

    assert_eq!(strings(&mobile["platforms"]), ["android", "iOS"]);
    let phone = strings(&mobile["permissions"]);
    assert_eq!(
        phone,
        [
            "allow-get-state",
            "allow-set-server",
            "allow-open-sign-in-link"
        ]
    );
    for tool in ["tools", "key"] {
        assert!(
            phone.iter().all(|p| !p.contains(tool)),
            "phones must not get {tool}"
        );
    }
}

#[test]
fn build_script_and_handler_list_the_same_commands() {
    let build = read("build.rs");
    let start = build
        .find("const COMMANDS: &[&str] = &[")
        .expect("commands array in build.rs");
    let tail = &build[start..];
    let open = tail.find("= &[").unwrap() + 4;
    let list = &tail[open..tail.find("];").unwrap()];
    let in_build: Vec<&str> = list
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(in_build, COMMANDS);

    let lib = read("src/lib.rs");
    for command in COMMANDS {
        assert!(
            lib.contains(&format!("commands::{command},")),
            "{command} not in invoke_handler"
        );
    }
    let handler = &lib[lib.find("generate_handler![").unwrap()..];
    let handler = &handler[..handler.find(']').unwrap()];
    assert_eq!(handler.matches("commands::").count(), COMMANDS.len());
}

#[test]
fn csp_is_strict_and_the_conf_has_no_dangerous_switches() {
    let conf = json("tauri.conf.json");
    let security = &conf["app"]["security"];
    let csp = security["csp"].as_str().unwrap();
    for want in [
        "default-src 'none'",
        "script-src 'self'",
        "style-src 'self'",
        "object-src 'none'",
        "base-uri 'none'",
        "form-action 'none'",
        "frame-ancestors 'none'",
        "connect-src ipc: http://ipc.localhost",
    ] {
        assert!(csp.contains(want), "csp lacks {want}: {csp}");
    }
    for never in [
        "unsafe-eval",
        "unsafe-inline",
        "unsafe-hashes",
        " *",
        "data:",
        "blob:",
        "https:",
    ] {
        assert!(!csp.contains(never), "csp has {never}: {csp}");
    }
    assert_eq!(security["freezePrototype"], true);
    // No window.__TAURI__ namespace for any page; the launcher uses the internal bridge.
    assert_eq!(conf["app"]["withGlobalTauri"], false);
    assert_eq!(
        strings(&security["capabilities"]),
        ["launcher", "launcher-mobile"]
    );
    let text = read("tauri.conf.json");
    for never in [
        "dangerous",
        "assetProtocol",
        "pattern",
        "\"remote\"",
        "devUrl",
    ] {
        assert!(!text.contains(never), "tauri.conf.json mentions {never}");
    }
    assert_eq!(
        conf["app"]["windows"],
        serde_json::json!([]),
        "the window is created in Rust"
    );
}

#[test]
fn identity_is_stable_and_the_version_has_one_source() {
    let conf = json("tauri.conf.json");
    assert_eq!(conf["productName"], "Dynodoc");
    let identifier = conf["identifier"].as_str().unwrap();
    assert_eq!(identifier, "online.dynodoc.client");
    assert!(
        !identifier.ends_with(".app"),
        "clashes with the macOS bundle extension"
    );
    assert!(
        conf.get("version").is_none(),
        "version belongs in Cargo.toml only"
    );
    let package: Value =
        serde_json::from_str(&fs::read_to_string(root().join("..").join("package.json")).unwrap())
            .unwrap();
    assert!(
        package.get("version").is_none(),
        "package.json must not carry a second version"
    );
    // The sidecar is a desktop-only part: the base file has none (a phone build would
    // otherwise look for engine-cli-<phone triple>), the three desktop overlays have it.
    assert!(conf["bundle"].get("externalBin").is_none());
    for platform in ["windows", "linux", "macos"] {
        let overlay = json(&format!("tauri.{platform}.conf.json"));
        assert_eq!(
            strings(&overlay["bundle"]["externalBin"]),
            ["binaries/engine-cli"]
        );
        assert_eq!(
            overlay.as_object().unwrap().len(),
            1,
            "{platform} overlay changes only the bundle"
        );
    }
    for platform in ["android", "ios"] {
        assert!(!root().join(format!("tauri.{platform}.conf.json")).exists());
    }
    assert_eq!(
        strings(&conf["plugins"]["deep-link"]["desktop"]["schemes"]),
        [crate::deeplink::SCHEME]
    );
}

#[test]
fn dependencies_stay_minimal() {
    let manifest = read("Cargo.toml");
    // The one plugin set justified in apps/desktop/README.md. Anything that reaches the
    // file system, shell, network, clipboard or updater from a page stays out.
    for banned in [
        "tauri-plugin-shell",
        "tauri-plugin-fs",
        "tauri-plugin-http",
        "tauri-plugin-dialog",
        "tauri-plugin-process",
        "tauri-plugin-updater",
        "tauri-plugin-clipboard",
        "tauri-plugin-global-shortcut",
        "tauri-plugin-notification",
        "tauri-plugin-os",
        "tauri-plugin-log",
        "devtools",
        "tray-icon",
        "protocol-asset",
        "isolation",
        "unstable",
    ] {
        assert!(!manifest.contains(banned), "Cargo.toml mentions {banned}");
    }
    let tauri_line = manifest
        .lines()
        .find(|l| l.starts_with("tauri = "))
        .expect("tauri dependency");
    // The Tauri command line normalises the line to `features = []` when it builds.
    assert!(tauri_line.contains("features = []"), "{tauri_line}");
    assert!(manifest.contains("\nname = \"dynodoc-desktop\""));
    assert!(manifest.contains("\nname = \"dynodoc_desktop_lib\""));
    assert!(manifest.contains("crate-type = [\"staticlib\", \"cdylib\", \"rlib\"]"));
    assert!(manifest.contains("[workspace]"), "own workspace");
    assert!(read("src/main.rs").contains("dynodoc_desktop_lib::run()"));
    assert!(read("src/lib.rs").contains("#[cfg_attr(mobile, tauri::mobile_entry_point)]"));
}

#[test]
fn the_engine_workspace_excludes_the_apps_folder() {
    let root_manifest = fs::read_to_string(root().join("../../../Cargo.toml")).unwrap();
    assert!(root_manifest.contains("exclude = [\"apps\"]"));
}

#[test]
fn the_launcher_page_is_static_and_has_no_inline_or_remote_code() {
    let html = fs::read_to_string(root().join("../ui/index.html")).unwrap();
    let js = fs::read_to_string(root().join("../ui/launcher.js")).unwrap();
    let css = fs::read_to_string(root().join("../ui/launcher.css")).unwrap();

    // Every script and stylesheet is a local file.
    assert_eq!(html.matches("<script").count(), 1);
    assert!(html.contains("<script src=\"launcher.js\" defer></script>"));
    assert_eq!(html.matches("<link").count(), 1);
    assert!(html.contains("<link rel=\"stylesheet\" href=\"launcher.css\" />"));
    for never in [
        " style=",
        "<style",
        "javascript:",
        "srcdoc",
        "<iframe",
        "<object",
        "<embed",
        "<form action",
        "<base",
    ] {
        assert!(!html.contains(never), "index.html has {never}");
    }
    // No inline event handler attributes.
    for line in html.lines() {
        let attributes = line
            .split_whitespace()
            .filter(|w| w.starts_with("on") && w.contains('='));
        assert_eq!(attributes.count(), 0, "inline handler: {line}");
    }
    // Every loaded or linked address is a local file, except the one help link.
    let mut seen = 0;
    for attribute in ["src=\"", "href=\"", "action=\""] {
        for (at, _) in html.match_indices(attribute) {
            let value = html[at + attribute.len()..].split('"').next().unwrap();
            seen += 1;
            let local = !value.contains(':') && !value.contains("//");
            assert!(
                local || value == "https://dynodoc.online/support",
                "{attribute}{value}"
            );
        }
    }
    assert!(
        seen >= 4,
        "expected the script, stylesheet, image and help link"
    );

    for never in [
        "eval(",
        "new Function",
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "document.write",
        "fetch(",
        "XMLHttpRequest",
        "WebSocket",
        "import(",
        "importScripts",
        "postMessage",
        "localStorage",
        "sessionStorage",
        "setTimeout(\"",
        "javascript:",
    ] {
        assert!(!js.contains(never), "launcher.js has {never}");
    }
    assert!(
        !css.contains("@import") && !css.contains("url("),
        "launcher.css loads nothing"
    );
    // Page text is only ever put in with textContent / value.
    assert!(js.contains("textContent"));
}

#[test]
fn the_workflow_never_runs_on_push() {
    let workflow =
        fs::read_to_string(root().join("../../../.github/workflows/desktop.yml")).unwrap();
    let triggers = &workflow[workflow.find("\non:").unwrap()..workflow.find("\njobs:").unwrap()];
    assert!(triggers.contains("workflow_dispatch"));
    assert!(triggers.contains("desktop-v*"));
    for never in ["pull_request", "schedule", "branches:", "paths:"] {
        assert!(!triggers.contains(never), "desktop.yml triggers on {never}");
    }
    let push = &triggers[triggers.find("push:").unwrap()..];
    assert!(push.contains("tags:"), "push is for tags only");
}

#[test]
fn the_script_injected_into_every_page_cannot_reach_the_app() {
    // It runs in the pages of the configured server too, so it must do nothing but call the
    // page's own window.open (from a trusted click, or for target=_blank).
    let js = read("src/link_targets.js");
    for never in [
        "__TAURI",
        "invoke",
        "fetch(",
        "XMLHttpRequest",
        "eval(",
        "Function(",
        "innerHTML",
        "postMessage",
        "localStorage",
        "cookie",
        "location.href",
        "location.assign",
        "location.replace",
        "document.write",
    ] {
        assert!(!js.contains(never), "link_targets.js has {never}");
    }
    // The secret is a placeholder in the file and a closure variable in the script.
    assert!(js.contains("var secret = '__GESTURE_SECRET__';"));
    assert_eq!(js.matches("__GESTURE_SECRET__").count(), 1);
    // Only a trusted click is believed, and the original window.open is kept first.
    assert!(js.contains("!event.isTrusted"));
    assert!(js.contains("var nativeOpen = window.open;"));
    assert!(js.contains("marker + secret"));
    assert!(js.contains("nativeOpen.call(window, link.href, '_blank')"));
    assert!(js.contains(&format!("'https://{}/'", crate::handoff::MARKER_HOST)));
    assert!(read("src/window.rs").contains("include_str!(\"link_targets.js\")"));
}

#[test]
fn the_permission_handler_and_the_launcher_bridge_are_fixed_choices() {
    let lib = read("src/lib.rs");
    assert!(lib.contains(".on_permission_request(|_, kind| window::permission_response(kind))"));
    for never in ["PermissionResponse::Allow", "PermissionResponse::Default"] {
        assert!(!lib.contains(never), "lib.rs has {never}");
        let window = read("src/window.rs");
        let production = &window[..window.find("#[cfg(test)]\nmod tests").unwrap()];
        assert!(!production.contains(never), "window.rs has {never}");
    }
    // The launcher calls the internal bridge; nothing refers to the global namespace.
    let js = read("../ui/launcher.js");
    assert!(js.contains("window.__TAURI_INTERNALS__"));
    assert!(!js.contains("__TAURI__."));
}
