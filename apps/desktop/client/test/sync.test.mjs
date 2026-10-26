// Syncing a project on this computer with a server (data/sync.ts): files made on one side appear
// on the other with the same node ids, changes that do not touch the same field go both ways, the
// same field changed on both sides waits for a person's choice, nothing is written twice, and a
// server whose rules refuse a direct write gets the changes as a change request.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { applyEvent } from '../src/data/remote-map.ts';
import { applyPlan, canon, changesOf, chunkOps, diffNodes, listFiles, newLink, planSync, sendForReview } from '../src/data/sync.ts';

/** An engine in memory: the routes sync uses, with revisions, a direct-write rule and requests. */
function engine(name, { review = false } = {}) {
  const docs = new Map();
  const requests = [];
  let n = 0;
  const e = { name, docs, requests, writes: 0, review };
  const find = (id) => {
    const d = docs.get(id);
    if (!d) throw new Error(`no document ${id}`);
    return d;
  };
  const fold = (d, through) => {
    const nodes = {};
    for (const ev of d.events) if (ev.seq <= through) applyEvent(nodes, ev.payload);
    return nodes;
  };
  const reply = (status, body) => ({ status, body });
  e.transport = {
    async apiGet(path) {
      const [p, q = ''] = path.split('?');
      let m;
      if ((m = /^projects\/([^/]+)\/files$/.exec(p))) {
        const offset = Number(/offset=(\d+)/.exec(q)?.[1] ?? 0);
        const all = [...docs.values()].filter((d) => d.project === m[1]);
        return reply(200, { items: all.slice(offset).map((d) => ({ id: d.id, title: d.title, kind: d.kind, latest_seq: d.events.length })) });
      }
      if ((m = /^documents\/([^/]+)\/state$/.exec(p))) {
        const d = find(m[1]);
        return reply(200, { state: { nodes: fold(d, Number(/through_seq=(\d+)/.exec(q)[1])) } });
      }
      if ((m = /^documents\/([^/]+)$/.exec(p))) {
        const d = find(m[1]);
        return reply(200, { title: d.title, latest_seq: d.events.length, settings: { kind: d.kind }, state: { nodes: fold(d, d.events.length) } });
      }
      return reply(404, { error: 'not found' });
    },
    async apiSend(method, path, bodyIn) {
      let m;
      if (method === 'POST' && path === 'documents') {
        const id = `${name}-doc-${++n}`;
        docs.set(id, { id, title: bodyIn.title, kind: 'document', project: null, events: [] });
        return reply(200, { id });
      }
      if ((m = /^documents\/([^/]+)\/metadata$/.exec(path))) {
        find(m[1]).kind = bodyIn.settings.kind;
        return reply(200, {});
      }
      if ((m = /^documents\/([^/]+)\/location$/.exec(path))) {
        find(m[1]).project = bodyIn.space_id;
        return reply(200, {});
      }
      if ((m = /^documents\/([^/]+)\/batch$/.exec(path))) {
        const d = find(m[1]);
        if (review) return reply(403, { error: 'forbidden' });
        if (bodyIn.base_seq !== d.events.length) return reply(409, { error: 'conflict' });
        const events = bodyIn.ops.map((payload) => {
          const ev = { seq: d.events.length + 1, payload };
          d.events.push(ev);
          return { seq: ev.seq };
        });
        e.writes += bodyIn.ops.length;
        return reply(200, { events });
      }
      if ((m = /^documents\/([^/]+)\/change-requests$/.exec(path))) {
        const id = `${name}-cr-${requests.length + 1}`;
        requests.push({ id, document: m[1], ...bodyIn });
        return reply(200, { id });
      }
      if (method === 'POST' && path === 'projects') return reply(200, { id: `${name}-project` });
      return reply(404, { error: 'not found' });
    },
    async svcGet() { return reply(404, {}); },
    async svcSend() { return reply(404, {}); },
  };
  return e;
}

const POS = (i) => String.fromCharCode(96 + i);
const para = (id, text, i) => ({ type: 'NodeCreated', node_id: id, node_type: 'item', parent_id: 'root', pos: POS(i), fields: { label: text, content: { type: 'paragraph', content: [{ type: 'text', text }] } } });
const ROOT = { type: 'NodeCreated', node_id: 'root', node_type: 'form', parent_id: null, pos: 'a', fields: { title: 'Charter' } };

async function seedDoc(e, project, id, title, ops) {
  await e.transport.apiSend('POST', 'documents', { title });
  const made = [...e.docs.values()].pop();
  made.project = project;
  for (const op of ops) made.events.push({ seq: made.events.length + 1, payload: op });
  return made.id;
}

const edit = (doc, id, text) => doc.events.push({ seq: doc.events.length + 1, payload: { type: 'FieldEdited', node_id: id, field: 'label', value: text } });

function setup() {
  const local = engine('L');
  const remote = engine('R');
  const sides = { local: local.transport, remote: remote.transport, localProjectId: 'lp' };
  const link = newLink('srv_1', 'rp', 'Riverside');
  return { local, remote, sides, link };
}

const sync = async (ctx, picks = {}) => {
  const plan = await planSync(ctx.sides, ctx.link, picks);
  const done = await applyPlan(ctx.sides, ctx.link, plan, { host: 'the server' });
  return { plan, done };
};

test('a file only here is created on the server with the same node ids, and nothing more is sent next time', async () => {
  const ctx = setup();
  const id = await seedDoc(ctx.local, 'lp', 'd1', 'Charter', [ROOT, para('p1', 'One', 1), para('p2', 'Two', 2)]);
  const first = await sync(ctx);
  assert.equal(first.plan.counts['add-remote'], 1);
  assert.equal(first.done.created, 1);
  const copy = [...ctx.remote.docs.values()][0];
  assert.equal(copy.title, 'Charter');
  assert.equal(copy.project, 'rp');
  assert.deepEqual(copy.events.map((x) => x.payload.node_id), ['root', 'p1', 'p2']);
  assert.equal(ctx.link.files[id].remoteDocId, copy.id);
  const writes = ctx.remote.writes;
  const again = await sync(ctx);
  assert.equal(again.plan.counts.same, 1);
  assert.equal(ctx.remote.writes, writes, 'a second sync writes nothing');
  assert.ok(ctx.link.lastSyncedAt);
});

test('a file only on the server is created here', async () => {
  const ctx = setup();
  await seedDoc(ctx.remote, 'rp', 'r1', 'Budget', [ROOT, para('p1', 'Wages', 1)]);
  const { plan, done } = await sync(ctx);
  assert.equal(plan.counts['add-local'], 1);
  assert.equal(done.created, 1);
  const mine = [...ctx.local.docs.values()][0];
  assert.equal(mine.project, 'lp');
  assert.deepEqual(mine.events.map((x) => x.payload.node_id), ['root', 'p1']);
});

test('an edit here goes to the server, an edit there comes here, and each side keeps its own revisions', async () => {
  const ctx = setup();
  const id = await seedDoc(ctx.local, 'lp', 'd1', 'Charter', [ROOT, para('p1', 'One', 1), para('p2', 'Two', 2)]);
  await sync(ctx);
  const copy = [...ctx.remote.docs.values()][0];
  edit(ctx.local.docs.get(id), 'p1', 'One, amended here');
  const s1 = await sync(ctx);
  assert.equal(s1.plan.counts.send, 1);
  assert.equal(s1.done.sent, 1);
  const read = (e, doc, node) => {
    const nodes = {};
    for (const ev of doc.events) applyEvent(nodes, ev.payload);
    return nodes[node].current_fields.label;
  };
  assert.equal(read(ctx.remote, copy, 'p1'), 'One, amended here');
  edit(copy, 'p2', 'Two, amended there');
  const s2 = await sync(ctx);
  assert.equal(s2.plan.counts.receive, 1);
  assert.equal(read(ctx.local, ctx.local.docs.get(id), 'p2'), 'Two, amended there');
  assert.equal((await sync(ctx)).plan.counts.same, 1);
});

test('changes to different fields on both sides are both kept, on both sides', async () => {
  const ctx = setup();
  const id = await seedDoc(ctx.local, 'lp', 'd1', 'Charter', [ROOT, para('p1', 'One', 1), para('p2', 'Two', 2)]);
  await sync(ctx);
  const copy = [...ctx.remote.docs.values()][0];
  edit(ctx.local.docs.get(id), 'p1', 'One here');
  edit(copy, 'p2', 'Two there');
  const { plan } = await sync(ctx);
  assert.equal(plan.counts.both, 1);
  const labels = (doc) => {
    const nodes = {};
    for (const ev of doc.events) applyEvent(nodes, ev.payload);
    return ['p1', 'p2'].map((k) => nodes[k].current_fields.label);
  };
  assert.deepEqual(labels(ctx.local.docs.get(id)), ['One here', 'Two there']);
  assert.deepEqual(labels(copy), ['One here', 'Two there']);
  assert.equal((await sync(ctx)).plan.counts.same, 1);
});

test('the same field changed on both sides waits for a choice, writes nothing, and the choice is honoured', async () => {
  const ctx = setup();
  const id = await seedDoc(ctx.local, 'lp', 'd1', 'Charter', [ROOT, para('p1', 'Seven years', 1)]);
  await sync(ctx);
  const copy = [...ctx.remote.docs.values()][0];
  edit(ctx.local.docs.get(id), 'p1', 'Ten years');
  edit(copy, 'p1', 'Five years');
  const lw = ctx.local.writes;
  const rw = ctx.remote.writes;
  const held = await sync(ctx);
  assert.equal(held.plan.counts.conflict, 1);
  assert.equal(held.done.needs, 1);
  assert.equal(ctx.local.writes, lw);
  assert.equal(ctx.remote.writes, rw);
  const c = held.plan.items[0].conflicts[0];
  assert.equal(c.local, 'Ten years');
  assert.equal(c.remote, 'Five years');
  assert.match(c.key, /^f:p1:/);
  // Keep the server's version: it comes here, and nothing is written to the server.
  const picks = { [id]: Object.fromEntries(held.plan.items[0].conflicts.map((x) => [x.key, 'remote'])) };
  const chosen = await sync(ctx, picks);
  assert.equal(chosen.plan.items[0].action, 'receive');
  const nodes = {};
  for (const ev of ctx.local.docs.get(id).events) applyEvent(nodes, ev.payload);
  assert.equal(nodes.p1.current_fields.label, 'Five years');
  assert.equal(ctx.remote.writes, rw);
  assert.equal((await sync(ctx)).plan.counts.same, 1);
});

test('a node deleted on one side and edited on the other is one choice about the whole node', async () => {
  const ctx = setup();
  const id = await seedDoc(ctx.local, 'lp', 'd1', 'Charter', [ROOT, para('p1', 'One', 1), para('p2', 'Two', 2)]);
  await sync(ctx);
  const copy = [...ctx.remote.docs.values()][0];
  ctx.local.docs.get(id).events.push({ seq: ctx.local.docs.get(id).events.length + 1, payload: { type: 'NodeDeleted', node_id: 'p2' } });
  edit(copy, 'p2', 'Two, edited there');
  const held = await sync(ctx);
  assert.equal(held.plan.counts.conflict, 1);
  assert.equal(held.plan.items[0].conflicts[0].key, 'node:p2');
  assert.equal(held.plan.items[0].conflicts[0].local, 'Deleted here');
  const keep = await sync(ctx, { [id]: { 'node:p2': 'remote' } });
  assert.equal(keep.done.received, 1);
  const nodes = {};
  for (const ev of ctx.local.docs.get(id).events) applyEvent(nodes, ev.payload);
  assert.equal(nodes.p2.deleted, false);
  assert.equal(nodes.p2.current_fields.label, 'Two, edited there');
});

test('a paragraph added on each side lands in both files, in order', async () => {
  const ctx = setup();
  const id = await seedDoc(ctx.local, 'lp', 'd1', 'Charter', [ROOT, para('p1', 'One', 1)]);
  await sync(ctx);
  const copy = [...ctx.remote.docs.values()][0];
  ctx.local.docs.get(id).events.push({ seq: ctx.local.docs.get(id).events.length + 1, payload: para('here', 'Added here', 5) });
  copy.events.push({ seq: copy.events.length + 1, payload: para('there', 'Added there', 7) });
  await sync(ctx);
  const order = (doc) => {
    const nodes = {};
    for (const ev of doc.events) applyEvent(nodes, ev.payload);
    return Object.values(nodes).filter((x) => x.parent_id === 'root').sort((a, b) => (a.pos < b.pos ? -1 : 1)).map((x) => x.id);
  };
  assert.deepEqual(order(ctx.local.docs.get(id)), ['p1', 'here', 'there']);
  assert.deepEqual(order(copy), ['p1', 'here', 'there']);
});

test('a server whose rules refuse a direct write gets the changes as a change request, once', async () => {
  const ctx = setup();
  const id = await seedDoc(ctx.local, 'lp', 'd1', 'Charter', [ROOT, para('p1', 'One', 1)]);
  await sync(ctx);
  const copy = [...ctx.remote.docs.values()][0];
  ctx.remote.review = true;
  // The fake engine reads `review` from the closure it was made with: make it a rule now.
  const remote2 = engine('R2', { review: true });
  remote2.docs.set(copy.id, copy);
  const sides = { ...ctx.sides, remote: remote2.transport };
  edit(ctx.local.docs.get(id), 'p1', 'One, for review');
  const plan = await planSync(sides, ctx.link);
  assert.equal(plan.counts.send, 1);
  const done = await applyPlan(sides, ctx.link, plan, { host: 'the server' });
  assert.equal(done.needs, 1);
  assert.equal(plan.items[0].action, 'needs-review');
  assert.equal(copy.events.length, 2, 'nothing was written to the main line');
  const made = await sendForReview(sides, ctx.link, plan.items[0], 'Priya');
  assert.ok(made.requestId);
  assert.equal(remote2.requests.length, 1);
  assert.equal(remote2.requests[0].submit, true);
  assert.equal(remote2.requests[0].ops.length, 1);
  assert.equal(remote2.requests[0].base_seq, 2);
  const again = await planSync(sides, ctx.link);
  assert.equal(again.counts.waiting, 1, 'it does not ask again while the request is open');
});

test('a file gone from the server is reported and not sent again', async () => {
  const ctx = setup();
  await seedDoc(ctx.local, 'lp', 'd1', 'Charter', [ROOT, para('p1', 'One', 1)]);
  await sync(ctx);
  ctx.remote.docs.clear();
  const { plan } = await sync(ctx);
  assert.equal(plan.counts['missing-remote'], 1);
  assert.equal(ctx.remote.docs.size, 0);
});

test('a write that finds the file changed meanwhile is refused, and the next sync starts over', async () => {
  const ctx = setup();
  const id = await seedDoc(ctx.local, 'lp', 'd1', 'Charter', [ROOT, para('p1', 'One', 1)]);
  await sync(ctx);
  const copy = [...ctx.remote.docs.values()][0];
  edit(ctx.local.docs.get(id), 'p1', 'One here');
  const plan = await planSync(ctx.sides, ctx.link);
  edit(copy, 'p1', 'One there, meanwhile');
  const done = await applyPlan(ctx.sides, ctx.link, plan, { host: 'the server' });
  assert.equal(done.held.length, 1);
  assert.match(done.held[0].why, /changed on the other side/);
  assert.equal((await planSync(ctx.sides, ctx.link)).counts.conflict, 1);
});

test('operations are a plain diff: parents first, only what differs, nothing removed', () => {
  const a = { r: { id: 'r', type: 'form', pos: 'a', parent_id: null, current_fields: {} }, x: { id: 'x', type: 'item', pos: 'b', parent_id: 'r', current_fields: { label: 'x' } } };
  const b = {
    ...a,
    x: { ...a.x, current_fields: { label: 'y', extra: 1 }, pos: 'c' },
    z: { id: 'z', type: 'item', pos: 'a', parent_id: 'k', current_fields: { label: 'z' } },
    k: { id: 'k', type: 'section', pos: 'z', parent_id: 'r', current_fields: {} },
  };
  const ops = diffNodes(a, b);
  assert.deepEqual(ops.map((o) => `${o.type}:${o.node_id}`).slice(0, 2), ['NodeCreated:k', 'NodeCreated:z']);
  assert.ok(ops.some((o) => o.type === 'NodeMoved' && o.node_id === 'x' && o.new_pos === 'c'));
  assert.ok(ops.some((o) => o.type === 'FieldEdited' && o.field === 'label' && o.value === 'y'));
  assert.deepEqual(diffNodes(b, b), []);
  assert.deepEqual(diffNodes(b, a), [ops.find((o) => o.type === 'NodeMoved' && o.node_id === 'x') && { type: 'NodeMoved', node_id: 'x', new_parent_id: 'r', new_pos: 'b' }, { type: 'FieldEdited', node_id: 'x', field: 'extra', value: null }, { type: 'FieldEdited', node_id: 'x', field: 'label', value: 'x' }].filter(Boolean).sort((p, q) => (p.type === q.type ? 0 : p.type === 'NodeMoved' ? -1 : 1)));
  assert.equal(canon({ b: 1, a: [2, { d: 1, c: 2 }] }), canon({ a: [2, { c: 2, d: 1 }], b: 1 }));
  assert.equal(changesOf(a, a).size, 0);
});

test('large files are written in several batches, each within a size and a count', () => {
  const ops = Array.from({ length: 1000 }, (_, i) => ({ type: 'FieldEdited', node_id: `n${i}`, field: 'label', value: 'x'.repeat(100) }));
  const parts = chunkOps(ops);
  assert.ok(parts.length >= 3);
  assert.equal(parts.flat().length, 1000);
  for (const p of parts) assert.ok(p.length <= 400 && JSON.stringify(p).length < 320 * 1024);
});

test('a server that ignores the page offset does not make the listing loop', async () => {
  const items = Array.from({ length: 20 }, (_, i) => ({ id: `f${i}`, title: `F${i}`, latest_seq: 1 }));
  const t = { async apiGet() { return { status: 200, body: { items } }; } };
  const files = await listFiles(t, 'p');
  assert.equal(files.length, 20);
});
