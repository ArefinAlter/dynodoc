// Drives the real desktop app (a debug build) through WebView2's remote debugging port:
// start it with a scratch home, read what the page shows, click, type, take screenshots.
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync, existsSync, rmSync } from 'node:fs';

export const EXE = 'D:/verne-build/dynodoc/cargo-target/desktop/debug/dynodoc-desktop.exe';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
export { sleep };

export function launch({ home, port = 9333, pickFile, exe = process.env.ACC_EXE || EXE, programs = 'D:/verne-build/dynodoc/programs', extraEnv = {} }) {
  mkdirSync(home, { recursive: true });
  const env = {
    ...process.env,
    DYNODOC_DESKTOP_HOME: home,
    DYNODOC_PROGRAMS: programs,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    ...(pickFile ? { DYNODOC_TEST_PICK: pickFile } : {}),
    ...extraEnv,
  };
  delete env.ELECTRON_RUN_AS_NODE;
  const child = spawn(exe, [], { env, stdio: 'ignore', detached: false });
  return { child, port, home };
}

async function json(url) {
  const r = await fetch(url);
  return r.json();
}

export async function connect(port = 9333, timeoutMs = 60000) {
  const t0 = Date.now();
  let target;
  while (Date.now() - t0 < timeoutMs) {
    try {
      const list = await json(`http://127.0.0.1:${port}/json`);
      target = list.find((t) => t.type === 'page' && /tauri\.localhost|index\.html/.test(t.url));
      if (target) break;
    } catch {}
    await sleep(500);
  }
  if (!target) throw new Error('no page to drive');
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0;
  const waiting = new Map();
  ws.onmessage = (m) => {
    const d = JSON.parse(m.data);
    if (d.id && waiting.has(d.id)) {
      const w = waiting.get(d.id);
      waiting.delete(d.id);
      d.error ? w.rej(new Error(d.error.message)) : w.res(d.result);
    }
  };
  const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; waiting.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
  await send('Runtime.enable');
  await send('Page.enable');
  const page = {
    send,
    async eval(expr) {
      const r = await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
      return r.result.value;
    },
    async text(sel = 'body') { return page.eval(`document.querySelector(${JSON.stringify(sel)})?.innerText ?? ''`); },
    async has(sel) { return page.eval(`!!document.querySelector(${JSON.stringify(sel)})`); },
    async click(sel) {
      const ok = await page.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return false; e.click(); return true; })()`);
      if (!ok) throw new Error('nothing to click: ' + sel);
    },
    async clickText(text, tag = 'button') {
      const ok = await page.eval(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(tag)})].find((b) => b.textContent.trim().startsWith(${JSON.stringify(text)}) && !b.disabled); if (!e) return false; e.click(); return true; })()`);
      if (!ok) throw new Error('no ' + tag + ' with text: ' + text);
    },
    async type(sel, value) {
      await page.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); e.focus(); const set = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(e), 'value').set; set.call(e, ${JSON.stringify(value)}); e.dispatchEvent(new Event('input', { bubbles: true })); })()`);
    },
    async waitFor(fnExpr, timeoutMs = 60000, label = fnExpr) {
      const t0 = Date.now();
      while (Date.now() - t0 < timeoutMs) {
        try { if (await page.eval(fnExpr)) return true; } catch {}
        await sleep(400);
      }
      throw new Error('timed out waiting for ' + label);
    },
    async shot(path) {
      const r = await send('Page.captureScreenshot', { format: 'png' });
      writeFileSync(path, Buffer.from(r.data, 'base64'));
    },
    async invoke(cmd, args = {}) {
      return page.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
    },
    close() { try { ws.close(); } catch {} },
  };
  return page;
}

export function killTree(child) {
  try { spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' }); } catch {}
}

/** Closes the window the way a person does (WM_CLOSE) and waits for the app to end; then forces it. */
export async function closeGracefully(child, waitMs = 60000) {
  let exited = false;
  child.once('exit', () => { exited = true; });
  try { spawnSync('taskkill', ['/PID', String(child.pid)], { stdio: 'ignore' }); } catch {}
  const t0 = Date.now();
  while (!exited && Date.now() - t0 < waitMs) await sleep(300);
  if (!exited) { killTree(child); return false; }
  return true;
}

/** Stops a database an earlier run left running in a scratch home. */
export function stopOrphanDatabase(home) {
  const pg = 'D:/verne-build/dynodoc/programs/pg/bin/pg_ctl.exe';
  try { spawnSync(pg, ['stop', '-D', home + '/data/workspace/pgdata', '-m', 'immediate', '-s'], { stdio: 'ignore', timeout: 30000 }); } catch {}
}
export function clean(path) { /* callers stop an orphan database first */ if (existsSync(path)) rmSync(path, { recursive: true, force: true }); }

/** The ids of the Word, Excel and PowerPoint processes running now (so a test closes only the ones it started). */
export function officePids() {
  const out = spawnSync('tasklist', ['/FO', 'CSV', '/NH'], { encoding: 'utf8' }).stdout || '';
  const pids = new Set();
  for (const line of out.split(/\r?\n/)) {
    const m = /^"(WINWORD|EXCEL|POWERPNT)\.EXE","(\d+)"/i.exec(line.trim());
    if (m) pids.add(Number(m[2]));
  }
  return pids;
}

/** Ends the Office processes that were not running in `before`. Never touches the others. */
export function closeNewOffice(before) {
  for (const pid of officePids()) if (!before.has(pid)) spawnSync('taskkill', ['/PID', String(pid), '/F'], { stdio: 'ignore' });
}
