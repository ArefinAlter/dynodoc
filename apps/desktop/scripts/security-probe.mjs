#!/usr/bin/env node
// Black-box probe of the security model against a DEBUG build, Windows only (it drives
// WebView2 through its remote debugging port). Not part of CI. It starts the app with a
// throw-away home (DYNODOC_DESKTOP_HOME, debug builds only), points it at a local server,
// and then, from inside the web view, tries what a hostile server page would try.
//
//   node scripts/security-probe.mjs [--unicode-home] [path\to\dynodoc-desktop.exe]
//
// --unicode-home puts the throw-away home (and so the key file) in a folder whose name has
// an accented letter and Bengali letters.
//
// Phases, each with its own start of the app:
//   A  a saved, sealed server: what a hostile server page can and cannot do, including a page
//      that replaces Function.prototype.call/apply/bind, closest(), the link getters and
//      window.open to steal the click secret or fake a link, and the click rate limit.
//   B  a saved server whose file was edited outside the app: shown, not opened; the launcher
//      commands, real key creation, and confirming the address.
//   C  no saved server, a key on disk: the launcher does not read the key by itself.
//   D  a damaged key file: recovery text, nothing overwritten.
//
// It opens a few tabs in the system browser on purpose (a person's click on an external link
// must do that; two page-initiated public links are let through by the rate limiter), and it
// prints what it found about leaving a page that asks to be asked first. Exit code 1 if any
// expectation fails.

import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  closeSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readFileSync,
  readdirSync,
  lstatSync,
  rmdirSync,
  unlinkSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

if (process.platform !== 'win32') {
  console.error('security-probe: Windows only (WebView2 remote debugging).');
  process.exit(2);
}

const args = process.argv.slice(2);
const unicodeHome = args.includes('--unicode-home');
const exe = resolve(
  args.find((a) => !a.startsWith('--')) ??
    join(process.env.CARGO_TARGET_DIR ?? resolve('src-tauri', 'target'), 'debug', 'dynodoc-desktop.exe'),
);
if (!existsSync(exe)) {
  console.error(`security-probe: ${exe} not found (build with: npm run tauri -- build --debug --no-bundle)`);
  process.exit(2);
}
if (spawnSync('tasklist', ['/FI', 'IMAGENAME eq dynodoc-desktop.exe', '/NH'], { encoding: 'utf8' }).stdout.includes('dynodoc-desktop')) {
  console.error('security-probe: Dynodoc is already running; close it first (single-instance lock).');
  process.exit(2);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const failures = [];
function check(ok, what, detail = '') {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${what}${detail ? `  [${detail}]` : ''}`);
  if (!ok) failures.push(what);
}
const note = (text) => console.log(`      note: ${text}`);

// ------------------------------------------------------------------ the two servers
// A server page that is hostile in the way the injected click script has to withstand: it
// runs AFTER the script (which the shell injects first), replaces the built-ins the script
// could have used, and records everything that is passed to them. window.__seen is read by
// the probe; hooks keep every function's behaviour so the page works normally.
function hostilePage(port) {
  const fake = `http://127.0.0.1:${port}/fake-admin`;
  return `<!doctype html><title>hostile</title><h1>hostile</h1>
<p><button id="harmless">a harmless button</button></p>
<p><a id="victim" href="http://127.0.0.1:${port}/hostile-target">a real link to another site</a></p>
<script>
(function () {
  var seen = (window.__seen = []);
  var depth = 0;
  var origCall = Function.prototype.call;
  var origApply = Function.prototype.apply;
  var origBind = Function.prototype.bind;
  var origReflectApply = Reflect.apply;
  var FAKE = ${JSON.stringify(fake)};
  function note(kind, args) {
    if (depth > 0) return;
    depth += 1;
    try {
      var parts = [];
      for (var i = 0; i < args.length; i += 1) {
        try { parts.push(typeof args[i] === 'string' ? args[i] : String(args[i])); } catch (e) { parts.push('?'); }
      }
      seen.push(kind + ':' + parts.join(','));
    } catch (e) {} finally { depth -= 1; }
  }
  Function.prototype.call = function () { note('call', arguments); return origReflectApply(origCall, this, arguments); };
  Function.prototype.apply = function () { note('apply', arguments); return origReflectApply(origApply, this, arguments); };
  Function.prototype.bind = function () { note('bind', arguments); return origReflectApply(origBind, this, arguments); };
  Reflect.apply = function () { note('reflect-apply', arguments); return origReflectApply(origReflectApply, Reflect, arguments); };
  var fakeLink = document.createElement('a');
  fakeLink.setAttribute('href', FAKE);
  Element.prototype.closest = function () { note('closest', arguments); return fakeLink; };
  Object.defineProperty(HTMLAnchorElement.prototype, 'href', { configurable: true, get: function () { note('href', []); return FAKE; } });
  Object.defineProperty(HTMLAnchorElement.prototype, 'protocol', { configurable: true, get: function () { note('protocol', []); return 'http:'; } });
  Object.defineProperty(HTMLAnchorElement.prototype, 'origin', { configurable: true, get: function () { note('origin', []); return 'http://127.0.0.1:${port}'; } });
  Object.defineProperty(Event.prototype, 'target', { configurable: true, get: function () { note('target', []); return fakeLink; } });
  var realEncode = encodeURIComponent;
  window.encodeURIComponent = function () { note('encode', arguments); return realEncode.apply(null, arguments); };
  window.open = function () { note('open', arguments); return null; };
  window.URL = function () { note('URL', arguments); throw new Error('no'); };
  // The page's own call, to show the hooks are live.
  (function () {}).call(null, 'hook-check');
})();
</script>`;
}

function listen(label) {
  const hits = [];
  const details = [];
  const server = createServer((req, res) => {
    hits.push(req.url);
    details.push(
      `${req.url} dest=${req.headers['sec-fetch-dest']} site=${req.headers['sec-fetch-site']} referer=${req.headers.referer ?? '-'} auth=${req.headers.authorization ? 'yes' : 'no'}`,
    );
    if (req.url === '/favicon.ico') {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    if (req.url === '/hostile') {
      res.end(hostilePage(globalThis.PROBE_B_PORT));
      return;
    }
    if (req.url === '/bu-page') {
      // A page that asks to be asked before it is left (an editor with unsaved work).
      res.end(`<!doctype html><title>bu</title><p><button id="activate">edit</button></p>
<script>addEventListener('beforeunload', function (e) { navigator.sendBeacon('/bu-fired'); e.preventDefault(); e.returnValue = ''; });</script>`);
      return;
    }
    res.end(`<!doctype html><title>${label}</title><h1>${label}</h1><p id="path">${req.url}</p>
<p><a id="same-blank" target="_blank" href="/blank-target">same-origin new window</a></p>
<p><a id="same-blank-opener" target="_blank" rel="opener" href="/blank-opener">same-origin, opener allowed</a></p>
<p><a id="ext-blank" target="_blank" rel="noopener noreferrer" href="http://127.0.0.1:${globalThis.PROBE_B_PORT}/ext-anchor">external new window</a></p>
<p><a id="ext-plain" href="http://127.0.0.1:${globalThis.PROBE_B_PORT}/ext-plain">external plain link</a></p>
<p><a id="ext-synthetic" href="http://127.0.0.1:${globalThis.PROBE_B_PORT}/ext-synthetic">external, clicked by a script</a></p>`);
  });
  return new Promise((ok) =>
    server.listen(0, '127.0.0.1', () => ok({ server, hits, details, label, port: server.address().port })),
  );
}

const homePrefix = unicodeHome ? 'dynodoc-probe-caf\u00e9-\u09ac\u09be\u0982\u09b2\u09be-' : 'dynodoc-probe-';
const home = mkdtempSync(join(tmpdir(), homePrefix));
const A = await listen('server A (configured)');
const B = await listen('server B (another origin)');
globalThis.PROBE_B_PORT = B.port;
const serverA = `http://127.0.0.1:${A.port}`;
const configPath = join(home, 'config', 'config.json');
const sealPath = join(home, 'data', 'config.seal');
const keyFile = join(home, 'data', 'device-key.json');
mkdirSync(join(home, 'config'), { recursive: true });
mkdirSync(join(home, 'data'), { recursive: true });

// Node's fs.rmSync leaves a file behind in a folder whose name has Bengali letters (seen on
// Node 24 for Windows: it reports success and the file is still there), so this removes files
// with unlinkSync and folders by hand.
function removeFile(path) {
  try {
    unlinkSync(path);
  } catch (e) {
    if (e.code !== 'ENOENT') throw e;
  }
}
function removeTree(path) {
  try {
    for (const name of readdirSync(path)) {
      const child = join(path, name);
      if (lstatSync(child).isDirectory()) removeTree(child);
      else removeFile(child);
    }
    rmdirSync(path);
  } catch {
    /* best effort: a left-over temp folder is not a failure */
  }
}

// 'sealed': what the app itself would have saved; 'tampered': same address, other bytes;
// 'none': nothing saved.
function writeConfig(kind) {
  removeFile(configPath);
  removeFile(sealPath);
  if (kind === 'none') return;
  const text = `${JSON.stringify({ version: 1, server: serverA })}\n`;
  writeFileSync(configPath, text);
  writeFileSync(sealPath, `${createHash('sha256').update(text).digest('hex')}\n`);
  if (kind === 'tampered') writeFileSync(configPath, `\n${text}`);
}

// Debug builds print every decision to standard error; one file for the whole run.
const traceFile = join(tmpdir(), `dynodoc-probe-trace-${process.pid}.log`);
const traceFd = openSync(traceFile, 'w');
const traceMark = () => statSync(traceFile).size;
const traceFrom = (mark) => readFileSync(traceFile, 'utf8').slice(mark);

// ------------------------------------------------------------------ the app
let debugPort = 0;
let app = null;
let exitCode = null;

async function startApp() {
  debugPort = 9300 + Math.floor(Math.random() * 500);
  exitCode = null;
  app = spawn(exe, [], {
    env: {
      ...process.env,
      DYNODOC_DESKTOP_HOME: home,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
    },
    stdio: ['ignore', 'ignore', traceFd],
  });
  const mine = app;
  mine.on('exit', (code) => {
    if (app === mine) exitCode = code;
  });
  const page = await waitForPage(() => true, 30);
  await sleep(1500);
  return page;
}

async function stopApp(label) {
  if (exitCode === null) {
    spawnSync('taskkill', ['/PID', String(app.pid)]);
    for (let i = 0; i < 80 && exitCode === null; i += 1) await sleep(250);
  }
  check(exitCode === 0, `${label}: the app exits cleanly (code 0) when asked to close`, `exit code ${exitCode}`);
  if (exitCode === null) app.kill();
  await sleep(500);
}

async function targets() {
  try {
    const res = await fetch(`http://127.0.0.1:${debugPort}/json/list`);
    return (await res.json()).filter((t) => t.type === 'page');
  } catch {
    return [];
  }
}
// WebView2 may keep a hidden about:blank target for a refused pop-up; it is not a window.
async function pages() {
  return (await targets()).filter((t) => t.url !== 'about:blank');
}
async function waitForPage(test, seconds = 30) {
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    const page = (await pages()).find((t) => test(t.url));
    if (page) return page;
    await sleep(250);
  }
  return null;
}

// One DevTools command over a fresh connection.
async function cdp(method, params) {
  const [page] = await pages();
  if (!page) throw new Error('no page target');
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((ok, bad) => {
    ws.onopen = ok;
    ws.onerror = () => bad(new Error('websocket error'));
  });
  // A DevTools command that gets no answer in 60 seconds means the app is gone or stuck:
  // fail instead of hanging (some of these commands run the key store).
  const answer = await Promise.race([
    new Promise((ok) => {
      ws.onmessage = (m) => {
        const data = JSON.parse(m.data);
        if (data.id === 1) ok(data);
      };
      ws.send(JSON.stringify({ id: 1, method, params }));
    }),
    sleep(60000).then(() => {
      throw new Error(`no answer to ${method} in 60 s (app exited: ${exitCode !== null})`);
    }),
  ]);
  ws.close();
  return answer.result;
}

// Runtime.evaluate. The page's CSP is NOT bypassed (allowUnsafeEvalBlockedByCSP false).
async function evaluate(expression) {
  const result = await cdp('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
    allowUnsafeEvalBlockedByCSP: false,
    userGesture: true,
  });
  if (result?.exceptionDetails) return { thrown: result.exceptionDetails.exception?.description ?? 'exception' };
  return result?.result?.value;
}
const here = () => evaluate('location.href');

// A real mouse click at the centre of an element (a genuine, trusted user gesture).
async function click(selector) {
  const box = await evaluate(
    `(() => { const e = document.querySelector(${JSON.stringify(selector)}); e.scrollIntoView({ block: 'center' }); const r = e.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`,
  );
  for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) {
    await cdp('Input.dispatchMouseEvent', { type, x: box.x, y: box.y, button: 'left', clickCount: 1 });
  }
}

// Several real clicks at one element over ONE connection, so they arrive within milliseconds.
async function clickBurst(selector, times) {
  const box = await evaluate(
    `(() => { const e = document.querySelector(${JSON.stringify(selector)}); e.scrollIntoView({ block: 'center' }); const r = e.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`,
  );
  const [page] = await pages();
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((ok, bad) => {
    ws.onopen = ok;
    ws.onerror = () => bad(new Error('websocket error'));
  });
  let id = 0;
  for (let n = 0; n < times; n += 1) {
    for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) {
      id += 1;
      ws.send(JSON.stringify({ id, method: 'Input.dispatchMouseEvent', params: { type, x: box.x, y: box.y, button: 'left', clickCount: 1 } }));
    }
  }
  await sleep(800);
  ws.close();
}

const invokeAll = (commands) => `(async () => {
  const call = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke;
  if (!call) return [];
  const out = [];
  for (const [name, args] of ${JSON.stringify(commands)}) {
    try { out.push({ name, ok: true, value: await call(name, args) }); }
    catch (e) { out.push({ name, ok: false, error: String(e).slice(0, 400) }); }
  }
  return out;
})()`;
const find = (list, name, nth = 0) => (list ?? []).filter((e) => e.name === name)[nth];
const visible = (id) => evaluate(`(() => { const e = document.getElementById(${JSON.stringify(id)}); return Boolean(e) && !e.hidden; })()`);
const text = (id) => evaluate(`(document.getElementById(${JSON.stringify(id)}) || {}).textContent`);
const value = (id) => evaluate(`(document.getElementById(${JSON.stringify(id)}) || {}).value`);
const siteNone = (server, from, path) => server.details.slice(from).some((d) => d.startsWith(`${path} `) && d.includes('site=none'));
// The launcher is ready when its script has run and filled in the footer.
async function launcherReady() {
  for (let i = 0; i < 60; i += 1) {
    if (String(await text("about")).includes("Dynodoc desktop")) return;
    await sleep(250);
  }
}

try {
  // =========================================================================== phase A
  if (!process.env.PROBE_SKIP_A) {
  console.log('--- phase A: a saved, sealed server; what a hostile server page can do');
  writeConfig('sealed');
  const first = await startApp();
  check(first?.url.startsWith(serverA), 'the window opens the configured server on start-up', first?.url);

  const remoteCommands = [
    ['get_state', {}],
    ['set_server', { address: 'https://app.dynodoc.online' }],
    ['open_sign_in_link', { link: 'x' }],
    ['tools_version', {}],
    ['device_key_status', {}],
    ['device_key_create', {}],
    ['device_public_key', {}],
    ['plugin:opener|open_url', { url: 'https://example.com' }],
    ['plugin:opener|open_path', { path: 'C:\\Windows\\System32\\calc.exe' }],
    ['plugin:deep-link|get_current', {}],
    ['plugin:deep-link|register', { protocol: 'evil' }],
    ['plugin:window-state|filename', {}],
    ['plugin:event|listen', { event: 'x', target: { kind: 'Any' }, handler: 0 }],
    ['plugin:window|close', { label: 'main' }],
    ['plugin:webview|webview_close', { label: 'main' }],
    ['plugin:path|resolve_directory', { directory: 13 }],
    ['plugin:app|version', {}],
    ['definitely_not_a_command', {}],
  ];
  const remote = await evaluate(invokeAll(remoteCommands));
  check(remote?.length === remoteCommands.length, 'remote page can see the IPC bridge object (so the refusals below are real)');
  const allowed = (remote ?? []).filter((v) => v.ok).map((v) => v.name);
  check(allowed.length === 0, 'remote page: EVERY command and plugin command is refused', allowed.join(',') || `${remoteCommands.length} refused`);
  const globals = await evaluate(`({ tauri: typeof window.__TAURI__, internals: typeof window.__TAURI_INTERNALS__ })`);
  check(globals?.tauri === 'undefined', 'withGlobalTauri is off: no window.__TAURI__ namespace in a server page', JSON.stringify(globals));
  check(readFileSync(configPath, 'utf8').includes(serverA), 'remote page could not change the saved server');
  check(!existsSync(keyFile), 'remote page could not create a device key');

  // ---- a page cannot bring up the launcher (it can run key operations)
  const t1 = traceMark();
  await evaluate(`location.href = 'http://tauri.localhost/index.html'; 0`);
  await sleep(1500);
  await evaluate(`window.open('http://tauri.localhost/index.html'); 0`);
  await sleep(1500);
  check((await here()).startsWith(serverA), 'a server page that navigates to the launcher address stays on the server');
  check(/navigation http:\/\/tauri\.localhost\/index\.html -> Deny/.test(traceFrom(t1)), 'the shell refused that navigation (launcher needs the shell to ask for it)');

  // ---- links out: a page may not reach this machine, the network or the mail program
  const mB = B.details.length;
  const t2 = traceMark();
  for (const bad of [
    `http://127.0.0.1:${B.port}/ext-nav`,
    `http://localhost:${B.port}/ext-localhost`,
    'http://192.168.1.1/router',
    'http://10.0.0.1/',
    'http://169.254.169.254/latest/meta-data/',
    'http://[::ffff:127.0.0.1]/',
    'mailto:someone@example.org?subject=hello',
  ]) {
    await evaluate(`location.href = ${JSON.stringify(bad)}; 0`);
    await sleep(500);
    await evaluate(`window.open(${JSON.stringify(bad)}); 0`);
    await sleep(500);
  }
  const forged = `https://gesture.dynodoc.invalid/${'0'.repeat(32)}?u=${encodeURIComponent(`http://127.0.0.1:${B.port}/forged`)}`;
  await evaluate(`window.open(${JSON.stringify(forged)}); 0`);
  await sleep(1500);
  const traced = traceFrom(t2);
  check(!B.details.slice(mB).some((d) => d.includes('site=none')), 'a page cannot make the system browser open loopback, LAN or link-local addresses, or a forged click marker', B.details.slice(mB).join(' | '));
  check(!/hand-off/.test(traced), 'no hand-off to the system was even attempted for those', (traced.match(/hand-off.*/g) ?? []).join(' | '));
  check((await here()).startsWith(serverA) && (await pages()).length === 1, 'the window stayed on the server and no second window appeared');
  const denied = (traced.match(/navigation .* -> Deny/g) ?? []).length;
  check(denied >= 7 && !/-> (Allow|OpenExternal)/.test(traced), 'every one of the seven addresses was decided Deny by the shell', `${denied} navigations denied`);

  // ---- a page that loops to flood the browser gets one hand-off, then a slow trickle
  const t3 = traceMark();
  await evaluate(`(async () => { for (let i = 0; i < 20; i += 1) { location.href = 'https://flood' + i + '.invalid/'; await new Promise((r) => setTimeout(r, 60)); } for (let i = 0; i < 20; i += 1) { window.open('https://popup' + i + '.invalid/'); await new Promise((r) => setTimeout(r, 60)); } return 0; })()`);
  await sleep(1500);
  let flood = traceFrom(t3);
  const opened1 = (flood.match(/hand-off .* -> Opened/g) ?? []).length;
  const limited = (flood.match(/hand-off .* -> RateLimited/g) ?? []).length;
  check(opened1 === 1 && limited >= 30, '40 page-initiated public links in 2.5 s: one is handed to the browser, the rest are dropped', `opened ${opened1}, rate limited ${limited}`);
  await sleep(3200);
  const t4 = traceMark();
  await evaluate(`location.href = 'https://later.invalid/'; 0`);
  await sleep(1500);
  check(/hand-off .* -> Opened/.test(traceFrom(t4)), 'after the 3 second gap the next one is let through');
  check((await here()).startsWith(serverA), 'the window never left the server during the flood');

  // ---- dropped schemes and credential URLs
  const beforeD = B.details.length;
  for (const bad of [
    'file:///C:/Windows/win.ini',
    'javascript:document.title="pwned"',
    'data:text/html,<h1>data</h1>',
    `http://user@127.0.0.1:${B.port}/creds`,
    `http://127.0.0.1:${A.port}@127.0.0.1:${B.port}/at-trick`,
    `blob:http://127.0.0.1:${A.port}/x`,
    'about:blank',
  ]) {
    await evaluate(`location.href = ${JSON.stringify(bad)}; 0`);
    await sleep(600);
  }
  await evaluate(`window.open('javascript:document.title="pwned"'); window.open('file:///C:/Windows/win.ini'); 0`);
  await sleep(1500);
  const titleNow = await evaluate('document.title');
  // A request with Sec-Fetch-Site: none comes from the system browser; one with the server's
  // page as referrer comes from the web view itself, which WebView2 sends before the shell
  // cancels the navigation.
  const browserRequests = B.details.slice(beforeD).filter((d) => d.includes('site=none'));
  const webViewRequests = B.details.slice(beforeD).filter((d) => !d.includes('site=none'));
  check(browserRequests.length === 0, 'dropped schemes and credential URLs were not handed to the system browser', browserRequests.join(' | '));
  note(`WebView2 itself sent ${webViewRequests.length} request(s) for the cancelled navigations: ${webViewRequests.map((d) => d.split(' ')[0]).join(' ')}`);
  check(titleNow !== 'pwned' && (await here()).startsWith(serverA), 'javascript:/file:/data:/about: did not run or load in the window');

  // ---- links in the window and a person's clicks
  await evaluate(`location.href = '/same-origin-page'; 0`);
  await sleep(1500);
  check(A.hits.includes('/same-origin-page') && (await here()).endsWith('/same-origin-page'), 'a same-origin link loads in the window');
  const hitsA = A.hits.length;
  await click('#same-blank');
  await sleep(2500);
  check(A.hits.slice(hitsA).includes('/blank-target') && (await pages()).length === 1, 'a real click on a same-origin target=_blank link loads it in the one window', A.hits.slice(hitsA).join(' '));
  await evaluate(`location.href = '/same-origin-page-2'; 0`);
  await sleep(1500);
  const hitsA2 = A.hits.length;
  await click('#same-blank-opener');
  await sleep(2500);
  check(A.hits.slice(hitsA2).includes('/blank-opener') && (await pages()).length === 1, 'a real click on a same-origin target=_blank rel=opener link loads it in the one window', A.hits.slice(hitsA2).join(' '));

  await evaluate(`location.href = '/same-origin-page-3'; 0`);
  await sleep(1500);
  const mSynthetic = B.details.length;
  await evaluate(`document.getElementById('ext-synthetic').click(); 0`);
  await sleep(3000);
  check(!siteNone(B, mSynthetic, '/ext-synthetic'), 'a click made by a SCRIPT on a link to a local address is not a person\'s click: nothing opens', B.details.slice(mSynthetic).join(' | '));
  const mBlank = B.details.length;
  await click('#ext-blank');
  await sleep(4000);
  check(siteNone(B, mBlank, '/ext-anchor'), 'a real click on an external target=_blank link opens the system browser (a person may open a local address)', B.details.slice(mBlank).join(' | '));
  check((await here()).includes('/same-origin-page-3'), 'the window stayed where it was after the external target=_blank click');
  const mPlain = B.details.length;
  await click('#ext-plain');
  await sleep(4000);
  check(siteNone(B, mPlain, '/ext-plain'), 'a real click on a plain external link opens the system browser');
  check((await here()).includes('/same-origin-page-3'), 'the window stayed where it was after the plain external click');
  await evaluate(`window.open('/popup-relative', '_blank'); 0`);
  await sleep(2000);
  check(A.hits.includes('/popup-relative') && (await pages()).length === 1, 'window.open of a same-origin address loads it in the one window');

  // ---- a hostile page: it replaces the built-ins the click script could have used
  // The shell injects the click script before any page script, so the script must have kept
  // its own copy of everything it uses. This page then replaces Function.prototype.call /
  // apply / bind, Reflect.apply, Element.prototype.closest, the link and event getters,
  // encodeURIComponent, URL and window.open, records every argument, and lies about links.
  await evaluate(`location.href = '/hostile'; 0`);
  await sleep(1500);
  check((await here()).endsWith('/hostile'), 'the hostile page loaded in the window');
  const hooks = await evaluate('window.__seen');
  check(Array.isArray(hooks) && hooks.some((s) => s.includes('hook-check')), 'the hostile page\'s hooks are live (they see the page\'s own calls)');
  const mH = B.details.length;
  const tH = traceMark();
  await click('#harmless');
  await sleep(1500);
  check(
    !B.details.slice(mH).some((d) => d.startsWith('/fake-admin')) && !/hand-off/.test(traceFrom(tH)),
    'hostile page: a real click on a harmless button opens nothing, although closest(), the click target and every href getter say "link to an address of the page\'s choosing"',
    B.details.slice(mH).join(' | '),
  );
  await click('#victim');
  await sleep(3500);
  check(
    siteNone(B, mH, '/hostile-target') && !B.details.slice(mH).some((d) => d.startsWith('/fake-admin')),
    'hostile page: a real click on a real link opens THAT link in the system browser, not the address the page\'s getters claim',
    B.details.slice(mH).join(' | '),
  );
  const seenCalls = await evaluate('window.__seen');
  const leaked = (seenCalls ?? []).filter((s) => /gesture\.dynodoc\.invalid|[0-9a-f]{40}/i.test(s));
  check(Array.isArray(seenCalls) && seenCalls.length > 0 && leaked.length === 0, 'hostile page: nothing it replaced was ever handed the click marker or its secret', `${seenCalls?.length} calls recorded; leaks: ${leaked.join(' | ') || 'none'}`);
  const onGlobal = await evaluate(`Object.getOwnPropertyNames(window).filter(function (k) { try { var v = window[k]; return typeof v === 'string' && (v.indexOf('gesture.dynodoc') >= 0 || /^[0-9a-f]{64}$/.test(v)); } catch (e) { return false; } })`);
  check(Array.isArray(onGlobal) && onGlobal.length === 0, 'hostile page: the secret is not the value of any property of window', JSON.stringify(onGlobal));

  // ---- the click budget: 1 per second, apart from the page's own budget
  await sleep(1200);
  const tB = traceMark();
  await clickBurst('#victim', 6);
  await sleep(2500);
  const burst = traceFrom(tB);
  const burstOpened = (burst.match(/hand-off .* -> Opened/g) ?? []).length;
  const burstDropped = (burst.match(/hand-off .* -> RateLimited/g) ?? []).length;
  check(burstOpened === 1 && burstDropped >= 4, 'six real clicks within milliseconds: one opens, the rest hit the click budget', `opened ${burstOpened}, rate limited ${burstDropped}`);
  await sleep(1200);
  const tS = traceMark();
  await evaluate(`location.href = 'https://starve.invalid/'; 0`);
  await sleep(300);
  await click('#victim');
  await sleep(2500);
  const starve = traceFrom(tS);
  check(/hand-off https:\/\/starve\.invalid\/ -> Opened/.test(starve) && /hand-off http:\/\/127\.0\.0\.1:\d+\/hostile-target -> Opened/.test(starve), 'a page-initiated hand-off a moment earlier does not use up the click budget (separate buckets)', (starve.match(/hand-off.*/g) ?? []).join(' | '));
  await evaluate(`location.href = '/same-origin-page-3'; 0`);
  await sleep(1500);

  // ---- permissions
  const perms = await evaluate(`(async () => {
    const r = {};
    r.geo = await new Promise((ok) => navigator.geolocation.getCurrentPosition(() => ok('granted'), (e) => ok('denied:' + e.code), { timeout: 5000 }));
    try { await navigator.mediaDevices.getUserMedia({ audio: true }); r.mic = 'granted'; } catch (e) { r.mic = e.name; }
    try { await navigator.mediaDevices.getUserMedia({ video: true }); r.cam = 'granted'; } catch (e) { r.cam = e.name; }
    try { r.notif = await Notification.requestPermission(); } catch (e) { r.notif = e.name; }
    return r;
  })()`);
  check(perms?.geo?.startsWith('denied') && perms.mic !== 'granted' && perms.cam !== 'granted' && perms.notif !== 'granted', 'location, microphone, camera and notifications are not granted', JSON.stringify(perms));

  // ---- leaving a page that asks to be asked first (finding: deep link focus theft)
  await evaluate(`location.href = '/bu-page'; 0`);
  await sleep(1500);
  await click('#activate');
  const [buPage] = await pages();
  const ws = new WebSocket(buPage.webSocketDebuggerUrl);
  await new Promise((ok) => (ws.onopen = ok));
  const dialogs = [];
  ws.onmessage = (m) => {
    const d = JSON.parse(m.data);
    if (d.method === 'Page.javascriptDialogOpening') dialogs.push(d.params);
  };
  ws.send(JSON.stringify({ id: 1, method: 'Page.enable' }));
  await sleep(500);
  const mark = A.hits.length;
  spawn(exe, ['dynodoc://open/workspace'], { env: { ...process.env, DYNODOC_DESKTOP_HOME: home }, stdio: 'ignore' });
  await sleep(4000);
  const firedFirst = A.hits.slice(mark).includes('/bu-fired');
  const dialogShown = dialogs.some((d) => d.type === 'beforeunload');
  const navigatedFirst = A.hits.slice(mark).includes('/workspace');
  note(`a Rust-initiated navigation (dynodoc:// link) from a page with a beforeunload handler: handler fired=${firedFirst}, leave-page dialog shown=${dialogShown}, left the page without asking=${navigatedFirst}`);
  if (dialogShown) {
    ws.send(JSON.stringify({ id: 2, method: 'Page.handleJavaScriptDialog', params: { accept: false } }));
    await sleep(1500);
    check(!(A.hits.slice(mark).includes('/workspace')) && (await here()).endsWith('/bu-page'), 'answering "stay" in the leave-page dialog keeps the page (a link cannot discard unsaved work silently)');
  } else {
    check(false, 'a Rust-initiated navigation from a page that asked to be asked first shows the leave-page dialog', `fired=${firedFirst} navigated=${navigatedFirst}`);
  }
  ws.close();
  if (dialogShown) {
    // Leave for real, so the next steps start from a clean page.
    const ws2 = new WebSocket((await pages())[0].webSocketDebuggerUrl);
    await new Promise((ok) => (ws2.onopen = ok));
    ws2.send(JSON.stringify({ id: 1, method: 'Page.enable' }));
    ws2.onmessage = (m) => {
      if (JSON.parse(m.data).method === 'Page.javascriptDialogOpening') ws2.send(JSON.stringify({ id: 3, method: 'Page.handleJavaScriptDialog', params: { accept: true } }));
    };
    spawn(exe, ['dynodoc://open/workspace'], { env: { ...process.env, DYNODOC_DESKTOP_HOME: home }, stdio: 'ignore' });
    await sleep(4000);
    ws2.close();
  }

  await stopApp('phase A');

  } // end of phase A
  // =========================================================================== phase B
  console.log('--- phase B: the saved address was edited outside the app; the launcher');
  writeConfig('tampered');
  const aBefore = A.hits.length;
  const launcherPage = await startApp();
  check(launcherPage?.url.startsWith('http://tauri.localhost/'), 'an edited config opens the launcher, not the server', launcherPage?.url);
  await launcherReady();
  check(A.hits.length === aBefore, 'and sends the saved address no request at all', A.hits.slice(aBefore).join(' '));
  check((await visible('unconfirmed')) === true && (await text('unconfirmed-address')) === serverA, 'the launcher shows the held-back address and asks the person to confirm it', await text('unconfirmed-address'));
  let about = await text('about');
  for (let i = 0; i < 20 && !String(about).includes('Tools:'); i += 1) {
    await sleep(500);
    about = await text('about');
  }
  check(typeof about === 'string' && about.includes('Dynodoc desktop') && about.includes('Tools: engine-cli'), 'launcher shows the app version and the bundled tool version', about);

  const csp = await evaluate(`(() => { try { return 'eval ran: ' + eval('1+1'); } catch (e) { return 'blocked: ' + e.name; } })()`);
  check(String(csp).startsWith('blocked'), "the launcher's CSP blocks eval", csp);
  const inline = await evaluate(`(async () => { const s = document.createElement('script'); s.textContent = 'window.__inline = 1'; document.head.appendChild(s); return String(window.__inline); })()`);
  check(inline === 'undefined', "the launcher's CSP blocks an injected inline script", inline);
  const launcherGlobals = await evaluate(`typeof window.__TAURI__`);
  check(launcherGlobals === 'undefined', 'the launcher works without window.__TAURI__ (it uses the internal bridge)');

  const launcherCommands = [
    ['get_state', {}],
    ['device_key_status', {}],
    ['tools_version', {}],
    ['set_server', { address: 'javascript:alert(1)' }],
    ['set_server', { address: 'http://evil.example' }],
    ['set_server', { address: 'https://app.dynodoc.online@evil.example' }],
    ['open_sign_in_link', { link: 'https://evil.example/login?token=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' }],
    ['plugin:opener|open_url', { url: 'https://example.com' }],
    ['plugin:opener|open_path', { path: 'C:\\Windows\\System32\\calc.exe' }],
    ['plugin:window|close', { label: 'main' }],
    ['plugin:deep-link|register', { protocol: 'evil' }],
    ['plugin:path|resolve_directory', { directory: 13 }],
  ];
  const lres = await evaluate(invokeAll(launcherCommands));
  const state = find(lres, 'get_state');
  check(state?.ok === true && state.value.server === null && state.value.unconfirmed === serverA, 'launcher: get_state reports no open server and the held-back address', JSON.stringify(state?.value));
  check(find(lres, 'device_key_status')?.ok === true, 'launcher: device_key_status works', JSON.stringify(find(lres, 'device_key_status')?.value));
  check(find(lres, 'tools_version')?.ok === true && /^engine-cli \d/.test(find(lres, 'tools_version').value), 'launcher: tools_version runs the bundled engine-cli', find(lres, 'tools_version')?.value);
  for (const n of [0, 1, 2]) {
    const r = find(lres, 'set_server', n);
    check(r?.ok === false, `launcher: hostile server address #${n + 1} is refused`, r?.error);
  }
  check(find(lres, 'open_sign_in_link')?.ok === false, 'launcher: a sign-in link for another server is refused', find(lres, 'open_sign_in_link')?.error);
  for (const name of ['plugin:opener|open_url', 'plugin:opener|open_path', 'plugin:window|close', 'plugin:deep-link|register', 'plugin:path|resolve_directory']) {
    check(find(lres, name)?.ok === false, `launcher: ${name} is not granted even to the launcher`, find(lres, name)?.error);
  }
  check(!readFileSync(configPath, 'utf8').includes('evil'), 'the saved config is unchanged after the refused addresses');

  // The real key operations, through the real sidecar and Windows DPAPI.
  const keyOps = await evaluate(invokeAll([
    ['device_key_create', {}],
    ['device_key_status', {}],
    ['device_public_key', {}],
    ['device_key_create', {}],
  ]));
  const created = find(keyOps, 'device_key_create', 0);
  check(created?.ok === true && /^[0-9a-f]{64}$/.test(created.value), `launcher: device_key_create makes a key and returns its public key${unicodeHome ? ' (in a folder with non-ASCII letters)' : ''}`, created?.value ?? created?.error);
  check(existsSync(keyFile), 'the key file exists in the app data folder', keyFile);
  if (existsSync(keyFile)) {
    const file = JSON.parse(readFileSync(keyFile, 'utf8'));
    check(file.format === 'dynodoc-key-v1' && file.keystore?.provider === 'windows-dpapi' && file.public_key === created?.value, 'the key file is an encrypted dynodoc-key-v1 bound to Windows DPAPI with the same public key', `${file.format} ${file.keystore?.provider}`);
  }
  const status = find(keyOps, 'device_key_status')?.value;
  check(status?.exists === true && status?.damaged === false, 'launcher: device_key_status now says the key exists and looks finished', JSON.stringify(status));
  check(find(keyOps, 'device_public_key')?.ok === true && find(keyOps, 'device_public_key').value === created?.value, 'launcher: device_public_key returns the same public key');
  const second = find(keyOps, 'device_key_create', 1);
  check(second?.ok === false && /never replaced/.test(second.error), 'launcher: a second device_key_create is refused (never overwrites)', second?.error);

  // Confirm the held-back address with a real click: it opens, and is sealed again.
  const aConfirm = A.hits.length;
  await click('#confirm');
  const confirmed = await waitForPage((u) => u.startsWith(serverA), 15);
  check(Boolean(confirmed) && A.hits.slice(aConfirm).includes('/'), 'a real click on "Use this address" opens the confirmed server', confirmed?.url);
  const cfg = readFileSync(configPath);
  check(readFileSync(sealPath, 'utf8').trim() === createHash('sha256').update(cfg).digest('hex'), 'and the config is sealed again');
  await stopApp('phase B');

  // =========================================================================== phase C
  console.log('--- phase C: no saved server, a key on disk; the launcher does not read the key by itself');
  writeConfig('none');
  note('config.json exists after removal: ' + existsSync(configPath) + ', seal exists: ' + existsSync(sealPath) + ', home: ' + home);
  const c = await startApp();
  check(c?.url.startsWith('http://tauri.localhost/'), 'no saved server opens the launcher', c?.url);
  await launcherReady();
  check((await visible('key-have')) === true && (await visible('key-public')) === false && (await value('public-key')) === '', 'the launcher says a key exists but does not show or read its public half until asked');
  check((await visible('unconfirmed')) === false, 'and shows no "check the saved address" notice');
  await click('#key-show');
  for (let i = 0; i < 40 && !(await value('public-key')); i += 1) await sleep(500);
  check((await value('public-key')) === created?.value && (await visible('key-public')) === true, 'a real click on "Show the public key" reads it back and shows it', 'value ' + (await value('public-key')) + ', key-error: ' + (await text('key-error')));
  await stopApp('phase C');

  // =========================================================================== phase D
  console.log('--- phase D: a key file cut off half way');
  const whole = readFileSync(keyFile);
  const cut = whole.subarray(0, Math.floor(whole.length / 2));
  writeFileSync(keyFile, cut);
  const d = await startApp();
  check(d?.url.startsWith('http://tauri.localhost/'), 'the launcher opens', d?.url);
  await launcherReady();
  const recovery = await text('key-error');
  check(/delete this file/i.test(recovery ?? '') && (recovery ?? '').includes('device-key.json'), 'a damaged key file is explained with recovery text naming the file', recovery);
  check((await visible('key-create')) === false, 'and no "create" button is offered over it');
  const damagedOps = await evaluate(invokeAll([['device_key_create', {}], ['device_public_key', {}], ['device_key_status', {}]]));
  const dc = find(damagedOps, 'device_key_create');
  check(dc?.ok === false && /never replaced/.test(dc.error) && /delete this file/i.test(dc.error), 'device_key_create refuses and says how to start again', dc?.error);
  const dp = find(damagedOps, 'device_public_key');
  check(dp?.ok === false && /delete this file/i.test(dp.error), 'device_public_key says how to start again instead of failing silently', dp?.error);
  check(find(damagedOps, 'device_key_status')?.value?.damaged === true, 'device_key_status reports damaged');
  check(Buffer.compare(readFileSync(keyFile), cut) === 0, 'the cut-off file was not touched, replaced or deleted');
  await stopApp('phase D');
} catch (error) {
  check(false, `probe crashed: ${error.stack ?? error}`);
} finally {
  if (app && exitCode === null) {
    spawnSync('taskkill', ['/PID', String(app.pid), '/F']);
  }
  closeSync(traceFd);
  A.server.close();
  B.server.close();
  if (failures.length) {
    console.log(`\nrequests seen by server A:\n${A.details.join('\n')}`);
    console.log(`\nrequests seen by server B:\n${B.details.join('\n')}`);
    console.log(`\ndecisions made by the app (${traceFile}):\n${readFileSync(traceFile, 'utf8')}`);
  }
  removeTree(home);
}

console.log(failures.length ? `\n${failures.length} check(s) failed.` : '\nSecurity probe passed.');
process.exit(failures.length ? 1 : 0);
