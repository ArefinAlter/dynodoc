//! Tests of the configuration files that carry the security model, so a careless edit to
//! a capability, the CSP or a dependency fails `cargo test` instead of shipping.

/// Every command the page may call, across the command modules, in the order of build.rs and of
/// the handler list in lib.rs.
fn commands() -> Vec<&'static str> {
    let mut all: Vec<&'static str> = crate::commands::COMMANDS.to_vec();
    all.extend(crate::cmd_local::COMMANDS);
    all.extend(crate::cmd_files::COMMANDS);
    all.extend(crate::cmd_agent::COMMANDS);
    all.extend(crate::cmd_accounts::COMMANDS);
    all
}
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
        let allowed: Vec<String> = commands().iter().map(|c| permission(c)).collect();
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
    let all: Vec<String> = commands().iter().map(|c| permission(c)).collect();
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
            "allow-open-sign-in-link",
            "allow-start-browser-sign-in"
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
    assert_eq!(in_build, commands());

    let lib = read("src/lib.rs");
    for (module, list) in [
        ("commands", crate::commands::COMMANDS.to_vec()),
        ("cmd_local", crate::cmd_local::COMMANDS.to_vec()),
        ("cmd_files", crate::cmd_files::COMMANDS.to_vec()),
        ("cmd_agent", crate::cmd_agent::COMMANDS.to_vec()),
    ] {
        for command in list {
            assert!(
                lib.contains(&format!("{module}::{command},")),
                "{command} not in invoke_handler"
            );
        }
    }
    let handler = &lib[lib.find("generate_handler![").unwrap()..];
    let handler = &handler[..handler.find("])").unwrap()];
    assert_eq!(
        handler.matches("::").count(),
        commands().len(),
        "the handler lists exactly the commands"
    );
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
    // The Android project takes its versionName and versionCode from the config's version, and
    // the Tauri command line does not fall back to Cargo.toml for it (without one the APK says
    // 1.0 / code 1 forever, which a store would refuse to update). The Android overlay repeats
    // the Cargo.toml version and nothing else; this test keeps the two equal.
    let android = json("tauri.android.conf.json");
    assert_eq!(
        android,
        serde_json::json!({ "version": env!("CARGO_PKG_VERSION") }),
        "tauri.android.conf.json must hold exactly the Cargo.toml version"
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
    // iPhone has no overlay; Android has the version-only one checked above (no sidecar there).
    assert!(!root().join("tauri.ios.conf.json").exists());
    assert_eq!(
        strings(&conf["plugins"]["deep-link"]["desktop"]["schemes"]),
        [crate::deeplink::SCHEME]
    );
    // Android: the same scheme, no app link (no autoVerify). Host open with the three path
    // shapes, and host signin (the end of the browser sign-in) with no path at all.
    let mobile = conf["plugins"]["deep-link"]["mobile"].as_array().unwrap();
    assert_eq!(mobile.len(), 4);
    for entry in mobile {
        assert_eq!(strings(&entry["scheme"]), [crate::deeplink::SCHEME]);
        assert_eq!(entry["appLink"], false);
    }
    let hosts: Vec<&str> = mobile.iter().map(|e| e["host"].as_str().unwrap()).collect();
    assert_eq!(hosts, ["open", "open", "open", "signin"]);
    let sign_in = &mobile[3];
    for key in [
        "path",
        "pathPrefix",
        "pathPattern",
        "pathAdvancedPattern",
        "pathSuffix",
    ] {
        assert!(
            sign_in.get(key).is_none(),
            "the sign-in filter names no path: {key}"
        );
    }
    let shapes: Vec<String> = mobile[..3]
        .iter()
        .map(|e| {
            e.get("path")
                .or_else(|| e.get("pathPrefix"))
                .and_then(|p| p.as_array())
                .and_then(|p| p[0].as_str())
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(shapes, ["/workspace", "/projects/", "/documents/"]);
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

    // Every script and stylesheet is a local file: the client, and the host of the Office codec
    // (which starts the codec's worker; see `the_office_codec_has_no_network_of_its_own`).
    assert_eq!(html.matches("<script").count(), 2);
    assert!(html.contains("<script src=\"launcher.js\" defer></script>"));
    assert!(html.contains("<script src=\"office-host.js\" defer></script>"));
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
fn the_office_codec_has_no_network_of_its_own() {
    let host = fs::read_to_string(root().join("../ui/office-host.js")).unwrap();
    let worker = fs::read_to_string(root().join("../ui/office-worker.js")).unwrap();
    let launcher = fs::read_to_string(root().join("../ui/launcher.js")).unwrap();
    // The page's side only starts a worker from a file of the app and passes data; the client
    // itself (launcher.js) stays free of cross-context messaging.
    assert!(
        host.contains("new Worker(\"office-worker.js\")")
            || host.contains("new Worker('office-worker.js')")
    );
    assert!(!launcher.contains("new Worker"));
    // Neither file may reach the network (the page's CSP allows nothing but `ipc:` as well).
    for never in [
        "fetch(",
        "WebSocket",
        "sendBeacon",
        "EventSource",
        "importScripts(\"http",
    ] {
        assert!(!host.contains(never), "office-host.js has {never}");
        assert!(!worker.contains(never), "office-worker.js has {never}");
    }
    // The worker is built from the web workspace's own importers, exporters and push planner.
    for want in ["_ddcomments", "DynodocRevision", "DynodocDocument"] {
        assert!(worker.contains(want), "office-worker.js lacks {want}");
    }
    // The CSP must keep allowing a worker from the app's own files and nothing else.
    let csp = json("tauri.conf.json")["app"]["security"]["csp"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!csp.contains("worker-src") || csp.contains("worker-src 'self'"));
    assert!(csp.contains("script-src 'self'") && !csp.contains("blob:"));
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
fn the_workflow_builds_exactly_the_lockfile_and_hardens_android() {
    let workflow =
        fs::read_to_string(root().join("../../../.github/workflows/desktop.yml")).unwrap();
    let mut checked = 0;
    for line in workflow.lines().map(str::trim) {
        let Some(command) = line.strip_prefix("run: ") else {
            continue;
        };
        let builds = command.starts_with("cargo test")
            || command.starts_with("cargo clippy")
            || command.starts_with("cargo build")
            || command.starts_with("npm run tauri -- build")
            || command.starts_with("npm run tauri -- android build")
            || command.starts_with("npm run tauri -- ios build");
        if builds {
            assert!(command.contains("--locked"), "no --locked: {command}");
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} build commands found");
    // A tag run must be gated before any signing secret is added.
    assert!(workflow.contains("protected GitHub environment"));
    // Android: the project is COMMITTED, so CI never runs `android init` (it would rewrite the
    // hardened files); the committed project is only checked (`--check` changes nothing), and
    // that happens before the build.
    assert!(!workflow.contains("tauri -- android init"));
    let harden = workflow.find("scripts/harden-android.mjs --check").unwrap();
    let build = workflow.find("tauri -- android build").unwrap();
    assert!(harden < build);
    assert!(!workflow.contains("node scripts/harden-android.mjs\n"));
    // The scripts' own tests run in CI too (all of them, including the real-output fixtures).
    assert!(workflow.contains("node --test scripts/*.test.mjs"));
    // And what shipped is checked too: the merged manifest and the signing key of every built
    // APK, after the build and before the upload.
    let merged = workflow.find("scripts/check-apk-manifest.mjs").unwrap();
    let upload = workflow.find("name: dynodoc-android-debug-apk").unwrap();
    assert!(build < merged && merged < upload);
    // The tools are named by version, not "whatever the runner image has".
    for pinned in [
        "platforms;android-37.0",
        "build-tools;37.0.0",
        "ndk;28.2.13676358",
    ] {
        assert!(workflow.contains(pinned), "{pinned}");
    }
}

#[test]
fn the_android_hardening_script_is_the_one_the_tests_exercise() {
    let script = read("../scripts/harden-android.mjs");
    for want in [
        "android:allowBackup=\"false\"",
        "usesCleartextTraffic",
        "process.exit(1)",
    ] {
        assert!(script.contains(want), "harden-android.mjs lacks {want}");
    }
    assert!(root().join("../scripts/harden-android.test.mjs").exists());
    // The merged-manifest check refuses what the text check refuses, in the APK itself.
    let checker = read("../scripts/check-apk-manifest.mjs");
    for want in [
        "allowBackup",
        "usesCleartextTraffic",
        "debuggable",
        "process.exit(1)",
    ] {
        assert!(
            checker.contains(want),
            "check-apk-manifest.mjs lacks {want}"
        );
    }
    assert!(root()
        .join("../scripts/check-apk-manifest.test.mjs")
        .exists());
}

#[test]
fn the_script_injected_into_every_page_cannot_reach_the_app_or_be_subverted() {
    // It runs in the pages of the configured server too, so it must do nothing but call the
    // page's own window.open (from a trusted click, or for target=_blank), and it must call
    // nothing the page can replace by the time of a click.
    let js = read("src/link_targets.js");
    let code: String = js
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
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
        assert!(!code.contains(never), "link_targets.js has {never}");
    }
    // Nothing the page can replace may be looked up at click time: no method calls on page
    // objects at all, no closest, no URL objects, no regular expressions, no array methods or
    // iterators, no template strings. (Everything is a captured, uncurried built-in.)
    for never in [
        ".call(",
        ".apply(",
        "Reflect",
        "closest",
        "new URL",
        "URL(",
        "RegExp",
        ".test(",
        ".exec(",
        ".match(",
        ".replace(",
        ".indexOf(",
        ".startsWith(",
        ".slice(",
        ".concat(",
        ".join(",
        ".push(",
        ".map(",
        ".forEach(",
        " of ",
        "Symbol",
        "...",
        "`",
        ".then(",
        "async ",
        "await ",
        "JSON",
        "Object.keys",
        "Array.",
        "String(",
        "addEventListener(",
    ] {
        assert!(!code.contains(never), "link_targets.js uses {never}");
    }
    // `.bind(` appears once, in the capture of the uncurry helper, and `.call` only there.
    assert_eq!(code.matches(".bind(").count(), 1);
    assert!(code.contains("var uncurry = Function.prototype.bind.bind(Function.prototype.call);"));
    assert_eq!(code.matches("Function.prototype.call").count(), 1);
    // Every built-in is captured by the injection-time statements, and used through them.
    for captured in [
        "getter(Event.prototype, 'target')",
        "getter(Event.prototype, 'defaultPrevented')",
        "getter(MouseEvent.prototype, 'button')",
        "uncurry(Event.prototype.preventDefault)",
        "getter(Node.prototype, 'parentNode')",
        "getter(Node.prototype, 'nodeName')",
        "getter(HTMLAnchorElement.prototype, 'href')",
        "getter(HTMLAnchorElement.prototype, 'protocol')",
        "getter(HTMLAnchorElement.prototype, 'origin')",
        "uncurry(Element.prototype.hasAttribute)",
        "uncurry(Element.prototype.getAttribute)",
        "uncurry(String.prototype.toLowerCase)",
        "uncurry(EventTarget.prototype.addEventListener)",
        "var open = uncurry(win.open);",
        "var encode = encodeURIComponent;",
    ] {
        assert!(code.contains(captured), "link_targets.js lacks {captured}");
    }
    // The secret is a placeholder in the file and a closure variable in the script, and is
    // used in exactly one place: the marker address given to the captured window.open.
    assert!(js.contains("var secret = '__GESTURE_SECRET__';"));
    assert_eq!(js.matches("__GESTURE_SECRET__").count(), 1);
    assert_eq!(code.matches("secret").count(), 2, "declaration and one use");
    assert!(code.contains("open(win, marker + secret + '?u=' + encode(href), '_blank');"));
    // Only a trusted click is believed.
    assert!(code.contains("event.isTrusted !== true"));
    assert!(code.contains(&format!("'https://{}/'", crate::handoff::MARKER_HOST)));
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

// ---- the committed Android project (src-tauri/gen/android) ----------------------------------

fn android(relative: &str) -> String {
    read(&format!("gen/android/{relative}"))
}

/// The files under gen/android that are committed (build output and the files the build writes
/// with machine paths in them are git-ignored by gen/android's own .gitignore files).
fn android_sources() -> Vec<PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                if matches!(
                    name.as_str(),
                    "build"
                        | ".gradle"
                        | ".kotlin"
                        | ".idea"
                        | "generated"
                        | "jniLibs"
                        | "assets"
                        | ".tauri"
                ) {
                    continue;
                }
                walk(&path, out);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("kt" | "kts" | "xml" | "gradle" | "properties" | "pro")
            ) && !matches!(
                name.as_str(),
                "tauri.settings.gradle"
                    | "tauri.build.gradle.kts"
                    | "proguard-tauri.pro"
                    | "tauri.properties"
                    | "local.properties"
            ) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("gen/android"), &mut out);
    out
}

#[test]
fn the_android_project_is_committed_without_machine_paths_or_secrets() {
    let files = android_sources();
    assert!(
        files.len() >= 15,
        "only {} Android source files found",
        files.len()
    );
    for path in files {
        let text = fs::read_to_string(&path).unwrap();
        let lower = text.to_lowercase();
        for banned in [
            "c:\\users",
            "c:/users",
            "d:\\dynodoc",
            "e:\\android",
            "/home/",
            "a2i",
            "storepassword",
            "keypassword",
        ] {
            // Only the loader of the (git-ignored) keystore.properties may name the keys.
            if path.ends_with("app/build.gradle.kts") && banned.ends_with("password") {
                continue;
            }
            assert!(
                !lower.contains(banned),
                "{} contains {banned}",
                path.display()
            );
        }
    }
    let ignore = android(".gitignore");
    for ignored in [
        "local.properties",
        "keystore.properties",
        "/tauri.settings.gradle",
        ".kotlin",
    ] {
        assert!(
            ignore.contains(ignored),
            "gen/android/.gitignore lacks {ignored}"
        );
    }
    let app_ignore = android("app/.gitignore");
    for ignored in [
        "/tauri.build.gradle.kts",
        "/tauri.properties",
        "/proguard-tauri.pro",
    ] {
        assert!(
            app_ignore.contains(ignored),
            "app/.gitignore lacks {ignored}"
        );
    }
    // The root .gitignore of the app keeps the Android project and nothing else of gen/.
    let top = read("../.gitignore");
    assert!(top.contains("src-tauri/gen/*") && top.contains("!src-tauri/gen/android/"));
}

#[test]
fn the_android_manifest_is_hardened_and_routes_only_the_deep_links() {
    let manifest = android("app/src/main/AndroidManifest.xml");
    assert!(manifest.contains("android:allowBackup=\"false\""));
    assert!(manifest.contains("android:dataExtractionRules=\"@xml/data_extraction_rules\""));
    assert!(manifest.contains("android:usesCleartextTraffic=\"${usesCleartextTraffic}\""));
    for never in [
        "android:networkSecurityConfig",
        "autoVerify",
        "android:debuggable",
        "android:testOnly",
        "android.permission.CAMERA",
        "READ_EXTERNAL_STORAGE",
        "WRITE_EXTERNAL_STORAGE",
        "ACCESS_FINE_LOCATION",
        "RECORD_AUDIO",
    ] {
        assert!(!manifest.contains(never), "the manifest has {never}");
    }
    assert_eq!(manifest.matches("<uses-permission ").count(), 1);
    assert!(manifest.contains("<uses-permission android:name=\"android.permission.INTERNET\" />"));
    assert_eq!(
        manifest.matches("android:exported=\"true\"").count(),
        1,
        "only MainActivity"
    );
    // The deep-link plugin writes these from tauri.conf.json at build time; the committed
    // manifest must be exactly what it writes: scheme dynodoc, host open, three path shapes.
    assert_eq!(
        manifest
            .matches("<data android:scheme=\"dynodoc\" />")
            .count(),
        4
    );
    assert_eq!(
        manifest.matches("<data android:host=\"open\" />").count(),
        3
    );
    // The end of the browser sign-in: dynodoc://signin?code=..&state=.. (host signin, no path).
    assert_eq!(
        manifest.matches("<data android:host=\"signin\" />").count(),
        1
    );
    assert_eq!(manifest.matches("<data ").count(), 11);
    assert!(manifest.contains("<data android:path=\"/workspace\" />"));
    assert!(manifest.contains("<data android:pathPrefix=\"/projects/\" />"));
    assert!(manifest.contains("<data android:pathPrefix=\"/documents/\" />"));
    let gradle = android("app/build.gradle.kts");
    assert!(!gradle.contains("\"usesCleartextTraffic\"] = \"true\""));
    assert!(gradle.contains("targetSdk = 37"));
    // 29, not 24: the download manager needs no storage permission from Android 10 on, and
    // Android 7 to 9 are open to task hijacking (StrandHogg) the manifest cannot fully close.
    assert!(gradle.contains("minSdk = 29"));
    let conf = json("tauri.conf.json");
    assert_eq!(conf["bundle"]["android"]["minSdkVersion"], 29);
    // The activity has a task of its own (an empty affinity), and is the only one declared.
    assert!(manifest.contains("android:taskAffinity=\"\""));
    assert_eq!(manifest.matches("android:taskAffinity").count(), 1);
    assert!(!manifest.contains("allowTaskReparenting=\"true\""));
    // Gradle checks every dependency against the committed checksums.
    let verification = android("gradle/verification-metadata.xml");
    assert!(verification.contains("<verify-metadata>true</verify-metadata>"));
    assert!(verification.matches("<component ").count() >= 400);
    assert!(verification.contains("aapt2-9.3.1-15703166-linux.jar"));
    assert!(
        !verification.contains("<trust "),
        "no dependency may be trusted without a checksum"
    );
    assert!(!verification.contains("<ignored-keys"));
    // The extraction rules exclude every domain from both transfers (harden-android.mjs checks
    // the same in CI; this keeps `cargo test` honest too).
    let rules = android("app/src/main/res/xml/data_extraction_rules.xml");
    for section in ["cloud-backup", "device-transfer"] {
        assert!(rules.contains(&format!("<{section}>")), "{section}");
    }
    assert!(!rules.contains("<include"));
    assert_eq!(rules.matches("<exclude domain=").count(), 18);
    let wrapper = android("gradle/wrapper/gradle-wrapper.properties");
    assert!(
        wrapper.contains("distributionSha256Sum="),
        "the Gradle download must be pinned by checksum"
    );
}

#[test]
fn the_android_code_this_repository_adds_has_no_javascript_bridge() {
    // Tauri's generated Kotlin has one (wry's `ipc`, guarded by the permission files, probed
    // from a hosted page: every call refused). What THIS repository adds must have none.
    for file in ["MainActivity.kt", "OfflineClient.kt"] {
        let source = android(&format!("app/src/main/java/online/dynodoc/client/{file}"));
        // Comments may name what is not there.
        let text: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for never in [
            "addJavascriptInterface",
            "@JavascriptInterface",
            "evaluateJavascript",
            "setJavaScriptEnabled",
            "allowUniversalAccessFromFileURLs",
            "setWebContentsDebuggingEnabled",
        ] {
            assert!(!text.contains(never), "{file} uses {never}");
        }
    }
    let activity = android("app/src/main/java/online/dynodoc/client/MainActivity.kt");
    for stated in [
        "allowFileAccess = false",
        "allowContentAccess = false",
        "MIXED_CONTENT_NEVER_ALLOW",
        "setSupportMultipleWindows(false)",
        "javaScriptCanOpenWindowsAutomatically = false",
    ] {
        assert!(activity.contains(stated), "MainActivity lacks {stated}");
    }
    // Tapjacking: touches while another window covers the page are dropped.
    assert!(activity.contains("filterTouchesWhenObscured = true"));
    // A re-created activity does not replay the link that started it.
    assert!(activity
        .contains("intent = Intent(this, MainActivity::class.java).setAction(Intent.ACTION_MAIN)"));
    // Downloads: a touch in the last 5 s, one per 3 s, an https file of exactly the page's origin
    // (scheme, host and port), no cookies ever sent, a cleaned name, nothing installable.
    let code: String = activity
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for stated in [
        "now - lastTouch > 5000",
        "now - lastDownload < 3000",
        "Origins.same(url, page)",
        "getCookie(url)",
        "DownloadNames.safe(",
    ] {
        assert!(code.contains(stated), "MainActivity lacks {stated}");
    }
    for never in ["addRequestHeader", "\"Cookie\"", ".host == page.host"] {
        assert!(!code.contains(never), "MainActivity has {never}");
    }
    // The offline page holds no script and links only to the address that failed.
    let offline = android("app/src/main/java/online/dynodoc/client/OfflineClient.kt");
    assert!(!offline.contains("<script"));
    assert!(offline.contains("Html.escapeHtml"));
    // The edge cases Android never reports to Rust: a main-frame request to another origin is
    // refused with an empty 403, the launcher leaves the history, a dead renderer re-creates the
    // activity.
    for stated in [
        "request.isForMainFrame && refuses(",
        "\"text/html\", \"utf-8\", 403, \"Forbidden\",",
        "view.clearHistory()",
        "override fun onRenderProcessGone",
        "activity.recreate()",
    ] {
        assert!(offline.contains(stated), "OfflineClient lacks {stated}");
    }
    assert!(
        offline.contains("\"onRenderProcessGone\""),
        "it must be in the forwarded set"
    );
    // The pure helpers have JVM tests (Gradle: testUniversalDebugUnitTest).
    for name in ["DownloadNamesTest.kt", "OriginsTest.kt", "AppTokenTest.kt"] {
        assert!(root()
            .join("gen/android/app/src/test/java/online/dynodoc/client")
            .join(name)
            .exists());
    }
}

#[test]
fn the_android_link_script_is_the_only_page_script_android_gets() {
    let window = read("src/window.rs");
    let production = &window[..window.find("#[cfg(test)]\nmod tests").unwrap()];
    assert!(production.contains("include_str!(\"blank_targets.js\")"));
    assert!(production.contains("include_str!(\"link_targets.js\")"));
    assert!(root().join("../scripts/blank-targets.test.mjs").exists());
    assert!(root().join("../scripts/android-probe.mjs").exists());
}

#[test]
fn the_script_run_in_the_finish_page_is_one_template_with_its_own_node_test() {
    // The only script the shell ever runs in a page after it has loaded (every platform): the
    // template in finish_hash.js, filled in by signin::finish_script, run by one `window.eval`.
    let signin = read("src/signin.rs");
    assert!(signin.contains("include_str!(\"finish_hash.js\")"));
    assert!(root().join("src/finish_hash.js").exists());
    assert!(root().join("../scripts/finish-hash.test.mjs").exists());
    let window = read("src/window.rs");
    let production = &window[..window.find("#[cfg(test)]\nmod tests").unwrap()];
    assert_eq!(production.matches("window.eval(").count(), 1);
    for file in [
        "commands.rs",
        "nav.rs",
        "deeplink.rs",
        "menu.rs",
        "handoff.rs",
        "server.rs",
    ] {
        let source = read(&format!("src/{file}"));
        let production = &source[..source.find("#[cfg(test)]").unwrap_or(source.len())];
        assert!(
            !production.contains(".eval(") && !production.contains("evaluate_script"),
            "{file} runs a script in a page"
        );
    }
}
