// Scenario 7: kill the app and the database in the middle of writes; start again; the data must be
// there, the engine must start, and the hash chain of every file must verify (checked by the
// engine's own command line tool against the database, not by the page).
import { spawnSync, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, existsSync, writeFileSync, mkdirSync } from 'node:fs';
import { launch, connect, sleep, killTree, closeGracefully } from './cdp.mjs';

const HOME = process.argv[2] || 'D:/verne-build/dynodoc/scratch/home-acc2';
const EV = 'D:/verne-build/dynodoc/evidence/acc7/';
mkdirSync(EV, { recursive: true });
const log = [];
const say = (...a) => { const l = a.join(' '); console.log(l); log.push(l); };
const PG = 'D:/verne-build/dynodoc/programs/pg/bin/';
const CLI = 'D:/verne-build/dynodoc/programs/engine-cli.exe';

function secretsFor(home) {
  const dir = (home + '\\data').toLowerCase();
  const suffix = createHash('sha256').update(dir).digest('hex').slice(0, 8);
  const target = `local-secrets.online.dynodoc.client.${suffix}`;
  const r = spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'D:\\verne-build\\dynodoc\\scratch\\drive\\readsecret.ps1', target], { encoding: 'utf8' });
  if (r.status !== 0) throw new Error('credential not found: ' + target);
  return JSON.parse(r.stdout.trim());
}

function dbPort(home) {
  const t = readFileSync(home + '/data/workspace/pgdata/postmaster.pid', 'utf8').split('\n');
  return { pid: Number(t[0]), port: Number(t[3]) };
}

function engineVerify(home, docId, head) {
  const s = secretsFor(home);
  const { port } = dbPort(home);
  const url = `postgres://dynodoc_runtime:${s.runtime}@127.0.0.1:${port}/dynodoc`;
  const args = ['verify', docId];
  if (head) args.push('--expect-head', head);
  const r = spawnSync(CLI, args, { env: { ...process.env, DATABASE_URL: url }, encoding: 'utf8' });
  return { ok: r.status === 0, out: (r.stdout + r.stderr).replace(/postgres:\/\/\S+/g, 'postgres://[removed]').trim().slice(0, 300) };
}

async function start(home, port) {
  const app = launch({ home, port });
  const page = await connect(port);
  await page.waitFor("(!!document.querySelector('.sidebar') && !document.querySelector('.splash'))", 300000, 'the workspace');
  return { app, page };
}

const port = 9444;
let { app, page } = await start(HOME, port);
say('started; local status:', JSON.stringify(await page.invoke('local_status')).slice(0, 120));
const projects = await page.invoke('api_get', { path: 'projects', env: 'local' });
const pid = projects.body.items[0].id;
const files = (await page.invoke('api_get', { path: `projects/${pid}/files`, env: 'local' })).body.items;
const before = {};
for (const f of files) {
  const d = await page.invoke('api_get', { path: `documents/${f.id}`, env: 'local' });
  const ev = await page.invoke('api_get', { path: `documents/${f.id}/events?limit=1000`, env: 'local' });
  const last = ev.body.items[ev.body.items.length - 1];
  const hex = (v) => (typeof v === 'string' ? v : v.map((b) => b.toString(16).padStart(2, '0')).join(''));
  before[f.id] = { title: f.title, seq: d.body.latest_seq, head: hex(last.chain_hash) };
  const v = engineVerify(HOME, f.id, before[f.id].head);
  say('  before:', f.title, 'seq', d.body.latest_seq, 'engine-cli verify:', v.ok ? 'ok' : v.out);
}
// A burst of writes: many small documents, each created with one batch of 41 operations, four at a time.
say('writing a burst of new documents');
await page.eval(`(async () => {
  window.__burst = { sent: 0, ok: 0, fail: 0 };
  const A = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
  const ulid = () => { let t = Date.now(), s = ''; for (let i = 0; i < 10; i++) { s = A[t % 32] + s; t = Math.floor(t / 32); } for (let i = 0; i < 16; i++) s += A[Math.floor(Math.random() * 32)]; return s; };
  const call = (m, p, b) => window.__TAURI_INTERNALS__.invoke('api_send', { method: m, path: p, body: b, ifMatch: null, env: 'local' });
  const one = async (i) => {
    window.__burst.sent++;
    try {
      const d = await call('POST', 'documents', { title: 'Burst ' + i });
      const root = ulid();
      const ops = [{ type: 'NodeCreated', node_id: root, node_type: 'form', pos: '000000', fields: { label: 'Burst ' + i } }];
      for (let k = 0; k < 40; k++) { const id = ulid(); ops.push({ type: 'NodeCreated', node_id: id, node_type: 'item', parent_id: root, pos: String(k).padStart(8, '0'), fields: { label: 'Line ' + k + ' ' + 'x'.repeat(300), content: { type: 'paragraph', attrs: { blockId: id }, content: [{ type: 'text', text: 'Line ' + k }] } } }); }
      const r = await call('POST', 'documents/' + d.body.id + '/batch', { ops, base_seq: 0 });
      r.status === 200 ? window.__burst.ok++ : window.__burst.fail++;
    } catch { window.__burst.fail++; }
  };
  let next = 0;
  const worker = async () => { while (next < 80) { await one(next++); } };
  for (let w = 0; w < 4; w++) worker();
})()`);
await sleep(1500);
say('burst so far:', JSON.stringify(await page.eval('window.__burst')));
// The crash: the app and the database go at once, with no chance to shut down.
const db = dbPort(HOME);
spawnSync('taskkill', ['/PID', String(db.pid), '/T', '/F'], { stdio: 'ignore' });
killTree(app.child);
await sleep(2500);
say('killed the app and the database (postmaster', db.pid + ')');
// Start again.
({ app, page } = await start(HOME, port));
say('restarted; local status:', JSON.stringify(await page.invoke('local_status')).slice(0, 120));
const pglog = readFileSync(HOME + '/data/workspace/logs/postgres.log', 'utf8').split('\n').filter((l) => /recovery|redo|database system was|ready to accept/i.test(l)).slice(-5);
say('database log after the crash:', pglog.join(' | '));
const after = (await page.invoke('api_get', { path: `projects/${pid}/files`, env: 'local' })).body.items;
for (const f of after) {
  const ev = await page.invoke('api_get', { path: `documents/${f.id}/events?limit=1000`, env: 'local' });
  const items = ev.body.items;
  const seqs = items.map((e) => e.seq);
  const contiguous = seqs.every((s, i) => s === i + 1);
  const v = engineVerify(HOME, f.id, before[f.id]?.head);
  const apiv = await page.invoke('api_get', { path: `documents/${f.id}/verify`, env: 'local' });
  say('  after:', f.title, 'events', items.length, '(was', before[f.id]?.seq + ')', 'contiguous', contiguous, '| server verify', JSON.stringify(apiv.body), '| engine-cli verify through the earlier head:', v.ok ? 'ok' : v.out);
}
const rows = JSON.parse(execFileSync('D:/verne-build/dynodoc/scratch/venv/Scripts/python.exe', ['D:/verne-build/dynodoc/scratch/drive/crashcheck.py', String(dbPort(HOME).port), secretsFor(HOME).owner], { encoding: 'utf8' }));
let full = 0, empty = 0, partial = 0, badChain = 0;
for (const [id, , count] of rows) {
  if (count === 41) full++; else if (count === 0) empty++; else partial++;
  if (!engineVerify(HOME, id).ok) badChain++;
}
say('burst documents after the crash: complete', full, '| empty (created, batch not committed)', empty, '| partial', partial, '| chain failures', badChain);
await page.click('[data-view=history]'); await sleep(800);
await page.eval("[...document.querySelectorAll('button')].find((b) => b.textContent.trim().startsWith('Verify every file')).click()"); await sleep(3500);
say('UI verify:', (await page.eval("[...document.querySelectorAll('.toast')].map((t) => t.innerText.replace(/\\\\s+/g, ' ')).join(' | ')")));
await page.shot(EV + 'after-crash.png');
page.close();
say('closed gracefully:', await closeGracefully(app.child));
writeFileSync(EV + 'acc7.log', log.join('\n'));
