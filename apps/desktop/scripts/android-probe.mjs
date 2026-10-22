#!/usr/bin/env node
// Black-box probe of the Android debug build, on an emulator or a USB device connected to THIS
// computer (adb only ever talks to localhost). It drives the real Android System WebView
// through its debugging port (the debug build turns it on; a release build does not), taps with
// real touch events, and reads the app's decisions from logcat (debug builds log them).
//
//   node scripts/android-probe.mjs [phase ...]      (no phase = all)
//   phases: ipc links frames download upload back bypass renderer deeplink rotate recreate dark offline signin
//
// The `signin` phase uses the hosted cloud only as a stand-in server for pages that do not exist
// there yet (it gets 404 pages; nothing is signed in). A local server cannot be used: the app forbids
// cleartext traffic on Android, `http://localhost` included.
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
/** What the Kotlin layer says about itself (tag Dynodoc): downloads, refusals, a dead renderer. */
const appLogs = () => adb('logcat', '-d', '-s', 'Dynodoc:V').split('\n').filter((l) => /\bDynodoc\b/.test(l)).map((l) => l.replace(/^.*?Dynodoc\s*:\s*/, ''));
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
    /** Any DevTools command (for example Page.setBypassCSP): the result object. */
    async send(method, params = {}) { return (await send(method, params)).result; },
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
  for (let attempt = 0; ; attempt += 1) {
    shell('uiautomator', 'dump', '/sdcard/probe-ui.xml');
    const xml = shell('cat', '/sdcard/probe-ui.xml');
    const m = xml.match(/class="android\.webkit\.WebView"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
    if (m) return m.slice(1).map(Number);
    // A loaded emulator shows "... isn't responding" over the app: press Wait and look again.
    const wait = xml.match(/text="Wait"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
    if (wait && attempt < 3) {
      const [x1, y1, x2, y2] = wait.slice(1).map(Number);
      shell('input', 'tap', String(Math.round((x1 + x2) / 2)), String(Math.round((y1 + y2) / 2)));
      note('dismissed an "isn\'t responding" dialog of the loaded emulator');
      continue;
    }
    throw new Error('no web view in the UI hierarchy');
  }
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
  // Since decision 017 a phone never stays on the server's sign-in page: when `/login` has loaded the
  // shell shows its launcher instead (the sign-in page cannot sign this window in). The probe phases
  // below still need a page of the hosted server to act as the "hostile page", so they take the
  // pasted-link route, which the shell leaves alone (`/login?token=...`, a made-up token that is
  // never confirmed): the launcher's open_sign_in_link command, as a person pasting a link would.
  const bounced = await waitFor(async () => {
    try {
      const p = await connect();
      const url = await here(p);
      if (url.includes('tauri.localhost') && url.endsWith('#signin') && (await p.eval(`!document.getElementById('signin').hidden`))) return p;
      p.close();
    } catch { /* the web view is changing page */ }
    return null;
  }, "the launcher, brought up by the server's sign-in page", 60);
  await bounced.eval(`window.__TAURI_INTERNALS__.invoke('open_sign_in_link', { link: ${JSON.stringify(`${CLOUD}/login?token=${'a'.repeat(40)}`)} }).then(() => 'ok')`);
  bounced.close();
  const next = await connect();
  await waitFor(async () => (await here(next)).startsWith(`${CLOUD}/login`) && (await next.eval('document.readyState')) === 'complete' && (await next.eval('!!document.body')), 'the hosted sign-in page', 40);
  await sleep(1500);
  return next;
}

/**
 * After the system re-creates the web view the window reloads the server's entry page. The probe's
 * server is not signed in, so (decision 017) the entry page leads to /login and the shell shows its
 * launcher with the browser sign-in button: either that or a page of the server is a window that is
 * not blank. Returns the address it settled on.
 */
async function settledOnSiteOrSignInScreen(page, what, seconds = 60) {
  return waitFor(async () => {
    const url = await here(page);
    const ready = (await page.eval('document.readyState')) === 'complete';
    const signIn = url.includes('tauri.localhost') && url.endsWith('#signin');
    return ready && (url.startsWith(`${CLOUD}/`) || signIn) ? url : null;
  }, what, seconds);
}

// ---- phases ----------------------------------------------------------------------------------
const phases = {
  async ipc() {
    const page = await toCloud();
    const calls = [
      ['get_state', {}], ['set_server', { address: 'https://evil.example' }], ['open_sign_in_link', { link: 'https://x' }],
      ['start_browser_sign_in', {}], ['tools_version', {}], ['device_key_status', {}], ['device_key_create', {}], ['device_public_key', {}],
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
    // One per full 3 seconds the loop lasted (a loaded emulator stretches it), never more than 5.
    const stamped = adb('logcat', '-d', '-v', 'epoch', '-s', 'RustStdoutStderr:I').split('\n')
      .filter((l) => l.includes('dynodoc: hand-off '))
      .map((l) => ({ at: Number(l.trim().split(/\s+/)[0]), opened: /-> Opened/.test(l) }));
    const opened = stamped.filter((s) => s.opened).length;
    const span = stamped.length > 1 ? stamped[stamped.length - 1].at - stamped[0].at : 0;
    check(stamped.length === 40 && opened >= 1 && opened <= Math.min(5, 1 + Math.floor(span / 3)), `a page that navigates out 40 times gets one hand-off per 3 seconds (${opened} in ${span.toFixed(1)} s) and the rest are dropped`, `${stamped.length} requests`);
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
    shell('rm', '-f', '/sdcard/Download/robots.txt', '/sdcard/Download/robots-1.txt', '/sdcard/Download/robots-2.txt');
    // A script alone cannot start a download: no touch of the page in the last 5 seconds.
    clearLogs();
    await sleep(5500);
    await page.eval(`document.getElementById('file').click()`);
    await sleep(3000);
    check(appLogs().some((l) => l.includes('download refused: no touch')) && !/robots/.test(shell('ls', '/sdcard/Download')), 'a download started by a script, with no touch, is refused', appLogs().join(' | '));
    // A tap: the file is saved, UNLESS the site has set cookies (then the app refuses: it never sends
    // cookies, and a signed-in download needs them). Both outcomes are the designed ones.
    clearLogs();
    tap(at.file);
    await sleep(6000);
    const listing = shell('ls', '/sdcard/Download');
    const said = appLogs().join(' | ');
    check((/download enqueued/.test(said) && /robots/.test(listing)) || (/the site has cookies set/.test(said) && !/robots/.test(listing)), 'a tapped download of a same-site file is saved (no cookies set) or refused because cookies are set; no cookie is ever sent', `${said}; Download/: ${listing}`);
    note(`this run: ${/download enqueued/.test(said) ? 'saved' : 'refused (cookies)'}`);
    shot('download-toast');
    // At most one per 3 seconds, however many taps.
    clearLogs();
    tap(at.file);
    tap(at.file);
    tap(at.file);
    await sleep(3000);
    check(appLogs().filter((l) => l.includes('download enqueued')).length <= 1, 'three quick taps start at most one download', appLogs().join(' | '));
    // With a cookie set for the site, a download is refused (the app never sends cookies, and a signed-in
    // download would be an HTML sign-in page without them), and nothing is fetched.
    await page.eval(`document.cookie = 'probe=1; path=/; max-age=120'`);
    shell('rm', '-f', '/sdcard/Download/robots.txt', '/sdcard/Download/robots-1.txt', '/sdcard/Download/robots-2.txt', '/sdcard/Download/robots-3.txt');
    await sleep(3500);
    clearLogs();
    tap(at.file);
    await sleep(4000);
    check(appLogs().some((l) => l.includes('the site has cookies set')) && !/robots/.test(shell('ls', '/sdcard/Download')), 'with a cookie set for the site the download is refused and nothing is fetched', appLogs().join(' | '));
    await page.eval(`document.cookie = 'probe=; path=/; max-age=0'`);
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

  async bypass() {
    // Navigations Android never reports to the shell's link rule: a form POST, and history.
    const page = await toCloud();
    check((await page.eval('history.length')) === 1, 'the launcher is not left in the history once the site has loaded', `history.length ${await page.eval('history.length')}`);
    await page.eval('history.back()');
    await sleep(2500);
    check((await here(page)).startsWith(CLOUD), 'history.back() from the first page of the site cannot bring up the launcher', await here(page));
    // A POST to another origin (the page's own Content-Security-Policy would stop it first, so it is switched off for the test).
    await page.send('Page.enable');
    await page.send('Page.setBypassCSP', { enabled: true });
    // It applies to documents created from now on: load the page again.
    await page.send('Page.reload');
    await sleep(3000);
    await waitFor(async () => (await page.eval('document.readyState')) === 'complete' && (await here(page)).startsWith(CLOUD), 'the page after the reload', 30);
    clearLogs();
    await page.eval(`(() => { const f = document.createElement('form'); f.method = 'post'; f.action = 'https://example.org/post'; f.target = '_self'; document.body.appendChild(f); f.submit(); })()`);
    await sleep(5000);
    const refused = appLogs().filter((l) => l.includes('refused a main-frame POST request'));
    check(refused.length === 1, 'a form POST to another origin is refused by the app (HTTP 403, a plain page, no request leaves the device)', appLogs().join(' | '));
    const shownNow = await connect().then(async (c) => { const t = await c.eval('document.body ? document.body.innerText : ""').catch(() => ''); c.close(); return t; });
    check(/did not open this address/.test(shownNow) && /Back to Dynodoc/.test(shownNow), 'and the person sees why, with a link back', String(shownNow).replace(/\n+/g, ' / ').slice(0, 100));
    shot('blocked-post');
    // The link back loads the site again.
    const back = await connect();
    const dpr = await back.eval('devicePixelRatio');
    const link = JSON.parse(await back.eval('JSON.stringify([...document.querySelectorAll("a")].map((a) => { const r = a.getBoundingClientRect(); return [a.textContent, r.x + r.width / 2, r.y + r.height / 2]; }))'));
    const home = link.find(([t]) => t === 'Back to Dynodoc');
    if (home) {
      const [left, top] = webViewBounds();
      tap([Math.round(left + home[1] * dpr), Math.round(top + home[2] * dpr)]);
      const again = await connect();
      await waitFor(async () => (await here(again)).startsWith(`${CLOUD}/`), 'the site after the link back', 30);
      check(true, 'the link back returns to the site');
      again.close();
    } else check(false, 'the blocked page has a link back');
    back.close();
    page.close();
  },

  async renderer() {
    // A dead web view renderer must not kill the app: it is re-created. Needs adb root (an emulator image that allows it).
    const page = await toCloud();
    const before = pid();
    if (!/restarting adbd as root|already running as root/.test(adb('root'))) {
      note('adb root is not available here: the renderer phase is skipped');
      page.close();
      return;
    }
    await sleep(3000);
    const ps = shell('ps', '-A', '-o', 'PID,NAME').split('\n').find((l) => /webview:sandboxed_process/.test(l));
    if (!ps) { check(false, 'found the web view renderer process'); page.close(); return; }
    clearLogs();
    shell('kill', '-9', ps.trim().split(/\s+/)[0]);
    await sleep(8000);
    check(appLogs().some((l) => l.includes('renderer is gone')), 'the app is told its renderer died', appLogs().join(' | '));
    check(pid() === before, 'and survives it (same process)');
    page.close();
    const again = await connect();
    const settled = await settledOnSiteOrSignInScreen(again, 'the site (or the sign-in screen) again after the renderer died', 60);
    check(true, 'the activity was re-created and the site (for a server that is not signed in: the sign-in screen) is shown again', settled);
    again.close();
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
    // The same with a server SAVED: the new web view reloads the window's first address, the launcher,
    // which is refused now; the app must go to the server's entry instead of showing a blank window.
    const cloud = await toCloud();
    clearLogs();
    shell('settings', 'put', 'system', 'font_scale', '1.3');
    await sleep(6000);
    cloud.close();
    const again = await connect();
    const settled = await settledOnSiteOrSignInScreen(again, 'the site (or the sign-in screen) after the re-creation', 60);
    check(true, 'with a server saved, a re-created web view comes back on the server (not signed in: its sign-in screen), not blank', settled);
    check(logs().some((l) => l.includes('launcher refused, server saved')), 'by way of the refused-launcher redirect', logs().join(' | '));
    shot('after-font-change-with-server');
    shell('settings', 'put', 'system', 'font_scale', '1.0');
    await sleep(3000);
    again.close();
    // A re-created activity does not replay the deep link that started it.
    const id = '0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44';
    shell('am', 'force-stop', APP);
    clearLogs();
    shell('am', 'start', '-a', 'android.intent.action.VIEW', '-d', `dynodoc://open/projects/${id}`);
    await sleep(9000);
    check(logs().some((l) => l.includes(`navigation ${CLOUD}/projects/${id}`)), 'cold start with a project link opens the project', logs().join(' | '));
    clearLogs();
    shell('settings', 'put', 'system', 'font_scale', '1.3');
    await sleep(8000);
    check(!logs().some((l) => l.includes(`/projects/${id}`)), 'and the re-created activity does not open it again', logs().join(' | '));
    shell('settings', 'put', 'system', 'font_scale', '1.0');
    await sleep(2000);
  },

  // The browser sign-in (docs/decisions/017) on the real Android shell and WebView, against the
  // hosted cloud as the stand-in server. The cloud has neither /app-signin nor /app-signin/finish
  // yet, so it answers ordinary 404 pages: nothing is signed in and no secret is involved (the
  // challenge and the state are random and the code is made up). What it shows is the app's half:
  //   - the launcher appears by itself when the server's sign-in page has loaded (a phone has no menu);
  //   - its button makes ONE attempt and asks the system browser for /app-signin?challenge=&state=;
  //   - Android's own routing of dynodoc://signin?... reaches the app (the manifest filter);
  //   - a link with nothing pending, or with the wrong state, does nothing; the right one takes the
  //     window to /app-signin/finish with code, state and verifier in the FRAGMENT only, and
  //     SHA-256(verifier) is the challenge the browser was given; a replay does nothing;
  //   - the web view names the app in its User-Agent (DynodocApp/<version> (android), appended);
  //   - the launcher shows the request's pairing code, the same four characters the server's page
  //     shows; and the shell sets the fragment only once the finish page says it is ready (the
  //     cloud's 404 page stands in for it, so the probe sets the page's ready marker itself).
  // The challenge and state are read from the browser's launch intent (dumpsys), because the app's
  // own debug log never prints a query.
  async signin() {
    /** Chrome's first run: "Use without an account" (the emulator image's Chrome starts there once). */
    async function dismissChromeFirstRun() {
      for (let attempt = 0; attempt < 6; attempt += 1) {
        if (!resumed().includes('FirstRunActivity')) return;
        shell('uiautomator', 'dump', '/sdcard/probe-ui.xml');
        const xml = shell('cat', '/sdcard/probe-ui.xml');
        const m = xml.match(/text="(?:Use without an account|No thanks|Got it|Accept[^"]*)"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
        if (m) {
          const [x1, y1, x2, y2] = m.slice(1).map(Number);
          shell('input', 'tap', String(Math.round((x1 + x2) / 2)), String(Math.round((y1 + y2) / 2)));
          note('got past a Chrome first-run screen');
        }
        await sleep(2500);
      }
    }
    const page0 = await launch({ fresh: true });
    await waitFor(async () => (await here(page0)).includes('tauri.localhost') && (await page0.eval(`document.getElementById('about')?.textContent`)), 'the launcher');
    clearLogs();
    await page0.eval(`document.getElementById('use-cloud').click()`);
    page0.close();
    // 1. The cloud sends the signed-out window to its sign-in page; the app shows its launcher there.
    const page = await waitFor(async () => {
      try {
        const p = await connect();
        const url = await here(p);
        if (url.includes('tauri.localhost') && url.endsWith('#signin') && (await p.eval(`document.getElementById('browser-signin') && !document.getElementById('browser-signin').hidden`))) return p;
        p.close();
      } catch { /* the web view is changing page */ }
      return null;
    }, "the launcher, brought up by the server's sign-in page", 60);
    check(true, 'a saved server that is not signed in: the app shows its launcher with "Sign in with your browser"');
    check(logs().some((l) => l.includes('sign-in page, showing the launcher')), 'the shell says why', logs().filter((l) => l.includes('showing the launcher')).join(' | '));
    shot('signin-launcher');

    const open = (link) => shell('am', 'start', '-a', 'android.intent.action.VIEW', '-d', `'${link}'`, APP);
    const hex = (n) => [...Array(n)].map(() => Math.floor(Math.random() * 16).toString(16)).join('');
    // 2. Nothing pending: Android routes the link to the app (the manifest filter), which does nothing.
    clearLogs();
    open(`dynodoc://signin?code=${hex(64)}&state=${'a'.repeat(32)}`);
    await sleep(3000);
    check(logs().some((l) => l.includes('sign-in link refused -> NothingPending')), 'a link with no attempt pending reaches the app through the manifest and is refused', logs().join(' | '));

    // 3. The button.
    clearLogs();
    shell('am', 'force-stop', 'com.android.chrome');
    const live = await connect();
    await live.eval(`document.getElementById('browser-signin-go').click()`);
    await sleep(2500);
    check(logs().some((l) => l.startsWith(`hand-off ${CLOUD}/app-signin?... -> Opened`)), 'the button asks the system browser to open <server>/app-signin?...', logs().join(' | '));
    // The emulator's Chrome shows its first-run screen once; get past it, and ask it what it was opened on.
    await dismissChromeFirstRun();
    adb('forward', 'tcp:9333', 'localabstract:chrome_devtools_remote');
    const tab = await waitFor(async () => (await (await fetch('http://127.0.0.1:9333/json/list')).json()).find((t) => t.url.startsWith(`${CLOUD}/app-signin?`)), 'the browser tab', 20).catch(() => null);
    check(Boolean(tab), 'the emulator\'s browser really opens <server>/app-signin?...', tab ? 'found' : 'no such tab (a Chrome first-run screen, or no browser?)');
    if (!tab) { live.close(); page.close(); return; }
    const found = [null, tab.url.slice(tab.url.indexOf('?') + 1)];
    const query = new URLSearchParams(found[1]);
    const challenge = query.get('challenge');
    const state = query.get('state');
    check([...query.keys()].join(',') === 'challenge,state,platform' && query.get('platform') === 'android', 'with challenge, state and platform=android', [...query.keys()].join(',') + ' ' + query.get('platform'));
    check(/^[A-Za-z0-9_-]{43}$/.test(challenge ?? '') && /^[A-Za-z0-9_-]{32}$/.test(state ?? ''), 'the challenge is 43 and the state 32 base64url characters');
    const { createHash: sha } = await import('node:crypto');
    const alphabet = '23456789ABCDEFGHJKLMNPQRSTUVWXYZ';
    const digest = sha('sha256').update(state ?? '').digest();
    const expectedPairing = [0, 1, 2, 3].map((i) => alphabet[digest[i] & 31]).join('');
    const shownPairing = await live.eval(`document.getElementById('browser-signin-pairing').hidden ? '' : document.getElementById('browser-signin-code').textContent`);
    check(shownPairing === expectedPairing && /^[2-9A-HJ-NP-Z]{4}$/.test(String(shownPairing)), "the launcher shows the request's pairing code (the same four characters the server's page shows)", `${shownPairing} vs ${expectedPairing}`);
    shot('signin-after-button');
    live.close();
    page.close();
    shell('am', 'start', '-n', `${APP}/.MainActivity`);
    await sleep(2000);

    // 4. A wrong state is refused and the attempt stays; the right one finishes.
    clearLogs();
    open(`dynodoc://signin?code=${hex(64)}&state=${'b'.repeat(32)}`);
    await sleep(3000);
    check(logs().some((l) => l.includes('sign-in link refused -> StateMismatch')), 'a link with the wrong state is refused and the attempt stays', logs().join(' | '));
    const code = hex(64);
    clearLogs();
    open(`dynodoc://signin?code=${code}&state=${state}`);
    const done = await waitFor(async () => {
      try {
        const p = await connect();
        const url = await here(p);
        if (url.startsWith(`${CLOUD}/app-signin/finish`)) return p;
        p.close();
      } catch { /* the web view is changing page */ }
      return null;
    }, 'the finish page', 60);
    check(Boolean(done), 'the right link takes the window to <server>/app-signin/finish');
    const agent = await done.eval('navigator.userAgent');
    check(/ DynodocApp\/\d+\.\d+\.\d+ \(android\)$/.test(String(agent)) && /Chrome\//.test(String(agent)), "the web view names the app: its own User-Agent with DynodocApp/<version> (android) at the end", String(agent).slice(-90));
    // The cloud's 404 page stands in for the finish page and never says it is ready: say so for it,
    // as the real page does when it listens for a change of its fragment. The shell waits for that.
    const early = String(await done.eval('location.hash'));
    check(early === '', 'until the page says it is ready the fragment is not set', early);
    await done.eval(`document.documentElement.setAttribute('data-finish-ready', '1')`);
    await sleep(2500);
    const href = await done.eval('location.href');
    const hash = new URLSearchParams(String(await done.eval('location.hash')).replace(/^#/, ''));
    check([...hash.keys()].join(',') === 'code,state,verifier' && hash.get('code') === code && hash.get('state') === state, 'with code, state and verifier in the address FRAGMENT', [...hash.keys()].join(','));
    check(!href.split('#')[0].includes('?') && !href.split('#')[0].includes(code), 'and nothing in the path or query', href.split('#')[0]);
    const { createHash } = await import('node:crypto');
    const verifier = hash.get('verifier') ?? '';
    check(/^[A-Za-z0-9_-]{43}$/.test(verifier) && createHash('sha256').update(verifier).digest('base64url') === challenge, "and SHA-256(verifier) is the challenge the browser was given (RFC 7636 S256)");
    const wholeLog = adb('logcat', '-d');
    const withVerifier = wholeLog.split('\n').filter((l) => l.includes(verifier));
    check(withVerifier.length === 0, 'the verifier is nowhere in the device log', withVerifier.map((l) => l.split(verifier).join('<verifier>').split(code).join('<code>').split(state).join('<state>').slice(0, 220)).join(' | '));
    check(!logs().some((l) => l.includes(code) || l.includes(state)) && !appLogs().some((l) => l.includes(code) || l.includes(state)), "and neither the code nor the state is in the app's own log lines");
    // Android's activity manager logs the address of every activity start, including the app link
    // (code and state, never the verifier). Only adb and developer tools can read that log; the
    // code is useless without the verifier. Noted, not hidden.
    note(`the system log line of the link start holds the code: ${wholeLog.includes(code)} (ActivityTaskManager, readable only through adb)`);
    shot('signin-finish');

    // 5. A replay does nothing.
    clearLogs();
    open(`dynodoc://signin?code=${code}&state=${state}`);
    await sleep(3500);
    check(logs().some((l) => l.includes('sign-in link refused -> NothingPending')), 'the same link again (a replay) finds nothing pending', logs().join(' | '));
    // 6. Forced login: from an ordinary page, a script cannot send the window to the finish page with
    //    a fragment or a query in its address (the shell's navigation rule; the finish page, the token
    //    and the bridge are the other layers). A server REDIRECT to the finish page is not tried: the
    //    hosted cloud has no endpoint that redirects there (decision 017 says so).
    await done.eval(`location.href = '/zzz-forced-login-probe'`);
    done.close();
    const ordinary = await waitFor(async () => {
      try {
        const p = await connect();
        if ((await here(p)).endsWith('/zzz-forced-login-probe')) return p;
        p.close();
      } catch { /* the web view is changing page */ }
      return null;
    }, 'an ordinary page of the cloud', 40);
    check(Boolean(ordinary), 'the window moves to an ordinary page of the server');
    const stay = String(await ordinary.eval(`window.__stay = String(Math.random())`));
    const forged = `code=${hex(64)}&state=${'c'.repeat(32)}&verifier=${'d'.repeat(43)}`;
    for (const [what, script] of [
      ['location.href with a fragment', `location.href = '/app-signin/finish#${forged}'`],
      ['location.href with a query', `location.href = '/app-signin/finish?${forged}'`],
      ['a trailing slash and a fragment', `location.href = '/app-signin/finish/#${forged}'`],
    ]) {
      clearLogs();
      await ordinary.eval(script);
      await sleep(2500);
      let kept = 'lost';
      let where = '';
      try { kept = String(await ordinary.eval('window.__stay')) === stay ? 'kept' : 'lost'; where = await here(ordinary); } catch { /* the page went away: lost */ }
      check(kept === 'kept' && where.endsWith('/zzz-forced-login-probe') && logs().some((l) => l.includes('app-signin/finish') && l.includes('-> Deny')), `a page cannot send the window to the finish page by ${what}`, `marker ${kept}, ${where}, ${logs().filter((l) => l.includes('finish')).join(' | ').slice(0, 160)}`);
    }
    ordinary.close();
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
