# Browser and device support

Date: 21 October 2026. Follows decision 016 (devices, processors, the desktop client and the
Android and iPhone apps). This page covers the web app only: which browsers it is meant for,
what is tested and how, what is only emulated, what a small screen cannot do well, how to install
the app, and what the offline behaviour is and is not.

## Supported browsers

The commitment is the last two major versions of each of these. Older versions may work; nothing
here promises it.

| Family | Where | Notes |
|---|---|---|
| Chrome | Windows, macOS, Linux, Android | Android Chrome is the phone target |
| Edge | Windows, macOS | Same engine as Chrome |
| Firefox | Windows, macOS, Linux | Cannot install the app (see "Install the app") |
| Safari | macOS, iOS, iPadOS | iPhone and iPad Safari is the Apple phone and tablet target; every browser on iOS uses the same WebKit engine |

What the code relies on, so a floor can be read off: `:has()` (Chrome 105, Firefox 121,
Safari 15.4), `color-mix()` (Chrome 111, Firefox 113, Safari 16.2), dynamic viewport units `dvh`
(Chrome 108, Firefox 101, Safari 15.4), `structuredClone` and `Array.prototype.at` (Chrome 98 and 92,
Firefox 94 and 90, Safari 15.4), streaming `fetch`, `<dialog>`, service workers. The practical floor
is therefore Chrome and Edge 111, Firefox 121, Safari and iOS 16.4 (16.4 also lets other iOS
browsers add the app to the Home Screen). These are read from the features used, not tested.

## What is tested, and how

Playwright runs the browser suite (`frontend/researcher/e2e`) against an isolated stack: a disposable
PostgreSQL, the migrated and runtime-role database, `engine-api` and the built web app with
`DEV_AUTH`. `playwright.config.ts` defines these projects:

| Project | Engine and device | Runs |
|---|---|---|
| `chromium` | Chrome for Testing 148 | the whole suite, always |
| `firefox` | Firefox 150 | with `BROWSER_MATRIX`: the whole suite locally, the `@smoke` tests in CI |
| `webkit` | the Playwright WebKit build (about Safari 26.4's engine) | same |
| `pixel` | Pixel 7: 412 x 839 viewport, touch, Android 14 Chrome user agent (Chromium) | with `BROWSER_MATRIX`: the public, workspace, projects, `browsers` and `pwa` specs |
| `iphone` | iPhone 15: 393 x 659 viewport, touch, iOS 17.5 Safari user agent (WebKit) | same |

```sh
# From frontend/researcher, with the isolated stack running (see .github/workflows/ci-frontend.yml)
node node_modules/@playwright/test/cli.js install chromium firefox webkit
BROWSER_MATRIX=full  PILOT_E2E=true PLAYWRIGHT_BASE_URL=http://127.0.0.1:4173 \
  node node_modules/@playwright/test/cli.js test --project=firefox --project=webkit
BROWSER_MATRIX=smoke PILOT_E2E=true PLAYWRIGHT_BASE_URL=http://127.0.0.1:4173 \
  node node_modules/@playwright/test/cli.js test --project=pixel --project=iphone
```

Without `BROWSER_MATRIX` only Chromium exists, which is what the two existing CI jobs run. The
`browsers` job in `.github/workflows/ci-frontend.yml` runs the tests tagged `@smoke` (public pages,
sign-in, workspace list, one document opened, saved and reopened, the projects page, a spreadsheet on a
phone, and every service worker and manifest test) on Firefox, WebKit, Pixel and iPhone. Browser downloads
are cached by Playwright version. The smoke set is 14 tests per project; on the machine used here it took about
40 to 60 seconds per project, 3.5 minutes for the four.

Harness notes, so a red run is read correctly:
- Playwright's WebKit does not apply `page.route()` mocks to a page a service worker controls, even when
  the worker lets the request through (checked: Chromium and Firefox apply them, WebKit returns the real
  server's answer). The `webkit` and `iphone` projects therefore block workers, and `pwa.spec.ts` turns
  them back on for itself. A user on Safari is not affected; no tool intercepts their requests.
- `context.setOffline()` reaches the worker's own requests on Chromium only, so the offline page is tested
  by taking the server away: the spec puts a small pass-through proxy in front of the app, loads a page
  through it, then stops the proxy. That runs on every engine, and also shows a 503 from a live server is
  passed through, not replaced by the offline page.
- Firefox and WebKit are slower under tracing. Their projects use a 15 s `expect` timeout (Chromium keeps 5 s)
  because creating a workbook takes about 3.5 s on every engine. Assertions are unchanged.
- On Windows the WebKit build can hang at browser launch or teardown after a failed test and leave
  `WebKitNetworkProcess.exe` behind. Kill leftovers before rerunning. Not seen on Linux, which is only
  unverified because CI has not run this yet.

The unit tests (`vitest`) cover the service worker's routing and caching decisions
(`src/lib/pwa/sw-policy.test.ts`), the install wording per browser (`install.test.ts`), the keyboard inset
(`src/lib/viewport.test.ts`) and the copy helper (`src/lib/clipboard.test.ts`).

## What is emulation only

Playwright's phone projects set the screen size, pixel ratio, touch input and user agent on a desktop build
of the engine. They do not run a phone's operating system. They therefore check layout, element sizes,
input types, API availability and our own code paths. They do not check, and this record does not claim:

- iOS Safari's zoom into a focused field (we check that fields are at least 16px, the rule that prevents it),
  its collapsing address bar, the home indicator and notch (safe-area insets are always 0 in emulation),
  or the on-screen keyboard covering the page (we publish its height as `--keyboard-inset`, tested as a
  pure function only);
- real touch behaviour such as long-press menus, momentum scrolling, pinch zoom, or edge-swipe back;
- WKWebView and Android System WebView, which the native shells use (decision 016 lists them as a risk);
- installing the app through each operating system's own flow, or home-screen behaviour once installed;
- performance on a phone's processor and memory.

WebKit in these runs is Playwright's own build, on Windows for local runs and Linux in CI. It is not Safari
on macOS or iOS. Real-device passes are a release task for the owner or a QA service (decision 016).

## Known limits by device class

**Phones (under about 700px wide).**
- Documents open in continuous view, which reflows to the screen. A printed page cannot be shown at a
  readable size on a phone. "Print layout" in the status bar still shows the page as it prints; you pan to
  read it.
- The editors' header uses three rows so every command stays reachable at 44px: back, title and Share; the
  document commands; then notifications, day or night, help, comments and version history. Side panels
  (comments, history, sharing) take the whole screen.
- Spreadsheets can be read, scrolled, selected and a cell's value changed in the formula bar. Typing straight
  into cells is cramped, and the sheet says so once per visit ("Spreadsheets are easier to edit on a larger
  screen"). A presentation opens and shows its slides, but editing slides on a phone was not
  exercised in these runs, and shapes are moved by dragging, which is awkward on a small screen.
- Keyboard shortcut hints are hidden; the copy, cut and paste messages mention touch.
- A spreadsheet cell is edited in place by a double tap (iOS Safari never sends `dblclick`, so the grid
  detects two taps within 600 ms) or through the formula bar.
- The toolbar rows, tab bar, ribbon and status line take most of a phone's height; with the 659px viewport of
  an iPhone 15 in Safari the sheet grid shows about two rows until the notice is dismissed. A phone-specific
  compact toolbar that hides the ribbon until a tab is tapped would help; it is not built (it would also
  change what the existing end-to-end specs can click).

**Tablets.** Under 700px they behave as a phone, above it as a desktop with touch. Touch-screen rules
(16px fields, 44px controls) follow the pointer type, not the width, so a tablet gets them at any width and a
narrow desktop window does not.

**Landscape phones.** The editor header and ribbon leave little room for the document. Content is padded for
a notch (`viewport-fit=cover` with `env(safe-area-inset-*)`) but this could not be checked without a device.

**Safari.** WebKit's tracking prevention can delete script-written storage (such as the editor's local
recovery copies) for a site not visited for seven days; it does not apply to an app added to the Home Screen
and is documented by WebKit, not tested here. The clipboard read used by Paste may show a confirmation
bubble. Firefox and Safari do not offer an install prompt.

## Install the app

The profile page has an "Install app" section that shows what applies to the browser in use.

| Where | How |
|---|---|
| Chrome or Edge on Windows, macOS, Linux | Use the install icon at the right of the address bar, the browser menu ("Install Dynodoc"), or the Install app button on the profile page |
| Chrome on Android | The Install app button on the profile page, or the menu, "Install app" or "Add to Home screen" |
| Safari on iPhone or iPad | Share, then Add to Home Screen, then Add (on iOS 16.4 and later other browsers can do this too) |
| Safari 17 or later on macOS | File, then Add to Dock |
| Firefox on Android | Menu, then Install |
| Firefox on a computer | Not possible; use another browser or keep a pinned tab |

The app opens at the workspace (`/workspace`) and has Workspace and Projects shortcuts. The manifest
(`static/site.webmanifest`) names the app, sets `display: standalone`, the colours of the app in day mode, a
192 and a 512 icon and a separate maskable 512 icon; the Home Screen icon for iOS is `apple-touch-icon.png`
(180). `scripts/gen-pwa-icons.mjs` rebuilds the icons from `static/brand/mark.svg` with the installed
Chromium. No screenshots are listed: Chrome shows them in its install dialog, but they would have to be real
captures of the signed-in app and none were made.

## What offline does and does not do

The service worker (`src/service-worker.ts`, rules in `src/lib/pwa/sw-policy.ts`) does two things only.

1. If loading a page fails because the network is missing or the server cannot be reached, it shows a small
   "You are offline" page with a Try again button instead of the browser's error page. A reply from the
   server, including an error such as 503, is shown as sent.
2. It keeps the content-hashed JavaScript and CSS files (`/_app/immutable/`) and the self-hosted fonts after
   they are first used, so repeat visits start faster.

Cache policy in five lines:

1. Installed at install time: only `/offline.html`, `/icon-192.png` and `/favicon.svg`.
2. Stored while running: only complete (200), same-origin, public files under `/_app/immutable/` and `/fonts/`; never
   a response with `Set-Cookie`, `Cache-Control: private` or `no-store`, `Vary: *` or an HTML type.
3. Never touched at all (the browser handles them as if there were no worker): anything that is not a GET, the
   API and proxy (`/api`), sign-in (`/auth`), `/admin`, the Office panes (`/connectors`, `/integrations`),
   shared links (`/view`), printing, telemetry, `/healthz`, event streams, range requests and requests with an
   `Authorization` header.
4. No page, signed-in or not, is ever stored.
5. Caches are named by build and every other `dynodoc-` cache is deleted when a new worker activates.

It does not make documents available offline, does not queue edits, does not store anything from your account
on the device and does not change how sign-in cookies work. An editor holding unsaved work keeps it on screen
while offline, but saving needs the server; the existing browser recovery copies are unchanged.

**Updates.** A new worker installs in the background and waits. The page then shows a notice, "A new version
of Dynodoc is ready. Updating reloads this page." with Update now and Not now. Nothing reloads until Update now
is pressed, and that reload goes through the normal leave-page warning, so an editor with unsaved changes still
asks first. Without a choice, the new version takes over the next time every window has been closed.

**Hosting requirement.** A service worker runs under the Content-Security-Policy of its own script response.
The Nginx rule for the app host adds a default policy with no `connect-src` to responses that have none, and the
worker could not fetch anything under it. `infra/demo/nginx-app.conf` therefore gives `/service-worker.js`
`connect-src 'self'` and `/offline.html` a policy that allows its inline styles, and
`frontend/researcher/scripts/check-proxy-headers.mjs` asserts both. A different front end must do the same.
The page policy gained `manifest-src 'self'` (same origin only); without it a browser refuses to read the
manifest under `default-src 'none'` and the app can never be installed.

## Checked and not checked

Run for this record on 21 October 2026, on Windows 11 against the isolated stack, with Chromium 148, Firefox 150 and
the Playwright WebKit 26.4 build. The full suite (123 tests) on Chromium, the same suite on Firefox and WebKit, and the
public, workspace and projects specs on both phone projects.

Known failures that remain on non-Chromium engines, none seen on Chromium:

- WebKit, `changes.spec.ts` "teammates push edited Word files...": fails in every run so far, at a different step each
  time. One cause was a real defect and is fixed (see below); the others are not explained: the owner's open editor
  not showing a merged paragraph within 15 s, text typed into a personal draft not raising an unsaved state, and the
  rebase dialog. The same flow in single-user probes works on WebKit. Treat WebKit's multi-user editing as unverified.
- WebKit, `office.spec.ts` "live pages reflow long editable paragraphs...": one line sits 1/16 px (2292.98 against a
  boundary at 2293.04) before a page boundary. There is no visible effect; the planner asks for a pixel of clearance
  that WebKit's layout does not reflect.
- Firefox: three tests failed once in the 24-minute full run (`changes.spec.ts` "teammates push...", `office.spec.ts`
  "conditional formatting...", `product.spec.ts` "administrator previews and permanently erases...") and pass when run
  alone (`changes.spec.ts` three times in a row). They look like timing under load, for example a fill that arrives while the
  editor is still read-only.

Found by this work and fixed, beyond layout:

- The page policy blocked `/site.webmanifest` (`default-src 'none'` with no `manifest-src`); WebKit logged it. The app could
  never have been installed.
- When another person's change arrived while someone was typing, the editor replaced the whole document and the caret
  jumped to the end of the last paragraph, so the rest of the typing went there (seen on WebKit).
  `RichEditor.svelte` now maps the caret across the change (`src/lib/workspace/selection-map.ts`).
- iOS Safari never sends `dblclick`, so a spreadsheet cell could not be edited by double tap.
- Below 700px comments, version history and help were hidden with no other way to reach them.
- Text in some flex rows was squeezed until labels overlapped (a side effect of the first touch rule, caught by the
  projects spec and now covered by its own test).

Not verified: any real phone or tablet, Safari itself, WKWebView, Android WebView, installing on any operating system, the
real iOS keyboard, zoom, toolbar and notch behaviour, `-webkit-` prefixes and `100dvh` fallbacks (added by inspection;
Playwright's WebKit build is new enough not to need them), and the full `check-proxy-headers.mjs` script (it needs
Linux and Nginx). Its two new assertions were exercised another way: the Nginx rules were run in an nginx 1.27 container
in front of the app, where the worker installs and works with the entries and cannot install without them (the
page keeps working, with no offline page).
