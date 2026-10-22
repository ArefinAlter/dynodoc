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

**Status (see the end of this file for exactly what was run):** the desktop shell is tried on
Windows (and compiled on Linux); the Android app was **built and run on an Android 16 emulator on
22 October 2026** (see "Mobile" below). Nothing is signed, notarised, in a store, or tried on a Mac,
an iPhone or a physical phone.

## What each file does

| Path | Purpose |
|---|---|
| `src-tauri/Cargo.toml` | Crate `dynodoc-desktop`, **its own Cargo workspace and lockfile** (the engine workspace excludes `apps/`). The version here is the only version of the app. |
| `src-tauri/src/main.rs` | Thin desktop entry point. |
| `src-tauri/src/lib.rs` | `run()` (also the mobile entry point): plugins, commands, window, menu. |
| `src-tauri/src/server.rs` | Validates and normalises the server address and the pasted sign-in link; origin comparison. Pure functions. |
| `src-tauri/src/nav.rs` | The navigation policy: what may load in the window (the launcher only when the shell asked for it), what goes to the system browser, what is dropped; the window title; log redaction. Pure functions. |
| `src-tauri/src/handoff.rs` | Handing a link to the system: public addresses only for a page (1 per 3 s, 5 per minute), mail and local addresses only for a click (a secret marker from the injected script; its own budget, 1 per second and 10 per minute) or a menu item. The secret comes from the operating system's random source. Pure functions. |
| `src-tauri/src/deeplink.rs` | Parser for `dynodoc://` links (three accepted forms). Pure functions. |
| `src-tauri/src/sidecar.rs` | Runs the bundled `engine-cli` with a fixed argument list, a time limit and an output limit; recognises a cut-off key file and says how to start again. |
| `src-tauri/src/commands.rs` | The seven commands the launcher page may call. |
| `src-tauri/src/config.rs` | Saves the chosen server in the app's config folder (atomic, re-validated on read) with a SHA-256 seal in the local data folder; a config that no longer matches its seal is shown, not opened. |
| `src-tauri/src/window.rs` | Creates the window and hooks navigation, new-window and deep-link events to the policy. In debug builds it prints each decision to standard error. |
| `src-tauri/src/blank_targets.js` | **Android only.** The one script Android injects (no secret, no marker): a tap on `<a target="_blank">` is dropped by Android's web view without any callback, so the script turns the target into `_self` for that click and puts it back; the link then goes through the navigation rule like any other. `scripts/blank-targets.test.mjs` runs it against fake nodes. |
| `src-tauri/src/link_targets.js` | **Desktop only** (not injected on Android: its marker would arrive as a navigation and be dropped). A short script the shell injects into every page of the window, before the page's own scripts. It has no access to the app; it only calls `window.open`. WebView2 raises no event for a click on `<a target="_blank">`, so it turns that click into `window.open`; and a trusted click (`event.isTrusted`) on a link that leaves the page or on a mail link is passed on with a secret that tells the shell a click happened. It keeps its own copy of every built-in it uses (so a page that replaces `Function.prototype.call`, `closest()` or `window.open` sees nothing), and `scripts/link-targets.test.mjs` runs it against such a page. |
| `src-tauri/src/menu.rs` | The native menu (desktop only). |
| `src-tauri/src/policy_tests.rs` | Tests of the configuration files: capabilities, CSP, dependencies, launcher page, workflow triggers. |
| `src-tauri/tauri.conf.json` | Product name, identifier, CSP, bundle settings, deep-link scheme. |
| `src-tauri/tauri.{windows,linux,macos}.conf.json` | Add the `engine-cli` sidecar on desktop only (a phone build must not look for it). |
| `src-tauri/capabilities/` | Who may call what. `launcher.json` (desktop) and `launcher-mobile.json` (phones). No `remote` section anywhere. |
| `src-tauri/icons/` | Generated from the Dynodoc mark with `tauri icon` (desktop, Android and iOS sets). |
| `ui/` | The launcher page: plain HTML, CSS and JavaScript shipped inside the app. No framework, no remote scripts. |
| `scripts/prepare-sidecar.mjs` | Builds `engine-cli` from the repository root and copies it to `src-tauri/binaries/engine-cli-<triple>[.exe]` (git-ignored). |
| `scripts/smoke.ps1` | Windows smoke test of a debug build (starts, config folder, saved server and window title, deep link, an edited config, key creation in folders with accented and Bengali names, clean exit). |
| `scripts/security-probe.mjs` | Windows black-box probe of a debug build: drives WebView2 through its debugging port and tries, from inside the web view, what a hostile server page would try (see "Security model"). Not part of CI. |
| `src-tauri/tauri.android.conf.json` | Repeats the Cargo.toml version for Android (the Tauri command line gives an Android build no version otherwise: `1.0`, code 1, forever); a test keeps them equal. |
| `src-tauri/gen/android/` | The Android project Tauri 2.12 generates, **committed** (source only; its own `.gitignore` files keep build output, `local.properties`, `keystore.properties` and the files the build writes with machine paths in them out). Changed from the template: `allowBackup="false"` and `dataExtractionRules` (everything excluded), cleartext off in every build type, `windowSoftInputMode`, the Gradle download pinned by checksum, `file_paths.xml` narrowed to the one photo folder, window and bar colours, optional release signing from `keystore.properties`, and three Kotlin files: `MainActivity.kt` (insets, Back, downloads, web view settings), `OfflineClient.kt` (a plain "Try again" page instead of Android's stock error page) and the template's own. No JavaScript bridge is added (a test greps for it). |
| `scripts/harden-android.mjs` (+ `.test.mjs`) | Checks (`--check`, used in CI) or restores (no flag) the hardened state of the Android project: `allowBackup="false"`, extraction rules that exclude every domain, cleartext off, no network security config. Fails if anything unsafe is left. |
| `scripts/check-apk-manifest.mjs` (+ `.test.mjs`, `fixtures/`) | After the Android build: parses the MERGED manifest of the built APK (`aapt2 dump xmltree`, or `apkanalyzer`) into a tree and fails unless backup and cleartext are off, only INTERNET (and AndroidX's own signature permission) is requested, only `MainActivity` and AndroidX's DUMP-guarded profile receiver are exported, the link filters are only `dynodoc://open/...` without `autoVerify`, SDK levels match the config, and the signing key is the debug key (debug) or not (release). Anything it cannot parse is a failure. The fixtures are real tool output. |
| `scripts/android-build.ps1` | Windows: builds the APK with or without the right to create symbolic links (see "Mobile"). |
| `scripts/android-probe.mjs` | Black-box probe of the debug build on an emulator or a USB device: drives the real Android WebView over adb and its debugging port, real taps (see "Mobile"). |
| `scripts/link-targets.test.mjs` | Runs the injected click script in a Node realm against a hostile page (patched `call`/`apply`/`bind`/`closest`/getters/`window.open`, fake link objects) and fails if the secret reaches the page or a fake link starts a hand-off. Two control tests run the previous script and must see it leak. |
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

*Whose account?* A pasted link signs this window in as **whichever account the link was issued
for**, so paste only a link from an email you asked for in this window: someone else's link would
sign you in as them (login CSRF). The web app's own `/login` page still shows "Continue to
workspace" and needs a click before it uses the token, so nothing is signed in silently. Showing the
account's email address on that page before the click would close the gap; that is server work, not
built, and is on the owner's question list.

**This computer's key.** The launcher can create a device signing key with
`engine-cli device keygen --out <app local data>/device-key.json --encrypt --keystore` and show its
public key. The private key is encrypted with a random secret held by the operating system's key
store: Windows data protection (DPAPI), the macOS login keychain through `security`, or the Linux
Secret Service through `secret-tool` (as implemented in `engine-cli`, see
[KEY-FILE-FORMAT.md](../../docs/KEY-FILE-FORMAT.md)). The system may show its own prompt; the
shell waits up to 90 seconds for key creation **and for reading the public key back** (both can
wait for a key-store prompt) and 20 seconds for `--version`, then stops the tool. The launcher only
says that a key exists; it reads the public half (which decrypts the key through the key store)
when you press **Show the public key**, never when the page loads. If a key file is damaged (for
example creation was interrupted) or key creation fails or is killed, the launcher says "delete this
file in <path> and create the key again" and names the file: nothing is deleted or overwritten for
you, and creation still refuses to replace any file. The key file is under the local (not roamed) data folder, for example
`%LOCALAPPDATA%\online.dynodoc.client\device-key.json`. Copy the public key into the web app under
**Profile > Signing keys**; **enrolling happens there**, not in this window. The shell never reads or
writes a key file itself: all key code stays in `engine-cli`. The macOS and Linux key-store paths
have not been run on those systems (stated in KEY-FILE-FORMAT.md too).

**Menu.** File (Change server, Sign in with an emailed link, This computer's key, Open server in
browser, Quit), Edit, View (Reload), Help (Dynodoc support, About). No tray icon, no auto-update,
no analytics. The window remembers its size, position and maximised state, follows the system
light or dark theme, and is high-DPI aware.

**Window title.** It names the server the page is from (`Dynodoc - app.dynodoc.online`, in its
ASCII form), so the origin is always visible; the launcher is plain `Dynodoc`.

**A changed saved address.** The saved address is sealed (its SHA-256 is kept in the local data
folder). If the file was edited by another program or user, or the seal is gone, the window opens
the launcher with "Please check the saved server address" and does not load it until you press
**Use this address**. The seal detects edits by anything that does not also rewrite the seal;
malware running as you can rewrite both.

**Links.** `dynodoc://open/workspace`, `dynodoc://open/projects/<uuid>` and
`dynodoc://open/documents/<uuid>` open that page on the configured server. **A link from another
program brings this window forward and navigates it, even if you were working in it.** That limit
is accepted and documented: the protection for unsaved work is the web app's own leave-page
warning. Checked in WebView2 (probe phase A): when the shell navigates, the page's `beforeunload`
handler runs and WebView2 shows its "Leave site?" dialog; answering Stay keeps the page. A page that
does not ask is left at once. A second launch that arrives while the first is still starting up
can reach the deep-link plugin before the shell is listening; the plugin keeps the **latest** such link
and the shell reads it at the end of start-up (`get_current`), so an earlier link in that window is
dropped (there is no queue of our own). The Windows installers
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
   launcher. `withGlobalTauri` is off, so there is no `window.__TAURI__` in any page; the launcher
   uses `window.__TAURI_INTERNALS__.invoke`, the function `@tauri-apps/api` wraps (it needs no
   bundler). Tauri still puts that bridge object in every page, **including a server's**, and the
   permission files refuse every call from it (probe: 18 calls from a server page, all refused).
   The launcher itself loads only when the shell asked for it (a one-shot grant), or while no
   server is configured (then nothing but the launcher can be on screen, and Android re-creates the
   window on a font size change): a server page that navigates the window to the launcher's address
   is refused (otherwise it could bring up the key operations).
2. **The window can only go to your server, and a page cannot flood or aim your browser.** Any
   other web address opens in your system browser and never in the window, but WebView2 does not
   say whether a person clicked, so: a *page's* request (script, redirect, `window.open`) may hand
   over only a **public** `http(s)` address, at most **once per 3 seconds and five per minute**
   (the rest are dropped silently), and never a mail link, `localhost`, a loopback, private,
   link-local or otherwise reserved address (IPv4, IPv6, IPv4-mapped, `.local`/`.lan`/... names,
   single-word names). A *click* on a link (a trusted `click` event, which a script cannot fake), or
   a menu item, may open those too. A click is recognised by a marker address that carries a secret.
   **Where the secret lives, exactly:** it is made from the operating system's random source when
   the app starts, written into the injected script, and kept in one variable inside that script's
   closure. The script runs before any page script and keeps its own copy of every built-in it uses
   (called through "uncurried" functions, brand-checked getters, no `closest()`, `URL`, regular
   expressions or array methods), so a page that later replaces `Function.prototype.call`,
   `closest()`, `window.open`, `encodeURIComponent` or the link getters sees neither the secret nor
   the marker (`scripts/link-targets.test.mjs`, 12 tests, and the probe's hostile page). **The
   secret is per run (per start of the app), not per page load:** the script text is registered
   once with the window and runs again, unchanged, in every page; rotating it per load would mean
   removing and re-registering the script on every navigation, which is not done. The protection is
   that no page can read it, not that it changes. A page that forges the marker without the secret
   gets nothing.
   **What a hostile page can still do:** if a page rewrites the link under the pointer, or puts an
   invisible overlay link where a person is about to click, the person's real click is a real click,
   and the page chooses the target (a local address or a mail link included). That is what any
   website can do in a browser, and the shell bounds it: marker-sourced opens have their own budget
   of **one per second and ten per minute**, separate from the page's budget (so neither can use the
   other's up), and menu items have none. Addresses with a user name or password and anything that
   is not a web or mail link (`javascript:`, `data:`, `file:`, `blob:`, `about:`, other apps'
   schemes) are dropped.
   **"Public" is only about spelling.** A page's request is checked by the address as written. A
   best-effort list refuses the well-known wildcard-DNS services (`nip.io`, `sslip.io`, `xip.io`,
   `localtest.me`, `lvh.me`, `vcap.me`, `lacolhost.com`, `localhost.direct`, `local.gd`,
   `traefik.me`, `127-0-0-1.org.uk`) and any name that spells an IPv4 address (`10.0.0.5.example.com`,
   `10-0-0-5.example.com`). It cannot be complete: any domain can point a name at a private address,
   and a public name can be re-pointed after the check (DNS rebinding); neither can be seen from here.
   What it limits is a page opening your browser at such a name, one per 3 seconds. Because the
   page's budget is its own, a page can use all of it but cannot take a person's click budget.
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
   off logging, limits time and output, and checks the answer's shape. The public key is read only
   on request, never when the launcher loads.
6. **Everything else is denied.** Camera, microphone, location, notifications, clipboard reading
   and every other web view permission are refused for every page. No inspector in release builds.
   No `eval` or remote script in the launcher, whose Content-Security-Policy is
   `default-src 'none'` plus its own scripts and styles. One copy of the app runs at a time.
   The debug log (debug builds only) never prints a query string or the click secret.

**What "the window can only go to your server" means on Android** (corrected on review): the Rust rule
decides every GET link navigation the web view reports; Android does not report a form POST or a
history step, so there a small layer in Kotlin answers a main-frame request to another origin with a
403 page and keeps the launcher out of the history (see "Mobile"). Frames are not covered by either.

Not covered: a page you chose to open can still do what any website can do inside its own window
(it is your server's responsibility; its own Content-Security-Policy is unchanged), and malware on
your computer is outside what any of this can stop. One behaviour of WebView2 found by the probe:
when a page navigates the window to another address, WebView2 sends the request for it (with the
page's origin as referrer) before the shell cancels the navigation, so a refused navigation can still
cause one request that is never shown. A page can already send requests to other addresses, so this
adds no new ability; it means "refused" is about what loads and what gets opened, not about the
network. Other web views (WKWebView, WebKitGTK) were not tested. Android's System WebView was tested
on an emulator on 22 October 2026 (see "Mobile").

**Frames on WebKit: unverified on WebKit; observed on Android.** WebView2 reports only the window's own
navigations to the shell. WebKitGTK and WKWebView also report navigations of `<iframe>`s, and the
library the shell uses (wry) says nothing about which is which on those platforms. Nothing was
observed there; this is an assumption written into the code (`nav.rs`, `frames_are_ambiguous`). On
Android (WebView 133, Android 16) the probe added three iframes (same site, another site,
`about:blank`) to a hosted page and the shell was told of none of them, so on that web view frames
never reach the navigation rule; the assumption stays on for Android because it is the safe one and
only one web view version was tried. Where frames cannot be told from the window, the addresses pages normally
use to fill a frame (`about:srcdoc`, `about:blank`, `data:`, and a `blob:` made by the configured
server's own page) are let through instead of breaking the page; everything else is decided as
usual, so a frame that goes to another site is still sent to the system browser or dropped. Such an
address is never the launcher and never uses up the launcher's one-time permission. What a page
gets by really navigating the *window* to one of them is its own content, with no native access.

### How this was tested

- **Pure functions, unit tests** (`cargo test`, 105 tests on Windows after the Android work of 22
  October 2026; the Windows-only key test is not built elsewhere, so 104 are expected there, not
  rerun on Linux since the 21 October change): the address
  validator, the sign-in link rule, the navigation and new-window decisions (including the
  launcher's own page only, and frame-like addresses), the hand-off rules (every
  loopback, private, link-local, mapped, NAT64 and 6to4 shape; wildcard-DNS names and names spelling
  an address; the two rate limiters against a flood, a trickle and a backwards clock, and that
  neither can spend the other's budget; marker forgeries; mail links only for a click or a menu item),
  the launcher grant, the config seal, the
  deep-link parser (one static table, a test that states the table again by hand, a test that
  `parse` contains no string literal, and an exact **whitelist** test over 370,000 generated links: the
  accepted set must equal the 20 listed strings), the sidecar argument allowlist, the cut-off key file
  detector and the second lock on every command, with hostile inputs (`https://app.dynodoc.online@evil.example`, punycode look-alikes, trailing dots,
  ports, IPv6, `javascript:`/`data:`/`file:`/`blob:`, backslashes, percent-encoding, very long
  input). The sidecar runner is tested against a child process for the time limit, the output limit
  and closed standard input, and against the real `engine-cli` (version everywhere; real key
  creation, read-back and refusal to overwrite on Windows).
- **Mutations** (each makes a test fail; checked by hand on 21 October 2026): `require_launcher`
  always Ok, or its call removed from a command; the permission handler returning Allow (in
  `window.rs` and in `lib.rs`); the scheme filter removed from `external_allowed` or `hand_off`;
  `.devtools(true)`; the rate limiter's gap set to 0; private ranges no longer refused; the marker
  secret always matching; the launcher always allowed; an unsealed config treated as confirmed; an
  extra deep-link shape (also one that no list of refused links mentions).
  **Round 2 (21 October 2026, each fails at least one test):** in the injected script, window.open
  called directly, `link.href` read directly, `.call` on a built-in, `closest()`, the `isTrusted`
  check removed (node harness, and the Rust policy test for all but the `href` one); a row
  `["open","settings"]` added to the deep-link table (3 tests), an inline `["open","settings"]`
  beside the table in `parse` (2 tests), a word of the table changed (3 tests); in `handoff.rs`/`nav.rs`
  a mail link allowed for a page, the click budget unlimited, clicks spending the page budget, the
  wildcard-DNS list not consulted, names spelling an address not refused, the secret always matching, a
  page allowed any host, the launcher grant opening any page of its origin, frame-like addresses
  never or always let through. The previous injected script, built into the debug app, fails 3
  probe checks against the hostile page (its real clicks were not recognised as clicks, so the
  link was not opened by the system browser and the click-budget checks failed).
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
  operations work through the real sidecar. It also checks the review fixes: a page that loops 40
  public links in 2.5 seconds gets one hand-off and the rest are dropped; loopback, LAN,
  link-local, mapped-IPv6 and mail addresses from a page open nothing and send the browser nothing;
  a click made by a script on a link to a local address opens nothing while a real click does; a
  page cannot bring up the launcher; **a hostile server page** that replaces `Function.prototype.call`,
  `apply`, `bind`, `Reflect.apply`, `closest()`, the link and event getters, `encodeURIComponent`,
  `URL` and `window.open` and records every argument never sees the click marker or the secret
  (nor does any property of `window` hold it), a real click on a harmless button still opens
  nothing although every getter claims "link to an address of the page's choosing", a real click on
  a real link still opens that link, six real clicks within milliseconds open one (the click
  budget), and a page-initiated hand-off a moment earlier does not use up the click budget; an edited config opens the launcher without a request to the
  saved address, and a real click on "Use this address" opens and re-seals it; the launcher does not
  read the key when it loads and a real click on "Show the public key" does; a cut-off key file gets
  recovery text and is left untouched; and `--unicode-home` runs the same in a folder named with an
  accented letter and Bengali letters.

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
- **Handed to the system (page-initiated):** public-looking `http(s)` addresses without credentials
  (not the listed wildcard-DNS services, not names spelling an address); at most one per 3 s and five
  per minute. **Only for a click or a menu item:** `mailto:` and loopback, private, link-local or
  otherwise local addresses; a click is limited to one per second and ten per minute, in a budget
  of its own; menu items are not limited.

## Dependencies, and why each

`tauri` (the framework, no optional features: no `devtools`, no tray, no asset protocol);
`tauri-plugin-opener` (open a link in the system browser; used from Rust only, no page can call
it); `tauri-plugin-deep-link` (the `dynodoc://` scheme on every platform);
`tauri-plugin-single-instance` with its `deep-link` feature (one running copy, links forwarded to
it; desktop only); `tauri-plugin-window-state` (remember size and position, desktop only; it is
left out of debug runs that use a test home so they never write to a real profile); `url`
(parsing, the same crate Tauri uses); `sha2` (the config seal; already in Tauri's own tree); `getrandom` 0.3 (the click secret from the
operating system's random source; already in Tauri's tree, the lockfile only gains an edge);
`serde`, `serde_json` (the config file and command answers). **Not used on purpose:** `tauri-plugin-shell` (the sidecar is started with
`std::process::Command`, so there are no generic "run a program" commands in the app, and we control
the time and output limits), and no file, http, dialog, updater, notification, clipboard or process
plugin. A test fails if one appears in `Cargo.toml`.

**Lockfile pin.** `Cargo.lock` pins `rust-ini` (a Linux-only dependency of `tauri-plugin-deep-link`)
to 0.21.1: 0.21.2 and 0.21.3 added fields to a public struct that the plugin (2.6.2) fills in by
hand, which fails to compile on Linux. Found by building in an Ubuntu 24.04 container. Do not run a
blanket `cargo update` without checking the Linux build; drop the pin when the plugin is updated.

## Mobile

**Android: built and run (22 October 2026).** The Android project is committed in
`src-tauri/gen/android` (Tauri 2.12.3, AGP 9.3.1, Gradle 9.6.1, Kotlin 2.2.10, compileSdk and
targetSdk 37, **minSdk 29**, see below). A debug APK (universal: arm64-v8a and x86_64, 44.6 MiB) was built on
Windows 11, its merged manifest audited, and installed on an x86_64 Android 16 emulator, where
`scripts/android-probe.mjs` drove it. The versions of the tool chain, the exact commands, what the
probe checks, the defects it found and the limits are in
[DESKTOP-AND-MOBILE-APPS.md](../../docs/DESKTOP-AND-MOBILE-APPS.md#android-what-was-run-on-22-october-2026).
**iPhone: still only prepared** (`ios init`/`ios build` need a Mac and were never run).

```sh
# in apps/desktop, after npm ci --ignore-scripts, with JAVA_HOME (JDK 17), ANDROID_HOME, NDK_HOME set
npm run tauri -- android build --debug --apk --target aarch64 x86_64 -- --locked
powershell -ExecutionPolicy Bypass -File scripts\android-build.ps1     # Windows without symbolic links
node scripts/check-apk-manifest.mjs src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk --build debug
node scripts/android-probe.mjs        # emulator or USB phone with the debug APK installed
npm run tauri -- ios init             # macOS with Xcode and Homebrew xcodegen (never run)
```

**Why `android-build.ps1`.** `tauri android build` links the compiled Rust library into the Gradle
project with a symbolic link; Windows allows that only to administrators, or to anyone once Developer
Mode is on. Without it the command line compiles the library and stops ("Creation symbolic link is not
allowed"). The script checks, and if links are not allowed it lets the command line compile, copies
the library into `app/src/main/jniLibs/<abi>/` and runs Gradle itself (skipping the `rustBuild*`
tasks, which would fail the same way). It builds the same APK. `-Release` builds a release APK
(signed only if `gen/android/keystore.properties` exists).

**The committed project differs from the template** (all checked by `harden-android.mjs --check`, by
`check-apk-manifest.mjs` on the built APK and by `policy_tests.rs`): `allowBackup="false"` (the
template has none, so Android's default, true, applied) and `dataExtractionRules` excluding every
domain (Android 12+ reads those instead); `usesCleartextTraffic` false in every build type (the
template enables it in debug); the Gradle distribution pinned by SHA-256 **and every dependency by
`gradle/verification-metadata.xml`** (523 components; refresh with `./gradlew
--write-verification-metadata sha256 assembleUniversalDebug assembleUniversalRelease
testUniversalDebugUnitTest -x rustBuild...Debug/Release` after a dependency or Tauri update; it was
written on Windows, so the Linux and macOS `aapt2` jars, the one platform-specific artifact, were added by
hand with the sha256 computed from Google's Maven and cross-checked against the sha1 it publishes:
**the first Linux build is the real test of the file**); `FileProvider` paths narrowed from "all shared
storage and cache" to the one folder it uses; the three deep-link intent filters
(`dynodoc://open/workspace`, `.../projects/`, `.../documents/`, host `open`, no `autoVerify`, written by
the deep-link plugin from `tauri.conf.json`); `windowSoftInputMode="adjustResize"`; `taskAffinity=""` on
the activity (a task of its own: Android's default affinity is the package name, which another app
can claim to slip its screen into this app's task, "StrandHogg"); **minSdk 29** (Android 10): Android 7
to 9 stay open to that task hijacking whatever the manifest says, and the download manager only
saves into `Download/` without a storage permission from Android 10 on (the other way out was
declaring `WRITE_EXTERNAL_STORAGE`; check current Android version shares before a store release, the app
has no users yet to lose); window colours for light and dark; and the Kotlin described next. The Material
library stays: Tauri's own Android library depends on it (1.7.0) and the theme is Material3, so
dropping our explicit 1.12.0 would only downgrade it. Gradle stays at 9.6.1 (not bumped: a bump needs the
new distribution's checksum re-verified and the metadata rewritten, for no gain here); the wrapper
jar's sha256 (`497c8c2a...a9c7`) is the one gradle.org publishes for 9.6.1, and the distribution's
`distributionSha256Sum` (`9c0f7fae...9e14`) is the published file's.

**What the Kotlin does** (`MainActivity.kt`, `OfflineClient.kt`; no JavaScript bridge, no
`addJavascriptInterface`, a test greps for it; wry's own `ipc` object is in every page, as on
desktop, and the probe sends 20 app and plugin commands from a hosted page: all refused):

- *Insets.* The page is kept out from under the status bar, the navigation bar, a camera cut-out
  and the keyboard (`enableEdgeToEdge` is mandatory for target SDK 35+ and the web view reports no
  safe area; without this the page drew under the bars and the keyboard covered the fields).
- *Back.* Tauri's activity leaves Back to the system, which closed the app from any page. Now Back
  goes back through the pages of the site shown; at the first page, or when the page before is
  another site (the launcher this app started on), the app goes to the background with its page
  kept, as a root screen does on Android 12+. (Calling the platform's `onBackPressed` from the
  callback did nothing on Android 16: found by the probe.)
- *Web view settings, stated rather than left to defaults:* no file or content access, no mixed
  content, no second window (`setSupportMultipleWindows(false)`), and no `window.open` without a tap.
- *Links out.* A tap on `target="_blank"` is dropped by Android's web view without any callback
  (measured). `blank_targets.js` makes it load in the one window, where `nav.rs` decides: the
  server's pages load, other public web addresses go to the system browser (one per 3 s, five per
  minute), mail links, local addresses and everything else are dropped. **On Android a page's own
  mail link does nothing**, because nothing says whether a person tapped it (WebView2 has the same
  problem; on desktop the injected secret script solves it, which cannot work here: its marker would
  arrive as a navigation). The desktop script is therefore not injected on Android at all.
- *Downloads.* A web view ignores them unless the app handles them, and a page's script can start
  them too (`a.click()`, an attachment, a hidden iframe), so only the narrowest case is handled:
  after a touch of the page in the last 5 seconds, at most one per 3 seconds, an https file of
  exactly the page's origin (scheme, host AND port, compared as origins, not as host strings), **with
  no cookies ever sent** (the download manager repeats a header on every redirect, to any host, so an
  open redirect on the server would hand over the session), under a cleaned name (control, invisible
  and bidirectional characters and path separators dropped, no leading dots, 100 characters, nothing
  installable or runnable: `.apk`, `.xapk`, `.dex`, `.jar`, `.sh`, ... refused whatever the type of
  the answer says, and an installable type refused whatever the name says). **Consequence: a download the site would only give to a
  signed-in person is not supported** (if the cookie jar holds cookies for the address, the person is
  told "Not supported in the app yet. Download this in your web browser."); so are `blob:` addresses
  (the web app builds CSV, key and project exports that way) and anything of another origin. A safe in-app
  fetch (redirects off, an Authorization the app controls) can come later. `DownloadNames` and
  `Origins` are pure Kotlin with JVM unit tests (`./gradlew testUniversalDebugUnitTest`).
- *Navigations Android never reports.* `shouldOverrideUrlLoading`, which feeds the Rust rule, is not
  called for a form POST or for a history step. So `OfflineClient` answers a main-frame http(s) request
  to an origin other than the site's (the origin of the first page of the site, once it started
  loading; the launcher's own address is exempt) with HTTP 403 and a short page with a link back,
  nothing leaves the device; and once the site has loaded, the launcher is dropped from the web view's
  history (`clearHistory`), so a page's `history.back()` cannot bring it up. Frames are not covered.
- *Dead renderer.* Without a handler Android kills the whole app with a renderer that died.
  `onRenderProcessGone` re-creates the activity (wry builds a fresh web view).
- *Re-creation.* The system re-creates the activity for a font size change, a dead renderer or after
  process death, and wry reloads the address the window was first created with, which is the launcher
  whenever the app was first run without a saved server. With a server saved that load is refused; on
  phones the shell then goes to the server's entry page (`nav::launcher_denied_goes_to`) instead of
  leaving a blank window. A re-created activity also replaces its launch intent, so the deep-link plugin
  does not replay the `dynodoc://` link that started it and pull the window back to that page.
- *Touches.* `filterTouchesWhenObscured`: a touch that arrives while another window covers the page is
  dropped (tapjacking).
- *Offline.* A failed load of the site shows a plain page ("Dynodoc cannot reach app.dynodoc.online",
  a **Try again** link to the failed address, no script) instead of Android's stock error page, which
  has no way to retry. `OfflineClient` wraps wry's client and refuses to install itself if a later wry
  overrides more than it forwards.

**What the web view gives a page that Tauri and wry add, stated so nobody is surprised.** wry registers
a JavaScript interface named `ipc` (`addJavascriptInterface`) on the web view: it is therefore present in
**every frame, including a cross-origin `<iframe>`**, not only the top page. Every call still has to
pass Tauri's permission files, which name the launcher's local content only (the probe's 20 calls from
a hosted page were all refused). The origin the permission check sees comes from wry's
`onPageStarted` (`RustWebViewClient.currentUrl`), that is, the address of the page that has STARTED
loading, which can run ahead of the page still shown during a navigation; and **`require_launcher(webview.url())`
in `commands.rs`, the check that the asking page is the bundled launcher, is the load-bearing second
lock**: it reads the web view's own current address, not wry's. Neither is something this app
changes. (A reviewer asked whether `dynodoc://open/projects/../workspace` is a bypass: it is not. The
plugin parses links with the `url` crate, which collapses dot segments before our parser sees the text, so
the link IS `dynodoc://open/workspace`; `../admin` collapses to a refused form.)

**Rust-side changes found by the first real run** (they apply to desktop too): the window opens the
server's `/workspace`, not its root (the hosted cloud's root redirects to the marketing site, which
the shell handed to the browser, leaving the window empty); the launcher may load again while no
server is configured (Android re-creates the activity and its web view, for example on a font size
change; the one-shot grant was spent and the window stayed blank).

**What does not work on Android yet** (honest list; details and options in the status document):
signing in inside the app (the emailed link opens in the browser; the "paste the link" form is in the
launcher, which a phone cannot reach once a server is saved: it needs Android App Links, owner and
server work); changing the server without clearing the app's data; Google sign-in; `blob:` downloads;
mail links from pages; camera, microphone, location and notifications (denied, and no permission is
declared); anything on a physical phone or on arm64 hardware (the arm64 library was compiled and
packaged, never run).

## What is not done

- No signing, notarisation, store submission, or physical-device test. Windows will warn about an
  unsigned installer (SmartScreen); macOS will refuse an unsigned, un-notarised app on first open
  unless the user overrides it.
- **No auto-update**: it needs update signing keys the owner has not created. There is no updater
  plugin in the project.
- No tray icon, no analytics, no crash reporting, no push notifications.
- Proper sign-in hand-over from the system browser (Google, or a link clicked in a mail program)
  needs a one-time-code exchange on the server.
- Downloads, printing and file-open behaviour inside the web view are unverified on desktop. On
  Android (emulator): a same-site https download works, `blob:` downloads are refused with a message,
  the system file chooser opens for an upload field, printing was not tried.
- Linux AppImage does not register the `dynodoc://` scheme by itself.
- **No GitHub Actions job has run.** `.github/workflows/desktop.yml` parses and GitHub lists its jobs,
  but this repository's Actions currently refuse to start any job ("recent account payments have
  failed or your spending limit needs to be increased"), so the workflow is untested. Also, a
  `workflow_dispatch` workflow cannot be started until its file is on the default branch.
- macOS and iPhone builds have not been run. Android: debug APK built and run on an emulator
  only (below); no physical phone, no Play signing.

## What was run on the author's machine (21 October 2026, Windows 11, x86-64)

After merging the main branch that holds the Windows non-ASCII key-file fix in `engine-cli`:

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`:
  clean; 97 tests pass. At the repository root `cargo fmt --all --check` and
  `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
- `scripts/prepare-sidecar.mjs --debug`: rebuilt `engine-cli` from the merged tree (27 MB, debug).
- **Non-ASCII folders (the fix in `crates/engine-cli/src/private_file.rs`).** With that rebuilt
  sidecar, `engine-cli device keygen --out <dir>\<name>\device-key.json --encrypt --keystore`
  followed by `device public-key` succeeded for folder names with an accented letter (`caf` + U+00E9),
  with Bengali letters (U+09AC U+09BE U+0982 U+09B2 U+09BE) and with both (`scripts/smoke.ps1`, phase
  5); and the app itself created, read back and refused to overwrite a key whose home folder had
  both kinds of letters (`node scripts/security-probe.mjs --unicode-home`; the probe now has 79
  checks). **Not
  verified:** that the same commands fail without the fix (the unfixed binary was not rebuilt to
  compare), and any non-ASCII path on macOS or Linux. (A side finding: Node 24 on Windows'
  `fs.rmSync` silently leaves a file in a folder with Bengali letters; the probe removes files
  with `unlinkSync`.)
- `cargo build --locked` (debug) and `npm run tauri -- build --debug --no-bundle --ci -- --locked`:
  build. Earlier the same day: unsigned `Dynodoc_0.1.0_x64-setup.exe` (7.9 MiB) and
  `Dynodoc_0.1.0_x64_en-US.msi` (12.5 MiB) were built (NSIS and WiX), never installed or run as
  installers, with the sidecar and the `dynodoc` protocol registration visible in the generated
  installer sources; and a release build ran and exited cleanly.
- `scripts/smoke.ps1` passes (window, config and data folders, saved server and its title, single-instance
  deep link, refused links, an edited config shown not opened, key creation in three non-ASCII
  folders, clean exit code 0).
- `scripts/security-probe.mjs` passes (79 checks), and passes with `--unicode-home`; see "How this
  was tested". **The probe is sensitive to a loaded machine:** with another build running and about
  0.4 GB of free memory, four runs in a row each failed one or two timing-dependent checks (the
  40-link flood took more than 3 s, so a second hand-off was correctly allowed; a leave-page "Stay"
  answer; one run lost the app process in phase B, as in the earlier hang). The first run and the two
  final runs, on an idle machine, passed everything. The flood check now allows one hand-off per full 3
  seconds the loop took, and the Stay check looks at the page shown, not at server hits. The Stay
  failure under load was not explained; the application code involved did not change.
- `node --test` on `link-targets.test.mjs` (12 tests), `harden-android.test.mjs` (8) and
  `check-apk-manifest.test.mjs` (8): pass; `node scripts/harden-android.mjs` against a project
  generated by `tauri android init` (stand-in SDK, NDK and JDK folders): pass, also a second run
  on the already patched project (the generated folder was deleted afterwards).
- `cargo check --lib --target aarch64-linux-android --locked`: passes, with no warning (the `Menu`
  variant of `Source` is marked as unused on phones).
- Linux, in an Ubuntu 24.04 container with a stub sidecar, after the FIRST round of review fixes
  (**not repeated after the second round**: the volumes were removed and a fresh build was not
  cheap): fmt, `clippy --locked -D warnings`, `cargo test --locked` (90 tests at that time; the
  Windows-only key test is not built), release build and `.deb`/`.rpm`/AppImage bundles. That run
  found one release-only warning (a function used only by the debug log), now fixed. The release
  binary under Xvfb (stays up, creates its folders, forwards a second launch) was run before the
  first review fixes and not repeated. So the non-Windows test count after round 2 (96 expected) and
  the desktop-Linux build of the round 2 code are unverified; the Android `cargo check` above
  compiled the library's non-Windows code that phones use (not the desktop-only code).
- One `security-probe.mjs` run hung once, with the app process gone mid-run and no Windows crash
  event; three later runs passed. The probe now fails after 60 seconds without an answer instead
  of waiting. Cause not found. In round 2 a run on a loaded machine lost the app process in phase B
  the same way (see above).

## What was run for Android (22 October 2026, same Windows 11 PC, x86-64)

- Tool chain (JDK 17.0.20.1, command-line tools 15859902, platform 37.0, build-tools 37.0.0, NDK
  28.2.13676358, emulator 37.2.12, system image `android-36` Google APIs x86_64) installed under
  `E:\android`, checksums checked where the vendors publish them; versions in
  [DESKTOP-AND-MOBILE-APPS.md](../../docs/DESKTOP-AND-MOBILE-APPS.md#android-building-on-windows).
- `scripts/android-build.ps1`: debug APK (universal, arm64-v8a and x86_64) 44.6 MiB; release APK
  (unsigned) 10.2 MiB, 16 minutes. `scripts/check-apk-manifest.mjs` passes on both (debug and release
  mode) and, against the unhardened first build, failed it for the right reasons.
- `scripts/android-probe.mjs` on an x86_64 Android 16 emulator (WHPX): all checks of the 11 phases pass
  (ipc, links, frames, download, upload, back, deeplink, rotate, recreate, dark, offline). One earlier
  run of the 40-link flood check failed because the loaded emulator stretched the loop past 3 s and a
  second hand-off was correctly allowed; the check now allows one per full 3 s of the loop's length.
- In `apps/desktop/src-tauri`: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`
  clean; `cargo test --locked` 105 passed (97 before; new: the launcher rule, the entry page, the
  desktop-only secret script, the Android script, the version overlay, the Android deep-link
  config, the committed project's manifest and Kotlin, the workflow); `cargo check --lib --locked
  --target aarch64-linux-android` clean, no warning. `node --test scripts/*.test.mjs`: 41 pass
  (link-targets 12, harden 10, check-apk-manifest 13, blank-targets 6).
- Desktop after the shared changes (entry page, launcher rule): debug build, `scripts/smoke.ps1`
  passes (it now expects `GET /workspace` for the saved server), `scripts/security-probe.mjs` passes
  (79 checks; it opened a few tabs in the default browser, as designed).
- Not run: iOS; a physical phone or arm64 device; Google Play signing or an AAB; GitHub Actions.

**After the independent review (same day):** `cargo test --locked` 107 passed; `node --test scripts/*.test.mjs`
44 passed (check-apk-manifest 16); Kotlin JVM unit tests (`./gradlew testUniversalDebugUnitTest`) 10 passed
(`DownloadNames` 7, `Origins` 3); root `cargo fmt --all --check` and `cargo clippy --workspace --all-targets
--locked -- -D warnings`, `cargo check --lib --locked --target aarch64-linux-android` (no warning), the desktop
debug build and `scripts/smoke.ps1` pass (desktop behaviour is unchanged: the new Rust code is phone-only,
so `security-probe.mjs` was not rerun). The debug APK (minSdk 29, empty task affinity) and the release APK
pass `check-apk-manifest.mjs` (debug and release; release unsigned). `scripts/android-probe.mjs` on a fresh
x86_64 Android 16 emulator: 13 phases, 72 checks, all pass, including the new `bypass` (form POST to another
origin, history to the launcher) and `renderer` (renderer process killed) phases, the download rules, and
re-creation with a server saved and with a deep link. Not exercised on a device: `filterTouchesWhenObscured`
(needs another app drawing over this one) and the Linux/macOS Gradle verification metadata.
