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

"Run" below means: run by the author on the machine named, on 21 October 2026 (Android: 22 October
2026, on the same Windows PC). **No GitHub
Actions job has run**: this repository's Actions refuse to start jobs at the moment ("recent
account payments have failed or your spending limit needs to be increased"), and a
`workflow_dispatch` workflow cannot be started until its file is on the default branch. So
`desktop.yml` has never executed; it parses, and GitHub lists its six jobs. Nothing is signed,
notarised, published or tried on a physical device.

| Platform | In the repository | Run so far | Not done |
|---|---|---|---|
| Windows x86-64 | Source, launcher, sidecar, menu, deep link, probes | Windows 11 PC, after the security-review fixes and a merge of main: format, clippy, 91 tests (including the real `engine-cli` creating a DPAPI-bound key), debug build, `smoke.ps1` (which also creates keys in folders with accented and Bengali names), `security-probe.mjs` (four phases inside WebView2, also with `--unicode-home`), `signin-probe.mjs` (the browser sign-in through the real shell); earlier: unsigned debug NSIS and MSI bundles, a release build that ran | Installers never installed; no release-profile run; no signing, so SmartScreen will warn |
| Linux x86-64 | The same source | In an Ubuntu 24.04 container on the Windows PC (the steps of the `linux` job, with a stub in place of `engine-cli`): format, clippy with `-D warnings`, tests, release build, `.deb` (2.0 MiB), `.rpm` (2.0 MiB) and AppImage (77 MiB) bundles, repeated after the security-review fixes (90 tests); the release binary under Xvfb with a D-Bus session (stays up, creates its config and data folders, a second launch is forwarded to it and exits) was run before those fixes and not repeated | Not run on a real desktop; the real `engine-cli` was not built on Linux; the bundles were not installed; AppImage does not register `dynodoc://`; no Secret Service key creation exercised; the app was ended with SIGTERM (exit 143), not by closing its window |
| Linux aarch64 | Workflow job behind a tick box | Nothing (private-repository availability of `ubuntu-24.04-arm` unverified) | Everything |
| macOS aarch64 and x86-64 | Same source; workflow jobs on `macos-latest` and `macos-15-intel` | Nothing; the Apple targets cannot even be type-checked on Windows | Everything: no Mac build, no Keychain key creation, no notarisation |
| Windows aarch64 | Workflow job behind a tick box | Nothing (`windows-11-arm`, availability unverified) | Everything |
| Android | The Tauri 2.12 Android project is **committed** (`apps/desktop/src-tauri/gen/android`, hardened), mobile entry point, phone capability file, icons, `minSdkVersion` 29, `targetSdkVersion` 37, deep-link filters, a small Kotlin layer (insets, Back, narrow downloads, offline page, refusal of navigations Android does not report; no JavaScript bridge), `scripts/android-build.ps1`, `scripts/android-probe.mjs`, workflow job `android` | **22 October 2026, Windows 11 PC:** debug APK built (universal, arm64-v8a plus x86_64, 44.6 MiB), merged manifest audited against real `aapt2` output, installed and run on an x86_64 Android 16 emulator (WebView 133, hardware-accelerated through WHPX) and driven by a black-box probe (see "Android: what was run on 22 October 2026"); a release APK was built (below) | Signing in by the system browser and a one-time code is built ([decision 017](decisions/017-native-app-sign-in-handover.md), "Signing in: the browser hand-over" below); @@ANDROID@@; changing the server: sign out in the web app and the launcher returns (a saved server that cannot be reached still needs the app's data cleared); no physical device; no Play signing, no AAB run; GitHub Actions job never executed; iOS not part of this |
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
- **Android:** see "Android: building on Windows" below for the exact tool versions. Short form: JDK 17,
  Android SDK (platform 37.0, build-tools 37.0.0, NDK 28.2.13676358), `ANDROID_HOME`, `NDK_HOME`,
  `JAVA_HOME`, Rust targets (`rustup target add aarch64-linux-android x86_64-linux-android`), then in
  `apps/desktop`: `npm run tauri -- android build --debug --apk --target aarch64 x86_64 -- --locked`
  (Linux, macOS, and Windows with Developer Mode on) or `powershell -File scripts\android-build.ps1`
  (Windows without the right to create symbolic links). The Android project is **committed**: do not
  run `android init` again unless you mean to regenerate and re-harden it.
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

## Android: building on Windows

Everything below was installed on 22 October 2026 outside the repository and outside drive C:
(`E:\android`), for this one task, with no change to the system or user environment (the variables
are set per command; `scripts/android-build.ps1` and `scripts/android-probe.mjs` read them from the
environment). Downloads were checked against the vendors' published checksums where they publish
them; `sdkmanager` verifies its own packages.

| Part | Version | Where from, how checked |
|---|---|---|
| JDK | Temurin 17.0.20.1+1 (zip) | Adoptium; SHA-256 `e53a79c3...c7cb0` equals Adoptium's `.sha256.txt` |
| Android command-line tools | 15859902 (`sdkmanager` 22.0) | dl.google.com; SHA-256 `90ae805d...b04a` equals the table on developer.android.com/studio |
| platform-tools | 37.0.1 | `sdkmanager` |
| platform | `android-37.0` (revision 2) | `sdkmanager`; the project's compileSdk and targetSdk are 37 |
| build-tools | 37.0.0 (`aapt2`, `apksigner`, `zipalign`) | `sdkmanager` |
| NDK | 28.2.13676358 (r28c) | `sdkmanager`; Tauri uses whichever NDK `NDK_HOME` names |
| emulator | 37.2.12 | `sdkmanager`; WHPX was "installed and usable" (`emulator -accel-check`) |
| system image | `system-images;android-36;google_apis;x86_64` (revision 7, Android 16, WebView 133.0.6943.137) | `sdkmanager` |
| Gradle | 9.6.1 (wrapper), SHA-256 `9c0f7fae...9e14` pinned in `gradle-wrapper.properties`; every dependency pinned by `gradle/verification-metadata.xml` | services.gradle.org checksum; the wrapper jar's SHA-256 is on gradle.org/release-checksums; the metadata is written by Gradle (see "Android: review follow-ups") |
| Android Gradle plugin, Kotlin | 9.3.1, 2.2.10 | Tauri 2.12.3's template (`tauri android init`) |
| Tauri | CLI 2.12.3 (npm), `tauri` 2.12.4, `wry` 0.57.0, deep-link plugin 2.6.2, opener 2.7.2 | `package-lock.json`, `Cargo.lock` (`--locked`; the `rust-ini` 0.21.1 pin is untouched) |

Only the `android-sdk-license` was accepted (the licence of the packages above). Disk used on `E:`:
about 8.5 GB for the SDK, JDK and emulator image, 1.2 GB of Gradle cache, 1.6 GB of Rust build output
for the debug build (more for release); nothing large in the repository. The emulator and adb also
write a few small files under `C:\Users\<you>\.android` (adb key, emulator state); the debug signing
key is `debug.keystore` in the Android user home (`ANDROID_USER_HOME`), outside the repository.

**Commands (Windows, from `apps/desktop`, after `npm ci --ignore-scripts`).** Set `JAVA_HOME`,
`ANDROID_HOME` (and `ANDROID_SDK_ROOT`), `NDK_HOME`, `GRADLE_USER_HOME` and, to keep the AVD off C:,
`ANDROID_AVD_HOME`; put `platform-tools`, `emulator` and `cmdline-tools\latest\bin` on `PATH`.

```powershell
sdkmanager --sdk_root=$env:ANDROID_HOME "platform-tools" "ndk;28.2.13676358" "platforms;android-37.0" "build-tools;37.0.0" "emulator" "system-images;android-36;google_apis;x86_64"
avdmanager create avd -n dynodoc36 -k "system-images;android-36;google_apis;x86_64" -d pixel_6
$env:CARGO_TARGET_DIR='E:/cargo-target/android'; $env:CARGO_BUILD_JOBS='3'; $env:CARGO_PROFILE_DEV_DEBUG='0'; $env:CARGO_INCREMENTAL='0'
powershell -ExecutionPolicy Bypass -File scripts\android-build.ps1          # debug APK, aarch64 + x86_64, universal
node scripts\check-apk-manifest.mjs src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk --build debug
emulator -avd dynodoc36 -no-window -no-audio -gpu host      # or without -no-window to see it
adb install -r src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk
node scripts\android-probe.mjs
```

`avdmanager` printed "Could not load devices from ...\devices.xml" for the Google APIs image and still
created the AVD. The emulator needs about 3.5 GB of memory with 2 GB given to the guest; on a PC
with under 2 GB free it showed "System UI isn't responding" for several minutes after the first boot
(it settled; the first boot took 2 minutes, later ones about one).

## Android: what was run on 22 October 2026

Windows 11 PC, x86-64, the Windows Hypervisor Platform accelerating an x86_64 Android 16 emulator
(Pixel 6 profile, 1080x2400, 420 dpi, 2 GB RAM, 4 cores). Nothing was run on an arm64 device: the
arm64 library was compiled and packaged and never executed. Nothing was run in GitHub Actions.

- **Build.** `tauri android build` compiled the Rust library for aarch64 and x86_64 and then stopped
  at the symbolic link; `scripts/android-build.ps1` did the rest (copy, Gradle) and produced
  `app-universal-debug.apk`, 44.6 MiB (two unstripped debug libraries of about 19.5 MiB each, stored
  uncompressed because `extractNativeLibs` is false; incremental rebuilds left the file at 63.5 MiB, with
  about 19 MiB of dead space inside the zip, until a full rebuild compacted it). Rebuilds after a source change took about a
  minute. **Release build:** `scripts\android-build.ps1 -Release` (16.4 minutes for both targets, `lto = true`, three jobs, while engine tests ran
on the same PC) built `app-universal-release-unsigned.apk`, **10.2 MiB** (the stripped release libraries are 4.13 MiB for
arm64-v8a and 4.5 MiB for x86_64). R8 shrinking worked: a copy signed with the SDK's debug key (local test
only; the checker correctly refuses that key for a release build) installed on the emulator, and the launcher, the
hosted sign-in page and the offline page with its retry all worked. The release manifest audit passes
(`debuggable` absent, `versionName 0.1.0`, `versionCode 1000`); the APK is unsigned, as it must be until the
owner's keystore exists (`check-apk-manifest.mjs --build release --allow-unsigned`). **WebView remote
debugging in release:** the release library contains no call to `setWebContentsDebuggingEnabled` (the
debug library has one; wry compiles it only with debug assertions or its `devtools` feature, which no
crate here enables) and the app is not debuggable. The emulator's DevTools socket was nevertheless open
for the release app, because the emulator image is a `userdebug` build (`ro.debuggable=1`), where Chromium's
WebView exposes every app; so "off in release" is verified from the library and the manifest, not on a
production-build phone.
- **Manifest, from the real tools** (`aapt2 dump xmltree`, `apkanalyzer manifest print`; both are now
  fixtures of `check-apk-manifest.test.mjs`): package `online.dynodoc.client`; `versionName 0.1.0`,
  `versionCode 1000` (from `tauri.android.conf.json`; the first build said `1.0` / `1`);
  minSdk 29, targetSdk 37, compileSdk 37 (raised from 24 on review: see "Android: review follow-ups"). Permissions: `INTERNET` only, plus
  `online.dynodoc.client.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION`, a signature-level permission
  AndroidX adds for its own receivers (no other app can hold it). Exported components: `MainActivity`
  (`singleTask`; launcher and leanback launcher filters and three `dynodoc://open/...` filters:
  actions `VIEW` and, added by the plugin for ChromeOS, `org.chromium.arc.intent.action.VIEW`;
  categories `DEFAULT` and `BROWSABLE`; scheme `dynodoc`, host `open`, path `/workspace` or prefix
  `/projects/` or `/documents/`; **no `autoVerify`**), and `androidx.profileinstaller.ProfileInstallReceiver`
  (exported, but requires `android.permission.DUMP`, which only the shell and the system hold). Not
  exported: `FileProvider`, `InitializationProvider`. Application flags: `allowBackup=false`,
  `dataExtractionRules` set, `usesCleartextTraffic=false`, no network security config, `debuggable=true`
  in the debug APK only. **What the unhardened template gave** (first build): no `allowBackup`
  (so true), cleartext true in debug, no extraction rules, no deep-link filters at all (the plugin
  writes them only when `plugins.deep-link.mobile` is set). Signing: V2 only, `C=US, O=Android,
  CN=Android Debug`, SHA-256 `71ad0481...8911`, the SDK's own debug key.
- **The checker against real output.** The first version of `check-apk-manifest.mjs` (written against
  invented samples) did parse real `aapt2` output and correctly failed the unhardened APK, but it
  looked only at `<application>`. It now builds the element tree (real indentation: attributes two
  columns in, children four), audits permissions, exported components, link filters (Android merges the
  plugin's one-attribute-per-`<data>` elements, so they are checked as one filter), SDK levels and the
  signing certificate (`apksigner` build-tools 37 prints `V2 Signer: certificate DN:` rather than
  `Signer #1`), and exits 2 on anything it cannot parse. 13 tests; the negative cases are made by editing
  the real output.
- **WebView.** Android System WebView 133 (`com.google.android.webview`) supports document-start
  scripts, so scripts the shell registers run before the page's own. (Wry falls back to
  `evaluateJavascript` in `onPageStarted` on web views without that feature, which is **not**
  guaranteed to run first; the desktop click secret relied on running first, which is one reason it
  is not injected on Android.) Settings applied by Tauri's `RustWebView`: JavaScript, DOM storage,
  database, geolocation flag (Rust refuses every permission request), autoplay without a tap,
  `javaScriptCanOpenWindowsAutomatically` true. Settings this repository adds (`MainActivity.kt`): no
  file or content access, mixed content never allowed, no second window, `window.open` only after a
  tap. Remote debugging is on in the debug build only (wry's `devtools` flag, which is
  `cfg!(debug_assertions)` here); the probe uses it, so **the probe cannot run against a release
  build**. Safe Browsing is on (the API default).
- **The launcher at phone size** (screenshots looked at): 16 px text, 44 px high buttons and fields,
  no horizontal scroll in portrait (412 css px) or landscape (866x364; the camera cut-out margin
  is applied), readable in light and dark. After the insets fix the page sits between the status and
  navigation bars; the bars show the window colour (the launcher's background), so there is no band of
  a different colour. The soft keyboard (email layout for the sign-in field) resizes the page and keeps
  the focused field and the button below it in view.
- **Probe results** (`scripts/android-probe.mjs`, run phase by phase and finally together; details in
  the report that came with this change): 20 app and plugin commands from a hosted page refused (raw
  `window.ipc.postMessage` needs a per-run key the page does not have); tap on an external link with and
  without `target=_blank`, and `window.open` from a tap, open the system browser once and leave the
  window where it is; mail, local-address and credential links, and `window.open` without a tap, do
  nothing; a page that sets `location` 40 times in two seconds gets one hand-off and 39 are
  rate-limited; a `target=_blank` link to the server's own page loads in the one window; three
  iframes are not reported to the shell at all on this web view; deep links `dynodoc://open/workspace`
  and `.../projects/<uuid>` work warm and cold, a link whose dot segments collapse to an accepted form
  is that form (the plugin parses with the `url` crate before our parser sees it; `../admin` is
  refused), every other hostile link (other host, other path, query, fragment, encoded dots,
  300-character id, NUL, `https://`) either is not routed by the manifest (Android starts nothing)
  or is dropped; the same links sent to the activity by name are dropped by the plugin's own check or
  by our parser; Back goes back inside the site and from the first page sends the app to the
  background; rotation does not restart the app; a font size change (which makes Android re-create the
  activity) leaves the launcher working; the web view follows dark mode; a same-site download reaches
  `Download/` when the site has set no cookies (otherwise it is refused: see "Android: review follow-ups"); a `blob:` download is refused with a message; the system file chooser opens for an
  upload field and Back returns; offline shows the retry page and **Try again** works once the
  connection is back.
- **Defects found and fixed** (each has a test or a probe check): window opened the server root
  (marketing site redirect; desktop too); launcher blank after the activity was re-created;
  `target=_blank` silently did nothing; the desktop click script would have swallowed every link out;
  Back closed the app from any page, and the first fix (calling the platform handler from the
  callback) did nothing on Android 16; page under the status bar and keyboard; no download handling;
  stock offline page; launcher text mentioning a File menu; `use-cloud` throwing if tapped before the
  app answered; template `FileProvider` exposing all shared storage; wrong app version.
- **Not working / not done, and why.**
  1. **Signing in inside the app** was not possible on 22 October (the emailed link and Google finish in
     the browser, which does not share the app's session, and a phone has no menu to paste a link).
     Replaced on 22 October by the browser hand-over of
     [decision 017](decisions/017-native-app-sign-in-handover.md): the launcher's "Sign in with your
     browser" button opens the browser, a one-time code comes back in `dynodoc://signin?...`, and the
     app's own window redeems it with a PKCE verifier that never left the app. Android App Links
     (`/.well-known/assetlinks.json` with the SHA-256 of the app signing key and an `autoVerify` filter)
     can later replace the custom scheme; they are not part of it, because they need the release signing
     key. See "Signing in: the browser hand-over" below for what was and was not run.
  2. **Changing the server.** Sign out in the web app: it goes to its sign-in page, the shell shows the
     launcher there (phones only, decided from the address alone), and a server can be chosen. A saved
     server that cannot be reached (a mistyped address) shows the offline page with "Try again" and
     still needs the app's data cleared (Settings > Apps > Dynodoc > Storage). A launcher deep link or
     an Android long-press shortcut would remove that; decision 017 weighs it and does not build it.
  3. **Downloads of signed-in files and `blob:` downloads** (the web app's CSV, device-key and project
     exports): refused with a message. A blob needs a script bridge, which this app deliberately does not
     have; a signed-in file needs cookies, which the app now never sends (see "Android: review follow-ups").
     Only a public https file of the page's own origin is saved.
  4. **Mail links and local-address links from a page do nothing** (no tap signal on Android).
  5. Printing, camera, microphone, location, notifications, clipboard read: denied or not tried.
  6. Doubled navigation on a cold deep link: the plugin delivers the link twice (event and
     `get_current`), so the same page is requested up to three times. Harmless, not fixed.
  7. Arm64 execution, a physical phone, Android versions other than 16 / WebView 133, Play signing,
     an AAB, the GitHub Actions job (its Linux path, `tauri android build` and the `sdkmanager` line,
     has not been executed anywhere; the Windows PC needs the script), iOS.

## Android: review follow-ups (22 October 2026)

An independent review of the branch found no critical or high problem and nothing was run on a
device by the reviewer; its medium and low findings were fixed and checked on the emulator (all
phases of `scripts/android-probe.mjs` pass again, the new checks included).

1. **Downloads (medium).** A page's script could start downloads (`a.click()`, an attachment, a hidden
   iframe) with no touch and no limit; the name and type came from the server (only `/` was removed);
   and the `Cookie` header added to the download manager is repeated on every redirect hop, so an open
   redirect on the server would hand the session cookie to another host. Now: a touch of the page
   in the last 5 seconds is required (recorded by the web view's own touch listener, so a touch
   dropped by `filterTouchesWhenObscured` does not count) and at most one download starts per 3
   seconds; **same site means the same origin (scheme, host and port) as the page shown**
   (`Origins`, a pure function with JVM tests; the page address is `WebView.getUrl()`, the committed
   page, never a string comparison of hosts); **no cookie is ever sent**; the name is cleaned
   (`DownloadNames`: control, format and bidirectional characters, line separators, private-use and
   unassigned characters dropped, path separators and `<>:"|?*` replaced, leading dots and spaces
   removed, 100 characters with the extension kept) and **anything installable or runnable is refused
   whatever the type says** (`.apk .xapk .apks .apkm .aab .dex .odex .jar .so .sh .bat .cmd .exe .msi ...`
   in any dot-separated part of the name, and the installable types whatever the name says).
   **Consequence, stated plainly: a download the site would give only to a signed-in person is not
   supported in the app** (if the cookie jar holds cookies for the address the app shows "Not
   supported in the app yet. Download this in your web browser."; a signed-in person always has them),
   so in practice the app saves public files only; exports the web app builds as `blob:` stay
   unsupported as before. A safe in-app fetch (redirects off, credentials the app controls) can come
   later. Checked on the emulator: a script's click is refused, a tap saves a public file, three quick
   taps start one, a cookie set for the site makes the download refused with nothing fetched, a
   `blob:` link is refused with a message; the name rules and the origin rules by 10 JVM tests.
   **minSdkVersion is now 29** (it was 24): the download manager saves into `Download/` without any
   storage permission only from Android 10, and Android 7 to 9 stay open to task hijacking
   ("StrandHogg", an app that claims this app's task affinity) whatever the manifest says. The
   alternative was declaring `WRITE_EXTERNAL_STORAGE` for API 24 to 28 and keeping those phones; with no
   users yet to lose, the smaller attack surface won. Check current Android version shares before a
   store release. (`tauri.conf.json`, `build.gradle.kts`, the tests and the checker all say 29.)
2. **Navigations Android never reports (low).** `shouldOverrideUrlLoading`, which feeds the Rust
   rule, is called for GET link navigations only, not for a form POST or a history step. `OfflineClient`
   now answers a main-frame http(s) request to an origin other than the site's (the origin of the first
   page of the site that started loading; the launcher's address is exempt; nothing is judged before a
   site page has started) with **HTTP 403 and a short page** with one link back (an empty body would have
   left a blank window: the web view shows the answer in place of the page that asked), and drops the
   launcher from the web view's history once the site has loaded, so `history.back()` cannot bring it up.
   Checked on the emulator: a form POST to another origin (the page's own `form-action 'self'` policy
   switched off for the test) is refused with the log line and the page; `history.length` is 1 and
   `history.back()` stays; the link back loads the site. Frames are not covered. The README line
   "the window can only go to your server" now says what is true on Android.
3. **The checker's namespace handling (low).** `check-apk-manifest.mjs` keyed attributes by local name
   only, so `http://evil/ns:exported="false"` after `android:exported="true"` hid an exported receiver. It
   now keeps android-namespace attributes (`attrs`) apart from every other (`other`, keyed `uri:name`),
   resolves apkanalyzer prefixes by their `xmlns` declarations, fails on a repeated attribute, and fails on
   any attribute outside the android namespace except the three the tools put on `<manifest>`; it also audits
   `allowTaskReparenting` and requires an empty `taskAffinity` on the activity. The mutation is a test case
   (aapt2 and apkanalyzer forms, a rebound `android` prefix included); the fixtures are regenerated from
   the new APK (real output, minSdk 29, `taskAffinity=""`).
4. **Task affinity and tapjacking (low).** `android:taskAffinity=""` on `MainActivity` and
   `webView.filterTouchesWhenObscured = true`; the manifest audit requires the empty affinity. The
   touch filter is not exercised by the probe (it needs another app drawing over this one): reviewed in
   code and by a policy test only.
5. **Gradle supply chain (low).** `gradle/verification-metadata.xml` (523 components, sha256, no trusted
   or ignored keys) is committed, written from the committed project by `./gradlew
   --write-verification-metadata sha256 assembleUniversalDebug assembleUniversalRelease
   testUniversalDebugUnitTest -x rustBuild...` (the `rustBuild*` tasks skipped as in
   `android-build.ps1`; refresh it the same way after any dependency, plugin or Tauri update, and
   review the diff). It was written on Windows, where Gradle resolves only the Windows `aapt2`; the Linux
   and macOS jars, the one platform-specific artifact, were added by hand from Google's Maven with the
   sha256 computed here and checked against the sha1 Maven publishes. **The first Linux or macOS build is
   the real test of the file**: if it reports a missing checksum, run the command above there.
   The Material library was kept: Tauri's own Android library depends on it (1.7.0) and the theme is
   Material3, so removing our explicit 1.12.0 would only downgrade it. Gradle was not bumped (a bump
   needs the new distribution's checksum re-verified from services.gradle.org and the metadata
   rewritten, for no gain). Provenance kept: the wrapper jar's sha256 `497c8c2a...a9c7` is the one
   gradle.org publishes for 9.6.1 and `distributionSha256Sum` (`9c0f7fae...9e14`) equals the published
   `gradle-9.6.1-bin.zip.sha256` (the reviewer repeated both checks). Side note: building Android's Gradle project also
   writes `build` folders inside the Tauri crates in `~/.cargo/registry/src` (on drive C: here); they
   are small and can be deleted.
6. **Robustness (low).** `onRenderProcessGone` is handled: a dead renderer used to take the whole app
   with it; now the activity is re-created. Checked by killing the renderer process on the emulator (the
   app process survived and the site came back). **With a server saved, a re-created activity reloaded the
   window's first address, the launcher, which is refused when a server is configured, and left a blank
   window** (reproduced on the emulator with a font size change; the log shows
   `navigation http://tauri.localhost/ -> Deny`): on phones the shell now goes to the server's entry page
   when the launcher is refused and a server is saved (`nav::launcher_denied_goes_to`; desktop still refuses
   outright). A re-created activity also replaces its launch intent, so the deep-link plugin does not
   replay the `dynodoc://` link that started it (checked: cold start with a project link, font size change,
   the project is not opened again). Not done: clearing the intent right after a cold-start link is handled (the plugin
   offers no hook; the replacement happens on re-creation).
7. **Documented (info).** wry's `ipc` JavaScript interface is present in every frame, including a
   cross-origin iframe; the origin the permission check sees comes from wry's `onPageStarted` (the page that
   has STARTED loading); `require_launcher(webview.url())` in `commands.rs` reads the web view's own
   current address and is the load-bearing second lock (README, "Mobile"). The reviewer's observation
   that `dynodoc://open/projects/../workspace` is not a bypass is right: the plugin parses with the `url`
   crate, which collapses dot segments before our parser sees the text, so the link is exactly the
   accepted `dynodoc://open/workspace`, and `../admin` collapses to a refused form (probe: both).

## Android: what the owner must provide for Google Play

Nothing below exists; no key, password or account was created or invented.

1. **A Google Play Console account** (one-time registration fee; identity verification; for a new
   personal account Google currently requires a closed test with a minimum number of testers before
   production: check the current text) and the app entry with the **application ID
   `online.dynodoc.client`, which cannot be changed after the first upload**.
2. **An upload keystore, created by the owner and kept offline** (JDK's `keytool`):
   `keytool -genkeypair -v -keystore dynodoc-upload.jks -alias upload -keyalg RSA -keysize 4096 -validity 9125`
   (the owner chooses and records the passwords). Back it up in two places; **never commit it**.
   Put its path and passwords in `apps/desktop/src-tauri/gen/android/keystore.properties`
   (`storeFile=`, `keyAlias=`, `storePassword=`, `keyPassword=`; the file is ignored by version control and
   `build.gradle.kts` signs the release build from it). Play App Signing is on for new apps: Google keeps
   the app signing key and re-signs; the upload key only proves uploads are yours. If the upload key is
   lost, Play support can reset it; if the app signing key were lost there is no recovery, which is why
   Play holds it.
3. **An AAB for the store** (APKs are not accepted for new apps): in `apps/desktop`, with the keystore in
   place and symbolic links allowed (Developer Mode on, or run it on Linux or macOS):
   `npm run tauri -- android build --aab --target aarch64 armv7 x86_64 i686 -- --locked`, output
   `src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab`.
   To try the bundle on a device before uploading, use Google's `bundletool` (download the jar from its
   GitHub releases page and check the published checksum): `java -jar bundletool.jar build-apks
   --bundle=app-universal-release.aab --output=app.apks --ks=dynodoc-upload.jks --ks-key-alias=upload`
   then `java -jar bundletool.jar install-apks --apks=app.apks`. **Not run here** (no keystore exists
   and the Windows PC cannot create the links the command needs); the APK flow above is what ran.
4. **App Links** to replace the custom scheme of the sign-in hand-over (see limit 1): after the first Play upload, copy the **app signing
   certificate SHA-256** from Play Console (Release > Setup > App integrity), publish
   `/.well-known/assetlinks.json` on `app.dynodoc.online` for `online.dynodoc.client`, add an
   `autoVerify` https intent filter for `/auth/verify` and `/login` to `plugins.deep-link.mobile`, and
   have the shell accept those links with the rules of `server::sign_in_url`. The manifest checker
   fails on `autoVerify` today on purpose: change it in the same commit.
5. Store listing, privacy policy URL, data-safety form and content rating, and the review risk
   described below ("Store review risk"): a shell around a website can be refused.

## Signing, notarisation and updates: what the owner must add (nothing exists)

No secret or certificate exists, and none is invented here. The workflow builds unsigned
artifacts. When the owner has the accounts below, add repository secrets (the names are suggestions to
be decided, not existing values) and extend the workflow; check each vendor's current terms.

| Need | Secrets to create later (suggested names) | Notes |
|---|---|---|
| macOS signing and notarisation | Developer ID Application certificate as a base64 `.p12` (`APPLE_CERTIFICATE`) and its password (`APPLE_CERTIFICATE_PASSWORD`); the signing identity (`APPLE_SIGNING_IDENTITY`); either an Apple ID, app-specific password and team (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`) or an App Store Connect API key (`APPLE_API_ISSUER`, `APPLE_API_KEY`, key file) | Apple Developer Program membership. Tauri reads these variables when they are set (unverified: check the current Tauri signing guide at build time). |
| iOS signing | A distribution certificate and provisioning profile, `APPLE_DEVELOPMENT_TEAM` | The same membership. Simulator builds need none. |
| Windows signing | Depends on the service chosen: a certificate file and password, or a cloud signing service's credentials | Tauri supports a custom sign command. Without it, SmartScreen warns. |
| Android release signing | An upload keystore (as base64 for CI), its password, the key alias and the key password (`gen/android/keystore.properties` locally, ignored by version control) | A Google Play Console account; creation steps, the AAB and `bundletool` commands and App Links are in "Android: what the owner must provide for Google Play". Debug APKs are signed with the SDK's debug key and cannot be published. |
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

- **Sign-in:** the emailed link and Google finish in the system browser. The shell hands the sign-in
  to the browser and takes a one-time code back (decision 017, below); "Sign in with an emailed link"
  (paste it) stays as a fallback. One open item: a crafted link to the finish page can sign a window
  in as the attacker's own account (decision 017, threat table).
- **Downloads, printing, file pickers, drag and drop, and iframes** are whatever the platform's
  web view does by default; none was exercised on desktop. On Android (emulator) a same-site https
  download of a public file goes to the system download manager (a signed-in one, and a `blob:` download
  such as the web app's exports, are refused with a message), the system file chooser opens for upload fields, and iframes behave as in a
  browser; printing and drag and drop were not tried. The web app's `frame-ancestors 'self'` means it
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
- **Frames on WebKit are unverified; on Android they were observed.** WebView2 reports only the window's own navigations
  to the shell; WebKitGTK and WKWebView also report `<iframe>` navigations without
  saying which is which. Nothing was observed there. On Android (WebView 133) three iframes added to a
  hosted page produced no call to the shell at all. The shell assumes the ambiguity everywhere but Windows
  and lets `about:srcdoc`, `about:blank`, `data:` and the server's own `blob:` addresses through (they
  are how pages fill frames); they never use up the launcher's one-time permission. Check on a real
  WebKit build before relying on it.
- **A link from another program navigates the window.** A `dynodoc://` link brings the window forward
  and takes it to that page, even if you were working in it. The protection for unsaved work is the web
  app's own leave-page warning. Checked: WebView2 runs the page's `beforeunload` handler for the shell's
  own navigation and shows its "Leave site?" dialog, and answering Stay keeps the page.
- **A pasted sign-in link signs in whichever account issued it** (the fallback; the browser sign-in
  names the account before anything is made). Only paste a link from an email you
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
3. The browser hand-over (decision 017) is built, so "Sign in with an emailed link" (paste) is now the
   fallback. Keep it? Should the web `/login` page show the account's email address before the person
   confirms a token (server work; it closes the login-CSRF gap of a pasted link)?
4. Accept the permission denial for all pages, or allow clipboard read for the web app's origin?
5. Which Windows signing route (certificate, cloud signing service, Microsoft Store)?
6. Arm runners: try `ubuntu-24.04-arm` and `windows-11-arm` on this private repository?
7. To run `desktop.yml` by hand the file must be on `main` first; merge it, then use "Run workflow".
8. **Android sign-in** (limit 1): the browser hand-over is built on the custom scheme. When the Play app
   signing key's SHA-256 is known, replace the scheme with Android App Links and an `assetlinks.json` on
   the web host? (A custom scheme can be claimed by any other app; PKCE makes what it sees useless, but
   App Links also remove the chooser and the lost-link case.)
9. **Changing the server on a phone** (limit 2): accept "sign out, then choose again" (and "clear the
   app's data" for a server that cannot be reached), or add a launcher deep link and an Android
   long-press shortcut (decision 017 weighs them; the launcher would then appear whenever any program asks)?
10. **`blob:` downloads on Android** (limit 3): the web app builds its exports as blobs. Either change the
    web app to serve them as plain https downloads (works today), or accept a script bridge in the Android
    app (a larger security decision), or leave exports to the browser.
11. Enable **Developer Mode** on the Windows PC (Settings > System > For developers) so that
    `tauri android build` and `--aab` work without `scripts/android-build.ps1`? (Not done here: system setting.)
12. The hosted cloud's root redirects to the marketing site. The shell now opens `/workspace`; is that the
    right entry for a self-hosted server too?
