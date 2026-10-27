// "Web workspace": the configured server's pages open in a window of their own, with the system's
// frame, the server's title and no native access at all. Run through run.mjs on a scratch home:
//   CDP_PORT=9481 node run.mjs D:/verne-build/dynodoc/evidence/frame/home webwin.mjs --no-wait
// A stand-in server on loopback serves one page that tries every command it can think of.
import { createServer } from 'node:http';
import { spawnSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';

const PORT = Number(process.env.WEB_PORT || 4187);
const PY = process.env.PYTHON || 'E:/Python310/python.exe';
const OUT = process.env.FRAME_OUT || 'D:/verne-build/dynodoc/evidence/frame';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const PAGE = `<!doctype html><html><head><meta charset="utf-8"><title>Stand-in server page</title></head><body style="font-family:sans-serif;padding:24px">
<h1>Stand-in server page</h1><p>This page is not the app. It tries to call the app and the window; the results are in window.__out.</p><pre id="r">working...</pre>
<script>
(async () => {
  const out = { internals: typeof window.__TAURI_INTERNALS__, global: typeof window.__TAURI__ };
  const attempt = async (name, cmd, args) => {
    try { const r = await window.__TAURI_INTERNALS__.invoke(cmd, args); out[name] = 'ALLOWED ' + JSON.stringify(r).slice(0, 80); }
    catch (e) { out[name] = 'refused: ' + String((e && e.message) || e).slice(0, 110); }
  };
  if (window.__TAURI_INTERNALS__) {
    await attempt('get_state', 'get_state');
    await attempt('prefs_load', 'prefs_load');
    await attempt('device_key_create', 'device_key_create');
    await attempt('window_reload', 'window_reload');
    await attempt('devtools_open', 'devtools_open');
    await attempt('window close', 'plugin:window|close');
    await attempt('window start_dragging', 'plugin:window|start_dragging');
    await attempt('window internal_toggle_maximize', 'plugin:window|internal_toggle_maximize');
    await attempt('window minimize', 'plugin:window|minimize');
    await attempt('window set_title', 'plugin:window|set_title', { value: 'taken over' });
    await attempt('window is_maximized', 'plugin:window|is_maximized');
    await attempt('window set_size', 'plugin:window|set_size', { value: { type: 'Logical', data: { width: 300, height: 200 } } });
    await attempt('event listen', 'plugin:event|listen', { event: 'x', target: { kind: 'Any' }, handler: 1 });
  }
  window.__out = out;
  document.getElementById('r').textContent = JSON.stringify(out, null, 1);
})();
</script></body></html>`;

async function targets(port) {
  const r = await fetch(`http://127.0.0.1:${port}/json`);
  return r.json();
}

async function cdp(target) {
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0;
  const waiting = new Map();
  ws.onmessage = (m) => {
    const d = JSON.parse(m.data);
    if (d.id && waiting.has(d.id)) { const w = waiting.get(d.id); waiting.delete(d.id); d.error ? w.rej(new Error(d.error.message)) : w.res(d.result); }
  };
  const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; waiting.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
  return {
    send,
    async eval(expr) {
      const r = await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
      return r.result.value;
    },
    close() { try { ws.close(); } catch {} },
  };
}

export default async function (page, { sleep: wait }) {
  await page.waitFor("!!document.querySelector('#profile-name') || (!!document.querySelector('.topbar') && !document.querySelector('.splash'))", 300000, 'the app to start');
  if (await page.has('#profile-name')) { await page.type('#profile-name', 'Priya Raman'); await page.click('#profile-go'); }
  await page.waitFor("!!document.querySelector('.topbar') && !document.querySelector('.splash')", 300000, 'the workspace');
  mkdirSync(OUT, { recursive: true });
  const server = createServer((req, res) => { res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }); res.end(PAGE); });
  await new Promise((r) => server.listen(PORT, '127.0.0.1', r));
  const results = {};
  try {
    const port = Number(process.env.CDP_PORT || 9333);
    const before = (await targets(port)).filter((t) => t.type === 'page').length;
    // The client asks for the web workspace of this server (the pill's own command).
    const opened = await page.invoke('set_server', { address: `http://127.0.0.1:${PORT}` });
    console.log('set_server ->', opened);
    let web;
    for (let i = 0; i < 60 && !web; i++) {
      await wait(500);
      web = (await targets(port)).find((t) => t.type === 'page' && t.url.startsWith(`http://127.0.0.1:${PORT}`));
    }
    if (!web) throw new Error('the web window did not open');
    const pages = (await targets(port)).filter((t) => t.type === 'page');
    results.windows = { before, after: pages.length, urls: pages.map((t) => t.url.replace(/\?.*/, '')) };
    const w = await cdp(web);
    await w.send('Runtime.enable');
    for (let i = 0; i < 40; i++) { if (await w.eval('!!window.__out')) break; await wait(300); }
    results.fromTheServersPage = await w.eval('window.__out');
    results.title = (await targets(port)).find((t) => t.url.startsWith(`http://127.0.0.1:${PORT}`))?.title;
    // The client window is still the client, and it still works.
    results.clientStill = await page.eval(`JSON.stringify({ frameless: document.documentElement.classList.contains('frameless'), controls: !!document.querySelector('#frame'), maximized: (await_ = null, null) })`).catch(() => 'n/a');
    results.clientCanCallWindow = await page.eval(`window.__TAURI_INTERNALS__.invoke('plugin:window|is_maximized').then((v) => 'is_maximized -> ' + v)`);
    // A picture of the web window (the system's frame, the server's title).
    const shot = spawnSync(PY, ['-c', `
import sys, ctypes
sys.path.insert(0, 'A:/ogun-demo/tools/ui-reference')
import capture_t3 as cap
from PIL import ImageGrab
u = cap.user32
try: u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
except Exception: u.SetProcessDPIAware()
h, t, pid, image = cap.find_window('web workspace', 'dynodoc-desktop')
cap.bring_front(h)
import time; time.sleep(0.6)
l, tp, r, b = cap.frame_rect(h)
ImageGrab.grab(bbox=(l - 20, tp - 20, r + 20, b + 20), all_screens=True).save(r'${OUT}/web-window.png')
print('window title:', t)
`], { encoding: 'utf8' });
    results.nativeWindow = (shot.stdout + shot.stderr).trim();
    // The client window cannot be sent to the server's page.
    await page.eval(`location.href = 'http://127.0.0.1:${PORT}/from-client'`).catch(() => {});
    await wait(1500);
    results.clientAfterNavigatingToTheServer = (await targets(port)).filter((t) => t.type === 'page').map((t) => t.url.replace(/\?.*/, ''));
    w.close();
  } finally {
    server.close();
  }
  console.log(JSON.stringify(results, null, 2));
}
