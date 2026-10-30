// The pre-alpha service, served at <workspace>/svc/ next to the Dynodoc server. It implements
// what the desktop client shows and the Dynodoc server has no place for, and hands out visitor
// sandboxes. Endpoints (see ../README.md):
//
//   GET  /svc/healthz                 liveness
//   GET  /svc/visitor                 a page that explains the sandbox and offers to start one
//   POST /svc/visitor/start           (form) claims a sandbox and sends the browser to the
//                                     workspace's own sign-in confirmation page
//   POST /svc/v1/visitor              (JSON, for the desktop app) claims a sandbox and returns its
//                                     one-use sign-in token, which the app confirms itself
//   GET  /svc/v1/session              who is asking, and whether this is a visitor sandbox
//   GET  /svc/v1/state/:key           the client's state kept for this account
//   PUT  /svc/v1/state/:key           (keys: working, local, threads, client)
//   GET  /svc/v1/notes?document=..    commit messages of revisions of files you can open
//   POST /svc/v1/notes                record one
//
// Who is asking comes from the workspace's own session cookie, read (never written) with the web
// tier's session secret, and the engine's /auth/me. Writes need the workspace's exact Origin.
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { chmodSync, readFileSync, rmSync } from 'node:fs';
import { engine } from './lib/engine.mjs';
import { cookieValue, readSession, sessionKey } from './lib/jwe.mjs';
import { addNote, allow, getState, notesFor, open, putState, STATE_KEYS } from './lib/store.mjs';

const env = (name, fallback) => {
  const file = process.env[`${name}_FILE`];
  if (file) return readFileSync(file, 'utf8').trim();
  return process.env[name] ?? fallback;
};

export function config() {
  return {
    api: env('ENGINE_API_URL', 'http://api:8080'),
    origin: env('PUBLIC_ORIGIN', 'https://app.dynodoc.online'),
    sessionSecret: env('SESSION_SECRET'),
    db: env('PREALPHA_DB', '/data/prealpha.sqlite'),
    socket: env('PREALPHA_SOCKET', ''),
    port: Number(env('PREALPHA_PORT', '0')),
    sandboxHours: Number(env('PREALPHA_SANDBOX_HOURS', '24')),
  };
}

const MAX_BODY = 300 * 1024;
const SECURITY_HEADERS = {
  'cache-control': 'no-store',
  'x-content-type-options': 'nosniff',
  'referrer-policy': 'no-referrer',
  'x-robots-tag': 'noindex, nofollow',
};

function send(res, status, body, headers = {}) {
  const json = typeof body !== 'string';
  const text = json ? JSON.stringify(body) : body;
  res.writeHead(status, {
    ...SECURITY_HEADERS,
    'content-type': json ? 'application/json' : 'text/html; charset=utf-8',
    'content-security-policy': "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
    ...headers,
  });
  res.end(text);
}

async function readBody(req) {
  const chunks = [];
  let size = 0;
  for await (const c of req) {
    size += c.length;
    if (size > MAX_BODY) throw Object.assign(new Error('Too large.'), { status: 413 });
    chunks.push(c);
  }
  return Buffer.concat(chunks).toString('utf8');
}

const hash = (s) => createHash('sha256').update(s).digest('hex').slice(0, 32);

const PAGE = (body) => `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="robots" content="noindex, nofollow"><title>Try Dynodoc</title>
<style>:root{color-scheme:light dark;--bg:#fafafa;--fg:#18181b;--muted:#52525b;--line:#e4e4e7;--card:#fff;--brand:#0f766e}
@media (prefers-color-scheme:dark){:root{--bg:#09090b;--fg:#fafafa;--muted:#a1a1aa;--line:#27272a;--card:#18181b;--brand:#2dd4bf}}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.55 system-ui,-apple-system,"Segoe UI",sans-serif}
main{max-width:560px;margin:10vh auto;padding:0 16px}.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:24px}
h1{font-size:20px;margin:0 0 8px}p,li{color:var(--muted)}ul{padding-left:18px}.pill{display:inline-block;font-size:12px;border:1px solid var(--line);border-radius:999px;padding:1px 8px;margin-bottom:12px}
button{font:inherit;background:var(--brand);color:#fff;border:0;border-radius:8px;padding:9px 16px;cursor:pointer}a{color:var(--brand)}</style></head>
<body><main><div class="card">${body}</div></main></body></html>`;

const VISITOR_PAGE = (hours) =>
  PAGE(`<span class="pill">Pre-alpha release</span><h1>Try Dynodoc with a sample workspace</h1>
<p>You get your own copy of a sample organisation: a team handbook, a budget, a review deck, a field guide and a grant application, with their history, drafts and change requests from six invented colleagues.</p>
<ul><li>It is yours alone; access expires ${hours} hours after you start. Sample content is placed in trash during cleanup.</li><li>Nothing is emailed; the addresses are sample ones.</li><li>Do not put anything private or confidential in it.</li></ul>
<form method="post" action="/svc/visitor/start"><button type="submit">Start the sample workspace</button></form>
<p><a href="/login">Sign in with your own account</a> instead.</p>`);

const BUSY_PAGE = PAGE('<h1>No sample workspace is free right now</h1><p>New ones are being prepared. Please try again in a minute.</p><p><a href="/svc/visitor">Back</a></p>');

export function makeHandler({ db, api, cfg, now = () => Date.now() }) {
  const key = sessionKey(cfg.sessionSecret);
  const requestedHours = Number(cfg.sandboxHours);
  const sandboxHours = Number.isFinite(requestedHours) && requestedHours > 0 ? Math.min(requestedHours, 24) : 24;
  const identities = new Map(); // token hash -> { identity, until }
  const access = new Map(); // identity:document -> until

  async function whoIs(req) {
    const value = cookieValue(req.headers.cookie, 'dynodoc_session');
    const session = value ? readSession(value, key, now()) : null;
    if (!session) return null;
    const h = hash(session.access_token);
    const held = identities.get(h);
    if (held && held.until > now()) return { identity: held.identity, token: session.access_token };
    try {
      const me = await api.get('/auth/me', session.access_token);
      identities.set(h, { identity: me.identity_id, until: now() + 300_000 });
      if (identities.size > 5000) identities.clear();
      return { identity: me.identity_id, token: session.access_token };
    } catch {
      return null;
    }
  }

  async function canOpen(who, documentId) {
    const k = `${who.identity}:${documentId}`;
    if ((access.get(k) ?? 0) > now()) return true;
    try {
      await api.get(`/documents/${documentId}/access`, who.token);
      access.set(k, now() + 60_000);
      if (access.size > 20000) access.clear();
      return true;
    } catch {
      return false;
    }
  }

  function claim(ip) {
    if (!allow(db, `claim:${hash(ip)}`, 6, 3_600_000, now()) || !allow(db, 'claim:all', 240, 3_600_000, now())) return { limited: true };
    const fresh = now() - 13 * 60_000;
    const row = db.prepare("SELECT * FROM sandbox WHERE status = 'ready' AND token IS NOT NULL AND token_at > ? AND created_at > ? ORDER BY ready_at LIMIT 1").get(fresh, now() - 24 * 3_600_000);
    if (!row) return { busy: true };
    const expires = now() + sandboxHours * 3_600_000;
    const changed = db.prepare("UPDATE sandbox SET status = 'claimed', claimed_at = ?, expires_at = ?, token = NULL WHERE id = ? AND status = 'ready'").run(now(), expires, row.id);
    if (!changed.changes) return { busy: true };
    return { token: row.token, email: row.email, display_name: row.display_name, expires_at: new Date(expires).toISOString() };
  }

  // The app sends Referrer-Policy: no-referrer, so Chrome posts same-origin forms with `Origin: null`. Accept that
  // only when the browser itself marks the request as same-origin (Sec-Fetch-Site cannot be set by a page).
  const sameOrigin = (req) =>
    req.headers.origin === cfg.origin ||
    (req.headers.origin === 'null' && req.headers['sec-fetch-site'] === 'same-origin');
  const clientIp = (req) => String(req.headers['x-real-ip'] || req.socket.remoteAddress || 'unknown');

  return async function handle(req, res) {
    const url = new URL(req.url, cfg.origin);
    const path = url.pathname;
    try {
      if (req.method === 'GET' && path === '/svc/healthz') return send(res, 200, { ok: true });
      if (req.method === 'GET' && path === '/svc/visitor') return send(res, 200, VISITOR_PAGE(sandboxHours));
      if (req.method === 'POST' && path === '/svc/visitor/start') {
        if (!sameOrigin(req)) return send(res, 403, PAGE('<h1>Please start from the Try page</h1><p><a href="/svc/visitor">Try Dynodoc</a></p>'));
        await readBody(req);
        const out = claim(clientIp(req));
        if (out.limited) return send(res, 429, PAGE('<h1>Too many sample workspaces from this address</h1><p>Please try again in an hour.</p>'));
        if (out.busy) return send(res, 503, BUSY_PAGE, { 'retry-after': '60' });
        return send(res, 303, '', { location: `/login?token=${encodeURIComponent(out.token)}&next=%2Fprojects` });
      }
      if (req.method === 'POST' && path === '/svc/v1/visitor') {
        if (!sameOrigin(req)) return send(res, 403, { error: 'Invalid origin' });
        await readBody(req);
        const out = claim(clientIp(req));
        if (out.limited) return send(res, 429, { error: 'Too many sample workspaces from this address. Try again in an hour.' });
        if (out.busy) return send(res, 503, { error: 'No sample workspace is free right now. Try again in a minute.' }, { 'retry-after': '60' });
        return send(res, 200, out);
      }
      if (!path.startsWith('/svc/v1/')) return send(res, 404, { error: 'Not found' });

      if (req.method !== 'GET' && !sameOrigin(req)) return send(res, 403, { error: 'Invalid origin' });
      const who = await whoIs(req);
      if (!who) return send(res, 401, { error: 'Please sign in' });
      if (!allow(db, `use:${who.identity}`, 600, 60_000, now())) return send(res, 429, { error: 'Too many requests. Slow down a little.' }, { 'retry-after': '30' });

      if (req.method === 'GET' && path === '/svc/v1/session') {
        const sb = db.prepare("SELECT display_name, expires_at FROM sandbox WHERE identity_id = ? AND status = 'claimed'").get(who.identity);
        return send(res, 200, { identity_id: who.identity, visitor: sb ? { display_name: sb.display_name, expires_at: new Date(sb.expires_at).toISOString() } : null });
      }
      const stateMatch = /^\/svc\/v1\/state\/([a-z]+)$/.exec(path);
      if (stateMatch) {
        const k = stateMatch[1];
        if (!STATE_KEYS.includes(k)) return send(res, 404, { error: 'Not found' });
        if (req.method === 'GET') return send(res, 200, getState(db, who.identity, k));
        if (req.method === 'PUT') {
          const body = JSON.parse((await readBody(req)) || 'null');
          if (!body || typeof body !== 'object' || !('value' in body)) return send(res, 400, { error: 'Send {"value": ...}' });
          const updated = putState(db, who.identity, k, body.value, now());
          return send(res, 200, { updated_at: updated });
        }
        return send(res, 405, { error: 'Method not allowed' });
      }
      if (path === '/svc/v1/notes' && req.method === 'GET') {
        const ids = url.searchParams.getAll('document').filter((d) => /^[0-9a-f-]{36}$/.test(d)).slice(0, 50);
        const allowed = [];
        for (const d of ids) if (await canOpen(who, d)) allowed.push(d);
        return send(res, 200, { items: notesFor(db, allowed) });
      }
      if (path === '/svc/v1/notes' && req.method === 'POST') {
        const n = JSON.parse((await readBody(req)) || 'null');
        const ok =
          n &&
          /^[0-9a-f-]{36}$/.test(n.document_id) &&
          typeof n.branch === 'string' && n.branch.length <= 64 &&
          Number.isInteger(n.from_seq) && Number.isInteger(n.to_seq) &&
          typeof n.summary === 'string' && n.summary.trim() && n.summary.length <= 200 &&
          (n.description === undefined || (typeof n.description === 'string' && n.description.length <= 4000)) &&
          (n.edits === undefined || (Array.isArray(n.edits) && JSON.stringify(n.edits).length <= 64 * 1024));
        if (!ok) return send(res, 400, { error: 'Invalid note' });
        if (!(await canOpen(who, n.document_id))) return send(res, 403, { error: 'You cannot open this file.' });
        const id = addNote(db, { ...n, merged_from: typeof n.merged_from === 'string' ? n.merged_from.slice(0, 64) : null, author_id: who.identity }, now());
        return send(res, 200, { id });
      }
      return send(res, 404, { error: 'Not found' });
    } catch (e) {
      const status = e.status && e.status >= 400 && e.status < 500 ? e.status : e instanceof SyntaxError ? 400 : 500;
      if (status === 500) console.error(JSON.stringify({ at: new Date().toISOString(), msg: 'request failed' }));
      return send(res, status, { error: status === 500 ? 'Something went wrong.' : e.message });
    }
  };
}

function main() {
  const cfg = config();
  const db = open(cfg.db);
  const api = engine(cfg.api);
  const server = createServer(makeHandler({ db, api, cfg }));
  server.headersTimeout = 15_000;
  server.requestTimeout = 30_000;
  if (cfg.socket) {
    rmSync(cfg.socket, { force: true });
    server.listen(cfg.socket, () => {
      chmodSync(cfg.socket, 0o666);
      console.log(JSON.stringify({ msg: 'pre-alpha service listening', socket: cfg.socket }));
    });
  } else {
    server.listen(cfg.port, '127.0.0.1', () => console.log(JSON.stringify({ msg: 'pre-alpha service listening', port: server.address().port })));
  }
}

if (import.meta.url === `file://${process.argv[1]}`) main();
