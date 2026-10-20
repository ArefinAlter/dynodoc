# Dynodoc desktop client (Tauri 2)

A native window for Dynodoc on Windows, macOS and Linux, built with Tauri 2, and the same
project prepared for Android and iPhone. It is a **shell**: it does not bundle the web app. It
opens the Dynodoc server you choose (the hosted workspace at `https://app.dynodoc.online`, or
your own installation) in the system web view, and adds a few things a browser tab cannot: a
signing key kept by the operating system, `dynodoc://` links, and a bundled `engine-cli` for
local tools.

Design and decisions: [decision 016](../../docs/decisions/016-devices-processors-desktop-and-mobile-apps.md).
Status per platform, owner prerequisites and store risk:
[DESKTOP-AND-MOBILE-APPS.md](../../docs/DESKTOP-AND-MOBILE-APPS.md).

**Status (see the end of this file for exactly what was run):** source only. Nothing is signed,
notarised, in a store, or tried on a Mac, an iPhone or a physical device.

## What each file does

| Path | Purpose |
|---|---|
| `src-tauri/Cargo.toml` | Crate `dynodoc-desktop`, **its own Cargo workspace and lockfile** (the engine workspace excludes `apps/`). The version here is the only version of the app. |
| `src-tauri/src/main.rs` | Thin desktop entry point. |
| `src-tauri/src/lib.rs` | `run()` (also the mobile entry point): plugins, commands, window, menu. |
| `src-tauri/src/server.rs` | Validates and normalises the server address and the pasted sign-in link; origin comparison. Pure functions. |
| `src-tauri/src/nav.rs` | The navigation policy: what may load in the window, what goes to the system browser, what is dropped. Pure functions. |
| `src-tauri/src/deeplink.rs` | Parser for `dynodoc://` links (three accepted forms). Pure functions. |
| `src-tauri/src/sidecar.rs` | Runs the bundled `engine-cli` with a fixed argument list, a time limit and an output limit. |
| `src-tauri/src/commands.rs` | The seven commands the launcher page may call. |
| `src-tauri/src/config.rs` | Saves the chosen server in the app's config folder (atomic, re-validated on read). |
| `src-tauri/src/window.rs` | Creates the window and hooks navigation, new-window and deep-link events to the policy. In debug builds it prints each decision to standard error. |
| `src-tauri/src/link_targets.js` | A 20-line script the shell injects into every page of the window: WebView2 raises no event for a click on `<a target="_blank">`, so it turns that click into `window.open`, which the new-window rule then handles. It has no access to the app. |
| `src-tauri/src/menu.rs` | The native menu (desktop only). |
| `src-tauri/src/policy_tests.rs` | Tests of the configuration files: capabilities, CSP, dependencies, launcher page, workflow triggers. |
| `src-tauri/tauri.conf.json` | Product name, identifier, CSP, bundle settings, deep-link scheme. |
| `src-tauri/tauri.{windows,linux,macos}.conf.json` | Add the `engine-cli` sidecar on desktop only (a phone build must not look for it). |
| `src-tauri/capabilities/` | Who may call what. `launcher.json` (desktop) and `launcher-mobile.json` (phones). No `remote` section anywhere. |
| `src-tauri/icons/` | Generated from the Dynodoc mark with `tauri icon` (desktop, Android and iOS sets). |
| `ui/` | The launcher page: plain HTML, CSS and JavaScript shipped inside the app. No framework, no remote scripts. |
| `scripts/prepare-sidecar.mjs` | Builds `engine-cli` from the repository root and copies it to `src-tauri/binaries/engine-cli-<triple>[.exe]` (git-ignored). |
| `scripts/smoke.ps1` | Windows smoke test of a debug build (starts, config folder, saved server, deep link, clean exit). |
| `scripts/security-probe.mjs` | Windows black-box probe of a debug build: drives WebView2 through its debugging port and tries, from inside the web view, what a hostile server page would try (see "Security model"). Not part of CI. |
| `package.json` | Holds `@tauri-apps/cli` only. Nothing in it ships. |
| `../../.github/workflows/desktop.yml` | Manual or tag-only builds: desktop bundles, Android debug APK, iOS simulator. |

## Identity (owner to confirm)

| | |
|---|---|
| Product name | Dynodoc |
| Bundle identifier | `online.dynodoc.client` |
| Deep-link scheme | `dynodoc://` |

The identifier is the reverse of the company domain `dynodoc.online` plus `client`. One identifier
serves macOS, iOS, Windows, Linux and Android (`.app` is avoided because it clashes with the
macOS bundle extension). **The owner must confirm it before any store use**: the Apple bundle ID
and the Google Play application ID cannot be changed after the first upload, and the identifier
also names the per-user config folder, so changing it later orphans saved settings.

## Build and run

Prerequisites, any desktop: Rust (the version pinned by the repository's `rust-toolchain.toml` is
used automatically), Node 20 or newer with npm.

- **Windows:** Visual Studio Build Tools (C++ workload) and WebView2 (included in Windows 11).
- **macOS:** Xcode command line tools.
- **Linux (Debian/Ubuntu):** `sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev libayatana-appindicator3-dev patchelf file build-essential`.

```sh
cd apps/desktop
npm ci --ignore-scripts              # installs the Tauri command line (no install script is needed)
node scripts/prepare-sidecar.mjs     # builds engine-cli (release) and copies it into src-tauri/binaries
npm run tauri -- dev                 # run a debug build
npm run tauri -- build               # installers/bundles in src-tauri/target/release/bundle
npm run tauri -- build --debug --no-bundle   # just the debug executable
```

`prepare-sidecar.mjs --debug` is faster for development; `--target <triple>` builds for another
target; `--skip-build` only copies an `engine-cli` you built yourself. The build refuses to start
without the sidecar file (Tauri checks `externalBin`), so run it first, also before `cargo test`
and `cargo clippy` in `src-tauri`.

Rust checks (inside `apps/desktop/src-tauri`):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

On this repository's machines set `CARGO_TARGET_DIR`, `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_INCREMENTAL=0` as for the engine. The engine's own `cargo fmt/clippy/test --workspace` at
the repository root does not see this project.

Smoke test (Windows, debug build; it uses a throw-away home folder and a local listener, never
your real profile): `powershell -ExecutionPolicy Bypass -File scripts\smoke.ps1`. Security probe
(Windows, debug build, same throw-away home; it opens a few tabs in your system browser on
purpose, because that is what a blocked in-window link must do): `node scripts\security-probe.mjs`.
Both honour `DYNODOC_DESKTOP_HOME`, which only debug builds read.

**Why `@tauri-apps/cli` from npm rather than `cargo install tauri-cli`:** it is pinned by a
lockfile, installs in seconds with no compile (the native binary comes as an optional platform
package, so `--ignore-scripts` is enough), and the CI workflow reuses the same lockfile. The cost
is a `package.json` in this folder; it holds that one dev dependency and nothing else.

## Using it

On first run the window shows the **launcher**. Choose "Use Dynodoc cloud", or type the address of
your own server (see "Server address rules" below). The choice is saved in the app's config
folder and the window opens it. Next time the window opens the server directly. **File > Change
server** returns to the launcher.

**Signing in.** Dynodoc signs in with an emailed link. A link clicked in your mail program opens
in your **web browser**, which does not share a session with this window. So: ask for the email in
this window, copy the link from the email, then choose **File > Sign in with an emailed link** and
paste it (the launcher checks it belongs to your server's sign-in page and carries one token and
nothing else, then opens it here). "Continue with Google" does not work inside this window yet:
Google's page opens in the system browser, and a proper hand-over from the browser to the app
needs server work that is not built (round 2 of decision 016).

**This computer's key.** The launcher can create a device signing key with
`engine-cli device keygen --out <app local data>/device-key.json --encrypt --keystore` and show its
public key. The private key is encrypted with a random secret held by the operating system's key
store: Windows data protection (DPAPI), the macOS login keychain through `security`, or the Linux
Secret Service through `secret-tool` (as implemented in `engine-cli`, see
[KEY-FILE-FORMAT.md](../../docs/KEY-FILE-FORMAT.md)). The system may show its own prompt; the
shell waits up to 90 seconds for key creation and 20 seconds for the quick operations, then stops
the tool. The key file is under the local (not roamed) data folder, for example
`%LOCALAPPDATA%\online.dynodoc.client\device-key.json`. Copy the public key into the web app under
**Profile > Signing keys**; **enrolling happens there**, not in this window. The shell never reads or
writes a key file itself: all key code stays in `engine-cli`. The macOS and Linux key-store paths
have not been run on those systems (stated in KEY-FILE-FORMAT.md too).

**Menu.** File (Change server, Sign in with an emailed link, This computer's key, Open server in
browser, Quit), Edit, View (Reload), Help (Dynodoc support, About). No tray icon, no auto-update,
no analytics. The window remembers its size, position and maximised state, follows the system
light or dark theme, and is high-DPI aware.

**Links.** `dynodoc://open/workspace`, `dynodoc://open/projects/<uuid>` and
`dynodoc://open/documents/<uuid>` open that page on the configured server. The Windows installers
register the scheme (visible in the generated NSIS script and WiX source; the installers were built
and never installed). macOS and Linux package registration is configured the same way and
unverified; a bare AppImage or a debug build does not register it.

## Security model, in plain words

The window talks to **one** server, the one you chose. Everything about that is enforced in Rust
and tested.

1. **A server's pages get no native access.** The only code allowed to call the app's commands is
   the launcher page bundled inside the app. Tauri's permission files (`capabilities/`) name the
   main window's *local* content only and have no `remote` section, so a page loaded from a server,
   even your own, cannot call anything. Each command also checks that the page asking is the
   launcher.
2. **The window can only go to your server.** Any other web address, from a link, a redirect or
   `window.open`, opens in your system browser and never in the window. Addresses with a user name
   or password, and anything that is not a plain web or mail link (`javascript:`, `data:`, `file:`,
   `blob:`, other apps' schemes), are dropped.
3. **The server address is checked.** `https` only (and `http` for `localhost`, `127.0.0.1` and
   `[::1]`, for testing), no user name, password, path, query or fragment, no backslashes, percent
   escapes, spaces or trailing dot, bounded length. International names are shown in their ASCII
   (punycode) form so a look-alike is visible.
4. **Links into the app are checked.** `dynodoc://` links come from anywhere, so only three exact
   forms are accepted, identifiers must be UUIDs, nothing can carry a token, a query, a port or a
   server name, and the page always opens on **your configured server**, never one named in the
   link.
5. **The local tool runs with a fixed command line.** The launcher cannot pass an argument. The app
   builds one of three argument lists itself, refuses any other, closes standard input, switches
   off logging, limits time and output, and checks the answer's shape.
6. **Everything else is denied.** Camera, microphone, location, notifications, clipboard reading
   and every other web view permission are refused for every page. No inspector in release builds.
   No `eval` or remote script in the launcher, whose Content-Security-Policy is
   `default-src 'none'` plus its own scripts and styles. One copy of the app runs at a time.

Not covered: a page you chose to open can still do what any website can do inside its own window
(it is your server's responsibility; its own Content-Security-Policy is unchanged), and malware on
your computer is outside what any of this can stop. One behaviour of WebView2 found by the probe:
when a page navigates the window to another address, WebView2 sends the request for it (with the
page's origin as referrer) before the shell cancels the navigation, so a refused navigation can still
cause one request that is never shown. A page can already send requests to other addresses, so this
adds no new ability; it means "refused" is about what loads and what gets opened, not about the
network. Other web views (WKWebView, WebKitGTK, Android) were not tested.

### How this was tested

- **Pure functions, unit tests** (`cargo test`): the address validator, the sign-in link rule, the
  navigation and new-window decisions, the deep-link parser and the sidecar argument allowlist, with
  hostile inputs (`https://app.dynodoc.online@evil.example`, punycode look-alikes, trailing dots,
  ports, IPv6, `javascript:`/`data:`/`file:`/`blob:`, backslashes, percent-encoding, very long
  input). The sidecar runner is tested against a child process for the time limit, the output limit
  and closed standard input, and against the real `engine-cli` (version everywhere; real key
  creation, read-back and refusal to overwrite on Windows).
- **Configuration tests** (`policy_tests.rs`): no `remote` capability anywhere, the exact permission
  lists, phones get no tool commands, the CSP has no unsafe source, no dangerous Tauri switches,
  banned plugins and features absent, the launcher page has no inline or remote code, the workflow
  never triggers on a push to a branch.
- **Black-box probe** (`scripts/security-probe.mjs`, Windows debug build): from inside the web view a
  server page's call to every command and plugin command is refused; a real mouse click on
  `target=_blank` and plain links to other origins opens the system browser and the window stays;
  `javascript:`, `file:`, `data:`, `blob:` and credential URLs do not load; location, camera,
  microphone and notifications are denied; the launcher's CSP blocks `eval` and injected scripts; the
  launcher itself cannot call opener, window or deep-link plugin commands; the real device-key
  operations work through the real sidecar.

### Server address rules

Examples that work: `https://app.dynodoc.online`, `dynodoc.example.org` (https is assumed),
`https://dynodoc.example.org:8443`, `http://localhost:5173`. Examples refused: `http://example.org`,
`https://example.org/workspace`, `https://user@example.org`, `https://example.org.`,
`javascript:alert(1)`.

## Allowlists in one place

- **Capabilities, desktop (`launcher.json`):** window `main`, local content only: `allow-get-state`,
  `allow-set-server`, `allow-open-sign-in-link`, `allow-tools-version`, `allow-device-key-status`,
  `allow-device-key-create`, `allow-device-public-key`. Phones (`launcher-mobile.json`): the first
  three only. No core, plugin, file, shell, http or opener permission is granted to anything.
- **Sidecar argument lists:** `--version`; `device keygen --out <key> --encrypt --keystore`;
  `device public-key <key>`, where `<key>` is always `<local data dir>/device-key.json`.
- **Deep links:** `dynodoc://open/workspace`, `dynodoc://open/projects/<uuid>`,
  `dynodoc://open/documents/<uuid>`.

## Dependencies, and why each

`tauri` (the framework, no optional features: no `devtools`, no tray, no asset protocol);
`tauri-plugin-opener` (open a link in the system browser; used from Rust only, no page can call
it); `tauri-plugin-deep-link` (the `dynodoc://` scheme on every platform);
`tauri-plugin-single-instance` with its `deep-link` feature (one running copy, links forwarded to
it; desktop only); `tauri-plugin-window-state` (remember size and position, desktop only; it is
left out of debug runs that use a test home so they never write to a real profile); `url`
(parsing, the same crate Tauri uses); `serde`, `serde_json` (the config file and command
answers). **Not used on purpose:** `tauri-plugin-shell` (the sidecar is started with
`std::process::Command`, so there are no generic "run a program" commands in the app, and we control
the time and output limits), and no file, http, dialog, updater, notification, clipboard or process
plugin. A test fails if one appears in `Cargo.toml`.

## Mobile

The project is prepared, not built. `lib.rs` carries `#[cfg_attr(mobile, tauri::mobile_entry_point)]`,
the library is `staticlib + cdylib + rlib`, `tauri.conf.json` sets Android `minSdkVersion` 24 and
iOS 15.0 (macOS 12.0), there is a separate phone capability file without the local tools, the
sidecar is declared only for desktop, and icon sets for Android and iOS are generated. To create
the native projects (they are generated, git-ignored, and CI does this on every run):

```sh
npm run tauri -- android init     # needs the Android SDK, NDK and JDK 17: ANDROID_HOME, NDK_HOME, JAVA_HOME
npm run tauri -- android build --debug --apk --target aarch64
npm run tauri -- ios init         # macOS with Xcode and Homebrew xcodegen
npm run tauri -- ios build --debug --target aarch64-sim
```

On phones there is no sidecar and no menu, `window.open` interception is not supported by the web
view hooks, and deep links by custom scheme are not configured (Android and iOS app links need
domain files on the web host that do not exist yet). Treat the mobile commands above as
unverified until the CI jobs have run.

## What is not done

- No signing, notarisation, store submission, or physical-device test. Windows will warn about an
  unsigned installer (SmartScreen); macOS will refuse an unsigned, un-notarised app on first open
  unless the user overrides it.
- **No auto-update**: it needs update signing keys the owner has not created. There is no updater
  plugin in the project.
- No tray icon, no analytics, no crash reporting, no push notifications.
- Proper sign-in hand-over from the system browser (Google, or a link clicked in a mail program)
  needs a one-time-code exchange on the server.
- Downloads, printing and file-open behaviour inside the web view are unverified on every platform.
- Linux AppImage does not register the `dynodoc://` scheme by itself.
- macOS, iPhone and Android builds have not been run by hand; see the status document.
