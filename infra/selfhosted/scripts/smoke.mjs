#!/usr/bin/env node
// End-to-end smoke test of the self-hosted bundle. Runs on Windows, macOS and Linux (Node 20+,
// Docker with Compose v2). smoke.sh is a launcher for this file on Linux and macOS.
//
//   node infra/selfhosted/scripts/smoke.mjs [options]
//
//   --port N              HTTPS port on 127.0.0.1 (default 18443); the HTTP port is N-363 (18080)
//   --no-build            use the images already built (dynodoc-local/*:dev); default builds them
//   --registry R --version V   run the RELEASE path instead: pull the images from R by digest
//                         through a generated release overlay (nothing is built)
//   --reuse DIR           debugging: run the checks against a stack left by --keep
//   --skip-browser        skip the Chromium checks (needs Playwright, see --playwright-from)
//   --playwright-from DIR a directory whose node_modules has @playwright/test and a Chromium
//                         (default: frontend/researcher of this repository, if it has node_modules)
//   --keep                leave the stack, volumes and temp directory in place (for debugging)
//   --project NAME        Compose project name (default dynodoc-smoke)
//   --report FILE         write the results as JSON
//
// It never touches the repository's own files: the bundle is copied to a temp directory, the
// environment and secrets are generated there with scripts/dynodoc-env.mjs, the stack runs in
// `evaluation` TLS mode on a loopback port, and everything it creates is removed at the end.
import { spawnSync } from 'node:child_process';
import { appendFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import http from 'node:http';
import https from 'node:https';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import tls from 'node:tls';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(here, '..');
const REPO = path.resolve(SRC, '..', '..');
const args = process.argv.slice(2);
const opt = (name, fallback) => { const i = args.indexOf(name); return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback; };
const has = (name) => args.includes(name);
const PORT = Number(opt('--port', 18443));
const HTTP_PORT = PORT - 363;
const PROJECT = opt('--project', 'dynodoc-smoke');
const ORIGIN = `https://localhost:${PORT}`;
const ADMIN = 'admin@smoke.test';
const TITLE = `Smoke document ${Date.now()}`;
const TEXT = 'Smoke test paragraph that must survive restarts.';

const results = [];
const t0 = Date.now();
function check(id, label, ok, detail = '') {
  results.push({ id, label, ok: Boolean(ok), detail: ok ? '' : String(detail).slice(0, 1500) });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${id}  ${label}${ok || !detail ? '' : `\n          ${String(detail).slice(0, 1500).split('\n').join('\n          ')}`}`);
  return Boolean(ok);
}
const info = (text) => console.log(`      ${text}`);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- docker helpers -------------------------------------------------------------------------
const cleanEnv = () => {
  const env = { ...process.env };
  for (const key of Object.keys(env))
    if (/^(DYNODOC_|COMPOSE_|ORIGIN$|DEV_AUTH$|ADMIN_EMAILS$|RUST_LOG$|SMTP_|SESSION_SECRET|WEB_SERVICE_KEY)/.test(key)) delete env[key];
  return env;
};
const docker = (a, o = {}) => spawnSync('docker', a, { encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, env: cleanEnv(), ...o });
let BUNDLE = '';
const composeArgs = (extra) => ['compose', '-p', PROJECT, '--project-directory', BUNDLE, '-f', path.join(BUNDLE, 'compose.yml'),
  ...(existsSync(path.join(BUNDLE, 'compose.release.generated.yml')) ? ['-f', path.join(BUNDLE, 'compose.release.generated.yml')] : []),
  '-f', path.join(BUNDLE, 'compose.smoke.yml'), ...extra];
const compose = (extra, o = {}) => docker(composeArgs(extra), { cwd: BUNDLE, ...o });
const out = (r) => `${r.stdout ?? ''}${r.stderr ?? ''}`.trim();
const containers = () => compose(['ps', '-a', '-q']).stdout.split(/\s+/).filter(Boolean);
const inspectAll = () => {
  const ids = containers();
  if (!ids.length) return [];
  return JSON.parse(docker(['inspect', ...ids]).stdout);
};
const svc = (c) => c.Config.Labels['com.docker.compose.service'];
const byService = () => Object.fromEntries(inspectAll().map((c) => [svc(c), c]));
const execIn = (service, command) => compose(['exec', '-T', service, 'sh', '-c', command]);

// ---- http helpers ---------------------------------------------------------------------------
function request(urlPath, { method = 'GET', headers = {}, body, secure = true, port } = {}) {
  return new Promise((resolve) => {
    const lib = secure ? https : http;
    const req = lib.request({
      host: '127.0.0.1', port: port ?? (secure ? PORT : HTTP_PORT), path: urlPath, method, agent: false,
      servername: 'localhost', rejectUnauthorized: false, headers: { host: secure ? `localhost:${PORT}` : `localhost:${HTTP_PORT}`, ...headers }
    }, (res) => {
      const chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, rawHeaders: res.rawHeaders, body: Buffer.concat(chunks).toString('utf8') }));
    });
    req.on('error', (error) => resolve({ status: 0, headers: {}, rawHeaders: [], body: '', error: error.message }));
    req.setTimeout(10000, () => req.destroy(new Error('timeout')));
    if (body) req.write(body);
    req.end();
  });
}
function tlsProbe(options) {
  return new Promise((resolve) => {
    const socket = tls.connect({ host: '127.0.0.1', port: PORT, servername: 'localhost', rejectUnauthorized: false, ...options }, () => {
      const result = { protocol: socket.getProtocol(), cipher: socket.getCipher(), cert: socket.getPeerCertificate() };
      socket.end();
      resolve(result);
    });
    socket.on('error', (error) => resolve({ error: error.message }));
    socket.setTimeout(5000, () => { socket.destroy(); resolve({ error: 'timeout' }); });
  });
}
function tcpOpen(port, host = '127.0.0.1') {
  return new Promise((resolve) => {
    const socket = net.connect({ host, port }, () => { socket.destroy(); resolve(true); });
    socket.on('error', () => resolve(false));
    socket.setTimeout(2000, () => { socket.destroy(); resolve(false); });
  });
}
async function waitFor(label, fn, timeoutMs = 120000) {
  const end = Date.now() + timeoutMs;
  let last;
  while (Date.now() < end) { last = await fn(); if (last) return last; await sleep(1000); }
  return null;
}

// ---- setup ----------------------------------------------------------------------------------
function prepareBundle() {
  const tmp = mkdtempSync(path.join(os.tmpdir(), 'dynodoc-smoke-'));
  BUNDLE = path.join(tmp, 'bundle');
  mkdirSync(BUNDLE);
  for (const f of ['compose.yml', 'compose.no-egress.yml', 'bundle.env.example']) cpSync(path.join(SRC, f), path.join(BUNDLE, f));
  for (const d of ['jobs', 'tls', 'config']) cpSync(path.join(SRC, d), path.join(BUNDLE, d), { recursive: true });
  cpSync(path.join(SRC, 'test', 'compose.smoke.yml'), path.join(BUNDLE, 'compose.smoke.yml'));
  mkdirSync(path.join(BUNDLE, 'test-sink'));
  cpSync(path.join(SRC, 'test', 'smtp-sink.mjs'), path.join(BUNDLE, 'test-sink', 'smtp-sink.mjs'));
  const gen = spawnSync(process.execPath, [
    path.join(REPO, 'frontend', 'researcher', 'scripts', 'dynodoc-env.mjs'), 'generate-env', '--dir', BUNDLE, '--origin', ORIGIN,
    '--container-secrets-dir', '/run/secrets', '--org-name', 'Smoke Test Organisation', '--brand-name', 'Smoke Docs',
    '--smtp-host', 'mailsink', '--smtp-from', 'docs@smoke.test', '--smtp-port', '2525', '--smtp-security', 'none-local'
  ], { encoding: 'utf8' });
  if (gen.status !== 0) throw new Error(`generate-env failed: ${gen.stderr}${gen.stdout}`);
  appendFileSync(path.join(BUNDLE, '.env'), [
    '', '# --- bundle settings (smoke test) ---',
    'DYNODOC_TLS_MODE=evaluation', `DYNODOC_HTTPS_PORT=${PORT}`, `DYNODOC_HTTP_PORT=${HTTP_PORT}`,
    `ADMIN_EMAILS=${ADMIN}`, 'DYNODOC_ORG_NOTICE=Smoke test installation. Questions: the IT desk, extension 123.', ''
  ].join('\n'));
  // Release path: images come from a registry by digest, through the generated overlay.
  const registry = opt('--registry');
  if (registry) {
    const version = opt('--version', 'dev');
    appendFileSync(path.join(BUNDLE, '.env'), `DYNODOC_IMAGE_REGISTRY=${registry}\nDYNODOC_VERSION=${version}\n`);
    const gen2 = spawnSync(process.execPath, [path.join(here, 'make-release-overlay.mjs'), '--registry', registry, '--version', version, '--out', path.join(BUNDLE, 'compose.release.generated.yml')], { encoding: 'utf8' });
    if (gen2.status !== 0) throw new Error(`make-release-overlay failed: ${gen2.stderr}${gen2.stdout}`);
    info(`release overlay: ${registry} ${version} (images by digest)`);
  }
  return tmp;
}
function secretValues() {
  const dir = path.join(BUNDLE, 'secrets');
  const values = {};
  for (const f of readdirSync(dir)) values[f] = readFileSync(path.join(dir, f), 'utf8').trim();
  return values;
}

async function up(label, extraArgs = [], extraEnv = {}) {
  const t = Date.now();
  const r = compose(['up', '-d', '--wait', '--wait-timeout', '300', '--no-build', ...extraArgs], { env: { ...cleanEnv(), ...extraEnv } });
  return { ok: r.status === 0, seconds: Math.round((Date.now() - t) / 1000), text: out(r), label };
}

// ---- the checks -----------------------------------------------------------------------------
async function main() {
  const reuse = opt('--reuse');
  if (reuse) BUNDLE = path.resolve(reuse);
  else prepareBundle();
  info(`bundle copy: ${BUNDLE}`);
  let playwright = null;
  if (!has('--skip-browser')) {
    const from = opt('--playwright-from', path.join(REPO, 'frontend', 'researcher'));
    try {
      playwright = createRequire(path.join(from, 'package.json'))('@playwright/test');
    } catch {
      info(`Playwright not found under ${from}/node_modules: the browser checks (h, k) will be reported as skipped.`);
    }
  }

  // Images.
  if (!has('--no-build') && !reuse && !opt('--registry')) {
    const t = Date.now();
    const r = docker(['compose', '-p', `${PROJECT}-build`, '--project-directory', SRC, '-f', path.join(SRC, 'compose.yml'), '-f', path.join(SRC, 'compose.build.yml'), '--profile', 'acme', 'build', 'api', 'web', 'preflight', 'proxy', 'acme'],
      { env: { ...cleanEnv(), ORIGIN, DYNODOC_TLS_MODE: 'evaluation' }, cwd: SRC });
    check('build', `images built from this tree with compose.build.yml (${Math.round((Date.now() - t) / 1000)} s)`, r.status === 0, out(r).slice(-1500));
    if (r.status !== 0) return;
  }

  // Bring it up.
  const first = reuse ? { ok: true, seconds: 0, text: '' } : await up('first start');
  check('up', `docker compose up -d --wait brings the whole stack up (${first.seconds} s)`, first.ok, first.text.slice(-2000));
  if (!first.ok) { info(out(compose(['logs', '--no-color', '--tail', '40']))); return; }
  const services = byService();
  info(`containers: ${Object.entries(services).map(([n, c]) => `${n}=${c.State.Status}${c.State.ExitCode ? `(${c.State.ExitCode})` : ''}`).join(' ')}`);

  // (a) only the proxy publishes, only on 127.0.0.1
  {
    const bindings = Object.entries(services).flatMap(([name, c]) => Object.entries(c.HostConfig.PortBindings ?? {}).flatMap(([port, list]) => (list ?? []).map((b) => ({ name, port, ip: b.HostIp, hostPort: b.HostPort }))));
    const others = bindings.filter((b) => b.name !== 'proxy');
    check('a1', 'only the proxy publishes a port (api, web, postgres and the jobs publish none)', bindings.length === 2 && others.length === 0, JSON.stringify(bindings));
    check('a2', 'the proxy publishes on 127.0.0.1 only (default DYNODOC_BIND)', bindings.length > 0 && bindings.every((b) => b.ip === '127.0.0.1'), JSON.stringify(bindings));
    const names = docker(['network', 'ls', '--filter', `label=com.docker.compose.project=${PROJECT}`, '--format', '{{.Name}}']).stdout.split('\n').filter(Boolean);
    const netInfo = JSON.parse(docker(['network', 'inspect', ...names]).stdout);
    const shape = Object.fromEntries(netInfo.map((n) => [n.Labels['com.docker.compose.network'], { internal: n.Internal, members: Object.values(n.Containers ?? {}).map((c) => c.Name.replace(`${PROJECT}-`, '').replace(/-\d+$/, '')).sort() }]));
    info(`networks: ${JSON.stringify(shape)}`);
    check('a3', 'engine, frontend and conversion networks are internal (no route out); published and egress are not', shape.engine?.internal && shape.frontend?.internal && shape.conversion?.internal && !shape.published?.internal && !shape.egress?.internal, JSON.stringify(shape));
    check('a4', 'the proxy is the only member of the published network; it shares a network with web only', JSON.stringify(shape.published?.members) === '["proxy"]' && JSON.stringify(shape.frontend?.members) === '["proxy","web"]' && !shape.engine?.members.includes('proxy'), JSON.stringify(shape));
  }

  // (b) HTTP -> HTTPS
  {
    const r = await request('/some/path?x=1', { secure: false });
    check('b', 'HTTP redirects to HTTPS on the configured origin (301, path and query kept)', r.status === 301 && r.headers.location === `${ORIGIN}/some/path?x=1`, `${r.status} ${r.headers.location} ${r.error ?? ''}`);
    const evil = await request('/x', { secure: false, headers: { host: 'evil.example' } });
    check('b2', 'the redirect target is the configured origin, whatever the Host header says', evil.status === 301 && evil.headers.location === `${ORIGIN}/x`, `${evil.status} ${evil.headers.location}`);
  }

  // (c) headers, CSP, HSTS, TLS
  {
    const r = await request('/login');
    const h = r.headers;
    const csp = h['content-security-policy'];
    const cspCount = r.rawHeaders.filter((x, i) => i % 2 === 0 && x.toLowerCase() === 'content-security-policy').length;
    check('c1', 'sign-in page answers 200 over HTTPS', r.status === 200, `${r.status} ${r.error ?? ''}`);
    check('c2', 'exactly one Content-Security-Policy, from the application, with the configured origin in connect-src', cspCount === 1 && /script-src/.test(csp ?? '') && csp.includes(`connect-src 'self' ${ORIGIN}`), `${cspCount} ${csp}`);
    check('c3', 'Strict-Transport-Security present (300 s in evaluation mode)', h['strict-transport-security'] === 'max-age=300', h['strict-transport-security']);
    check('c4', 'nosniff, no-referrer, SAMEORIGIN present', h['x-content-type-options'] === 'nosniff' && h['referrer-policy'] === 'no-referrer' && h['x-frame-options'] === 'SAMEORIGIN', JSON.stringify({ n: h['x-content-type-options'], r: h['referrer-policy'], f: h['x-frame-options'] }));
    check('c5', 'no server version or framework banner (Server: nginx only, no X-Powered-By)', h.server === 'nginx' && !h['x-powered-by'], `${h.server} ${h['x-powered-by']}`);
    check('c6', 'the evaluation mode is announced in a response header', /^evaluation;.*not for production/.test(h['x-dynodoc-tls-mode'] ?? ''), h['x-dynodoc-tls-mode']);
    check('c7', 'nothing is offered to search engines (x-robots-tag noindex)', /noindex/.test(h['x-robots-tag'] ?? ''), h['x-robots-tag']);
    const old = await tlsProbe({ minVersion: 'TLSv1', maxVersion: 'TLSv1.1', ciphers: 'DEFAULT@SECLEVEL=0' });
    const t12 = await tlsProbe({ minVersion: 'TLSv1.2', maxVersion: 'TLSv1.2' });
    const t13 = await tlsProbe({ minVersion: 'TLSv1.3', maxVersion: 'TLSv1.3' });
    check('c8', 'TLS 1.0 and 1.1 are refused; TLS 1.2 and 1.3 work', Boolean(old.error) && t12.protocol === 'TLSv1.2' && t13.protocol === 'TLSv1.3', JSON.stringify({ old: old.error, t12: t12.protocol ?? t12.error, t13: t13.protocol ?? t13.error }));
    check('c9', 'TLS 1.2 uses an ECDHE key exchange with an AEAD cipher', /^ECDHE-.*(GCM|CHACHA20)/.test(t12.cipher?.name ?? ''), t12.cipher?.name);
    const subject = JSON.stringify(t13.cert?.subject ?? {});
    check('c10', 'the certificate is the labelled evaluation one (EVALUATION, NOT FOR PRODUCTION; names include localhost)', /EVALUATION/.test(subject) && /NOT FOR PRODUCTION/.test(subject) && /DNS:localhost/.test(t13.cert?.subjectaltname ?? ''), subject + (t13.cert?.subjectaltname ?? ''));
    const mis = await request('/', { headers: { host: 'evil.example' } });
    check('c11', 'a request for another host name gets 421, not the application', mis.status === 421, mis.status);
    const health = await request('/healthz');
    check('c12', '/healthz through the proxy reports the web server alive', health.status === 200 && /"status":"ok"/.test(health.body), `${health.status} ${health.body}`);
    const big = await request('/auth/email', { method: 'POST', headers: { origin: ORIGIN, 'content-type': 'application/json' }, body: 'x'.repeat(17 * 1024 * 1024) });
    check('c13', 'a request body over 16 MiB is refused by the proxy (413)', big.status === 413 || big.status === 0, `${big.status} ${big.error ?? ''}`);
  }

  // proxy maps are the hosted ones
  {
    const mine = readFileSync(path.join(SRC, 'proxy', 'conf', 'maps.conf'), 'utf8');
    const block = mine.slice(mine.indexOf('# BEGIN HOSTED MAPS') + '# BEGIN HOSTED MAPS\n'.length, mine.indexOf('# END HOSTED MAPS'));
    const norm = (s) => s.split('\n').filter((l) => !l.startsWith('#')).join('\n').trim();
    const demoMaps = readFileSync(path.join(REPO, 'infra', 'demo', 'nginx-app.conf'), 'utf8').split('\n').slice(0, 20).join('\n');
    check('c14', 'the CSP and frame-option maps are byte for byte the hosted ones (infra/demo/nginx-app.conf)', norm(block) === norm(demoMaps), 'maps.conf differs from infra/demo/nginx-app.conf lines 1-20');
  }

  // (d) API not reachable
  {
    const host8080 = await request('/healthz', { secure: false, port: 8080 });
    check('d1', 'the API port is not reachable from the host (127.0.0.1:8080 does not answer as the API)', !/engine-api/.test(host8080.body), `${host8080.status} ${host8080.body}`);
    const pg = await tcpOpen(5432);
    info(`host 127.0.0.1:5432 open: ${pg} (this machine may run its own PostgreSQL; the bundle publishes nothing there, see a1)`);
    const viaProxy = await request('/api/engine/documents');
    check('d2', 'the API answers only through the web server: anonymous /api/engine/documents is 401', viaProxy.status === 401, viaProxy.status);
    const fromProxy = execIn('proxy', 'wget -T 3 -q -O- http://api:8080/healthz 2>&1; echo rc=$?');
    check('d3', 'from the proxy container the API is not reachable (no shared network, name does not resolve)', /rc=[1-9]/.test(fromProxy.stdout) && !/engine-api/.test(fromProxy.stdout), fromProxy.stdout);
    const pgFromProxy = execIn('proxy', 'wget -T 3 -q -O- http://postgres:5432 2>&1; echo rc=$?');
    check('d4', 'from the proxy container PostgreSQL is not reachable either', /rc=[1-9]/.test(pgFromProxy.stdout), pgFromProxy.stdout);
    const out1 = compose(['exec', '-T', 'api', 'bash', '-c', 'exec 3<>/dev/tcp/1.1.1.1/53 && echo connected || echo blocked']);
    check('d5', 'the API container has no route out of Docker (internal network)', /blocked/.test(out1.stdout), out(out1));
    const webToApi = compose(['exec', '-T', 'web', 'node', '-e', "fetch('http://api:8080/healthz').then(r=>r.text()).then(t=>console.log('api says',t)).catch(e=>console.log('failed',e.message))"]);
    check('d6', 'the web container reaches the API by name', /engine-api/.test(webToApi.stdout), out(webToApi));
  }

  // (e) hardening
  {
    const rows = [];
    const longRunning = ['postgres', 'api', 'web', 'proxy'];
    const jobs = ['preflight', 'db-roles', 'init'];
    for (const name of [...longRunning, ...jobs, 'secrets-init']) {
      const c = services[name];
      if (!c) { check('e0', `${name} container exists`, false, 'missing'); continue; }
      const hc = c.HostConfig;
      const capAdd = (hc.CapAdd ?? []).map((c) => c.replace(/^CAP_/, ''));
      const row = {
        service: name, user: c.Config.User, readonlyRoot: hc.ReadonlyRootfs, capDrop: (hc.CapDrop ?? []).join(','), capAdd: capAdd.join(','),
        noNewPrivs: (hc.SecurityOpt ?? []).includes('no-new-privileges:true'), privileged: hc.Privileged, memoryMB: Math.round((hc.Memory ?? 0) / 1048576), pids: hc.PidsLimit ?? 0,
        network: hc.NetworkMode, restart: hc.RestartPolicy?.Name
      };
      rows.push(row);
      const nonRoot = name === 'secrets-init' ? true : (row.user && !/^(0|root)(:|$)/.test(row.user));
      check(`e-${name}`, `${name}: ${name === 'secrets-init' ? 'root only for its one job, with CHOWN, FOWNER and DAC_READ_SEARCH and nothing else' : 'non-root'}, read-only root, all capabilities dropped${name === 'secrets-init' ? ' but three' : ''}, no-new-privileges, not privileged`,
        nonRoot && row.readonlyRoot && row.capDrop === 'ALL' && row.noNewPrivs && !row.privileged && (name === 'secrets-init' ? [...capAdd].sort().join(',') === 'CHOWN,DAC_READ_SEARCH,FOWNER' && row.network === 'none' : capAdd.length === 0),
        JSON.stringify(row));
      if (longRunning.includes(name)) check(`e-${name}-limits`, `${name}: memory and process limits set, restart policy unless-stopped`, row.memoryMB > 0 && row.pids > 0 && row.restart === 'unless-stopped', JSON.stringify(row));
    }
    console.log('      hardening evidence (docker inspect):');
    for (const r of rows) console.log(`        ${r.service.padEnd(13)} user=${String(r.user).padEnd(10)} ro=${r.readonlyRoot} capDrop=${r.capDrop} capAdd=${r.capAdd || '-'} nnp=${r.noNewPrivs} mem=${r.memoryMB}MB pids=${r.pids} net=${r.network}`);
    // What the processes really have.
    const live = [];
    for (const name of longRunning) {
      const r = execIn(name, "id -u; grep -E '^(CapEff|NoNewPrivs)' /proc/1/status | tr '\\n' ' '; (touch /proc-test-write 2>&1 || true) | head -1");
      const lines = r.stdout.split('\n');
      const uid = lines[0].trim();
      live.push({ name, uid, rest: lines.slice(1).join(' ').trim() });
      const expectUid = { postgres: '999', api: '10001', web: '10001', proxy: '101' }[name];
      check(`e-live-${name}`, `${name}: runs as uid ${expectUid} with no effective capabilities, NoNewPrivs 1 and a read-only root`, uid === expectUid && /CapEff:\s*0000000000000000/.test(r.stdout) && /NoNewPrivs:\s*1/.test(r.stdout) && /Read-only file system/.test(r.stdout), r.stdout + r.stderr);
    }
    for (const l of live) console.log(`        live ${l.name.padEnd(9)} uid=${l.uid} ${l.rest}`);
    // Only what each service needs
    const listing = (name) => execIn(name, 'ls /run/secrets | tr "\\n" " "').stdout.trim().split(/\s+/).sort().join(',');
    check('e-secrets-api', 'api sees only paseto_local_key, database_url and web_service_key', listing('api') === 'database_url,paseto_local_key,web_service_key', listing('api'));
    check('e-secrets-web', 'web sees no database or API-only secret', !/postgres_password|runtime_database_password|paseto_local_key|database_url/.test(listing('web')) && /session_secret/.test(listing('web')), listing('web'));
    const envNames = (name) => (services[name].Config.Env ?? []).map((e) => e.split('=')[0]);
    check('e-no-owner-creds', 'the api and web containers have no migration credential (no MIGRATION_DATABASE_URL, no owner password in files or environment)', !envNames('api').concat(envNames('web')).some((n) => /MIGRATION|POSTGRES_PASSWORD/.test(n)) && !/postgres_password/.test(listing('api') + listing('web')), `${envNames('api')} ${envNames('web')}`);
    const mounts = Object.values(services).flatMap((c) => (c.Mounts ?? []).map((m) => m.Source + m.Destination));
    check('e-no-docker-sock', 'no container mounts the Docker socket', !mounts.some((m) => /docker\.sock/.test(m)), mounts.join('\n'));
  }

  // pg_hba and role
  {
    const hba = execIn('postgres', 'grep -v "^#" /var/lib/postgresql/data/pg_hba.conf | grep -v "^$"');
    check('f0', 'PostgreSQL pg_hba.conf has no "trust" line: every connection, local socket included, needs a SCRAM password', !/\btrust\b/.test(hba.stdout) && /scram-sha-256/.test(hba.stdout), hba.stdout);
    const psql = (role, sql) => execIn('postgres', `PGPASSWORD=$(cat /run/secrets/${role === 'owner' ? 'postgres_password' : 'runtime_database_password'}) psql -h 127.0.0.1 -U ${role === 'owner' ? 'dynodoc' : 'dynodoc_runtime'} -d dynodoc -v ON_ERROR_STOP=1 -tA -c "${sql}" 2>&1; echo rc=$?`);
    const enc = psql('owner', 'SHOW password_encryption');
    check('f0b', 'passwords are stored as SCRAM-SHA-256', /scram-sha-256/.test(enc.stdout), enc.stdout);
    const read = psql('runtime', 'SELECT count(*) FROM identity');
    check('f1', 'the runtime role can log in and read application tables', /rc=0/.test(read.stdout), read.stdout);
    for (const [label, sql] of [
      ['CREATE TABLE', 'CREATE TABLE smoke_ddl(x int)'],
      ['CREATE TEMP TABLE', 'CREATE TEMP TABLE smoke_tmp(x int)'],
      ['DROP TABLE', 'DROP TABLE identity'],
      ['ALTER TABLE', 'ALTER TABLE identity ADD COLUMN smoke int'],
      ['TRUNCATE the event log', 'TRUNCATE event'],
      ['DELETE from the event log', 'DELETE FROM event'],
      ['UPDATE the event log', 'UPDATE event SET id = id'],
      ['CREATE EXTENSION', 'CREATE EXTENSION IF NOT EXISTS hstore'],
      ['DROP the migration table', 'DROP TABLE _sqlx_migrations']
    ]) {
      const r = psql('runtime', sql);
      check(`f2`, `the runtime role cannot: ${label} (${(r.stdout.match(/ERROR:.*/)?.[0] ?? r.stdout).slice(0, 80)})`, /rc=[1-9]/.test(r.stdout) && /permission denied|must be owner|denied/i.test(r.stdout), r.stdout);
    }
    const cr = compose(['exec', '-T', 'api', 'sh', '-c', 'DATABASE_URL=$(cat /run/secrets/database_url) engine-cli check-runtime 2>&1; echo rc=$?']);
    check('f3', "the API container's own connection passes engine-cli check-runtime (no DDL, no ownership, no extra grants)", /rc=0/.test(cr.stdout), cr.stdout.slice(-600));
    const initLog = compose(['logs', '--no-color', 'init']).stdout;
    check('f4', 'init ran migrate, grant-runtime and check-runtime and reported the database ready', /applying migrations/.test(initLog) && /granting the runtime login/.test(initLog) && /checking the runtime login/.test(initLog) && /database ready/.test(initLog), initLog.slice(-800));
  }

  // (g) sign-in page in self-hosted mode
  {
    const login = await request('/login');
    check('g1', 'the sign-in page shows the organisation name and notice', login.body.includes('Smoke Test Organisation') && login.body.includes('Smoke test installation. Questions: the IT desk, extension 123.'), login.body.slice(0, 300));
    check('g2', 'it carries the configured product name, offers email sign-in and no Google button', login.body.includes('Smoke Docs') && /Email me a sign-in link/.test(login.body) && !/Continue with Google/i.test(login.body), '');
    check('g3', 'it links nothing to the hosted site (no dynodoc.online, no privacy or terms links)', !/dynodoc\.online/.test(login.body) && !/href="\/(privacy|terms)"/.test(login.body), '');
    const hostedPages = await Promise.all(['/features', '/privacy', '/terms', '/pricing', '/sitemap.xml', '/llms.txt'].map((p) => request(p).then((r) => [p, r.status])));
    const telemetry = await request('/telemetry', { method: 'POST', headers: { origin: ORIGIN, 'content-type': 'application/json' }, body: '{}' });
    check('g4', 'marketing, legal, sitemap pages answer 404, and POST /telemetry answers 404 (page metrics off)', hostedPages.every(([, s]) => s === 404) && telemetry.status === 404, JSON.stringify([...hostedPages, ['POST /telemetry', telemetry.status]]));
    const root = await request('/');
    check('g5', "the front page sends a visitor to sign-in (no marketing home page)", [302, 303, 307].includes(root.status) && root.headers.location === '/login', `${root.status} ${root.headers.location}`);
    const opts = await request('/auth/options');
    check('g6', '/auth/options: email on, development off', /"email":true/.test(opts.body) && /"development":false/.test(opts.body), opts.body);
  }

  // (h)(k) browser
  let session = null;
  let flow = null;
  if (playwright) {
    flow = await import(pathToFileURL(path.join(SRC, 'test', 'browser-flow.mjs')).href);
    const sinkId = () => containers().find((id) => docker(['inspect', '-f', '{{index .Config.Labels "com.docker.compose.service"}}', id]).stdout.trim() === 'mailsink');
    const readMail = async () => {
      const r = docker(['exec', sinkId(), 'node', '-e', "fetch('http://127.0.0.1:2580/messages').then(r=>r.text()).then(t=>process.stdout.write(t))"]);
      try { return JSON.parse(r.stdout); } catch { return []; }
    };
    try {
      // (--email is for repeated runs against a kept stack: the API allows five links per address per 15 minutes.)
      session = await flow.signInAndCreate({ playwright, baseUrl: ORIGIN, email: opt('--email', ADMIN), title: TITLE, readMail, step: (s) => info(`browser: ${s}`) });
      check('h', 'email sign-in works end to end: the link was sent by SMTP to the sink, built on the configured origin, and signs in', true);
      check('k', 'Chromium through the proxy (evaluation certificate accepted on purpose): signed in, created a document, saw it listed', true);
      check('k2', 'the browser reported no uncaught page errors', session.problems.length === 0, session.problems.join('\n'));
      const admin = await session.page.goto('/admin');
      info(`first administrator opens /admin: HTTP ${admin?.status()}`);
      check('h2', 'the first administrator (ADMIN_EMAILS) can open /admin', admin && admin.status() < 400, admin?.status());
    } catch (error) {
      check('h', 'email sign-in end to end, and (k) the browser smoke', false, error.stack ?? error.message);
    }
  } else {
    check('h', 'email sign-in end to end through the mail sink -- SKIPPED (no Playwright)', true);
    results[results.length - 1].skipped = true;
    check('k', 'Chromium smoke -- SKIPPED (no Playwright)', true);
    results[results.length - 1].skipped = true;
  }

  // secrets stay out of inspect, config and logs
  {
    const values = Object.values(secretValues()).flatMap((v) => v.split('\n')).map((v) => v.trim()).filter((v) => v.length >= 20);
    const inspectText = JSON.stringify(inspectAll());
    const configText = out(compose(['config']));
    const logText = out(compose(['logs', '--no-color']));
    const leak = (text) => values.filter((v) => text.includes(v)).length;
    check('s1', `no secret value appears in docker inspect output (${values.length} values checked)`, leak(inspectText) === 0, `${leak(inspectText)} found`);
    check('s2', 'no secret value appears in `docker compose config`', leak(configText) === 0, `${leak(configText)} found`);
    check('s3', 'no secret value appears in any container log', leak(logText) === 0, `${leak(logText)} found`);
    const envFile = readFileSync(path.join(BUNDLE, '.env'), 'utf8');
    check('s4', 'the generated .env holds no secret value', values.filter((v) => envFile.includes(v)).length === 0, '');
  }

  // (i) restarts
  if (session) {
    const still = async (label) => {
      const healthy = await waitFor(label, async () => (await request('/healthz')).status === 200, 120000);
      if (!healthy) return false;
      try { await flow.documentStillThere({ page: session.page, title: TITLE, text: TEXT }); return true; } catch (error) { info(`${label}: ${error.message.split('\n')[0]}`); return false; }
    };
    let r = compose(['restart']);
    await waitFor('healthy after restart', () => !compose(['ps', '--format', '{{.Health}}']).stdout.includes('starting'), 180000);
    check('i0', 'docker compose restart (every service at once) leaves the document and the sign-in intact', r.status === 0 && await still('all'), out(r));
    for (const name of ['proxy', 'web', 'api', 'postgres']) {
      r = compose(['restart', name]);
      await waitFor(`${name} healthy`, () => !compose(['ps', '--format', '{{.Health}}']).stdout.includes('starting'), 120000);
      check(`i-${name}`, `restarting ${name} alone leaves the data intact and the stack working`, r.status === 0 && await still(name), out(r));
    }
  }

  // (j) down / up
  {
    const fingerprintBefore = (await tlsProbe({})).cert?.fingerprint256;
    const idsBefore = Object.fromEntries(Object.entries(byService()).map(([n, c]) => [n, c.Id]));
    const again = await up('up again, nothing changed');
    const idsAfter = Object.fromEntries(Object.entries(byService()).map(([n, c]) => [n, c.Id]));
    check('j1', `docker compose up -d again with nothing changed is a no-op for the running services (${again.seconds} s)`, again.ok && ['postgres', 'api', 'web', 'proxy'].every((n) => idsBefore[n] === idsAfter[n]), again.text.slice(-800));
    const down = compose(['down']);
    check('j2', 'docker compose down removes containers and networks but keeps the volumes', down.status === 0 && docker(['volume', 'ls', '-q', '--filter', `label=com.docker.compose.project=${PROJECT}`]).stdout.includes('database'), out(down));
    const second = await up('up after down');
    check('j3', `docker compose up -d after down is idempotent: migrations re-checked, grants re-applied, stack healthy (${second.seconds} s)`, second.ok, second.text.slice(-1500));
    const log2 = compose(['logs', '--no-color', 'init']).stdout;
    check('j4', 'the second init run finds nothing to migrate and still passes check-runtime', /database ready/.test(log2), log2.slice(-500));
    check('j5', 'the evaluation certificate survived (the same one is served: not regenerated on every start)', fingerprintBefore && (await tlsProbe({})).cert?.fingerprint256 === fingerprintBefore, '');
    if (session) {
      const ok2 = await waitFor('after down/up', async () => (await request('/healthz')).status === 200, 120000);
      let kept = false;
      try { if (ok2) { await flow.documentStillThere({ page: session.page, title: TITLE, text: TEXT }); kept = true; } } catch (error) { info(error.message.split('\n')[0]); }
      check('j6', 'the document and the signed-in session are still there after down and up', kept, '');
    }
  }

  // failures stop the stack
  {
    const envPath = path.join(BUNDLE, '.env');
    const original = readFileSync(envPath, 'utf8');
    let r = await up('provided without files', [], { DYNODOC_TLS_MODE: 'provided' });
    check('x1', 'DYNODOC_TLS_MODE=provided without tls/ files: up fails, secrets-init says what is missing', !r.ok && /tls\/fullchain\.pem is missing or empty/.test(out(compose(['logs', '--no-color', 'secrets-init']))), r.text.slice(-600) + out(compose(['logs', '--no-color', 'secrets-init'])).slice(-400));
    r = await up('restore');
    check('x1b', 'and the stack comes back with the normal setting', r.ok, r.text.slice(-500));

    writeFileSync(envPath, `${original}\nDYNODOC_TELEMETRY=maybe\n`);
    r = await up('bad .env');
    const pre = out(compose(['logs', '--no-color', 'preflight']));
    check('x2', 'a mistake in .env fails the preflight job and stops the stack (before the database is touched)', !r.ok && /invalid configuration/.test(pre) && /DYNODOC_TELEMETRY/.test(pre), r.text.slice(-500) + pre.slice(-500));
    writeFileSync(envPath, original);

    const pwPath = path.join(BUNDLE, 'secrets', 'postgres_password');
    const pwOriginal = readFileSync(pwPath, 'utf8');
    writeFileSync(pwPath, 'wrong-owner-password-wrong-owner-password\n');
    r = await up('wrong owner password');
    const roles = out(compose(['logs', '--no-color', 'db-roles']));
    check('x3', 'a wrong database owner password fails db-roles and stops the stack with the database untouched', !r.ok && /password authentication failed/.test(roles), r.text.slice(-400) + roles.slice(-400));
    writeFileSync(pwPath, pwOriginal);
    r = await up('restore');
    check('x3b', 'with the right password the stack comes back', r.ok, r.text.slice(-500));
    const same = await request('/healthz');
    check('x4', 'the web server answers again after the restore', same.status === 200, same.status);
  }

  // the mail check, from where the web server sits
  {
    const r = compose(['--profile', 'maintenance', 'run', '--rm', '-T', 'mailcheck', '--to', 'admin@smoke.test', '--json']);
    let parsed = null;
    try { parsed = JSON.parse(r.stdout.trim().split('\n').pop()); } catch { /* reported below */ }
    check('m1', 'docker compose run --rm mailcheck --to ... does the real SMTP handshake with the configured server and passes', r.status === 0 && parsed?.ok === true, out(r).slice(-800));
  }

  // a provided certificate at stack level: refusal, start, and the documented renewal
  {
    const proxyImage = `${opt('--registry', 'dynodoc-local')}/proxy:${opt('--version', 'dev')}`;
    const tlsDir = path.join(BUNDLE, 'tls');
    const mkcert = (name) => docker(['run', '--rm', '--user', '0', '--entrypoint', 'sh', '-v', `${tlsDir}:/out`, proxyImage, '-c',
      `openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout /out/${name}.key -out /out/${name}.pem -days 30 -subj /CN=localhost -addext subjectAltName=DNS:localhost >/dev/null 2>&1; chmod 600 /out/${name}.key`]);
    mkcert('a'); mkcert('b');
    const put = (cert, key) => { writeFileSync(path.join(tlsDir, 'fullchain.pem'), readFileSync(path.join(tlsDir, `${cert}.pem`))); writeFileSync(path.join(tlsDir, 'privkey.pem'), readFileSync(path.join(tlsDir, `${key}.key`))); };
    const env = { DYNODOC_TLS_MODE: 'provided' };
    const composeP = (extra) => compose(extra, { env: { ...cleanEnv(), ...env } });
    put('a', 'b'); // a certificate with someone else's key
    let r = await up('provided, wrong key', [], env);
    const proxyLog = out(compose(['logs', '--no-color', '--tail', '40', 'proxy']));
    check('t1', 'provided mode with a key that does not belong to the certificate: up fails, the proxy says why, nothing listens', !r.ok && /does not match the certificate/.test(proxyLog) && (await tlsProbe({})).cert === undefined, r.text.slice(-300) + proxyLog.slice(-600));
    put('a', 'a');
    r = await up('provided, good', [], env);
    const pa = await tlsProbe({});
    const head = await request('/login');
    check('t2', 'provided mode with a good certificate: the stack comes up and serves that certificate with a one-year HSTS', r.ok && pa.cert?.subject?.CN === 'localhost' && !/EVALUATION/.test(JSON.stringify(pa.cert?.subject)) && head.headers['strict-transport-security'] === 'max-age=31536000' && !head.headers['x-dynodoc-tls-mode'], `${r.text.slice(-500)} ${JSON.stringify(pa.cert?.subject)} ${head.headers['strict-transport-security']}`);
    // The documented renewal: replace the files, stage them, check, reload; no request may fail meanwhile.
    put('b', 'b');
    let failed = 0, total = 0, stop = false;
    const loop = (async () => { while (!stop) { total++; if ((await request('/healthz')).status !== 200) failed++; await sleep(50); } })();
    const staged = composeP(['run', '--rm', '--no-deps', '-T', 'secrets-init']);
    const checked = composeP(['run', '--rm', '--no-deps', '-T', 'proxy', 'check']);
    const reloaded = composeP(['exec', '-T', 'proxy', 'proxy-entrypoint', 'reload']);
    await sleep(1000);
    stop = true; await loop;
    const pb = await tlsProbe({});
    check('t3', `the documented renewal (replace files, secrets-init, proxy check, proxy-entrypoint reload) serves the new certificate with no failed request (${total} requests)`,
      staged.status === 0 && checked.status === 0 && reloaded.status === 0 && pb.cert?.fingerprint256 && pb.cert.fingerprint256 !== pa.cert?.fingerprint256 && failed === 0, `${out(staged)}\n${out(checked)}\n${out(reloaded)}\nfailed=${failed}`);
    // A bad replacement is checked and refused; the old certificate keeps serving.
    put('a', 'b');
    const stagedBad = composeP(['run', '--rm', '--no-deps', '-T', 'secrets-init']);
    const checkedBad = composeP(['run', '--rm', '--no-deps', '-T', 'proxy', 'check']);
    const reloadedBad = composeP(['exec', '-T', 'proxy', 'proxy-entrypoint', 'reload']);
    const pc = await tlsProbe({});
    check('t4', 'a bad replacement fails `proxy check`, is not applied by reload, and the previous certificate keeps serving', stagedBad.status === 0 && checkedBad.status !== 0 && reloadedBad.status !== 0 && pc.cert?.fingerprint256 === pb.cert?.fingerprint256 && /NOT applied/.test(out(reloadedBad)), `${out(checkedBad)}\n${out(reloadedBad)}`);
    // Back to the evaluation setting for the rest of the run.
    for (const f of readdirSync(tlsDir)) if (f !== 'README.md') rmSync(path.join(tlsDir, f), { force: true });
    r = await up('back to evaluation');
    check('t5', 'back to evaluation mode: the stack comes up again with the labelled certificate', r.ok && /EVALUATION/.test(JSON.stringify((await tlsProbe({})).cert?.subject ?? {})), r.text.slice(-400));
  }

  // backup by hand and restore (the README's procedure)
  {
    const pgsh = (cmd, input) => spawnSync('docker', composeArgs(['exec', '-T', 'postgres', 'sh', '-c', `PGPASSWORD=$(cat /run/secrets/postgres_password); export PGPASSWORD; ${cmd}`]), { cwd: BUNDLE, env: cleanEnv(), encoding: null, input, maxBuffer: 1024 * 1024 * 1024 });
    const count = (sql) => Number(pgsh(`psql -h 127.0.0.1 -U dynodoc -d dynodoc -tAc "${sql}"`).stdout.toString().trim());
    const before = { events: count('select count(*) from event'), identities: count('select count(*) from identity'), docs: count('select count(*) from document') };
    info(`before backup: ${JSON.stringify(before)}`);
    const dump = pgsh('pg_dump -h 127.0.0.1 -U dynodoc -Fc dynodoc');
    const file = path.join(BUNDLE, 'backups', 'smoke.dump');
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, dump.stdout);
    check('z1', `the by-hand backup (pg_dump -Fc through docker compose exec) writes a custom-format dump (${(dump.stdout.length / 1024).toFixed(0)} KiB)`, dump.status === 0 && dump.stdout.subarray(0, 5).toString() === 'PGDMP' && dump.stdout.length > 10000, dump.stderr.toString());
    // Restore over the production database: stop the writers, recreate the database, load the dump, start.
    compose(['stop', 'proxy', 'web', 'api']);
    const reset = pgsh('psql -h 127.0.0.1 -U dynodoc -d postgres -v ON_ERROR_STOP=1 -c "DROP DATABASE dynodoc" -c "CREATE DATABASE dynodoc"');
    const restore = pgsh('pg_restore -h 127.0.0.1 -U dynodoc -d dynodoc --no-owner --exit-on-error', readFileSync(file));
    check('z2', 'restore: database recreated and the dump loaded with pg_restore, no errors', reset.status === 0 && restore.status === 0, `${reset.stderr}${restore.stderr}`.slice(0, 800));
    const upAgain = await up('after restore');
    check('z3', 'docker compose up -d after a restore re-applies the login and grants and starts everything (init is the same step as an upgrade)', upAgain.ok, upAgain.text.slice(-800));
    const after = { events: count('select count(*) from event'), identities: count('select count(*) from identity'), docs: count('select count(*) from document') };
    check('z4', 'the restored database has the same event, identity and document counts', JSON.stringify(before) === JSON.stringify(after) && before.events > 0, `${JSON.stringify(before)} vs ${JSON.stringify(after)}`);
    const verify = compose(['--profile', 'maintenance', 'run', '--rm', '-T', 'database-admin', 'verify-snapshots', '--all']);
    check('z5', 'docker compose run --rm database-admin verify-snapshots --all passes on the restored database', verify.status === 0, out(verify).slice(-600));
    const cr2 = compose(['--profile', 'maintenance', 'run', '--rm', '-T', 'database-admin', 'check-runtime']);
    check('z6', 'database-admin check-runtime passes after the restore (runtime login re-granted)', cr2.status === 0, out(cr2).slice(-600));
    if (session) {
      const ok3 = await waitFor('after restore', async () => (await request('/healthz')).status === 200, 120000);
      let kept = false;
      try { if (ok3) { await flow.documentStillThere({ page: session.page, title: TITLE, text: TEXT }); kept = true; } } catch (error) { info(error.message.split('\n')[0]); }
      check('z7', 'after the restore the document is there and the browser session still works', kept, '');
    }
  }

  if (session) await session.browser.close();
}

let tmpDir = '';
try {
  await main();
} catch (error) {
  check('fatal', 'the smoke test ran to the end', false, error.stack ?? error.message);
} finally {
  if (!has('--keep')) {
    if (BUNDLE) {
      compose(['down', '-v', '--remove-orphans']);
      docker(['compose', '-p', `${PROJECT}-build`, 'down', '-v']);
    }
    tmpDir = BUNDLE ? path.dirname(BUNDLE) : '';
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  } else info(`kept: project ${PROJECT}, ${BUNDLE}`);
}
const failed = results.filter((r) => !r.ok);
const skipped = results.filter((r) => r.skipped);
console.log(`\n${results.length - failed.length} passed (${skipped.length} skipped), ${failed.length} failed, ${Math.round((Date.now() - t0) / 1000)} s`);
const report = opt('--report');
if (report) writeFileSync(report, JSON.stringify({ project: PROJECT, origin: ORIGIN, seconds: Math.round((Date.now() - t0) / 1000), results }, null, 2));
process.exit(failed.length ? 1 : 0);
