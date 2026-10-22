#!/usr/bin/env node
// Black-box probe of the Android debug build, on an emulator or a USB device connected to THIS
// computer (adb only ever talks to localhost). It drives the real Android System WebView
// through its debugging port (the debug build turns it on; a release build does not), taps with
// real touch events, and reads the app's decisions from logcat (debug builds log them).
//
//   node scripts/android-probe.mjs [phase ...]      (no phase = all)
//   phases: ipc links frames download upload back deeplink rotate recreate dark offline
//
// Needs ANDROID_HOME (or ANDROID_SDK_ROOT), the debug APK installed (online.dynodoc.client) and
// network access to the hosted cloud: the hosted sign-in page is the carrier for the test links
// (nothing is typed into it, nothing is signed in). It clears the app's data first, so the app
// starts on its launcher, and it changes system settings of the emulator (rotation, dark mode,
// font size, network) and restores them. Screenshots go to PROBE_SHOTS (default: the OS temp
// folder). Exit code 1 if a check fails.
import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const APP = 'online.dynodoc.client';
const CLOUD = 'https://app.dynodoc.online';
const sdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT;
if (!sdk) { console.error('set ANDROID_HOME'); process.exit(2); }
const ADB = join(sdk, 'platform-tools', process.platform === 'win32' ? 'adb.exe' : 'adb');
const shots = process.env.PROBE_SHOTS ?? join(tmpdir(), 'dynodoc-android-probe');
mkdirSync(shots, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function adb(...args) {
  const r = spawnSync(ADB, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, MSYS_NO_PATHCONV: '1' } });
  return (r.stdout ?? '').replace(/\r/g, '').trim();
}
const shell = (...a) => adb('shell', ...a);

let failed = 0;
function check(ok, what, detail = '') {
  if (!ok) failed += 1;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${what}${detail ? `  [${detail}]` : ''}`);
}
function note(text) { console.log(`NOTE  ${text}`); }
function shot(name) {
  const r = spawnSync(ADB, ['exec-out', 'screencap', '-p'], { maxBuffer: 64 * 1024 * 1024 });
  const file = join(shots, `${name}.png`);
  writeFileSync(file, r.stdout);
  return file;
}

// ---- the app -------------------------------------------------------------------------------
const pid = () => shell('pidof', APP).split(/\s+/)[0] || null;
const resumed = () => (shell('dumpsys', 'activity', 'activities').match(/topResumedActivity=ActivityRecord\{[^ ]+ [^ ]+ ([^ ]+)/) ?? [])[1] ?? '';
const logs = () => adb('logcat', '-d', '-s', 'RustStdoutStderr:I').split('\n').filter((l) => l.includes('dynodoc:')).map((l) => l.replace(/^.*dynodoc: /, ''));
const clearLogs = () => adb('logcat', '-c');
async function waitFor(fn, what, seconds = 30) {
  const end = Date.now() + seconds * 1000;
  for (;;) {
    const value = await fn().catch(() => null);
    if (value) return value;
    if (Date.now() > end) throw new Error(`timed out: ${what}`);
    await sleep(500);
  }
}

/** Connect to the WebView's debugging port. */
async function connect() {
  const p = await waitFor(async () => pid(), 'the app process');
  adb('forward', 'tcp:9222', `localabstract:webview_devtools_remote_${p}`);
  const page = await waitFor(async () => (await (await fetch('http://127.0.0.1:9222/json/list')).json()).find((x) => x.type === 'page'), 'a debuggable page');
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = () => no(new Error('the debugging port refused')); setTimeout(() => no(new Error('the debugging port did not answer')), 10000); });
  let id = 0;
  const waiting = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && waiting.has(d.id)) { waiting.get(d.id)(d); waiting.delete(d.id); } };
  const send = (method, params = {}) => new Promise((ok, no) => { const i = (id += 1); waiting.set(i, ok); ws.send(JSON.stringify({ id: i, method, params })); setTimeout(() => no(new Error(`no answer to ${method}`)), 15000); });
  return {
    // userGesture: the page sees the call as made during a tap (a script's pushState without one is skipped by Back).
    async eval(expression, userGesture = false) {
      const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true, userGesture });
      if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.text + ' ' + (r.result.exceptionDetails.exception?.description ?? ''));
      return r.result?.result?.value;
    },
    close() { ws.close(); },
  };
}

async function launch({ fresh = false } = {}) {
  if (fresh) shell('pm', 'clear', APP);
  shell('am', 'force-stop', APP);
  clearLogs();
  shell('am', 'start', '-n', `${APP}/.MainActivity`);
  await waitFor(async () => resumed().startsWith(APP), 'the app in front', 20);
  return connect();
}

/** Where the web view sits on the screen: [left, top, right, bottom] in device pixels. */
function webViewBounds() {
  shell('uiautomator', 'dump', '/sdcard/probe-ui.xml');
  const xml = shell('cat', '/sdcard/probe-ui.xml');
  const m = xml.match(/class="android\.webkit\.WebView"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
  if (!m) throw new Error('no web view in the UI hierarchy');
  return m.slice(1).map(Number);
}

/** Put a panel of test elements at fixed places and return their centres in device pixels. */
async function panel(page, html) {
  await page.eval(`(() => { document.getElementById('probe')?.remove(); const d = document.createElement('div'); d.id = 'probe';
    d.style.cssText = 'position:fixed;left:0;top:0;right:0;z-index:2147483647;background:#fff;padding:8px'; d.innerHTML = ${JSON.stringify(html)}; document.body.appendChild(d); })()`);
  const dpr = await page.eval('devicePixelRatio');
  const [left, top] = webViewBounds();
  const rects = await page.eval(`JSON.stringify([...document.querySelectorAll('#probe [id]')].map((e) => { const r = e.getBoundingClientRect(); return [e.id, r.x + r.width / 2, r.y + r.height / 2]; }))`);
  return Object.fromEntries(JSON.parse(rects).map(([name, x, y]) => [name, [Math.round(left + x * dpr), Math.round(top + y * dpr)]]));
}
const tap = ([x, y]) => shell('input', 'tap', String(x), String(y));
const here = (page) => page.eval('location.href');
const handoffs = () => logs().filter((l) => l.startsWith('hand-off '));

async function toCloud() {
  const page = await launch({ fresh: true });
  await waitFor(async () => (await here(page)).includes('tauri.localhost') && (await page.eval(`document.getElementById('about')?.textContent`)), 'the launcher');
  await page.eval(`document.getElementById('use-cloud').click()`);
  page.close();
  const next = await connect();
  await waitFor(async () => (await here(next)).startsWith(`${CLOUD}/login`) && (await next.eval('document.readyState')) === 'complete' && (await next.eval('!!document.body')), 'the hosted sign-in page', 40);
  await sleep(1500);
  return next;
}

// ---- phases ----------------------------------------------------------------------------------
const phases = {
  async ipc() {
    const page = await toCloud();
    const calls = [
      ['get_state', {}], ['set_server', { address: 'https://evil.example' }], ['open_sign_in_link', { link: 'https://x' }],
      ['tools_version', {}], ['device_key_status', {}], ['device_key_create', {}], ['device_public_key', {}],
      ['plugin:opener|open_url', { url: 'https://example.org' }], ['plugin:opener|open_path', { path: '/sdcard' }],
      ['plugin:deep-link|get_current', {}], ['plugin:deep-link|register', { protocol: 'x' }],
      ['plugin:app|version', {}], ['plugin:app|exit', {}], ['plugin:window|close', { label: 'main' }],
      ['plugin:window|set_title', { label: 'main', value: 'x' }], ['plugin:webview|eval', { label: 'main', script: '1' }],
      ['plugin:event|listen', { event: 'x', target: { kind: 'Any' }, handler: 1 }], ['plugin:path|resolve_directory', { directory: 1 }],
      ['plugin:resources|close', { rid: 1 }], ['plugin:menu|new', { kind: 'Menu', options: {} }],
    ];
    const out = JSON.parse(await page.eval(`(async () => JSON.stringify(await Promise.all(${JSON.stringify(calls)}.map(async ([c, a]) => { try { await window.__TAURI_INTERNALS__.invoke(c, a); return [c, 'RESOLVED']; } catch (e) { return [c, 'refused']; } }))))()`));
    const resolved = out.filter(([, s]) => s === 'RESOLVED').map(([c]) => c);
    check(resolved.length === 0, `a hosted page's calls to ${out.length} app and plugin commands are all refused`, resolved.join(' '));
    check((await here(page)).startsWith(CLOUD), 'and the window is still on the server');
    page.close();
  },

  async links() {
    const page = await toCloud();
    const entry = await here(page);
    const cases = {
      extBlank: ['<a id="extBlank" href="https://example.org/ext-blank" target="_blank">external, target=_blank</a>', 'https://example.org/ext-blank', 'hand-off https://example.org/ext-blank -> Opened'],
      extSelf: ['<a id="extSelf" href="https://example.com/ext-self">external, same window</a>', 'https://example.com/ext-self', 'hand-off https://example.com/ext-self -> Opened'],
    };
    for (const [name, [html, , expectLog]] of Object.entries(cases)) {
      const at = await panel(page, `<p>${html}</p>`);
      clearLogs();
      tap(at[name]);
      await sleep(3000);
      const log = logs();
      check(log.some((l) => l.startsWith(expectLog)), `tap on "${name}": the system browser is asked to open it`, log.join(' | '));
      shell('am', 'start', '-n', `${APP}/.MainActivity`);
      await sleep(1500);
      check((await here(page)) === entry, `"${name}": the window stays where it was`, await here(page));
    }
    // A mail link and a link to this device itself: a page may not open them (a person's tap cannot be told apart on Android).
    const at = await panel(page, '<p><a id="mail" href="mailto:a@example.org">mail</a></p><p><a id="local" href="http://127.0.0.1:8080/x" target="_blank">local</a></p><p><a id="credential" href="https://user@example.org/" target="_blank">credential</a></p>');
    for (const name of ['mail', 'local', 'credential']) {
      clearLogs();
      tap(at[name]);
      await sleep(2000);
      check(!logs().some((l) => /hand-off .*-> Opened/.test(l)), `tap on "${name}": nothing is opened`, logs().join(' | '));
      check((await here(page)) === entry, `"${name}": the window stays`);
    }
    note(`mail and local-address links do nothing on Android (documented): ${logs().join(' | ')}`);
    // target=_blank to the server's own page loads in the one window.
    const same = await panel(page, '<p><a id="same" href="/robots.txt" target="_blank">same site, target=_blank</a></p>');
    clearLogs();
    tap(same.same);
    await sleep(3000);
    check((await here(page).catch(() => '')).endsWith('/robots.txt'), 'target=_blank to a page of the server loads in the same window', await here(page).catch(() => ''));
    check(logs().every((l) => !l.includes('Opened')), 'and does not touch the system browser');
    shell('input', 'keyevent', 'KEYCODE_BACK');
    await sleep(1500);
    // window.open: without a tap it is refused by the web view; with a tap it follows the same rules.
    // (An inline onclick would be blocked by the site's own Content-Security-Policy.)
    const w = await panel(page, '<p><button id="open">window.open</button></p>');
    await page.eval(`document.getElementById('open').addEventListener('click', () => window.open('https://example.net/popup'))`);
    clearLogs();
    await page.eval(`window.open('https://example.net/script-popup')`);
    await sleep(2000);
    check(logs().length === 0, 'window.open from a script with no tap does nothing', logs().join(' | '));
    tap(w.open);
    await sleep(3000);
    check(logs().some((l) => l.startsWith('hand-off https://example.net/popup -> Opened')), 'window.open from a tap goes to the system browser', logs().join(' | '));
    shell('am', 'start', '-n', `${APP}/.MainActivity`);
    await sleep(1500);
    // A page that sets location in a loop: one hand-off in 3 seconds.
    clearLogs();
    await page.eval(`(() => { for (let i = 0; i < 40; i++) setTimeout(() => { location.href = 'https://example.org/loop-' + i; }, i * 50); })()`).catch(() => {});
    await sleep(4000);
    const opened = logs().filter((l) => /hand-off .*-> Opened/.test(l)).length;
    check(opened === 1, 'a page that navigates out 40 times in 2 seconds gets one hand-off', `${opened}: ${logs().filter((l) => l.startsWith('hand-off')).join(' | ')}`);
    shell('am', 'start', '-n', `${APP}/.MainActivity`);
    page.close();
  },

  async frames() {
    // WebKit-style web views may report a sub-frame's navigation to the shell (nav.rs assumes so on Android).
    const page = await toCloud();
    clearLogs();
    await page.eval(`(() => { for (const src of ['/robots.txt', 'https://example.org/in-frame', 'about:blank']) { const f = document.createElement('iframe'); f.src = src; f.style.display = 'none'; document.body.appendChild(f); } })()`);
    await sleep(5000);
    const seen = logs();
    note('log after adding three iframes (same-site, other site, about:blank): ' + (seen.join(' | ') || '(nothing)'));
    const handed = seen.filter((l) => l.includes('hand-off') && l.includes('Opened'));
    check(handed.length === 0 || handed.length === 1, 'a hidden iframe to another site opens the browser at most once (rate limited)', handed.join(' | '));
    check((await here(page)).startsWith(CLOUD), 'and the window stays on the server');
    shell('am', 'start', '-n', `${APP}/.MainActivity`);
    page.close();
  },

  async download() {
    const page = await toCloud();
    const at = await panel(page, '<p><a id="file" href="/robots.txt" download="probe-robots.txt">same-site file</a></p><p><a id="blob" href="#" download="x.csv">blob</a></p><p><a id="other" href="https://example.org/f.txt" download>other site</a></p>');
    await page.eval(`document.getElementById('blob').href = URL.createObjectURL(new Blob(['a,b'], { type: 'text/csv' }))`);
    shell('rm', '-f', '/sdcard/Download/probe-robots.txt');
    tap(at.file);
    await sleep(6000);
    const listing = shell('ls', '/sdcard/Download');
    check(/robots|probe-robots/.test(listing), 'a download of a same-site file reaches the Downloads folder', listing);
    shot('download-toast');
    clearLogs();
    tap(at.blob);
    await sleep(1500);
    const toastShot = shot('blob-download');
    note(`blob: download tapped (screenshot ${toastShot}); the app must stay put and say it is not supported`);
    check((await here(page)).startsWith(CLOUD), 'a blob: download leaves the window where it is');
    tap(at.other);
    await sleep(3000);
    check(logs().some((l) => l.startsWith('hand-off https://example.org/f.txt -> Opened')) && !/f.txt/.test(shell('ls', '/sdcard/Download')), 'a download link to another site goes to the system browser like any link out; the app fetches nothing', logs().join(' | '));
    shell('am', 'start', '-n', `${APP}/.MainActivity`);
    page.close();
  },

  async upload() {
    const page = await toCloud();
    const at = await panel(page, '<p><input id="file" type="file"></p>');
    tap(at.file);
    await sleep(4000);
    const front = resumed();
    shot('file-chooser');
    check(!front.startsWith(APP), 'tapping a file input opens the system file chooser', front);
    shell('input', 'keyevent', 'KEYCODE_BACK');
    await sleep(1500);
    check(resumed().startsWith(APP), 'and Back from the chooser returns to the app', resumed());
    page.close();
  },

  async back() {
    const page = await toCloud();
    await page.eval(`history.pushState({}, '', '/probe-a'); history.pushState({}, '', '/probe-b')`, true);
    shell('input', 'keyevent', 'KEYCODE_BACK');
    await sleep(1000);
    check((await here(page)).endsWith('/probe-a'), 'Back goes back through the pages of the site (1)', await here(page));
    shell('input', 'keyevent', 'KEYCODE_BACK');
    await sleep(1000);
    check((await here(page)).includes('/login'), 'Back goes back through the pages of the site (2)', await here(page));
    const before = pid();
    shell('input', 'keyevent', 'KEYCODE_BACK');
    await sleep(2500);
    check(!resumed().startsWith(APP), 'Back at the first page of the site sends the app to the background, as a root screen does', resumed());
    shell('am', 'start', '-n', `${APP}/.MainActivity`);
    const again = await connect();
    await waitFor(async () => (await here(again)).startsWith(`${CLOUD}/`), 'the app again', 30);
    check(pid() === before, 'and it comes back with its page, in the same process', await here(again));
    again.close();
    page.close();
  },

  async deeplink() {
    const page = await toCloud();
    const send = async (uri, extra = []) => { clearLogs(); shell('am', 'start', '-a', 'android.intent.action.VIEW', '-d', uri, ...extra); await sleep(3500); };
    await send('dynodoc://open/workspace');
    check(logs().some((l) => l.includes(`navigation ${CLOUD}/workspace`)), 'warm start: dynodoc://open/workspace opens the workspace on the configured server', logs().join(' | '));
    const id = '0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44';
    await send(`dynodoc://open/projects/${id}`);
    check(logs().some((l) => l.includes(`navigation ${CLOUD}/projects/${id}`)), 'a project link opens that project', logs().join(' | '));
    // The plugin parses the link with the url crate, which collapses dot segments BEFORE our parser sees it:
    // a link that collapses to an accepted form is that form (nothing new is reachable: ../admin is refused above).
    await send('dynodoc://open/projects/../workspace');
    check(logs().some((l) => l.includes(`navigation ${CLOUD}/workspace`)), 'dot segments are collapsed first: .../projects/../workspace is the accepted workspace link', logs().join(' | '));
    // What the manifest does not even route, and what it routes but the parser refuses.
    const refused = [
      'dynodoc://open/admin', `dynodoc://open/projects/${id}?token=abc`, `dynodoc://open/projects/${id}#x`, 'dynodoc://evil/workspace',
      'dynodoc://open/projects/../admin', 'dynodoc://open/projects/not-a-uuid', `dynodoc://open/projects/${id}%2f..`,
      'dynodoc://open/workspace/extra', `dynodoc://open/documents/${'a'.repeat(300)}`, 'dynodoc://open/projects/%00', 'https://evil.example/workspace',
    ];
    for (const uri of refused) {
      const before = await here(page).catch(() => '');
      await send(uri);
      const moved = logs().filter((l) => l.startsWith('navigation '));
      check(moved.length === 0, `hostile link does nothing: ${uri.slice(0, 70)}`, moved.join(' | '));
      void before;
    }
    // The same links sent straight to the app (a program that names the component skips the manifest's filters).
    for (const uri of ['dynodoc://evil/workspace', 'dynodoc://open/admin', `dynodoc://open/projects/${id}?token=abc`, 'dynodoc://launcher']) {
      await send(uri, ['-n', `${APP}/.MainActivity`]);
      const moved = logs().filter((l) => l.startsWith('navigation '));
      check(moved.length === 0, `named-component link does nothing: ${uri.slice(0, 70)}`, moved.join(' | '));
    }
    // Cold start.
    shell('am', 'force-stop', APP);
    clearLogs();
    shell('am', 'start', '-a', 'android.intent.action.VIEW', '-d', 'dynodoc://open/workspace');
    await sleep(8000);
    check(logs().some((l) => l.includes(`navigation ${CLOUD}/workspace`)), 'cold start: the link opens the workspace', logs().join(' | '));
    // A link the manifest does not route is not even delivered: Android starts nothing.
    shell('am', 'force-stop', APP);
    clearLogs();
    shell('am', 'start', '-a', 'android.intent.action.VIEW', '-d', 'dynodoc://open/admin');
    await sleep(5000);
    check(pid() === null && logs().length === 0, 'cold start with a link the manifest does not route: Android does not start the app at all', logs().join(' | '));
    page.close();
  },

  async rotate() {
    const page = await toCloud();
    const before = pid();
    shell('settings', 'put', 'system', 'accelerometer_rotation', '0');
    shell('settings', 'put', 'system', 'user_rotation', '1');
    await sleep(3000);
    const wide = JSON.parse(await (await connect()).eval('JSON.stringify([innerWidth, innerHeight, document.documentElement.scrollWidth])'));
    shot('landscape');
    check(wide[0] > wide[1], 'rotating to landscape changes the page size', wide.join('x'));
    check(wide[2] <= wide[0], 'no horizontal scroll in landscape', wide.join(' '));
    check(pid() === before && resumed().startsWith(APP), 'the app is not restarted by rotation');
    shell('settings', 'put', 'system', 'user_rotation', '0');
    await sleep(2500);
    shell('settings', 'put', 'system', 'accelerometer_rotation', '1');
    check(pid() === before, 'and not by rotating back');
    page.close();
  },

  async recreate() {
    // The system re-creates the activity (and the web view) for a font size change.
    let page = await launch({ fresh: true });
    await waitFor(async () => (await here(page)).includes('tauri.localhost'), 'the launcher');
    shell('settings', 'put', 'system', 'font_scale', '1.3');
    await sleep(5000);
    page.close();
    page = await connect();
    const text = await page.eval('document.body ? document.body.innerText.slice(0, 40) : "(no body)"').catch(() => '(no page)');
    shot('launcher-after-font-change');
    check(String(text).includes('Dynodoc'), 'the launcher survives the system re-creating the web view (font size change)', String(text));
    shell('settings', 'put', 'system', 'font_scale', '1.0');
    await sleep(3000);
    page.close();
  },

  async dark() {
    const page = await toCloud();
    shell('cmd', 'uimode', 'night', 'yes');
    await sleep(3000);
    const p2 = await connect();
    const dark = await p2.eval('matchMedia("(prefers-color-scheme: dark)").matches');
    shot('hosted-dark');
    check(dark === true, 'the web view follows the system dark mode', String(dark));
    shell('cmd', 'uimode', 'night', 'no');
    await sleep(2000);
    p2.close();
    page.close();
  },

  async offline() {
    const page = await toCloud();
    shell('svc', 'wifi', 'disable');
    shell('svc', 'data', 'disable');
    await sleep(3000);
    await page.eval('location.reload()').catch(() => {});
    await sleep(7000);
    const offline = await connect();
    const text = String(await offline.eval('document.body ? document.body.innerText : ""').catch(() => ''));
    const file = shot('offline');
    note(`offline screenshot: ${file}`);
    check(/Try again/.test(text) && /cannot reach/.test(text), 'offline: the person sees what happened and a Try again link, not the stock error page', text.replace(/\n+/g, ' / ').slice(0, 120));
    shell('svc', 'wifi', 'enable');
    shell('svc', 'data', 'enable');
    await sleep(6000);
    const link = JSON.parse(await offline.eval('JSON.stringify([...document.querySelectorAll("a")].map((a) => { const r = a.getBoundingClientRect(); return [a.textContent, r.x + r.width / 2, r.y + r.height / 2]; }))'));
    const tryAgain = link.find(([t]) => t === 'Try again');
    check(Boolean(tryAgain), 'the Try again link is on the page');
    if (tryAgain) {
      const dpr = await offline.eval('devicePixelRatio');
      const [left, top] = webViewBounds();
      tap([Math.round(left + tryAgain[1] * dpr), Math.round(top + tryAgain[2] * dpr)]);
      const back = await connect();
      await waitFor(async () => (await here(back)).startsWith(`${CLOUD}/login`), 'the sign-in page after Try again', 30);
      check(true, 'after the connection is back, Try again loads the page');
      back.close();
    }
    offline.close();
    page.close();
  },
};

const wanted = process.argv.slice(2);
for (const name of wanted.length > 0 ? wanted : Object.keys(phases)) {
  if (!phases[name]) { console.error(`unknown phase ${name}`); process.exit(2); }
  console.log(`\n== ${name}`);
  try { await phases[name](); } catch (error) { failed += 1; console.log(`FAIL  phase ${name} stopped: ${error.message}`); }
}
console.log(`\n${failed === 0 ? 'all checks passed' : `${failed} check(s) FAILED`}; screenshots in ${shots}`);
process.exit(failed === 0 ? 0 : 1);
