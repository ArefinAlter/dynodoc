# Desktop client and mobile apps: status, builds and what the owner must provide

21 October 2026. Implements the shell of
[decision 016](decisions/016-devices-processors-desktop-and-mobile-apps.md) (round 1, desktop
skeleton, plus the preparation of Android and iPhone). The code is in
[`apps/desktop`](../apps/desktop/README.md); this file is the status and the plan around it.

## What it is

One Tauri 2 project that opens the Dynodoc server you choose (the hosted workspace, or a
self-hosted installation) in the system web view. It does not bundle the web app, so there is
no second UI. It adds a launcher page to choose the server, a strict navigation and link policy,
a device signing key held by the operating system, and a bundled `engine-cli`. The security model
is in the [README](../apps/desktop/README.md#security-model-in-plain-words); the short version:
a server's pages get no native access, the window can only go to the chosen server, other links
open in the system browser, `dynodoc://` links and the sidecar's command line are allowlisted.

## Status per platform

"Run" below means: run by the author on the machine named, on 21 October 2026. **No GitHub
Actions job has run**: this repository's Actions refuse to start jobs at the moment ("recent
account payments have failed or your spending limit needs to be increased"), and a
`workflow_dispatch` workflow cannot be started until its file is on the default branch. So
`desktop.yml` has never executed; it parses, and GitHub lists its six jobs. Nothing is signed,
notarised, published or tried on a physical device.

| Platform | In the repository | Run so far | Not done |
|---|---|---|---|
| Windows x86-64 | Source, launcher, sidecar, menu, deep link, probes | Windows 11 PC, after the security-review fixes and a merge of main: format, clippy, 91 tests (including the real `engine-cli` creating a DPAPI-bound key), debug build, `smoke.ps1` (which also creates keys in folders with accented and Bengali names), `security-probe.mjs` (four phases inside WebView2, also with `--unicode-home`); earlier: unsigned debug NSIS and MSI bundles, a release build that ran | Installers never installed; no release-profile run; no signing, so SmartScreen will warn |
| Linux x86-64 | The same source | In an Ubuntu 24.04 container on the Windows PC (the steps of the `linux` job, with a stub in place of `engine-cli`): format, clippy with `-D warnings`, tests, release build, `.deb` (2.0 MiB), `.rpm` (2.0 MiB) and AppImage (77 MiB) bundles, repeated after the security-review fixes (90 tests); the release binary under Xvfb with a D-Bus session (stays up, creates its config and data folders, a second launch is forwarded to it and exits) was run before those fixes and not repeated | Not run on a real desktop; the real `engine-cli` was not built on Linux; the bundles were not installed; AppImage does not register `dynodoc://`; no Secret Service key creation exercised; the app was ended with SIGTERM (exit 143), not by closing its window |
| Linux aarch64 | Workflow job behind a tick box | Nothing (private-repository availability of `ubuntu-24.04-arm` unverified) | Everything |
| macOS aarch64 and x86-64 | Same source; workflow jobs on `macos-latest` and `macos-15-intel` | Nothing; the Apple targets cannot even be type-checked on Windows | Everything: no Mac build, no Keychain key creation, no notarisation |
| Windows aarch64 | Workflow job behind a tick box | Nothing (`windows-11-arm`, availability unverified) | Everything |
| Android | Mobile entry point, phone capability file, icons, `minSdkVersion` 24; workflow job `android` generates the project and builds a debug APK | `cargo check --lib --target aarch64-linux-android` passes on Windows (also proves the sidecar declaration stays out of a phone build) | Gradle, an emulator or device; Play signing; app links; there is no sidecar on phones. `tauri android init` was run with stand-in SDK folders: the template has no `allowBackup` and enables cleartext in debug, which `scripts/harden-android.mjs` fixes and checks (README) |
| iPhone | Same source, iOS 15.0 minimum; workflow job `ios` (simulator, unsigned) | Nothing | Everything: needs a Mac runner and, for devices, an Apple Developer team |

## How to build each

All commands start in `apps/desktop` after `npm ci --ignore-scripts`. The sidecar must be
prepared first on desktop: `node scripts/prepare-sidecar.mjs [--debug] [--target <triple>]`.

- **Windows:** Visual Studio Build Tools (C++), WebView2 (in Windows 11). `npm run tauri -- build`
  (NSIS and MSI need to download their tools; `--no-bundle` skips installers).
- **macOS:** Xcode command line tools. `npm run tauri -- build --bundles app,dmg`. A universal
  binary is not set up; build on each architecture, as CI does.
- **Linux:** `sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev
  libayatana-appindicator3-dev patchelf file`, then `npm run tauri -- build --bundles
  deb,rpm,appimage`.
- **Android:** JDK 17, Android SDK and NDK (`ANDROID_HOME`, `NDK_HOME`), Rust targets
  (`rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android
  x86_64-linux-android`). `npm run tauri -- android init`, then `npm run tauri -- android build
  --debug --apk --target aarch64`. The generated project lives in `src-tauri/gen/android` and is
  git-ignored.
- **iPhone:** a Mac with Xcode, `brew install xcodegen`, `rustup target add aarch64-apple-ios-sim`.
  `npm run tauri -- ios init`, then `npm run tauri -- ios build --debug --target aarch64-sim`.
  Device builds need an Apple development team (`APPLE_DEVELOPMENT_TEAM`).

CI: `.github/workflows/desktop.yml`, **manual or tag `desktop-v*` only, never on push or pull
request** (private repository: runner minutes are billed and a macOS minute counts ten times a
Linux one). Manual runs let you tick which jobs to run; Linux, Windows and Android are on by
default, macOS, iOS and the Arm runners are off. Artifacts are kept for seven days and are
unsigned. A manual run only works once the file is on the default branch (GitHub refuses
`workflow_dispatch` for a workflow that exists only on another branch), and a tag run starts every
job except the Arm ones, so check what a tag will cost first.

## Signing, notarisation and updates: what the owner must add (nothing exists)

No secret or certificate exists, and none is invented here. The workflow builds unsigned
artifacts. When the owner has the accounts below, add repository secrets (the names are suggestions to
be decided, not existing values) and extend the workflow; check each vendor's current terms.

| Need | Secrets to create later (suggested names) | Notes |
|---|---|---|
| macOS signing and notarisation | Developer ID Application certificate as a base64 `.p12` (`APPLE_CERTIFICATE`) and its password (`APPLE_CERTIFICATE_PASSWORD`); the signing identity (`APPLE_SIGNING_IDENTITY`); either an Apple ID, app-specific password and team (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`) or an App Store Connect API key (`APPLE_API_ISSUER`, `APPLE_API_KEY`, key file) | Apple Developer Program membership. Tauri reads these variables when they are set (unverified: check the current Tauri signing guide at build time). |
| iOS signing | A distribution certificate and provisioning profile, `APPLE_DEVELOPMENT_TEAM` | The same membership. Simulator builds need none. |
| Windows signing | Depends on the service chosen: a certificate file and password, or a cloud signing service's credentials | Tauri supports a custom sign command. Without it, SmartScreen warns. |
| Android release signing | An upload keystore as base64, its password, the key alias and the key password | A Google Play Console account. Debug APKs are signed with a throw-away debug key and cannot be published. |
| Auto-update | An update signing key pair (`TAURI_SIGNING_PRIVATE_KEY` and its password) and a place that hosts update manifests | Separate from operating-system signing. **No updater is built**; adding one needs the updater plugin, a manifest host and a decision on release channels. |
| Universal links and app links | `apple-app-site-association` and `assetlinks.json` served by the web host | Needs the final identifiers (below). |

## Identifiers the owner must confirm before any store use

- Bundle identifier / application ID: `online.dynodoc.client` (one value for all five platforms).
  Chosen as the reverse of `dynodoc.online`. It is permanent once an app is uploaded to a store.
- Product name `Dynodoc` and the `dynodoc://` scheme. A scheme can be claimed by any other app on a
  computer; the app treats every link as hostile input for that reason.

## Store review risk

Unchanged from decision 016 and restated plainly: a shell around a website can be refused
by the Apple App Store under its minimum-functionality rule (guideline 4.2; read the current text
at submission), and Google Play has comparable rules about apps that only wrap a website (check
current policy text). The native value planned for round 3 (biometric unlock, push notifications
for review requests, a share extension, an offline review queue) exists partly to answer that.
Until it is built, expect review risk for the iPhone app and treat the Android app the same way.
Do not claim store readiness from this round.

## Known limitations of the web view approach

- **Sign-in:** the emailed link opens in the system browser. The shell offers "Sign in with an
  emailed link" (paste it) as a workaround. Google sign-in does not complete inside the window.
  A proper hand-over (the browser finishing sign-in and giving the app a one-time code) needs
  server work and is round 2.
- **Downloads, printing, file pickers, drag and drop, and iframes** are whatever the platform's
  web view does by default; none was exercised. The web app's `frame-ancestors 'self'` means it
  cannot be framed by the launcher, which is why the window navigates to it instead.
- **Web view versions** differ per platform (WebView2, WKWebView, WebKitGTK, Android System
  WebView). The browser matrix of decision 016 covers browsers, not these shells.
- **Permissions** (camera, microphone, location, notifications, clipboard read) are denied for
  every page, so any web feature that needs them will not work in the shell. Tested in WebView2:
  location, camera, microphone and notifications are not granted.
- **New windows and links out.** A click on `<a target="_blank">` raises no event in WebView2, so the
  shell injects a short script into every page that turns such a click into `window.open`; the
  new-window rule then loads same-server links in the one window and opens other web links in the
  system browser. The shell never opens a second window. `<form target="_blank">` and a middle-click
  are not handled. WebView2 does not say whether a person clicked, so a page's own request to open
  a link is limited: public `http(s)` addresses only, one per 3 seconds and five per minute, never a
  mail link or a local-network address. A trusted click on a link (or a menu item) may open those too;
  the injected script proves the click with a secret made from the operating system's random source
  when the app starts (per run, not per page load). The script keeps its own copy of every built-in
  it uses and holds the secret in one closure variable, so a page that replaces built-ins afterwards
  does not see it. A real click on a link the page controls (a link moved under the pointer, an
  invisible overlay) is still a real click, so clicks have a budget of their own: one per second,
  ten per minute. **"Public" is only about how the address is spelled.** A best-effort list refuses
  the well-known wildcard-DNS services and names that spell an IPv4 address, but any domain can point
  a name at a private address and DNS can be re-pointed after the check (DNS rebinding); the shell
  cannot see either. What a page gets from it is at most one browser tab per 3 seconds, never a mail
  link.
- **Frames on WebKit and Android are unverified.** WebView2 reports only the window's own navigations
  to the shell; WebKitGTK, WKWebView and perhaps Android also report `<iframe>` navigations without
  saying which is which. Nothing was observed on those platforms. The shell assumes the ambiguity there
  and lets `about:srcdoc`, `about:blank`, `data:` and the server's own `blob:` addresses through (they
  are how pages fill frames); they never use up the launcher's one-time permission. Check on a real
  WebKit build before relying on it.
- **A link from another program navigates the window.** A `dynodoc://` link brings the window forward
  and takes it to that page, even if you were working in it. The protection for unsaved work is the web
  app's own leave-page warning. Checked: WebView2 runs the page's `beforeunload` handler for the shell's
  own navigation and shows its "Leave site?" dialog, and answering Stay keeps the page.
- **A pasted sign-in link signs in whichever account issued it.** Only paste a link from an email you
  asked for in this window. The web `/login` page still needs a click ("Continue to workspace") before
  it uses the token. Showing the account's email address there before the click would close the gap;
  that is server work (owner question 3).
- **A changed saved address is shown, not opened.** The saved server is sealed with a SHA-256 in the
  local data folder; if the file was edited by anything that did not also rewrite the seal, the launcher
  asks the person to confirm it. Malware running as the user can rewrite both.
- **Refused navigations still cost one request.** WebView2 sends the request for a navigation before
  the shell cancels it (seen in the probe), so a server page can make the web view fetch an address
  without showing it. Pages can already send requests; the shell does not make that worse, but
  "refused" means "not shown and not opened", not "never sent". Other web views were not tested.
- **Hidden pop-up target.** After a refused `window.open`, WebView2 may list a blank target for a
  while; it is not a window.

## Open questions for the owner

1. **GitHub Actions on this private repository will not start any job** until the billing problem
   is fixed ("recent account payments have failed or your spending limit needs to be increased").
   Until then no installer, APK or simulator build can be produced or tested in CI.
2. Is `online.dynodoc.client` the right identifier (and is the reverse-domain `online.dynodoc` correct for the company)?
3. Should the shell's "Sign in with an emailed link" workaround stay, or should sign-in hand-over
   (server work) come first? Either way: should the web `/login` page show the account's email
   address before the person confirms a token (server work; it closes the login-CSRF gap of a pasted
   link)?
4. Accept the permission denial for all pages, or allow clipboard read for the web app's origin?
5. Which Windows signing route (certificate, cloud signing service, Microsoft Store)?
6. Arm runners: try `ubuntu-24.04-arm` and `windows-11-arm` on this private repository?
7. To run `desktop.yml` by hand the file must be on `main` first; merge it, then use "Run workflow".
