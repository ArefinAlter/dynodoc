#!/usr/bin/env node
// Black-box probe of the browser sign-in (docs/decisions/017) against a DEBUG build of the real
// shell, Windows only. Not part of CI. A stand-in server plays the Dynodoc server; the real
// shell, the real operating-system menu (its command sent as a click would) and the real single-instance forwarding of
// dynodoc:// links do the rest. What it checks:
//
//   1. a sign-in link with nothing pending changes nothing;
//   2. File > "Sign in with your browser..." (sent as the click would) asks the SYSTEM
//      browser (a tab opens in your default browser, on purpose) for
//      <server>/app-signin?challenge=..&state=..&platform=desktop, and the verifier is not in it;
//   3. a link with the wrong state changes nothing and does not cancel the attempt;
//   4. a link with the right state takes the window to <server>/app-signin/finish, a plain address;
//      once the page has loaded the shell sets its address FRAGMENT to code, state and verifier,
//      so they are in no address the web view requested: no request the server saw has them
//      (path, query or Referer), the verifier hashes to the challenge from step 2, and the
//      page the server sent read the fragment back and removed it from the address;
//   5. the same link again (a replay) does nothing;
//   6. a second attempt after the first works, and a link for the FIRST attempt's state no
//      longer does;
//   7. a page of the server cannot start the sign-in (its call to the command is refused and
//      nothing is opened), while the launcher's own button can (a real mouse click).
//
//   node scripts/signin-probe.mjs [path\to\dynodoc-desktop.exe]
//
// Build the exe first (with the web view's assets inside):
//   cargo build --locked --features tauri/custom-protocol      (in src-tauri; debug profile)
// Exit code 1 if a check fails.
import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

if (process.platform !== 'win32') {
  console.error('signin-probe: Windows only (WebView2 remote debugging, UI Automation).');
  process.exit(2);
}
const exe = resolve(
  process.argv.slice(2).find((a) => !a.startsWith('--')) ??
    join(process.env.CARGO_TARGET_DIR ?? resolve('src-tauri', 'target'), 'debug', 'dynodoc-desktop.exe'),
);
if (!existsSync(exe)) {
  console.error(`signin-probe: ${exe} not found`);
  process.exit(2);
}
if (spawnSync('tasklist', ['/FI', 'IMAGENAME eq dynodoc-desktop.exe', '/NH'], { encoding: 'utf8' }).stdout.includes('dynodoc-desktop')) {
  console.error('signin-probe: Dynodoc is already running; close it first (single-instance lock).');
  process.exit(2);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const failures = [];
function check(ok, what, detail = '') {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${what}${detail ? `  [${detail}]` : ''}`);
  if (!ok) failures.push(what);
}
const b64url = (buffer) => buffer.toString('base64url');

// ------------------------------------------------------------------ the stand-in server
const requests = [];
const reports = [];
const server = createServer((req, res) => {
  const chunks = [];
  req.on('data', (c) => chunks.push(c));
  req.on('end', () => {
    requests.push({ method: req.method, url: req.url, referer: req.headers.referer ?? '-' });
    if (req.method === 'POST' && req.url === '/report') {
      reports.push(Buffer.concat(chunks).toString('utf8'));
      res.writeHead(204).end();
      return;
    }
    if (req.url === '/favicon.ico') {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    if (req.url.startsWith('/app-signin/finish')) {
      // What the real page does: read the fragment (here: report it back, so the probe can see
      // exactly what the shell put there), remove it from the address, and read it again
      // whenever it changes (the shell sets it after the page has loaded, and the app can open
      // this page again while it is still showing, which is no page load).
      res.end(`<!doctype html><title>finish</title><p>finishing</p>
<script>
function report() {
  var hash = location.hash;
  if (!hash) return;
  history.replaceState(null, '', location.pathname);
  fetch('/report', { method: 'POST', body: hash }).catch(function () {});
}
report();
addEventListener('hashchange', report);
</script>`);
    } else if (req.url.startsWith('/app-signin')) {
      res.end('<!doctype html><title>browser sign-in</title><h1>browser sign-in page</h1>');
    } else {
      res.end(`<!doctype html><title>workspace</title><h1>workspace</h1><p id="path">${req.url}</p>`);
    }
  });
});
await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
const origin = `http://127.0.0.1:${server.address().port}`;

// ------------------------------------------------------------------ the app
const home = mkdtempSync(join(tmpdir(), 'dynodoc-signin-probe-'));
mkdirSync(join(home, 'config'), { recursive: true });
mkdirSync(join(home, 'data'), { recursive: true });
const configText = `${JSON.stringify({ version: 1, server: origin })}\n`;
writeFileSync(join(home, 'config', 'config.json'), configText);
writeFileSync(join(home, 'data', 'config.seal'), `${createHash('sha256').update(configText).digest('hex')}\n`);

const traceFile = join(tmpdir(), `dynodoc-signin-trace-${process.pid}.log`);
const traceFd = openSync(traceFile, 'w');
const debugPort = 9300 + Math.floor(Math.random() * 500);
let exitCode = null;
const app = spawn(exe, [], {
  env: { ...process.env, DYNODOC_DESKTOP_HOME: home, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}` },
  stdio: ['ignore', 'ignore', traceFd],
});
app.on('exit', (code) => {
  exitCode = code;
});

async function pages() {
  try {
    return (await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json()).filter((t) => t.type === 'page' && t.url !== 'about:blank');
  } catch {
    return [];
  }
}
async function currentUrl() {
  const [page] = await pages();
  return page?.url ?? '';
}
async function waitFor(test, seconds, what) {
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    const value = await test();
    if (value) return value;
    await sleep(250);
  }
  return null;
}

// A link from another program: the second launch of the app is forwarded to the first.
function sendLink(link) {
  spawnSync(exe, [link], { env: { ...process.env, DYNODOC_DESKTOP_HOME: home }, stdio: 'ignore', timeout: 30000 });
}

// Chooses an item of the File menu the way a click would: the window is sent the command of the
// menu item at `position` (0 = Change server..., 1 = Sign in with your browser..., ...), found
// through the operating system's own menu handle. Nothing needs the keyboard or the focus.
function clickMenu(position) {
  const script = `
$ErrorActionPreference = 'Stop'
Add-Type -Namespace Probe -Name Native -MemberDefinition '
  [DllImport("user32.dll")] public static extern System.IntPtr GetMenu(System.IntPtr h);
  [DllImport("user32.dll")] public static extern System.IntPtr GetSubMenu(System.IntPtr menu, int position);
  [DllImport("user32.dll")] public static extern uint GetMenuItemID(System.IntPtr menu, int position);
  [DllImport("user32.dll")] public static extern bool PostMessage(System.IntPtr h, uint message, System.IntPtr w, System.IntPtr l);'
$p = Get-Process -Id ${app.pid}
if ($p.MainWindowHandle -eq 0) { throw 'no window' }
$bar = [Probe.Native]::GetMenu($p.MainWindowHandle)
if ($bar -eq [System.IntPtr]::Zero) { throw 'no menu bar' }
$file = [Probe.Native]::GetSubMenu($bar, 0)
if ($file -eq [System.IntPtr]::Zero) { throw 'no File menu' }
$id = [Probe.Native]::GetMenuItemID($file, ${position})
if ($id -eq 4294967295) { throw 'no such menu item' }
[Probe.Native]::PostMessage($p.MainWindowHandle, 0x0111, [System.IntPtr][int]$id, [System.IntPtr]::Zero) | Out-Null
`;
  const run = spawnSync('powershell', ['-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8', timeout: 60000 });
  return run.status === 0 ? '' : (run.stderr || run.stdout || 'powershell failed').trim().split('\n')[0];
}

// One DevTools command over a fresh connection to the window's page.
async function cdp(method, params) {
  const [page] = await pages();
  if (!page) throw new Error('no page target');
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((ok, bad) => {
    ws.onopen = ok;
    ws.onerror = () => bad(new Error('websocket error'));
  });
  const answer = await Promise.race([
    new Promise((ok) => {
      ws.onmessage = (m) => {
        const data = JSON.parse(m.data);
        if (data.id === 1) ok(data);
      };
      ws.send(JSON.stringify({ id: 1, method, params }));
    }),
    sleep(30000).then(() => {
      throw new Error(`no answer to ${method} in 30 s`);
    }),
  ]);
  ws.close();
  return answer.result;
}
async function evaluate(expression) {
  const result = await cdp('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true, userGesture: true });
  if (result?.exceptionDetails) return { thrown: result.exceptionDetails.exception?.description ?? 'exception' };
  return result?.result?.value;
}
// A real mouse click at the centre of an element (a genuine, trusted user gesture).
async function click(selector) {
  const box = await evaluate(
    `(() => { const e = document.querySelector(${JSON.stringify(selector)}); e.scrollIntoView({ block: 'center' }); const r = e.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`,
  );
  for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) {
    await cdp('Input.dispatchMouseEvent', { type, x: box.x, y: box.y, button: 'left', clickCount: 1 });
  }
}

const hex = () => randomBytes(32).toString('hex');
const sinceMark = () => requests.length;
const finishHits = (from = 0) => requests.slice(from).filter((r) => r.url.startsWith('/app-signin/finish'));
const reportsFrom = (from = 0) => reports.slice(from);
const startHits = (from = 0) => requests.slice(from).filter((r) => r.url.startsWith('/app-signin?'));

try {
  const first = await waitFor(async () => (await currentUrl()).startsWith(origin) && (await currentUrl()), 40, 'the window on the server');
  check(Boolean(first), 'the window opens the saved server', first || '');
  await sleep(1500);

  // 1. nothing pending: a link does nothing.
  let mark = sinceMark();
  let seenReports = reports.length;
  sendLink(`dynodoc://signin?code=${hex()}&state=${b64url(randomBytes(24))}`);
  await sleep(3500);
  check(finishHits(mark).length === 0 && reports.length === seenReports, 'a sign-in link with no attempt pending does not open the finish page');
  check((await currentUrl()).startsWith(`${origin}/workspace`), 'and the window stays where it was', await currentUrl());

  // 2. the menu starts an attempt and asks the system browser.
  mark = sinceMark();
  const failed = clickMenu(1);
  check(failed === '', 'File > Sign in with your browser... can be clicked', failed);
  const start = await waitFor(async () => startHits(mark)[0], 30, 'the browser to ask for the sign-in page');
  check(Boolean(start), 'the system browser asks the server for /app-signin?...', start?.url ?? 'no request seen (is a default browser set?)');
  if (!start) throw new Error('no browser request: cannot go on');
  const query = new URL(start.url, origin).searchParams;
  const challenge = query.get('challenge');
  const state1 = query.get('state');
  check([...query.keys()].join(',') === 'challenge,state,platform', 'the address carries exactly challenge, state and platform', [...query.keys()].join(','));
  check(query.get('platform') === 'desktop', 'platform is desktop');
  check(/^[A-Za-z0-9_-]{43}$/.test(challenge ?? ''), 'the challenge is 43 base64url characters', challenge ?? '');
  check(/^[A-Za-z0-9_-]{32}$/.test(state1 ?? ''), 'the state is 32 base64url characters', state1 ?? '');

  // 3. the wrong state changes nothing and does not cancel the attempt.
  mark = sinceMark();
  seenReports = reports.length;
  sendLink(`dynodoc://signin?code=${hex()}&state=${b64url(randomBytes(24))}`);
  await sleep(3500);
  check(finishHits(mark).length === 0 && reports.length === seenReports, 'a link with the wrong state does nothing');

  // 4. the right state finishes: the fragment, and only the fragment, carries the secrets.
  mark = sinceMark();
  const code1 = hex();
  sendLink(`dynodoc://signin?code=${code1}&state=${state1}`);
  const finish = await waitFor(async () => finishHits(mark)[0], 30, 'the finish page');
  check(Boolean(finish), 'a link with the right state takes the window to /app-signin/finish', finish?.url ?? '');
  const reported = await waitFor(async () => reports.at(-1), 15, 'the page to report its fragment');
  const fragment = new URLSearchParams((reported ?? '').replace(/^#/, ''));
  check(finish?.url === '/app-signin/finish', 'the request line has no query: the secrets are not in it', finish?.url ?? '');
  check(finishHits(mark).length === 1, 'and the page was requested once, with no fragment to send (the shell set it after the load)');
  check([...fragment.keys()].join(',') === 'code,state,verifier', 'the fragment has code, state and verifier', [...fragment.keys()].join(','));
  check(fragment.get('code') === code1 && fragment.get('state') === state1, 'with the code and state of the link');
  const verifier = fragment.get('verifier') ?? '';
  check(/^[A-Za-z0-9_-]{43}$/.test(verifier), 'the verifier is 43 base64url characters');
  check(b64url(createHash('sha256').update(verifier).digest()) === challenge, 'and SHA-256(verifier) is the challenge the browser was given (RFC 7636 S256)');
  const everything = requests.map((r) => `${r.method} ${r.url} ${r.referer}`).join('\n');
  check(!everything.includes(verifier) && !everything.includes(code1), 'no request the server saw (address, Referer) holds the verifier or the code');
  const trace = readFileSync(traceFile, 'utf8');
  check(!trace.includes(verifier) && !trace.includes(code1) && !trace.includes(challenge) && !trace.includes(state1), "the app's debug log holds none of them");

  // 5. a replay does nothing.
  mark = sinceMark();
  seenReports = reports.length;
  sendLink(`dynodoc://signin?code=${code1}&state=${state1}`);
  await sleep(3500);
  check(finishHits(mark).length === 0 && reports.length === seenReports, 'the same link again (a replay) does nothing');
  check((await currentUrl()) === `${origin}/app-signin/finish`, 'and the address in the window holds no fragment any more', await currentUrl());

  // 6. a second attempt works, and the first one's state is dead.
  mark = sinceMark();
  check(clickMenu(1) === '', 'a second attempt can be started');
  const start2 = await waitFor(async () => startHits(mark)[0], 30, 'the second browser request');
  const state2 = start2 ? new URL(start2.url, origin).searchParams.get('state') : null;
  check(Boolean(state2) && state2 !== state1, 'with a state of its own', state2 ?? '');
  mark = sinceMark();
  seenReports = reports.length;
  sendLink(`dynodoc://signin?code=${hex()}&state=${state1}`);
  await sleep(3500);
  check(finishHits(mark).length === 0 && reports.length === seenReports, "the first attempt's state no longer works");
  const code2 = hex();
  sendLink(`dynodoc://signin?code=${code2}&state=${state2}`);
  // The window is still on the finish page, so only the fragment changes: no page load.
  const second = await waitFor(async () => reportsFrom(seenReports)[0], 30, 'the second finish');
  check(Boolean(second), "the second attempt's state finishes it (the open finish page reads the new fragment)");
  const again = new URLSearchParams((second ?? '').replace(/^#/, ''));
  check(again.get('code') === code2 && again.get('state') === state2, 'with its own code and state');
  const challenge2 = start2 ? new URL(start2.url, origin).searchParams.get('challenge') : '';
  check(b64url(createHash('sha256').update(again.get('verifier') ?? '').digest()) === challenge2, "and a verifier whose SHA-256 is the second attempt's challenge");
  check(again.get('verifier') !== verifier, 'which is not the first verifier');

  // 7. a page of the server cannot start it; the launcher's button can.
  mark = sinceMark();
  const refusedCall = await evaluate(`(async () => { try { await window.__TAURI_INTERNALS__.invoke('start_browser_sign_in'); return 'ALLOWED'; } catch (e) { return 'refused: ' + String(e && e.message || e).slice(0, 80); } })()`);
  await sleep(2500);
  check(String(refusedCall).startsWith('refused'), "a server page's call to start_browser_sign_in is refused", String(refusedCall));
  check(startHits(mark).length === 0, 'and nothing was opened');
  check(clickMenu(0) === '', 'File > Change server... brings up the launcher');
  const launcher = await waitFor(async () => (await currentUrl()).includes('tauri.localhost') && (await currentUrl()), 20, 'the launcher');
  check(Boolean(launcher), 'the launcher is showing', launcher || (await currentUrl()));
  await sleep(1500);
  const visible = await evaluate(`!document.getElementById('browser-signin').hidden`);
  check(visible === true, 'with the saved server, the launcher offers Sign in with your browser');
  mark = sinceMark();
  await click('#browser-signin-go');
  const viaButton = await waitFor(async () => startHits(mark)[0], 30, 'the browser to be asked by the button');
  check(Boolean(viaButton), "the launcher's button asks the system browser for /app-signin?...", viaButton?.url ?? '');
  const status = await evaluate(`document.getElementById('browser-signin-status').textContent`);
  check(/Your browser has opened/.test(String(status)), 'and tells the person what to do next', String(status).slice(0, 60));
  const buttonState = viaButton ? new URL(viaButton.url, origin).searchParams.get('state') : null;
  mark = sinceMark();
  seenReports = reports.length;
  sendLink(`dynodoc://signin?code=${hex()}&state=${buttonState}`);
  check(Boolean(await waitFor(async () => finishHits(mark)[0], 30, 'the finish from the button attempt')), 'and the link for that attempt finishes it');
} catch (error) {
  check(false, 'the probe ran to the end', String(error.message ?? error));
} finally {
  if (exitCode === null) {
    spawnSync('taskkill', ['/PID', String(app.pid)]);
    for (let i = 0; i < 40 && exitCode === null; i += 1) await sleep(250);
  }
  check(exitCode === 0, 'the app exits cleanly when asked to close', `exit code ${exitCode}`);
  if (exitCode === null) app.kill();
  server.close();
  console.log(statSync(traceFile).size > 0 ? `debug log: ${traceFile}` : '');
}
console.log(failures.length === 0 ? '\nALL PASSED' : `\n${failures.length} FAILED:\n- ${failures.join('\n- ')}`);
process.exit(failures.length === 0 ? 0 : 1);
