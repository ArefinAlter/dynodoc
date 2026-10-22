// Tests the bundled proxy's TLS handling on its own: a stub web server, the real proxy image,
// real certificates. No database, no application. Needs Docker and Node.
//
//   node infra/selfhosted/test/proxy-tls.mjs            (images dynodoc-local/proxy:dev and web:dev)
//   PROXY_IMAGE=... WEB_IMAGE=... node infra/selfhosted/test/proxy-tls.mjs
//
// What it proves (each line is an assertion printed as ok/FAIL):
//   provided   a good chain is served; a bad key makes the proxy refuse to start with a message
//              and publish nothing; a renewed certificate put in place is served within the check
//              interval with no failed request while it happens; a bad replacement is refused
//              and the old certificate keeps being served
//   evaluation a self-signed certificate is made for the name with SANs and a label that says it
//              is not for production, is reused after a restart, and is replaced before expiry
//              with no failed request
import { spawnSync } from 'node:child_process';
import tls from 'node:tls';
import https from 'node:https';

const PROXY = process.env.PROXY_IMAGE ?? 'dynodoc-local/proxy:dev';
const WEB = process.env.WEB_IMAGE ?? 'dynodoc-local/web:dev';
const ID = `pt${process.pid}`;
const NET = `${ID}-net`;
const PORT = Number(process.env.PROXY_TEST_PORT ?? 28444);
const results = { pass: 0, fail: 0 };
const containers = [];
const volumes = [];

const docker = (args, options = {}) => spawnSync('docker', args, { encoding: 'utf8', ...options });
const ok = (condition, label, detail = '') => {
  results[condition ? 'pass' : 'fail']++;
  console.log(`${condition ? 'ok  ' : 'FAIL'}  ${label}${condition || !detail ? '' : `\n        ${String(detail).split('\n').join('\n        ')}`}`);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function peer() {
  return new Promise((resolve) => {
    const socket = tls.connect({ host: '127.0.0.1', port: PORT, servername: 'localhost', rejectUnauthorized: false }, () => {
      const cert = socket.getPeerCertificate();
      socket.end();
      resolve({ fingerprint: cert.fingerprint256, subject: cert.subject, subjectaltname: cert.subjectaltname, validTo: cert.valid_to });
    });
    socket.on('error', () => resolve(null));
    socket.setTimeout(3000, () => { socket.destroy(); resolve(null); });
  });
}
function get(path = '/') {
  return new Promise((resolve) => {
    const request = https.get({ host: '127.0.0.1', port: PORT, path, servername: 'localhost', rejectUnauthorized: false, agent: false, headers: { host: `localhost:${PORT}` } }, (response) => {
      response.resume();
      resolve(response.statusCode);
    });
    request.on('error', () => resolve(0));
    request.setTimeout(3000, () => { request.destroy(); resolve(0); });
  });
}

function startWeb() {
  // A stub web server: plain answers, a server-sent event stream on /sse (an event at once, then
  // one every 400 ms for 1.2 s), and a WebSocket-style upgrade that answers 101 and sends a word.
  const script = `
const http = require('http');
const s = http.createServer((q, r) => {
  if (q.url === '/sse') {
    r.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
    r.write('data: first\\n\\n');
    let n = 0;
    const t = setInterval(() => { r.write('data: tick' + (++n) + '\\n\\n'); if (n >= 3) { clearInterval(t); r.end(); } }, 400);
    return;
  }
  r.setHeader('content-type', 'text/plain'); r.end('stub');
});
s.on('upgrade', (q, sock) => { sock.write('HTTP/1.1 101 Switching Protocols\\r\\nUpgrade: websocket\\r\\nConnection: Upgrade\\r\\n\\r\\n'); sock.write('hello-ws'); sock.end(); });
s.listen(3000);`;
  docker(['run', '-d', '--name', `${ID}-web`, '--network', NET, '--network-alias', 'web', WEB, 'node', '-e', script]);
  containers.push(`${ID}-web`);
}
function startProxy(name, mode, volume, extraEnv = []) {
  const args = [
    'run', '-d', '--name', name, '--network', NET, '-p', `127.0.0.1:${PORT}:8443`,
    '--read-only', '--tmpfs', '/tmp', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true',
    '-e', `ORIGIN=https://localhost:${PORT}`, '-e', `DYNODOC_TLS_MODE=${mode}`, '-e', 'DYNODOC_TLS_RELOAD_SECONDS=2',
    ...extraEnv.flatMap((e) => ['-e', e]),
    ...(mode === 'provided' ? ['-v', `${volume}:/etc/dynodoc/tls:ro`] : ['-v', `${volume}:/var/lib/dynodoc-tls`]),
    PROXY
  ];
  const r = docker(args);
  containers.push(name);
  return r;
}
const logs = (name) => { const r = docker(['logs', name]); return `${r.stdout}${r.stderr}`; };
const stop = (name) => docker(['rm', '-f', name]);
async function waitServing(timeoutMs = 15000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) { const p = await peer(); if (p) return p; await sleep(300); }
  return null;
}
async function waitFingerprintChange(from, timeoutMs = 20000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) { const p = await peer(); if (p && p.fingerprint !== from) return p; await sleep(300); }
  return null;
}
/** Requests in a loop while `action` runs; returns the count of failures. */
async function noDowntime(action) {
  let stopFlag = false, total = 0, failed = 0;
  const loop = (async () => {
    while (!stopFlag) { total++; if ((await get('/')) !== 200) failed++; await sleep(40); }
  })();
  await action();
  stopFlag = true;
  await loop;
  return { total, failed };
}

// Fixtures: two valid chains for localhost (A, then B for the renewal), one with a mismatched key.
const FIXTURE = `
set -e
cd /out
mk() { openssl "$@" >/dev/null 2>&1; }
mk req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout ca.key -out ca.pem -days 3650 -subj "/CN=Test Root" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"
leaf() {
  mk req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout $1.key -out $1.csr -subj "/CN=localhost"
  printf 'subjectAltName=DNS:localhost\\nbasicConstraints=CA:FALSE\\n' > $1.ext
  mk x509 -req -in $1.csr -CA ca.pem -CAkey ca.key -CAcreateserial -out $1.pem -days 365 -extfile $1.ext
  cat $1.pem ca.pem > $1.chain.pem
}
leaf a; leaf b; leaf c
mkdir -p good renewed mismatch
cp a.chain.pem good/fullchain.pem;      cp a.key good/privkey.pem
cp b.chain.pem renewed/fullchain.pem;   cp b.key renewed/privkey.pem
cp b.chain.pem mismatch/fullchain.pem;  cp c.key mismatch/privkey.pem
chmod -R a+rX /out
`;
const SWAP = (set) => `cp /out/${set}/fullchain.pem /dest/fullchain.pem.new && cp /out/${set}/privkey.pem /dest/privkey.pem.new && mv /dest/privkey.pem.new /dest/privkey.pem && mv /dest/fullchain.pem.new /dest/fullchain.pem && chmod a+r /dest/*`;

async function main() {
  if (docker(['image', 'inspect', PROXY]).status !== 0 || docker(['image', 'inspect', WEB]).status !== 0) {
    console.error(`Images ${PROXY} and ${WEB} must exist (build them with compose.build.yml).`);
    process.exit(2);
  }
  docker(['network', 'create', NET]);
  const fixtures = `${ID}-fix`, provided = `${ID}-prov`, evalstate = `${ID}-eval`;
  volumes.push(fixtures, provided, evalstate);
  docker(['volume', 'create', fixtures]); docker(['volume', 'create', provided]); docker(['volume', 'create', evalstate]);
  let r = docker(['run', '--rm', '--user', '0', '--entrypoint', 'sh', '-v', `${fixtures}:/out`, PROXY, '-c', FIXTURE]);
  if (r.status !== 0) { console.error('fixture generation failed', r.stderr); process.exit(2); }
  const seed = (set) => docker(['run', '--rm', '--user', '0', '--entrypoint', 'sh', '-v', `${fixtures}:/out:ro`, '-v', `${provided}:/dest`, PROXY, '-c', SWAP(set)]);
  startWeb();

  // ---- provided: a bad key is refused, nothing listens ----
  seed('mismatch');
  startProxy(`${ID}-proxy`, 'provided', provided);
  await sleep(2500);
  let state = docker(['inspect', '-f', '{{.State.Status}} {{.State.ExitCode}}', `${ID}-proxy`]).stdout.trim();
  const refusedLog = logs(`${ID}-proxy`);
  ok(/^(exited|restarting) 78$/.test(state) || state.startsWith('exited'), 'provided, key does not match: the proxy exits (does not run)', state);
  ok(/does not match the certificate/.test(refusedLog) && /will not start/.test(refusedLog), 'provided, key does not match: the log says why in plain words', refusedLog);
  ok((await peer()) === null, 'provided, key does not match: nothing listens on the published port');
  stop(`${ID}-proxy`);

  // ---- provided: good chain served; renewal with no downtime; a bad replacement is refused ----
  seed('good');
  startProxy(`${ID}-proxy`, 'provided', provided);
  const first = await waitServing();
  ok(first !== null, 'provided, good chain: the proxy serves HTTPS');
  if (first) {
    ok(/localhost/.test(first.subject?.CN ?? ''), 'provided: the served certificate is the operator\'s, not a generated one', JSON.stringify(first));
    const down = await noDowntime(async () => { await sleep(500); seed('renewed'); await waitFingerprintChange(first.fingerprint); await sleep(500); });
    const second = await peer();
    ok(second && second.fingerprint !== first.fingerprint, 'provided: a renewed certificate is served after the files are replaced', JSON.stringify(second));
    ok(down.failed === 0 && down.total > 10, `provided: no request failed while the certificate was replaced (${down.total} requests)`, `${down.failed} failed`);
    ok(/Configuration reloaded/.test(logs(`${ID}-proxy`)), 'provided: the log records the reload');
    seed('mismatch');
    await sleep(5000);
    const third = await peer();
    ok(third && third.fingerprint === second.fingerprint, 'provided: a bad replacement is not applied; the previous certificate keeps being served');
    ok(/NOT applied/.test(logs(`${ID}-proxy`)), 'provided: the log says the bad replacement was not applied', logs(`${ID}-proxy`).split('\n').slice(-12).join('\n'));
    ok((await get('/')) === 200, 'provided: requests still succeed after a refused replacement');

    // Server-sent events must not be buffered, and WebSocket upgrades must get through.
    const sse = await new Promise((resolve) => {
      const started = Date.now();
      const chunks = [];
      const request = https.get({ host: '127.0.0.1', port: PORT, path: '/sse', servername: 'localhost', rejectUnauthorized: false, agent: false, headers: { host: `localhost:${PORT}` } }, (response) => {
        response.on('data', (c) => chunks.push({ at: Date.now() - started, text: c.toString() }));
        response.on('end', () => resolve({ chunks, total: Date.now() - started, type: response.headers['content-type'] }));
      });
      request.on('error', () => resolve({ chunks, total: 0, type: '' }));
    });
    ok(sse.type === 'text/event-stream' && sse.chunks.length >= 3 && sse.chunks[0].at < 600 && sse.total > 1000 && sse.chunks[0].at < sse.total - 800,
      'provided: a server-sent event stream arrives event by event, not buffered until it ends', JSON.stringify(sse));
    const ws = await new Promise((resolve) => {
      const request = https.request({ host: '127.0.0.1', port: PORT, path: '/socket', servername: 'localhost', rejectUnauthorized: false, agent: false,
        headers: { host: `localhost:${PORT}`, connection: 'Upgrade', upgrade: 'websocket', 'sec-websocket-version': '13', 'sec-websocket-key': 'dGhlIHNhbXBsZSBub25jZQ==' } });
      request.on('upgrade', (response, socket, head) => {
        let body = head.toString();
        socket.on('data', (d) => { body += d.toString(); });
        socket.on('end', () => resolve({ status: response.statusCode, upgrade: response.headers.upgrade, body }));
        socket.on('error', () => resolve({ status: response.statusCode, upgrade: response.headers.upgrade, body }));
      });
      request.on('response', (response) => resolve({ status: response.statusCode, body: '' }));
      request.on('error', (e) => resolve({ status: 0, body: e.message }));
      request.end();
    });
    ok(ws.status === 101 && /websocket/i.test(ws.upgrade ?? '') && /hello-ws/.test(ws.body), 'provided: a WebSocket upgrade is passed through (101 and the data after it)', JSON.stringify(ws));
  }
  stop(`${ID}-proxy`);

  // ---- evaluation: made on first start, labelled, reused, renewed before expiry ----
  startProxy(`${ID}-eval`, 'evaluation', evalstate, ['DYNODOC_HSTS_MAX_AGE=']);
  const e1 = await waitServing();
  ok(e1 !== null, 'evaluation: a certificate is made at first start and served');
  if (e1) {
    ok(/EVALUATION/.test(JSON.stringify(e1.subject)) && /NOT FOR PRODUCTION/.test(JSON.stringify(e1.subject)), 'evaluation: the certificate subject says EVALUATION and NOT FOR PRODUCTION', JSON.stringify(e1.subject));
    ok(/DNS:localhost/.test(e1.subjectaltname) && /IP Address:127.0.0.1/.test(e1.subjectaltname), 'evaluation: the certificate lists localhost and 127.0.0.1 as names', e1.subjectaltname);
    const banner = logs(`${ID}-eval`);
    ok(/NOT FOR PRODUCTION/.test(banner) && /EVALUATION/.test(banner), 'evaluation: the log carries the not-for-production banner');
    docker(['restart', `${ID}-eval`]);
    const e2 = await waitServing();
    ok(e2 && e2.fingerprint === e1.fingerprint, 'evaluation: a restart reuses the certificate (no new one on every start)');
    const here = new URL('./fake-expiring-eval-cert.sh', import.meta.url);
    // Make the certificate look five days from expiry (as a second container on the same volume).
    const script = (await import('node:fs')).readFileSync(here, 'utf8');
    docker(['run', '--rm', '--entrypoint', 'sh', '-v', `${evalstate}:/var/lib/dynodoc-tls`, PROXY, '-c', script]);
    const down = await noDowntime(async () => { await waitFingerprintChange(e2.fingerprint, 30000); await sleep(500); });
    const e3 = await peer();
    ok(e3 && new Date(e3.validTo) > new Date(Date.now() + 80 * 86400e3), 'evaluation: a certificate close to expiry is replaced by a new 90-day one', JSON.stringify(e3));
    ok(down.failed === 0, `evaluation: no request failed during the renewal (${down.total} requests)`, `${down.failed} failed`);
  }
  stop(`${ID}-eval`);
}

try {
  await main();
} finally {
  for (const name of containers) stop(name);
  docker(['network', 'rm', NET]);
  for (const v of volumes) docker(['volume', 'rm', '-f', v]);
}
console.log(`\n${results.pass} passed, ${results.fail} failed`);
process.exit(results.fail ? 1 : 0);
