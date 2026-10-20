// Run against the isolated CI web server. Never use production session secrets.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import http from 'node:http';
import net from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { EncryptJWT } from 'jose';

assert.equal(process.platform, 'linux', 'This check requires Linux and Nginx.');
assert.equal(process.env.APP_ENV, 'test', 'Use an isolated test environment.');
const secret = process.env.SESSION_SECRET;
assert.ok(secret?.length >= 32, 'The isolated server needs SESSION_SECRET.');
const upstream = new URL(process.env.PROXY_TEST_UPSTREAM || 'http://127.0.0.1:4173');
assert.equal(upstream.protocol, 'http:');
assert.equal(upstream.hostname, '127.0.0.1', 'The upstream must be local.');
const templates = new URL('../../../infra/demo/', import.meta.url);
const scratch = await mkdtemp(join(tmpdir(), 'dynodoc-proxy-'));
const fixture = http.createServer((_request, response) => {
  // Model asset preloads plus session/OAuth cookie rotation without any account.
  response.setHeader('link', `<${'x'.repeat(8000)}>; rel=modulepreload`);
  response.setHeader('set-cookie', [
    `dynodoc_proxy_fixture=${'x'.repeat(1200)}; Path=/; HttpOnly; SameSite=Lax`,
    'oauth_state=; Path=/auth; Max-Age=0; HttpOnly; SameSite=Lax'
  ]);
  response.end('synthetic header fixture');
});
let nginx;
let closed;
let nginxErrors = '';

async function reservePort() {
  const server = net.createServer();
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return server;
}
function request(url, cookie) {
  return new Promise((resolve, reject) => {
    const pending = http.get(
      url,
      { headers: cookie ? { cookie: `dynodoc_session=${cookie}` } : {}, timeout: 10000 },
      (response) => {
        let headerBytes = Buffer.byteLength(
          `HTTP/${response.httpVersion} ${response.statusCode} ${response.statusMessage}\r\n\r\n`
        );
        for (let i = 0; i < response.rawHeaders.length; i += 2)
          headerBytes += Buffer.byteLength(
            `${response.rawHeaders[i]}: ${response.rawHeaders[i + 1]}\r\n`
          );
        response.resume();
        response.on('end', () =>
          resolve({ status: response.statusCode, headers: response.headers, headerBytes })
        );
      }
    );
    pending.on('timeout', () => pending.destroy(new Error('Proxy request timed out.')));
    pending.on('error', reject);
  });
}
function checkSecurity(response, pane = false) {
  assert.equal(response.headers['strict-transport-security'], 'max-age=31536000');
  assert.equal(response.headers['x-content-type-options'], 'nosniff');
  const csp = response.headers['content-security-policy'];
  assert.ok(csp && !csp.includes(','), 'One complete CSP, without duplicate policies.');
  assert.match(csp, /object-src 'none'/);
  assert.doesNotMatch(csp, /script-src[^;]*unsafe-(inline|eval)/);
  assert.equal(response.headers['x-frame-options'], pane ? undefined : 'SAMEORIGIN');
  if (!pane) assert.match(csp, /frame-ancestors 'self'(?:;|$)/);
}
async function stopNginx() {
  if (!nginx) return;
  const force = setTimeout(() => nginx.kill('SIGTERM'), 5000);
  nginx.kill('SIGQUIT');
  await closed;
  clearTimeout(force);
  nginx = undefined;
}
async function startNginx(config, appURL) {
  const path = join(scratch, 'nginx.conf');
  await writeFile(path, config);
  const checked = spawnSync('nginx', ['-p', `${scratch}/`, '-c', path, '-e', 'stderr', '-t'], {
    encoding: 'utf8'
  });
  assert.equal(checked.status, 0, checked.stderr || checked.error?.message);
  nginxErrors = '';
  nginx = spawn('nginx', ['-p', `${scratch}/`, '-c', path, '-e', 'stderr', '-g', 'daemon off;'], {
    stdio: ['ignore', 'ignore', 'pipe']
  });
  closed = new Promise((resolve) => nginx.once('close', resolve));
  nginx.on('error', (error) => (nginxErrors += error.message));
  nginx.stderr.on('data', (data) => (nginxErrors = (nginxErrors + data).slice(-8192)));
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      if ((await request(`${appURL}/healthz`)).status === 200) return;
    } catch {
      // The isolated worker may still be binding its port.
    }
    await delay(100);
  }
  throw new Error(`Nginx did not start: ${nginxErrors}`);
}

try {
  await new Promise((resolve) => fixture.listen(0, '127.0.0.1', resolve));
  const fixtureURL = `http://127.0.0.1:${fixture.address().port}`;
  const reserved = await Promise.all(Array.from({ length: 4 }, reservePort));
  const ports = reserved.map((server) => server.address().port);
  await Promise.all(reserved.map((server) => new Promise((resolve) => server.close(resolve))));
  const sites = await Promise.all(
    ['nginx-app.conf', 'nginx-admin.conf'].map(async (name, index) => {
      // Keep every production location and its inheritance. Only replace TLS,
      // listening ports and upstream destinations for this private HTTP fixture.
      let source = await readFile(new URL(name, templates), 'utf8');
      source = source
        .replace('listen 80;', `listen 127.0.0.1:${ports[index * 2]};`)
        .replace('listen 443 ssl;', `listen 127.0.0.1:${ports[index * 2 + 1]};`)
        .replace(/^\s*ssl_[^;]+;\r?$/gm, '')
        .replace(/access_log [^;]+;/g, 'access_log off;')
        .replace(/http:\/\/127\.0\.0\.1:308[89]/g, upstream.origin);
      source = source.replace(
        /(location \/auth\/ \{[\s\S]*?proxy_pass )[^;]+;/,
        `$1${fixtureURL};`
      );
      return source;
    })
  );
  const config = `worker_processes 1;
pid ${scratch}/nginx.pid;
error_log stderr error;
events { worker_connections 64; }
http {
access_log off;
client_body_temp_path ${scratch}/body;
proxy_temp_path ${scratch}/proxy;
fastcgi_temp_path ${scratch}/fastcgi;
uwsgi_temp_path ${scratch}/uwsgi;
scgi_temp_path ${scratch}/scgi;
${sites.join('\n')}
}
`;
  const appURL = `http://127.0.0.1:${ports[1]}`;
  const adminURL = `http://127.0.0.1:${ports[3]}`;

  // Prove the fixture catches the original failure, independently of bundle size.
  await startNginx(
    config.replace(/^\s*proxy_(?:buffer_size|buffers|busy_buffers_size) [^;]+;\r?$/gm, ''),
    appURL
  );
  const failure = await request(`${appURL}/auth/header-check`);
  assert.equal(failure.status, 502);
  checkSecurity(failure);
  assert.equal((await request(`${adminURL}/auth/header-check`)).status, 502);
  await stopNginx();

  await startNginx(config, appURL);
  const cookie = await new EncryptJWT({
    access_token: 'invalid-synthetic-header-check',
    refresh_token: 'invalid-synthetic-header-check',
    expires_in: 600,
    expires_at: Date.now() + 600000
  })
    .setProtectedHeader({ alg: 'dir', enc: 'A256GCM' })
    .setIssuedAt()
    .setExpirationTime('10m')
    .encrypt(createHash('sha256').update(secret).digest());
  const fixtureId = '11111111-2222-3333-4444-555555555555';
  for (const path of [
    '/workspace',
    '/projects',
    '/profile',
    `/projects/${fixtureId}`,
    `/projects/${fixtureId}/files/${fixtureId}`
  ]) {
    const direct = await request(`${upstream.origin}${path}`, cookie);
    const proxied = await request(`${appURL}${path}`, cookie);
    assert.equal(direct.status, 200);
    assert.equal(proxied.status, 200);
    checkSecurity(proxied);
    assert.match(proxied.headers['content-security-policy'], /script-src[^;]+(?:nonce-|sha256-)/);
    assert.equal(proxied.headers.link, direct.headers.link, 'Preserve asset preload headers.');
    assert.equal(proxied.headers['x-frame-options'], 'SAMEORIGIN');
    console.log(`${path}: direct headers ${direct.headerBytes} bytes; proxy 200`);
  }
  for (const url of [appURL, adminURL]) {
    const response = await request(`${url}/auth/header-check`);
    assert.equal(response.status, 200);
    checkSecurity(response);
    assert.equal(response.headers['set-cookie'].length, 2);
    assert.ok(response.headers.link.length > 8000);
    console.log(`auth preload/cookie fixture: ${response.headerBytes} bytes; proxy 200`);
  }
  assert.equal((await request(`${appURL}/workspace`)).status, 303, 'Keep the sign-in boundary.');
  for (const host of ['word', 'excel', 'powerpoint', 'commands.html']) {
    const response = await request(`${appURL}/connectors/${host}`);
    assert.equal(response.status, 200);
    checkSecurity(response, true);
    assert.match(
      response.headers['content-security-policy'],
      /script-src[^;]+appsforoffice\.microsoft\.com/
    );
    assert.match(response.headers['content-security-policy'], /frame-ancestors/);
  }
  for (const [path, status] of [
    ['/workspace', 303],
    ['/theme-init.js', 200],
    ['/missing-security-fixture', 404]
  ]) {
    const response = await request(`${appURL}${path}`);
    assert.equal(response.status, status);
    checkSecurity(response);
  }
  // The service worker runs under the policy of its own script response. It must be allowed
  // to fetch the app's own files, or its install would fail (or, worse, every page load
  // through it would). The offline page keeps its inline styles for a direct visit.
  const worker = await request(`${appURL}/service-worker.js`);
  assert.equal(worker.status, 200);
  checkSecurity(worker);
  assert.match(worker.headers['content-security-policy'], /connect-src 'self'(?:;|$)/);
  const offline = await request(`${appURL}/offline.html`);
  assert.equal(offline.status, 200);
  checkSecurity(offline);
  assert.match(offline.headers['content-security-policy'], /style-src 'unsafe-inline'/);
  assert.doesNotMatch(nginxErrors, /upstream sent too big header/);
  console.log('Production proxy header regression passed. No account or document was accessed.');
} finally {
  await stopNginx();
  await new Promise((resolve) => fixture.close(resolve));
  await rm(scratch, { recursive: true, force: true });
}
