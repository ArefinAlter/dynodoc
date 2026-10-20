#!/usr/bin/env node
// Black-box probe of the security model against a DEBUG build, Windows only (it drives
// WebView2 through its remote debugging port). Not part of CI. It starts the app with a
// throw-away home (DYNODOC_DESKTOP_HOME, debug builds only), points it at a local server,
// and then, from inside the web view, tries what a hostile server page would try.
//
//   node scripts/security-probe.mjs [path\to\dynodoc-desktop.exe]
//
// It opens a few tabs in the system browser on purpose: that is what a blocked-in-window
// link is supposed to do, and the probe's second server sees those requests arrive.
// Exit code 1 if any expectation fails.

import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, existsSync, writeFileSync, mkdirSync, rmSync, openSync, closeSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

if (process.platform !== 'win32') {
  console.error('security-probe: Windows only (WebView2 remote debugging).');
  process.exit(2);
}

const exe = resolve(
  process.argv[2] ??
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

function listen(label) {
  const hits = [];
  const details = [];
  const server = createServer((req, res) => {
    hits.push(req.url);
    details.push(`${req.url} dest=${req.headers["sec-fetch-dest"]} site=${req.headers["sec-fetch-site"]} referer=${req.headers.referer ?? "-"} auth=${req.headers.authorization ? "yes" : "no"} ua=${String(req.headers["user-agent"]).slice(0, 60)}`);
    if (req.url === '/favicon.ico') {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    res.end(`<!doctype html><title>${label}</title><h1>${label}</h1><p id="path">${req.url}</p>
<p><a id="same-blank" target="_blank" href="/blank-target">same-origin new window</a></p>
<p><a id="same-blank-opener" target="_blank" rel="opener" href="/blank-opener">same-origin, opener allowed</a></p>
<p><a id="ext-blank" target="_blank" rel="noopener noreferrer" href="http://127.0.0.1:${globalThis.PROBE_B_PORT}/ext-anchor">external new window</a></p>
<p><a id="ext-plain" href="http://127.0.0.1:${globalThis.PROBE_B_PORT}/ext-plain">external plain link</a></p>`);
  });
  return new Promise((ok) => server.listen(0, "127.0.0.1", () => ok({ server, hits, details, label, port: server.address().port })));
}

const home = mkdtempSync(join(tmpdir(), 'dynodoc-probe-'));
const A = await listen('server A (configured)');
const B = await listen("server B (another origin)");
globalThis.PROBE_B_PORT = B.port;
const debugPort = 9300 + Math.floor(Math.random() * 500);
mkdirSync(join(home, 'config'), { recursive: true });
writeFileSync(join(home, 'config', 'config.json'), JSON.stringify({ version: 1, server: `http://127.0.0.1:${A.port}` }));

// Debug builds print every navigation decision to standard error; keep it for the report.
const traceFile = join(tmpdir(), `dynodoc-probe-trace-${process.pid}.log`);
const traceFd = openSync(traceFile, 'w');
const app = spawn(exe, [], {
  env: {
    ...process.env,
    DYNODOC_DESKTOP_HOME: home,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
  },
  stdio: ['ignore', 'ignore', traceFd],
});
let exitCode = null;
app.on('exit', (code) => (exitCode = code));

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
  return (await targets()).filter((t) => t.url !== "about:blank");
}
async function waitForPage(test, seconds = 30) {
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    const page = (await targets()).find((t) => test(t.url));
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
  const answer = await new Promise((ok) => {
    ws.onmessage = (m) => {
      const data = JSON.parse(m.data);
      if (data.id === 1) ok(data);
    };
    ws.send(JSON.stringify({ id: 1, method, params }));
  });
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

// A real mouse click at the centre of an element (a genuine user gesture).
async function click(selector) {
  const box = await evaluate(
    `(() => { const r = document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`,
  );
  const under = await evaluate(`(document.elementFromPoint(${box.x}, ${box.y}) || {}).id || '(nothing)'`);
  console.log(`      clicking ${selector} at ${Math.round(box.x)},${Math.round(box.y)}; element there: ${under}`);
  for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) {
    await cdp('Input.dispatchMouseEvent', { type, x: box.x, y: box.y, button: 'left', clickCount: 1 });
  }
}

const invokeAll = (commands) => `(async () => {
  const call = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke;
  if (!call) return [];
  const out = [];
  for (const [name, args] of ${JSON.stringify(commands)}) {
    try { out.push({ name, ok: true, value: await call(name, args) }); }
    catch (e) { out.push({ name, ok: false, error: String(e).slice(0, 160) }); }
  }
  return out;
})()`;
const find = (list, name, nth = 0) => (list ?? []).filter((e) => e.name === name)[nth];

try {
  // ---------------------------------------------------------------- remote server page
  const first = await waitForPage((u) => u.startsWith(`http://127.0.0.1:${A.port}`));
  check(Boolean(first), 'the window opens the configured server on start-up');

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
  const sample = find(remote, 'get_state')?.error ?? '';
  console.log(`      sample refusal: ${sample}`);
  const configAfter = JSON.parse(readFileSync(join(home, 'config', 'config.json'), 'utf8'));
  check(configAfter.server === `http://127.0.0.1:${A.port}`, 'remote page could not change the saved server');
  check(!existsSync(join(home, 'data', 'device-key.json')), 'remote page could not create a device key');

  // ---------------------------------------------------------------- navigation policy
  const here = () => evaluate('location.href');
  await evaluate(`location.href = 'http://127.0.0.1:${B.port}/ext-nav'`);
  await sleep(4000);
  check(B.hits.includes('/ext-nav'), 'a link to another origin opened in the system browser', B.hits.join(' '));
  check((await here()).startsWith(`http://127.0.0.1:${A.port}`), 'the window stayed on the configured server');

  await evaluate(`window.open('http://127.0.0.1:${B.port}/ext-popup'); 0`);
  await sleep(4000);
  check(B.hits.includes('/ext-popup'), 'window.open to another origin opened in the system browser');
  check((await pages()).length === 1, 'no second window was created');

  const beforeD = B.details.length;
  for (const bad of [
    'file:///C:/Windows/win.ini',
    'javascript:document.title="pwned"',
    'data:text/html,<h1>data</h1>',
    `http://user@127.0.0.1:${B.port}/creds`,
    `http://127.0.0.1:${A.port}@127.0.0.1:${B.port}/at-trick`,
    `blob:http://127.0.0.1:${A.port}/x`,
  ]) {
    await evaluate(`location.href = ${JSON.stringify(bad)}; 0`);
    await sleep(800);
  }
  await evaluate(`window.open('javascript:document.title="pwned"'); window.open('file:///C:/Windows/win.ini'); 0`);
  await sleep(1500);
  const titleNow = await evaluate('document.title');
  // A request with Sec-Fetch-Site: none comes from the system browser (typed or opened
  // address); one with the server's page as referrer comes from the web view itself.
  const browserRequests = B.details.slice(beforeD).filter((d) => d.includes('site=none'));
  const webViewRequests = B.details.slice(beforeD).filter((d) => !d.includes('site=none'));
  check(browserRequests.length === 0, 'dropped schemes and credential URLs were not handed to the system browser', browserRequests.join(' | '));
  console.log(`      note: WebView2 itself sent ${webViewRequests.length} request(s) for the cancelled navigations: ${webViewRequests.map((d) => d.split(' ')[0]).join(' ')}`);
  check(titleNow !== 'pwned' && (await here()).startsWith(`http://127.0.0.1:${A.port}`), 'javascript:/file:/data: did not run or load in the window');

  await evaluate(`location.href = '/same-origin-page'; 0`);
  await sleep(1500);
  check(A.hits.includes('/same-origin-page') && (await here()).endsWith('/same-origin-page'), 'a same-origin link loads in the window');
  const hitsA = A.hits.length;
  await click('#same-blank');
  await sleep(2500);
  check(A.hits.slice(hitsA).includes('/blank-target') && (await pages()).length === 1, 'a real click on a same-origin target=_blank link loads it in the one window', A.hits.slice(hitsA).join(' '));
  console.log(`      pages now: ${(await targets()).map((t) => t.url).join(' , ')}`);
  await evaluate(`location.href = '/same-origin-page-2'; 0`);
  await sleep(1500);
  const hitsA2 = A.hits.length;
  await click('#same-blank-opener');
  await sleep(2500);
  check(A.hits.slice(hitsA2).includes('/blank-opener') && (await pages()).length === 1, 'a real click on a same-origin target=_blank rel=opener link loads it in the one window', A.hits.slice(hitsA2).join(' '));
  await evaluate(`location.href = '/same-origin-page-3'; 0`);
  await sleep(1500);
  const hitsB3 = B.details.length;
  await click('#ext-blank');
  await sleep(4000);
  check(B.details.slice(hitsB3).some((d) => d.startsWith('/ext-anchor') && d.includes('site=none')), 'a real click on an external target=_blank rel=noopener link opens the system browser', B.details.slice(hitsB3).join(' | '));
  check((await here()).includes('/same-origin-page-3'), 'the window stayed where it was after the external target=_blank click');
  await click('#ext-plain');
  await sleep(4000);
  check(B.details.some((d) => d.startsWith('/ext-plain') && d.includes('site=none')), 'a real click on a plain external link opens the system browser');
  check((await here()).includes('/same-origin-page-3'), 'the window stayed where it was after the plain external click');
  await evaluate(`window.open('/popup-relative', '_blank'); 0`);
  await sleep(2000);
  check(A.hits.includes('/popup-relative') && (await pages()).length === 1, 'window.open of a same-origin address loads it in the one window');

  // ---------------------------------------------------------------- permissions
  const perms = await evaluate(`(async () => {
    const r = {};
    r.geo = await new Promise((ok) => navigator.geolocation.getCurrentPosition(() => ok('granted'), (e) => ok('denied:' + e.code), { timeout: 5000 }));
    try { await navigator.mediaDevices.getUserMedia({ audio: true }); r.mic = 'granted'; } catch (e) { r.mic = e.name; }
    try { await navigator.mediaDevices.getUserMedia({ video: true }); r.cam = 'granted'; } catch (e) { r.cam = e.name; }
    try { r.notif = await Notification.requestPermission(); } catch (e) { r.notif = e.name; }
    return r;
  })()`);
  check(perms?.geo?.startsWith('denied') && perms.mic !== 'granted' && perms.cam !== 'granted' && perms.notif !== 'granted', 'location, microphone, camera and notifications are not granted', JSON.stringify(perms));

  // ---------------------------------------------------------------- the launcher
  await evaluate(`location.href = 'http://tauri.localhost/index.html'; 0`);
  const launcher = await waitForPage((u) => u.startsWith('http://tauri.localhost/'), 15);
  check(Boolean(launcher), 'the launcher origin loads in the window');
  await sleep(1500);
  const text = await evaluate('document.getElementById("about") && document.getElementById("about").textContent');
  check(typeof text === 'string' && text.includes('Dynodoc desktop') && text.includes('Tools: engine-cli'), 'launcher shows the app version and the bundled tool version', text);

  const csp = await evaluate(`(() => { try { return 'eval ran: ' + eval('1+1'); } catch (e) { return 'blocked: ' + e.name; } })()`);
  check(String(csp).startsWith('blocked'), "the launcher's CSP blocks eval", csp);
  const inline = await evaluate(`(async () => { const s = document.createElement('script'); s.textContent = 'window.__inline = 1'; document.head.appendChild(s); return String(window.__inline); })()`);
  check(inline === 'undefined', "the launcher's CSP blocks an injected inline script", inline);

  const launcherCommands = [
    ['get_state', {}],
    ['device_key_status', {}],
    ['tools_version', {}],
    ['set_server', { address: 'javascript:alert(1)' }],
    ['set_server', { address: 'http://evil.example' }],
    ['set_server', { address: `https://app.dynodoc.online@evil.example` }],
    ['open_sign_in_link', { link: 'https://evil.example/login?token=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' }],
    ['plugin:opener|open_url', { url: 'https://example.com' }],
    ['plugin:opener|open_path', { path: 'C:\\Windows\\System32\\calc.exe' }],
    ['plugin:window|close', { label: 'main' }],
    ['plugin:deep-link|register', { protocol: 'evil' }],
    ['plugin:path|resolve_directory', { directory: 13 }],
  ];
  const lres = await evaluate(invokeAll(launcherCommands));
  check(find(lres, 'get_state')?.ok === true && find(lres, 'get_state').value.server === `http://127.0.0.1:${A.port}`, 'launcher: get_state works and reports the configured server');
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
  const configNow = JSON.parse(readFileSync(join(home, 'config', 'config.json'), 'utf8'));
  check(configNow.server === `http://127.0.0.1:${A.port}`, 'the saved server is unchanged after the refused addresses');

  // The real key operations, through the real sidecar and Windows DPAPI.
  const keyOps = await evaluate(invokeAll([
    ['device_key_create', {}],
    ['device_key_status', {}],
    ['device_public_key', {}],
    ['device_key_create', {}],
  ]));
  const created = find(keyOps, 'device_key_create', 0);
  const keyFile = join(home, 'data', 'device-key.json');
  check(created?.ok === true && /^[0-9a-f]{64}$/.test(created.value), 'launcher: device_key_create makes a key and returns its public key', created?.value ?? created?.error);
  check(existsSync(keyFile), 'the key file exists in the app data folder');
  if (existsSync(keyFile)) {
    const file = JSON.parse(readFileSync(keyFile, 'utf8'));
    check(file.format === 'dynodoc-key-v1' && file.keystore?.provider === 'windows-dpapi' && file.public_key === created?.value, 'the key file is an encrypted dynodoc-key-v1 bound to Windows DPAPI with the same public key', `${file.format} ${file.keystore?.provider}`);
  }
  check(find(keyOps, 'device_key_status')?.value?.exists === true, 'launcher: device_key_status now says the key exists');
  check(find(keyOps, 'device_public_key')?.ok === true && find(keyOps, 'device_public_key').value === created?.value, 'launcher: device_public_key returns the same public key');
  check(find(keyOps, 'device_key_create', 1)?.ok === false, 'launcher: a second device_key_create is refused (never overwrites)', find(keyOps, 'device_key_create', 1)?.error);
} catch (error) {
  check(false, `probe crashed: ${error.stack ?? error}`);
} finally {
  if (exitCode === null) {
    spawnSync('taskkill', ['/PID', String(app.pid)]);
    for (let i = 0; i < 80 && exitCode === null; i += 1) await sleep(250);
  }
  check(exitCode === 0, 'the app exits cleanly (code 0) when asked to close', `exit code ${exitCode}`);
  closeSync(traceFd);
  if (exitCode === null) app.kill();
  A.server.close();
  B.server.close();
  rmSync(home, { recursive: true, force: true });
}

if (failures.length) {
  console.log(`\nrequests seen by server A:\n${A.details.join('\n')}`);
  console.log(`\nrequests seen by server B:\n${B.details.join('\n')}`);
  console.log(`\nnavigation decisions made by the app (${traceFile}):\n${readFileSync(traceFile, 'utf8')}`);
}
console.log(failures.length ? `\n${failures.length} check(s) failed.` : '\nSecurity probe passed.');
process.exit(failures.length ? 1 : 0);
