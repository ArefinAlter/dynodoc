// Tests the bundle's `acme` TLS mode against Pebble, the Let's Encrypt project's ACME test server
// (https://github.com/letsencrypt/pebble), with a DNS stub that points the test host name at the
// proxy. The real proxy and acme images run as in the bundle (same user, read-only root, shared
// volume); only the web server is a stub. Nothing leaves this machine, and NO real certificate
// authority is contacted: this proves the integration (HTTP-01 through the proxy, the hand-over
// of the certificate, the reload without downtime), not that Let's Encrypt will issue to you.
//
//   node infra/selfhosted/test/acme-pebble.mjs
//   PROXY_IMAGE=... ACME_IMAGE=... WEB_IMAGE=... to use other images
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import https from 'node:https';
import os from 'node:os';
import path from 'node:path';
import tls from 'node:tls';

const PROXY = process.env.PROXY_IMAGE ?? 'dynodoc-local/proxy:dev';
const ACME = process.env.ACME_IMAGE ?? 'dynodoc-local/acme:dev';
const WEB = process.env.WEB_IMAGE ?? 'dynodoc-local/web:dev';
// Pinned by digest; refresh with scripts/pin-bases.mjs.
const PEBBLE = process.env.PEBBLE_IMAGE ?? 'ghcr.io/letsencrypt/pebble:latest@sha256:ddf230642b1a584f519f32e347de1b05a6e4c1f6c35c1863b33effeab5f78199';
const CHALLTESTSRV = process.env.CHALLTESTSRV_IMAGE ?? 'ghcr.io/letsencrypt/pebble-challtestsrv:latest@sha256:12ce21884def456bcf9786542113949e1f19dc7738d2c70e156c2d0c38a1405b';
const PROJECT = `dynodoc-acme-test-${process.pid}`;
const HTTPS_PORT = Number(process.env.ACME_TEST_PORT ?? 28445);
const HTTP_PORT = HTTPS_PORT - 365;
const HOST = 'dynodoc.test';
const results = { pass: 0, fail: 0 };
const ok = (condition, label, detail = '') => {
  results[condition ? 'pass' : 'fail']++;
  console.log(`${condition ? 'ok  ' : 'FAIL'}  ${label}${condition || !detail ? '' : `\n        ${String(detail).split('\n').join('\n        ')}`}`);
};
const docker = (args, options = {}) => spawnSync('docker', args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...options });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const dir = mkdtempSync(path.join(os.tmpdir(), 'dynodoc-acme-'));
const compose = (args) => docker(['compose', '-p', PROJECT, '-f', path.join(dir, 'compose.yml'), '--project-directory', dir, ...args]);

// The pebble test CA certificate comes out of the pinned pebble image, not from this repository.
const extract = docker(['create', '--name', `${PROJECT}-x`, PEBBLE]);
if (extract.status !== 0) { console.error(extract.stderr); process.exit(2); }
docker(['cp', `${PROJECT}-x:/test/certs/pebble.minica.pem`, path.join(dir, 'pebble.minica.pem')]);
docker(['rm', `${PROJECT}-x`]);

writeFileSync(path.join(dir, 'pebble-config.json'), JSON.stringify({
  pebble: {
    listenAddress: '0.0.0.0:14000', managementListenAddress: '0.0.0.0:15000',
    certificate: 'test/certs/localhost/cert.pem', privateKey: 'test/certs/localhost/key.pem',
    // The test network has no port publishing, so the validator talks to the proxy's own port.
    httpPort: 8080, tlsPort: 8443, ocspResponderURL: '', externalAccountBindingRequired: false,
    retryAfter: { authz: 1, order: 1 }, keyAlgorithm: 'ecdsa'
  }
}));
const hardening = `read_only: true
    cap_drop: [ALL]
    security_opt: ["no-new-privileges:true"]
    init: true`;
writeFileSync(path.join(dir, 'compose.yml'), `name: ${PROJECT}
services:
  challtestsrv:
    image: ${CHALLTESTSRV}
    command: ["-defaultIPv4", "10.77.0.10", "-defaultIPv6", "", "-management", ":8055", "-dnsserver", ":8053", "-http01", "", "-https01", "", "-tlsalpn01", "", "-doh", ""]
    networks: {t: {ipv4_address: 10.77.0.20}}
  pebble:
    image: ${PEBBLE}
    command: ["-config", "/config/pebble-config.json", "-dnsserver", "10.77.0.20:8053"]
    environment: {PEBBLE_VA_NOSLEEP: "1"}
    volumes: ["./pebble-config.json:/config/pebble-config.json:ro"]
    networks: {t: {ipv4_address: 10.77.0.30}}
  web:
    image: ${WEB}
    command: ["node", "-e", "require('http').createServer((q,r)=>r.end('stub')).listen(3000)"]
    networks: {t: {aliases: [web]}}
  proxy:
    image: ${PROXY}
    user: "101:101"
    ${hardening}
    tmpfs: ["/tmp:size=64m"]
    environment:
      ORIGIN: https://${HOST}:${HTTPS_PORT}
      DYNODOC_TLS_MODE: acme
      DYNODOC_TLS_RELOAD_SECONDS: "2"
    volumes: ["tls-state:/var/lib/dynodoc-tls"]
    ports: ["127.0.0.1:${HTTP_PORT}:8080", "127.0.0.1:${HTTPS_PORT}:8443"]
    networks: {t: {ipv4_address: 10.77.0.10}}
    dns: [10.77.0.20]
  acme:
    image: ${ACME}
    user: "101:101"
    ${hardening}
    tmpfs: ["/tmp:size=16m"]
    environment:
      ORIGIN: https://${HOST}:${HTTPS_PORT}
      DYNODOC_ACME_EMAIL: test@${HOST}
      DYNODOC_ACME_ACCEPT_TOS: "true"
      DYNODOC_ACME_CA: https://pebble:14000/dir
      DYNODOC_BIND: 0.0.0.0
      DYNODOC_ACME_CHECK_SECONDS: "15"
      DYNODOC_ACME_RETRY_SECONDS: "5"
      LEGO_CA_CERTIFICATES: /pebble.minica.pem
      LEGO_RENEW_DAYS: "3650"
    volumes: ["tls-state:/var/lib/dynodoc-tls", "./pebble.minica.pem:/pebble.minica.pem:ro"]
    dns: [10.77.0.20]
    networks: [t]
networks:
  t:
    ipam: {config: [{subnet: 10.77.0.0/24}]}
volumes:
  tls-state: {}
`);

function peer() {
  return new Promise((resolve) => {
    const socket = tls.connect({ host: '127.0.0.1', port: HTTPS_PORT, servername: HOST, rejectUnauthorized: false }, () => {
      const cert = socket.getPeerCertificate();
      socket.end();
      resolve(cert);
    });
    socket.on('error', () => resolve(null));
    socket.setTimeout(3000, () => { socket.destroy(); resolve(null); });
  });
}
function get(host = HOST) {
  return new Promise((resolve) => {
    const request = https.get({ host: '127.0.0.1', port: HTTPS_PORT, path: '/', servername: HOST, rejectUnauthorized: false, agent: false, headers: { host: `${host}:${HTTPS_PORT}` } }, (response) => { response.resume(); resolve(response.statusCode); });
    request.on('error', () => resolve(0));
    request.setTimeout(3000, () => { request.destroy(); resolve(0); });
  });
}
const logs = (service) => { const r = compose(['logs', '--no-color', service]); return `${r.stdout}${r.stderr}`; };
async function waitFor(fn, ms) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(500); } return null; }

try {
  const up = compose(['up', '-d', '--no-build', '--pull', 'missing', 'challtestsrv', 'pebble', 'web', 'proxy']);
  if (up.status !== 0) throw new Error(up.stderr);
  await sleep(2000);
  // Before the ACME client runs: plain HTTP only, no HTTPS listener, the challenge path is served.
  ok((await peer()) === null, 'acme, no certificate yet: nothing listens for HTTPS (no placeholder certificate is invented)');
  ok(/no certificate has been issued yet/.test(logs('proxy')), 'acme, no certificate yet: the proxy log says it is waiting and why', logs('proxy').slice(-600));
  const refused = compose(['run', '--rm', '--no-deps', '-e', 'DYNODOC_ACME_ACCEPT_TOS=false', 'acme']);
  ok(refused.status !== 0 && /DYNODOC_ACME_ACCEPT_TOS is not 'true'/.test(`${refused.stdout}${refused.stderr}`), 'the ACME client refuses to ask for anything until the terms are accepted in .env', `${refused.stdout}${refused.stderr}`.slice(-400));
  const nomail = compose(['run', '--rm', '--no-deps', '-e', 'DYNODOC_ACME_EMAIL=', 'acme']);
  ok(nomail.status !== 0 && /DYNODOC_ACME_EMAIL is not set/.test(`${nomail.stdout}${nomail.stderr}`), 'the ACME client refuses to run without a contact address');
  const noca = compose(['run', '--rm', '--no-deps', '-e', 'DYNODOC_ACME_CA=', 'acme']);
  ok(noca.status !== 0 && /DYNODOC_ACME_CA is not set/.test(`${noca.stdout}${noca.stderr}`), 'the ACME client refuses to run until the certificate authority is chosen (no default)');
  const ipOrigin = compose(['run', '--rm', '--no-deps', '-e', 'ORIGIN=https://10.0.0.5', 'acme']);
  ok(ipOrigin.status !== 0 && /not a public DNS name/.test(`${ipOrigin.stdout}${ipOrigin.stderr}`), 'the ACME client refuses an IP address or a name without a dot');

  const start = compose(['up', '-d', '--no-build', 'acme']);
  if (start.status !== 0) console.error(start.stderr);
  const first = await waitFor(async () => { const c = await peer(); return c && c.fingerprint256 ? c : null; }, 120000);
  ok(first !== null, 'acme: a certificate is issued through HTTP-01 via the proxy and the proxy starts serving HTTPS with it', logs('acme').slice(-1500) + logs('proxy').slice(-800));
  if (first) {
    ok(/Pebble/.test(JSON.stringify(first.issuer)), 'acme: the served certificate was issued by the ACME server (Pebble), not generated locally', JSON.stringify(first.issuer));
    ok(first.subjectaltname?.includes(`DNS:${HOST}`), `acme: the certificate names ${HOST}`, first.subjectaltname);
    ok(!/EVALUATION/.test(JSON.stringify(first.subject)), 'acme: it is not the evaluation certificate');
    const plain = await new Promise((resolve) => { import('node:http').then(({ default: http }) => { http.get({ host: '127.0.0.1', port: HTTP_PORT, path: '/x', headers: { host: HOST } }, (r) => { r.resume(); resolve(r.headers.location); }).on('error', () => resolve(null)); }); });
    ok(plain === `https://${HOST}:${HTTPS_PORT}/x`, 'acme: plain HTTP redirects to the HTTPS origin once the certificate exists', plain);
    ok((await get()) === 200, 'acme: requests through HTTPS reach the web server');
    ok(/Published a new certificate/.test(logs('acme')) && /Configuration reloaded/.test(logs('proxy')), 'acme: the ACME client published it and the proxy reloaded (log lines)');
    // Renewal: the test sets LEGO_RENEW_DAYS very high so every check renews. Watch for a second certificate.
    let failed = 0, total = 0, stop = false;
    const loop = (async () => { while (!stop) { total++; if ((await get()) !== 200) failed++; await sleep(40); } })();
    const second = await waitFor(async () => { const c = await peer(); return c && c.fingerprint256 !== first.fingerprint256 ? c : null; }, 90000);
    await sleep(1000);
    stop = true;
    await loop;
    ok(second !== null, 'acme: a renewal replaces the certificate that is being served', logs('acme').slice(-800));
    ok(failed === 0 && total > 10, `acme: no request failed while the certificate was renewed and reloaded (${total} requests)`, `${failed} failed`);
    const state = compose(['exec', '-T', 'proxy', 'sh', '-c', 'ls /var/lib/dynodoc-tls /var/lib/dynodoc-tls/acme; stat -c "%a %U" /var/lib/dynodoc-tls/acme/current/privkey.pem']);
    ok(/600 nginx/.test(state.stdout), 'acme: the private key is mode 600 and owned by the proxy user', state.stdout);
    const lego = compose(['exec', '-T', 'acme', 'sh', '-c', 'id -u; ls -la /var/lib/dynodoc-tls/lego | head -5']);
    ok(/^101/.test(lego.stdout), 'acme: the ACME client runs as uid 101, not root', lego.stdout);
  }
} finally {
  compose(['down', '-v', '--remove-orphans']);
  rmSync(dir, { recursive: true, force: true });
}
console.log(`\n${results.pass} passed, ${results.fail} failed`);
process.exit(results.fail ? 1 : 0);
