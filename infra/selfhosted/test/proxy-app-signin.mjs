// Tests the app sign-in confirmation step (docs/decisions/017) THROUGH the two reverse-proxy
// configurations Dynodoc ships, in front of the real built web app and the real API:
//
//   hosted      infra/demo/nginx-app.conf, as it is, in an nginx container (only the server name, the
//               certificate and the upstream address are rewritten, and each rewrite is asserted)
//   selfhosted  the bundled proxy image (infra/selfhosted/images/Dockerfile.proxy) in evaluation
//               TLS mode, in front of the same web server through a TCP forwarder
//
// Why it exists: both proxies replace the application's Referrer-Policy with `no-referrer`, which
// makes a browser send `Origin: null` with a same-origin form post; the Continue form only works
// because the page carries its own `<meta name="referrer">`. A test without a proxy cannot see that.
// Each edge is driven in Chromium by `appSigninThroughEdge` (test/browser-flow.mjs, also run by
// scripts/smoke.mjs through the full bundle): headers that keep the two pages from being framed or
// sharing a window group, Continue working, a popup read-back attack failing, the POST limit.
//
//   node infra/selfhosted/test/proxy-app-signin.mjs [--edge hosted|selfhosted|both] [--keep]
//
// Needs: Docker; a running API and the built web app (`frontend/researcher/build`, run
// `vite build` first), with the environment the browser-test stack uses:
//   ENGINE_API_URL (default http://127.0.0.1:28080)   WEB_SERVICE_KEY   SESSION_SECRET
// and Playwright with Chromium (default: frontend/researcher's node_modules, see --playwright-from).
// The web app is started here with DEV_AUTH on (the browser tests' development sign-in), once per edge
// because its public ORIGIN must be the edge's. Everything it starts is removed at the end.
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(here, '..');
const REPO = path.resolve(SRC, '..', '..');
const args = process.argv.slice(2);
const opt = (name, fallback) => { const i = args.indexOf(name); return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback; };
const edgeChoice = opt('--edge', 'both');
const keep = args.includes('--keep');
// Built from this checkout on every run (cached layers make it cheap) under a tag of its own, so
// the test never uses or replaces another run's dynodoc-local/*:dev images.
const OWN_IMAGE = !process.env.PROXY_IMAGE;
const PROXY_IMAGE = process.env.PROXY_IMAGE ?? 'dynodoc-edge-test/proxy:local';
const NGINX_IMAGE = process.env.NGINX_IMAGE ?? 'nginx:1.27-alpine';
const NODE_IMAGE = process.env.NODE_IMAGE ?? 'node:22-slim';
const HOSTED_PORT = Number(process.env.HOSTED_EDGE_PORT ?? 28543);
const BUNDLE_PORT = Number(process.env.BUNDLE_EDGE_PORT ?? 28544);
const WEB_PORT = { hosted: 14191, selfhosted: 14192 };
const ID = `pas${process.pid}`;
const API = process.env.ENGINE_API_URL ?? 'http://127.0.0.1:28080';
const results = { pass: 0, fail: 0 };
const containers = [];
const children = [];
const temp = mkdtempSync(path.join(os.tmpdir(), 'dynodoc-edge-'));

const docker = (a, o = {}) => spawnSync('docker', a, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...o });
const check = (id, label, ok, detail = '') => {
  results[ok ? 'pass' : 'fail']++;
  console.log(`${ok ? 'ok  ' : 'FAIL'}  ${id}  ${label}${ok || !detail ? '' : `\n          ${String(detail).split('\n').join('\n          ').slice(0, 1600)}`}`);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function cleanup() {
  for (const child of children) { try { spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' }); } catch { /* gone */ } try { child.kill(); } catch { /* gone */ } }
  if (!keep) {
    for (const name of containers) docker(['rm', '-f', name]);
    docker(['network', 'rm', `${ID}-net`]);
    for (const volume of ['cert', 'eval']) docker(['volume', 'rm', '-f', `${ID}-${volume}`]);
    rmSync(temp, { recursive: true, force: true });
  }
}
process.on('exit', cleanup);

// ---- the real web app, once per edge ------------------------------------------------------------
async function startWeb(edge, origin) {
  const env = {
    ...process.env, HOST: '0.0.0.0', PORT: String(WEB_PORT[edge]), ORIGIN: origin, ENGINE_API_URL: API,
    DEV_AUTH: 'true', BODY_SIZE_LIMIT: '16M',
  };
  const child = spawn(process.execPath, ['build'], { cwd: path.join(REPO, 'frontend', 'researcher'), env, stdio: 'ignore' });
  children.push(child);
  for (let i = 0; i < 60; i += 1) {
    try { if ((await fetch(`http://127.0.0.1:${WEB_PORT[edge]}/healthz`)).ok) return; } catch { /* not yet */ }
    await sleep(500);
  }
  throw new Error(`the web app did not start for the ${edge} edge (is frontend/researcher/build built? is the API at ${API} up?)`);
}

// ---- edges ---------------------------------------------------------------------------------------
/** A throw-away self-signed certificate made with the proxy image's own openssl. */
function makeCertificate() {
  docker(['volume', 'create', `${ID}-cert`]);
  const r = docker(['run', '--rm', '-u', '0', '-v', `${ID}-cert:/out`, '--entrypoint', 'sh', PROXY_IMAGE, '-c',
    'openssl req -x509 -newkey rsa:2048 -nodes -days 2 -keyout /out/privkey.pem -out /out/fullchain.pem -subj /CN=localhost -addext subjectAltName=DNS:localhost >/dev/null 2>&1 && chmod 644 /out/*.pem && ls /out']);
  if (!/fullchain\.pem/.test(r.stdout)) throw new Error(`could not make a certificate: ${r.stdout}${r.stderr}`);
}

function startHosted() {
  let conf = readFileSync(path.join(REPO, 'infra', 'demo', 'nginx-app.conf'), 'utf8');
  const rewrite = (from, to, count) => {
    const found = conf.split(from).length - 1;
    if (found !== count) throw new Error(`infra/demo/nginx-app.conf changed: expected ${count} of ${JSON.stringify(from)}, found ${found}; update this test`);
    conf = conf.split(from).join(to);
  };
  rewrite('server_name app.dynodoc.online;', 'server_name localhost;', 2);
  rewrite('proxy_pass http://127.0.0.1:3088;', `proxy_pass http://host.docker.internal:${WEB_PORT.hosted};`, 4);
  writeFileSync(path.join(temp, 'hosted.conf'), conf);
  const name = `${ID}-hosted`;
  containers.push(name);
  const mount = (file) => `${path.join(temp, file)}:/etc/nginx/conf.d/app.conf:ro`;
  const r = docker(['run', '-d', '--name', name, '--add-host', 'host.docker.internal:host-gateway',
    '-p', `127.0.0.1:${HOSTED_PORT}:443`,
    '-v', mount('hosted.conf'), '-v', `${ID}-cert:/etc/letsencrypt/live/app.dynodoc.online:ro`,
    '--entrypoint', 'sh', NGINX_IMAGE, '-c', "rm -f /etc/nginx/conf.d/default.conf && nginx -t && exec nginx -g 'daemon off;'"]);
  if (r.status !== 0) throw new Error(`hosted nginx did not start: ${r.stdout}${r.stderr}`);
}

function startBundled() {
  const net = `${ID}-net`;
  docker(['network', 'create', net]);
  // `web` is where the bundled proxy looks for the application: a forwarder to the host's web app.
  const forward = `
const net = require('net');
net.createServer((c) => { const s = net.connect(${WEB_PORT.selfhosted}, 'host.docker.internal'); c.pipe(s); s.pipe(c); c.on('error', () => s.destroy()); s.on('error', () => c.destroy()); }).listen(3000);`;
  containers.push(`${ID}-web`);
  docker(['run', '-d', '--name', `${ID}-web`, '--network', net, '--network-alias', 'web', '--add-host', 'host.docker.internal:host-gateway', NODE_IMAGE, 'node', '-e', forward]);
  containers.push(`${ID}-bundle`);
  const r = docker(['run', '-d', '--name', `${ID}-bundle`, '--network', net, '-p', `127.0.0.1:${BUNDLE_PORT}:8443`,
    '--read-only', '--tmpfs', '/tmp', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true',
    '-e', `ORIGIN=https://localhost:${BUNDLE_PORT}`, '-e', 'DYNODOC_TLS_MODE=evaluation', '-v', `${ID}-eval:/var/lib/dynodoc-tls`, PROXY_IMAGE]);
  if (r.status !== 0) throw new Error(`the bundled proxy did not start: ${r.stdout}${r.stderr}`);
}

/** True once something answers a TLS handshake on the port. */
async function waitServing(port) {
  for (let i = 0; i < 60; i += 1) {
    const probe = spawnSync(process.execPath, ['-e', `const t=require('tls').connect({host:'127.0.0.1',port:${port},servername:'localhost',rejectUnauthorized:false},()=>{t.end();process.exit(0)});t.on('error',()=>process.exit(1));setTimeout(()=>process.exit(1),2000)`]);
    if (probe.status === 0) { await sleep(500); return true; }
    await sleep(500);
  }
  return false;
}

// ---- the browser ---------------------------------------------------------------------------------
async function drive(edge, port, playwright, flow) {
  const baseUrl = `https://localhost:${port}`;
  const browser = await playwright.chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, baseURL: baseUrl });
  const page = await context.newPage();
  try {
    await page.goto('/login');
    await page.getByLabel('Your name').fill('Edge tester');
    await page.getByLabel('Work email').fill(`edge-${edge}-${Date.now()}@example.test`);
    await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
    await page.getByRole('button', { name: /Continue in local development/ }).click();
    await page.waitForURL(/\/projects$/, { timeout: 20000 });
    check(`${edge}.0`, 'signed in through the proxy (development sign-in, secure cookie over https)', true);
    for (const r of await flow.appSigninThroughEdge({ page, context, baseUrl })) check(`${edge}.${r.id}`, r.label, r.ok, r.detail);
  } catch (error) {
    check(`${edge}.run`, 'the browser run completed', false, error.stack ?? String(error));
  } finally {
    await browser.close();
  }
}

async function main() {
  if (docker(['version', '--format', '{{.Server.Version}}']).status !== 0) throw new Error('Docker is not running');
  if (OWN_IMAGE) {
    const b = docker(['build', '-q', '-t', PROXY_IMAGE, '-f', path.join(SRC, 'images', 'Dockerfile.proxy'), SRC]);
    if (b.status !== 0) throw new Error(`could not build ${PROXY_IMAGE}: ${b.stderr}`);
  }
  for (const image of [PROXY_IMAGE, NGINX_IMAGE, NODE_IMAGE]) {
    if (docker(['image', 'inspect', image]).status !== 0 && docker(['pull', '-q', image]).status !== 0) throw new Error(`could not find or pull ${image}`);
  }
  const from = opt('--playwright-from', path.join(REPO, 'frontend', 'researcher'));
  const playwright = createRequire(path.join(from, 'package.json'))('@playwright/test');
  const flow = await import(pathToFileURL(path.join(here, 'browser-flow.mjs')).href);
  makeCertificate();

  if (edgeChoice === 'hosted' || edgeChoice === 'both') {
    await startWeb('hosted', `https://localhost:${HOSTED_PORT}`);
    startHosted();
    check('hosted.up', 'infra/demo/nginx-app.conf (rewritten only for name, certificate and upstream) serves the web app', await waitServing(HOSTED_PORT), docker(['logs', `${ID}-hosted`]).stderr);
    await drive('hosted', HOSTED_PORT, playwright, flow);
  }
  if (edgeChoice === 'selfhosted' || edgeChoice === 'both') {
    await startWeb('selfhosted', `https://localhost:${BUNDLE_PORT}`);
    startBundled();
    check('selfhosted.up', 'the bundled proxy image (evaluation TLS) serves the web app', await waitServing(BUNDLE_PORT), docker(['logs', `${ID}-bundle`]).stderr);
    await drive('selfhosted', BUNDLE_PORT, playwright, flow);
  }
  console.log(`\n${results.pass} passed, ${results.fail} failed`);
  process.exitCode = results.fail ? 1 : 0;
}

main().catch((error) => { console.error(error.stack ?? String(error)); process.exitCode = 1; }).finally(() => { cleanup(); process.exit(process.exitCode ?? 0); });
