// node --test deploy/pre-alpha/test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createCipheriv, createHash, randomBytes } from 'node:crypto';
import { makeHandler } from '../service/svc.mjs';
import { open, allow, addNote } from '../service/lib/store.mjs';
import { readSession, sessionKey, cookieValue } from '../service/lib/jwe.mjs';
import { ulid, between } from '../service/lib/ulid.mjs';

const SECRET = 'a'.repeat(40);
const ORIGIN = 'https://app.example.org';

/** What the web tier's `jose` EncryptJWT writes: compact JWE, dir + A256GCM. */
function seal(claims, secret = SECRET) {
  const key = createHash('sha256').update(secret).digest();
  const header = Buffer.from(JSON.stringify({ alg: 'dir', enc: 'A256GCM' })).toString('base64url');
  const iv = randomBytes(12);
  const c = createCipheriv('aes-256-gcm', key, iv);
  c.setAAD(Buffer.from(header, 'ascii'));
  const ct = Buffer.concat([c.update(JSON.stringify(claims)), c.final()]);
  return [header, '', iv.toString('base64url'), ct.toString('base64url'), c.getAuthTag().toString('base64url')].join('.');
}

function fakeApi({ me = {}, docs = new Set() } = {}) {
  return {
    async get(path, token) {
      if (path === '/auth/me') {
        if (!me[token]) throw Object.assign(new Error('401'), { status: 401 });
        return { identity_id: me[token] };
      }
      const m = /^\/documents\/([^/]+)\/access$/.exec(path);
      if (m && docs.has(`${token}:${m[1]}`)) return { can_manage: false };
      throw Object.assign(new Error('403'), { status: 403 });
    },
  };
}

async function call(handler, method, path, { headers = {}, body } = {}) {
  const chunks = body === undefined ? [] : [Buffer.from(typeof body === 'string' ? body : JSON.stringify(body))];
  const req = {
    method,
    url: path,
    headers,
    socket: { remoteAddress: '203.0.113.9' },
    async *[Symbol.asyncIterator]() {
      for (const c of chunks) yield c;
    },
  };
  return new Promise((resolve) => {
    const res = {
      writeHead(status, h) {
        this.status = status;
        this.headers = h;
      },
      end(text) {
        let json = null;
        try {
          json = JSON.parse(text);
        } catch {
          /* html */
        }
        resolve({ status: this.status, headers: this.headers, text, json });
      },
    };
    handler(req, res);
  });
}

function setup(extra = {}) {
  const db = open(':memory:');
  const cfg = { origin: ORIGIN, sessionSecret: SECRET, sandboxHours: 24 };
  const api = fakeApi(extra);
  return { db, handler: makeHandler({ db, api, cfg }) };
}

const cookie = (token) => `other=1; dynodoc_session=${seal({ access_token: token, refresh_token: 'r', expires_in: 3600, exp: Math.floor(Date.now() / 1000) + 3600 })}`;

test('reads the web tier session cookie and refuses tampered or foreign ones', () => {
  const key = sessionKey(SECRET);
  const good = seal({ access_token: 'tok', exp: Math.floor(Date.now() / 1000) + 60 });
  assert.equal(readSession(good, key).access_token, 'tok');
  assert.equal(readSession(seal({ access_token: 'tok' }, 'b'.repeat(40)), key), null);
  const parts = good.split('.');
  parts[3] = Buffer.from('x' + Buffer.from(parts[3], 'base64url').toString('latin1').slice(1), 'latin1').toString('base64url');
  assert.equal(readSession(parts.join('.'), key), null);
  assert.equal(readSession(seal({ access_token: 'tok', exp: 1 }), key), null);
  assert.equal(readSession('a.b.c', key), null);
  assert.equal(cookieValue('a=1; dynodoc_session=xyz', 'dynodoc_session'), 'xyz');
});

test('a visitor claims a ready sandbox once, by form or by the app, with a fresh token only', async () => {
  const { db, handler } = setup();
  const now = Date.now();
  db.prepare("INSERT INTO sandbox (id, email, display_name, status, created_at, ready_at, token, token_at) VALUES ('s1', 'v1@x.invalid', 'Visitor 1', 'ready', ?, ?, 'T1', ?)").run(now, now, now);
  db.prepare("INSERT INTO sandbox (id, email, display_name, status, created_at, ready_at, token, token_at) VALUES ('s2', 'v2@x.invalid', 'Visitor 2', 'ready', ?, ?, 'OLD', ?)").run(now, now, now - 20 * 60_000);
  const noOrigin = await call(handler, 'POST', '/svc/visitor/start', { body: '' });
  assert.equal(noOrigin.status, 403);
  const form = await call(handler, 'POST', '/svc/visitor/start', { headers: { origin: ORIGIN }, body: '' });
  assert.equal(form.status, 303);
  assert.equal(form.headers.location, '/login?token=T1&next=%2Fprojects');
  const row = db.prepare("SELECT status, token, expires_at FROM sandbox WHERE id = 's1'").get();
  assert.equal(row.status, 'claimed');
  assert.equal(row.token, null);
  assert.ok(row.expires_at > now + 23 * 3_600_000);
  // The other sandbox's token is too old to hand out.
  const app = await call(handler, 'POST', '/svc/v1/visitor', { headers: { origin: ORIGIN }, body: '{}' });
  assert.equal(app.status, 503);
});

test('claims are limited per address', async () => {
  const { db, handler } = setup();
  for (let i = 0; i < 6; i++) await call(handler, 'POST', '/svc/v1/visitor', { headers: { origin: ORIGIN, 'x-real-ip': '198.51.100.7' }, body: '{}' });
  const seventh = await call(handler, 'POST', '/svc/v1/visitor', { headers: { origin: ORIGIN, 'x-real-ip': '198.51.100.7' }, body: '{}' });
  assert.equal(seventh.status, 429);
  assert.equal(allow(db, 'x', 1, 1000, 0), true);
  assert.equal(allow(db, 'x', 1, 1000, 10), false);
  assert.equal(allow(db, 'x', 1, 1000, 2000), true);
});

test('state is per account, keyed, size limited, and writes need the origin', async () => {
  const { handler } = setup({ me: { ta: 'id-a', tb: 'id-b' } });
  assert.equal((await call(handler, 'GET', '/svc/v1/state/working')).status, 401);
  const put = await call(handler, 'PUT', '/svc/v1/state/working', { headers: { origin: ORIGIN, cookie: cookie('ta') }, body: { value: [{ id: 'w1' }] } });
  assert.equal(put.status, 200);
  const wrongOrigin = await call(handler, 'PUT', '/svc/v1/state/working', { headers: { origin: 'https://evil.example', cookie: cookie('ta') }, body: { value: [] } });
  assert.equal(wrongOrigin.status, 403);
  assert.deepEqual((await call(handler, 'GET', '/svc/v1/state/working', { headers: { cookie: cookie('ta') } })).json.value, [{ id: 'w1' }]);
  assert.equal((await call(handler, 'GET', '/svc/v1/state/working', { headers: { cookie: cookie('tb') } })).json.value, null);
  assert.equal((await call(handler, 'GET', '/svc/v1/state/secrets', { headers: { cookie: cookie('ta') } })).status, 404);
  const big = await call(handler, 'PUT', '/svc/v1/state/client', { headers: { origin: ORIGIN, cookie: cookie('ta') }, body: { value: 'x'.repeat(260 * 1024) } });
  assert.equal(big.status, 413);
});

test('notes are only read and written for files the person can open', async () => {
  const doc = '0190f5b2-0000-7000-8000-000000000001';
  const other = '0190f5b2-0000-7000-8000-000000000002';
  const { db, handler } = setup({ me: { ta: 'id-a' }, docs: new Set([`ta:${doc}`]) });
  addNote(db, { document_id: other, branch: 'main', from_seq: 1, to_seq: 1, kind: 'revision', summary: 'hidden', author_id: 'x' });
  const ok = await call(handler, 'POST', '/svc/v1/notes', { headers: { origin: ORIGIN, cookie: cookie('ta') }, body: { document_id: doc, branch: 'main', from_seq: 3, to_seq: 4, kind: 'revision', summary: 'Shorten core hours' } });
  assert.equal(ok.status, 200);
  const refused = await call(handler, 'POST', '/svc/v1/notes', { headers: { origin: ORIGIN, cookie: cookie('ta') }, body: { document_id: other, branch: 'main', from_seq: 3, to_seq: 4, kind: 'revision', summary: 'x' } });
  assert.equal(refused.status, 403);
  const bad = await call(handler, 'POST', '/svc/v1/notes', { headers: { origin: ORIGIN, cookie: cookie('ta') }, body: { document_id: doc, branch: 'main', from_seq: 3, to_seq: 4, kind: 'shell', summary: 'x' } });
  assert.equal(bad.status, 400);
  const list = await call(handler, 'GET', `/svc/v1/notes?document=${doc}&document=${other}`, { headers: { cookie: cookie('ta') } });
  assert.deepEqual(list.json.items.map((n) => [n.summary, n.author_id]), [['Shorten core hours', 'id-a']]);
});

test('ulids and position keys', () => {
  const a = ulid(0, Buffer.alloc(10));
  assert.equal(a, '0'.repeat(26));
  assert.match(ulid(), /^[0-9A-HJKMNP-TV-Z]{26}$/);
  const m = between('00000016', '00000032');
  assert.ok(m > '00000016' && m < '00000032');
  assert.ok(between('00000016') > '00000016');
});

test('a same-origin form posted under no-referrer (Origin: null) is accepted; a cross-site one is not', async () => {
  const { handler } = setup();
  const ok = await call(handler, 'POST', '/svc/visitor/start', { headers: { origin: 'null', 'sec-fetch-site': 'same-origin' }, body: '' });
  assert.notEqual(ok.status, 403);
  const cross = await call(handler, 'POST', '/svc/visitor/start', { headers: { origin: 'null', 'sec-fetch-site': 'cross-site' }, body: '' });
  assert.equal(cross.status, 403);
  const bare = await call(handler, 'POST', '/svc/visitor/start', { headers: { origin: 'null' }, body: '' });
  assert.equal(bare.status, 403);
});


test('failed retirement stays eligible for retry without logging the upstream error', async () => {
  const { retire } = await import('../service/provisioner.mjs');
  const db = open(':memory:');
  const now = Date.now();
  db.prepare("INSERT INTO sandbox (id, email, display_name, status, created_at, expires_at, resources) VALUES ('retry', 'v@x.invalid', 'V', 'claimed', ?, ?, ?)")
    .run(now - 90000000, now - 1000, JSON.stringify({ documents: [{ id: 'doc-1', owner: 'alex' }] }));
  const row = db.prepare("SELECT * FROM sandbox WHERE id='retry'").get();
  const api = { async post() { throw new Error('sensitive-canary'); } };
  const auth = { async token() { return 'test'; } };
  const lines = [];
  const original = console.log;
  console.log = (...args) => lines.push(args.join(' '));
  try {
    await assert.rejects(retire(db, api, auth, row), /retirement incomplete/);
    assert.equal(db.prepare("SELECT status FROM sandbox WHERE id='retry'").get().status, 'claimed');
    assert.ok(!lines.join(' ').includes('sensitive-canary'));
  } finally { console.log = original; db.close(); }
});


test('failed membership removal is retried; a missing space is already retired', async () => {
  const { retire } = await import('../service/provisioner.mjs');
  const db = open(':memory:');
  const now = Date.now();
  db.prepare("INSERT INTO sandbox (id, email, display_name, status, created_at, expires_at, resources) VALUES ('member-retry', 'v@x.invalid', 'V', 'claimed', ?, ?, ?)")
    .run(now - 90000000, now - 1000, JSON.stringify({ spaces: [{ id: 'space-1', owner: 'alex' }] }));
  const row = db.prepare("SELECT * FROM sandbox WHERE id='member-retry'").get();
  const auth = { async token() { return 'test'; } };
  try {
    await assert.rejects(retire(db, { async post() { throw new Error('private'); } }, auth, row), /retirement incomplete/);
    assert.equal(db.prepare("SELECT status FROM sandbox WHERE id='member-retry'").get().status, 'claimed');
    await retire(db, { async post() { throw Object.assign(new Error('missing'), { status: 404 }); } }, auth, row);
    assert.equal(db.prepare("SELECT status FROM sandbox WHERE id='member-retry'").get().status, 'expired');
  } finally { db.close(); }
});
