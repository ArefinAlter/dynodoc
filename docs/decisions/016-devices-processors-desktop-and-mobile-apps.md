# Decision 016: devices, processors, the desktop client and the Android and iPhone apps

Date: 20 October 2026. Owner: Verne. Status: accepted; the first round is built (see "What this round builds").

## Decisions

1. **Scope.** Four things are now in scope, in this order: (a) the web app works on every current
   desktop and mobile browser, (b) the engine, command line and servers run with full feature parity
   on the processors people use (Intel/AMD and Arm, including Apple silicon), (c) a desktop client
   for macOS, Windows and Linux, (d) Android and iPhone apps. This replaces the line in
   `E2EE-CLIENTS-AND-KEY-MANAGEMENT.md` that put native desktop shells and mobile clients "out of
   scope until demand evidence"; the owner has asked for them.
2. **One UI, three layers.** The web app (SvelteKit, server-rendered, cookie sessions through the
   same-origin proxy) stays the only product UI. Layer 1 is a **progressive web app** (installable,
   offline shell) that works everywhere with no store. Layer 2 is a **native shell built with
   Tauri 2** (system webview plus a Rust core) for macOS, Windows, Linux, Android and iOS: it loads
   the configured Dynodoc server (the hosted one or a self-hosted one) and adds what a browser
   cannot: device keys held in the operating system's key store, deep links, notifications, and a
   bundled `engine-cli` as a sidecar for local sync and the MCP server. There is no second UI
   implementation to keep in step.
3. **Why not the alternatives.** Electron ships a whole browser per app, has no mobile target and
   puts patching of that browser on us. Flutter or React Native would rewrite a UI that already
   exists and passes WCAG checks. Separate Swift and Kotlin apps would triple the UI work. Tauri
   reuses the Rust engine crates and the existing web app, targets all five platforms from one
   project, and uses the platform webview (WebView2, WKWebView, WebKitGTK, Android WebView), so
   security updates come with the operating system. The cost is that webview differences
   (especially WKWebView and old Android System WebView) need the browser matrix below.
4. **Processor parity.** Supported and built in CI on every change to the public engine repository:
   Linux x86-64 and aarch64, macOS aarch64 (Apple silicon) and x86-64, Windows x86-64.
   Windows aarch64 is built and its offline tests run where the runner allows (tier 2). Release
   builds of `engine-cli` are produced for all of these, with checksums, and container images for
   the API and web tier are multi-architecture (linux/amd64 and linux/arm64). Not supported: 32-bit
   targets, RISC-V, big-endian. Anything that behaves differently by platform (key stores, file
   permissions, path handling, line endings) is listed in `PLATFORM-SUPPORT.md` with its test.
5. **Browsers and devices.** Last two major versions of Chrome, Edge, Firefox and Safari on
   desktop; Safari on iOS and iPadOS; Chrome on Android. CI runs the browser suite on Chromium,
   Firefox and WebKit and on iPhone and Pixel emulation. Emulation is not a device: real-device
   passes are a release task for the owner or a QA service and are not claimed by this record.
6. **Native shell security model.** The shell talks to exactly one origin, the configured server.
   Native commands are reachable only from that origin and only through an allowlist (device key
   operations, opening a link in the system browser, the fixed sidecar commands); there is no
   generic shell, file or network access from page content. Other origins open in the system
   browser. The page's Content-Security-Policy is unchanged. Key operations that sign or export
   require the operating system's user-presence prompt where it exists. Deep-link payloads are
   validated and never carry tokens.
7. **Phases.**
   - **Round 1 (built here, source only):** this record; the processor matrix and prebuilt
     `engine-cli`; the PWA baseline; the browser and device matrix with fixes; a Tauri desktop
     shell skeleton with the hardened webview and the sidecar allowlist.
   - **Round 2:** desktop device-key custody and sync UI, installers and auto-update (needs
     signing), Android shell (CI builds on Linux), iOS shell (needs a macOS runner).
   - **Round 3:** native value for the stores: biometric unlock, push notifications for review
     requests (needs server work and provider accounts), share extension, offline review queue.

## What the owner must provide (none of it exists yet)

| Item | For | Notes (check current terms at build time; not verified here) |
|---|---|---|
| Apple Developer Program membership | iOS and macOS signing, notarisation, TestFlight, App Store | Required to run on a device outside the simulator and to distribute |
| Google Play Console account and an upload key | Android distribution | Debug APKs need nothing |
| Windows code signing (a certificate or a signing service) or a Microsoft Store listing | Windows installers without SmartScreen warnings | Unsigned builds work but warn |
| Domain files on the web host: `apple-app-site-association` and `assetlinks.json` | Universal links and Android app links | Served by the web app; added when the apps have identifiers |
| Push providers (APNs key, Firebase project) | Notifications | Round 3 |
| Real devices or a device lab | Release testing | Emulation only in CI |

## Risks stated plainly

- **App Store review.** A shell around a website can be refused under the store's minimum
  functionality rule (guideline 4.2; read the current text at submission). The native features in
  round 3 exist partly to answer that; until they are built, expect a review risk for the iPhone
  app.
- **Webview differences.** WKWebView and old Android System WebView lag desktop browsers; the
  matrix finds regressions, it cannot find every one.
- **Sidecar trust.** The bundled `engine-cli` is a signed-with-the-app binary run with a fixed
  argument allowlist; a compromised server origin must not be able to widen it.
- **Cost.** macOS and Windows runners cost more than Linux ones; the native app builds are manual
  or tag-triggered, while the processor matrix runs on the public engine repository.

## What this round builds

See the ledger entries in `PROJECT-STATUS.md` for what was verified. Nothing here is signed,
notarised, published to a store or tested on a physical device.

**Implementation status (21 October 2026, desktop shell and mobile preparation).** Source only.
`apps/desktop` holds a Tauri 2 project with its own Cargo workspace (the engine workspace excludes
`apps/`): a launcher page to choose the hosted or a self-hosted server, a navigation policy under
which the window only ever shows the launcher (when the shell itself asked for it) or the configured
server, and every other public web link opens in the system browser (a page's own requests are rate
limited and cannot reach the mail program or a local address; only a click on a link, within a budget
of its own, or a menu item can; "public" is judged by the address's spelling only), a saved
server address sealed against edits made outside the app, a `dynodoc://` link parser with three accepted forms, a fixed-argument
`engine-cli` sidecar (`--version`, `device keygen --encrypt --keystore`, `device public-key`) with time
and output limits, a native menu, window state, denied web view permissions, single instance, a
pasted-sign-in-link command, and Android and iPhone preparation (mobile entry point, phone capability
file without the tools, icons, minimum versions). Verified: on one Windows 11 PC the unit and
configuration tests, clippy, a debug build, a smoke test (including key creation in folders with
accented and Bengali names, with an `engine-cli` built from the tree that holds the Windows
non-ASCII key-file fix), a black-box probe of the security model inside WebView2 and unsigned debug
NSIS and MSI bundles (built, never installed); the Linux
compile, clippy and tests in an Ubuntu 24.04 container (see the ledger for the exact result).
**Android (22 October 2026):** the generated project is now committed and hardened; a debug APK
(arm64-v8a and x86_64, 44.6 MiB) was built on the Windows PC with JDK 17, Gradle 9.6.1, NDK
28.2.13676358 and build-tools 37.0.0 (`scripts/android-build.ps1`, because Windows refused the
symbolic link Tauri's command line needs), its merged manifest audited against real `aapt2`,
`apkanalyzer` and `apksigner` output (permissions: INTERNET only; one exported activity;
deep-link filters only `dynodoc://open/...`; backup and cleartext off), installed on an x86_64
Android 16 emulator and driven by `scripts/android-probe.mjs` (20 app and plugin commands from a
hosted page all refused; links out, `target=_blank`, `window.open`, rate limit, deep links
including hostile ones, Back, downloads, file chooser, rotation, dark mode, offline, soft
keyboard). It found and fixed defects that apply to desktop too (the window opened the server
root, which on the hosted cloud redirects to the marketing site; the launcher could not reload) and
Android-only ones (`target=_blank` did nothing, Back closed the app, the page drew under the system
bars, downloads and offline had no handling). **Android limits:** signing in inside the app does
not work (the emailed link opens in the browser, a phone cannot reach the launcher's paste form;
needs App Links), the server cannot be changed without clearing the app's data, `blob:` downloads
are refused with a message, mail links from pages do nothing, nothing was tried on a physical
phone. Not run: any GitHub Actions job (the account's Actions are blocked by a billing error),
macOS, iOS, any physical device, a Play-signed build or an AAB. Deviations from the text above, all on
purpose: remote pages get **no** native access at all in this round (decision item 6 allowed an
allowlist for the server's own origin), "open a link in the system browser" is done by the shell's own
policy and not offered as a command, one command was added (`open_sign_in_link`, because an emailed
link opens in the system browser, which does not share a session with the window), and the sidecar is
started with the standard library rather than the Tauri shell plugin. Details:
[DESKTOP-AND-MOBILE-APPS.md](../DESKTOP-AND-MOBILE-APPS.md) and `apps/desktop/README.md`.
