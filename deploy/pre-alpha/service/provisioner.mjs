// The provisioner: keeps a small pool of ready visitor sandboxes and retires expired ones.
// It has no listener. It reaches only the engine API on the stack's internal network, with the
// web tier's service key (the same key the web tier uses to sign people in), and the service's
// own database. It never sends mail: visitor and sample addresses are not real mailboxes.
import { randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { engine, sessions } from './lib/engine.mjs';
import { buildSandbox, PEOPLE } from './lib/sample.mjs';
import { addNote, dropState, open, putState } from './lib/store.mjs';

const env = (name, fallback) => {
  const file = process.env[`${name}_FILE`];
  if (file) return readFileSync(file, 'utf8').trim();
  return process.env[name] ?? fallback;
};

export const CONFIG = {
  api: env('ENGINE_API_URL', 'http://api:8080'),
  serviceKey: env('WEB_SERVICE_KEY'),
  db: env('PREALPHA_DB', '/data/prealpha.sqlite'),
  pool: Number(env('PREALPHA_POOL', '3')),
  maxLive: Number(env('PREALPHA_MAX_LIVE', '300')),
  sandboxHours: Number(env('PREALPHA_SANDBOX_HOURS', '24')),
  visitorDomain: env('PREALPHA_VISITOR_DOMAIN', 'visitors.dynodoc.invalid'),
  tickMs: Number(env('PREALPHA_TICK_MS', '30000')),
};

const log = (msg, extra = {}) => console.log(JSON.stringify({ at: new Date().toISOString(), msg, ...extra }));

export function newVisitor() {
  const code = randomBytes(4).toString('hex');
  return { id: `sbx_${code}`, email: `visitor-${code}@${CONFIG.visitorDomain}`, name: `Visitor ${code.slice(0, 4).toUpperCase()}` };
}

/** A sign-in token is good for 15 minutes; tokens older than this are replaced. */
const TOKEN_FRESH_MS = 9 * 60_000;
/** After a failed build the provisioner waits this long before it tries again. */
const BACKOFF_MS = 5 * 60_000;
let failedAt = 0;

export async function build(db, api, auth) {
  const v = newVisitor();
  const now = Date.now();
  db.prepare("INSERT INTO sandbox (id, email, display_name, status, created_at) VALUES (?, ?, ?, 'building', ?)").run(v.id, v.email, v.name, now);
  const res = { projects: [], documents: [], spaces: [] };
  let visitorTokens;
  try {
    visitorTokens = await auth.once(v.email, v.name);
    const me = await api.get('/auth/me', visitorTokens.access_token);
    db.prepare('UPDATE sandbox SET identity_id = ? WHERE id = ?').run(me.identity_id, v.id);
    const as = async (who) => (who === 'visitor' ? visitorTokens.access_token : auth.token(PEOPLE[who].email, PEOPLE[who].name));
    const note = (n) => addNote(db, n);
    const out = await buildSandbox({ api, as, visitor: { email: v.email, identity_id: me.identity_id }, note, res });
    putState(db, me.identity_id, 'working', out.working);
    const link = await auth.magicLink(v.email, v.name);
    db.prepare("UPDATE sandbox SET status = 'ready', ready_at = ?, token = ?, token_at = ?, resources = ? WHERE id = ?").run(Date.now(), link.magic_link_token, Date.now(), JSON.stringify(res), v.id);
    log('sandbox ready', { sandbox: v.id, ms: Date.now() - now });
  } catch (e) {
    failedAt = Date.now();
    db.prepare("UPDATE sandbox SET status = 'failed', error = ?, resources = ? WHERE id = ?").run(String(e.message).slice(0, 300), JSON.stringify(res), v.id);
    log('sandbox failed', { sandbox: v.id, error: String(e.message).slice(0, 300) });
  } finally {
    if (visitorTokens) await auth.logout(visitorTokens.refresh_token);
  }
}

/** Takes the visitor out of every project and space of their sandbox and moves its files to the
 *  trash (the engine has no hard delete outside the administrator's erasure). */
export async function retire(db, api, auth, row) {
  const res = JSON.parse(row.resources || '{}');
  const tokenOf = (who) => auth.token(PEOPLE[who].email, PEOPLE[who].name);
  for (const d of res.documents ?? []) {
    try {
      await api.post(`/documents/${d.id}/trash`, { deleted: true }, await tokenOf(d.owner));
    } catch (e) {
      log('trash failed', { sandbox: row.id, error: String(e.message).slice(0, 200) });
    }
  }
  for (const s of [...(res.projects ?? []), ...(res.spaces ?? [])]) {
    try {
      await api.post(`/spaces/${s.id}/members`, { email: row.email, role: 'remove' }, await tokenOf(s.owner));
    } catch {
      /* the visitor may never have been added */
    }
  }
  if (row.identity_id) dropState(db, row.identity_id);
  db.prepare("UPDATE sandbox SET status = 'expired', expired_at = ?, token = NULL WHERE id = ?").run(Date.now(), row.id);
  log('sandbox retired', { sandbox: row.id });
}

export async function tick(db, api, auth) {
  const now = Date.now();
  // Retire claimed sandboxes past their time, failed ones, and ready ones nobody took in a week.
  const due = db
    .prepare("SELECT * FROM sandbox WHERE (status = 'claimed' AND expires_at < ?) OR status = 'failed' OR (status = 'ready' AND created_at < ?) LIMIT 5")
    .all(now, now - 7 * 86_400_000);
  for (const row of due) await retire(db, api, auth, row);
  // A building row older than ten minutes was interrupted (a restart): retire what it made.
  for (const row of db.prepare("SELECT * FROM sandbox WHERE status = 'building' AND created_at < ?").all(now - 600_000)) await retire(db, api, auth, row);
  // Keep the sign-in tokens of ready sandboxes fresh.
  for (const row of db.prepare("SELECT * FROM sandbox WHERE status = 'ready' AND token_at < ?").all(now - TOKEN_FRESH_MS)) {
    try {
      const link = await auth.magicLink(row.email, row.display_name);
      db.prepare('UPDATE sandbox SET token = ?, token_at = ? WHERE id = ?').run(link.magic_link_token, Date.now(), row.id);
    } catch (e) {
      log('token refresh failed', { sandbox: row.id, error: String(e.message).slice(0, 200) });
    }
  }
  const ready = db.prepare("SELECT COUNT(*) AS n FROM sandbox WHERE status = 'ready'").get().n;
  const live = db.prepare("SELECT COUNT(*) AS n FROM sandbox WHERE status IN ('ready', 'claimed', 'building')").get().n;
  if (ready < CONFIG.pool && live < CONFIG.maxLive && Date.now() - failedAt > BACKOFF_MS) await build(db, api, auth);
}

async function main() {
  if (!CONFIG.serviceKey || CONFIG.serviceKey.length < 32) throw new Error('WEB_SERVICE_KEY is required');
  const db = open(CONFIG.db);
  const api = engine(CONFIG.api);
  const auth = sessions(api, CONFIG.serviceKey);
  log('provisioner started', { pool: CONFIG.pool, hours: CONFIG.sandboxHours });
  for (;;) {
    try {
      await tick(db, api, auth);
    } catch (e) {
      log('tick failed', { error: String(e.message).slice(0, 300) });
    }
    const ready = db.prepare("SELECT COUNT(*) AS n FROM sandbox WHERE status = 'ready'").get().n;
    await new Promise((r) => setTimeout(r, ready < CONFIG.pool ? 1000 : CONFIG.tickMs));
  }
}

if (import.meta.url === `file://${process.argv[1]}`) main();
