// The live mode's mapping between a Dynodoc server's nodes and events and the client's blocks,
// commits and requests, and the requests a change in the client turns into. No network: the
// transport is a fake that answers like the engine and records what it was asked.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { applyEdits, compareBlocks } from '../src/lib/diff.ts';
import { applyEvent, blocksOf, editsToOps, runsOf, describeRun, filenameOf, hex, blockOfKey, Unsupported } from '../src/data/remote-map.ts';
import { changed, load, reconcile, renameIds } from '../src/data/remote.ts';
import * as B from '../src/data/backend.ts';

let n = 0;
const id = () => `01TEST${String(++n).padStart(20, '0')}`;
const para = (text, heading) => ({ type: heading ? 'heading' : 'paragraph', content: text ? [{ type: 'text', text }] : [] });

function docNodes() {
  const nodes = {};
  const ops = [
    { type: 'NodeCreated', node_id: 'R', node_type: 'form', pos: '000000', fields: { label: 'Doc' } },
    { type: 'NodeCreated', node_id: 'A', node_type: 'item', parent_id: 'R', pos: '00000000', fields: { label: 'Title', content: para('Title', true) } },
    { type: 'NodeCreated', node_id: 'B', node_type: 'item', parent_id: 'R', pos: '00000016', fields: { label: 'First paragraph.', content: para('First paragraph.') } },
    { type: 'NodeCreated', node_id: 'C', node_type: 'item', parent_id: 'R', pos: '00000032', fields: { label: 'Second paragraph.', content: para('Second paragraph.') } },
  ];
  for (const op of ops) applyEvent(nodes, op);
  return nodes;
}

function sheetNodes() {
  const nodes = {};
  for (const op of [
    { type: 'NodeCreated', node_id: 'R', node_type: 'form', pos: '000000', fields: { label: 'Budget' } },
    { type: 'NodeCreated', node_id: 'S', node_type: 'section', parent_id: 'R', pos: '00000000', fields: { label: 'Budget', kind: 'sheet', columns: 8 } },
    { type: 'NodeCreated', node_id: 'H', node_type: 'item', parent_id: 'S', pos: '00000000', fields: { cell_0: 'Line', cell_2: 'Plan' } },
    { type: 'NodeCreated', node_id: 'T', node_type: 'item', parent_id: 'S', pos: '00000001', fields: { cell_0: 'Travel', cell_2: '24000' } },
  ])
    applyEvent(nodes, op);
  return nodes;
}

function deckNodes() {
  const nodes = {};
  for (const op of [
    { type: 'NodeCreated', node_id: 'R', node_type: 'form', pos: '000000', fields: { label: 'Deck' } },
    { type: 'NodeCreated', node_id: 'S1', node_type: 'section', parent_id: 'R', pos: '00000000', fields: { label: 'Q3', kind: 'slide', notes: '' } },
    { type: 'NodeCreated', node_id: 'T1', node_type: 'item', parent_id: 'S1', pos: '0000', fields: { kind: 'text', role: 'title', text: 'Q3', label: 'Q3' } },
    { type: 'NodeCreated', node_id: 'B1', node_type: 'item', parent_id: 'S1', pos: '0001', fields: { kind: 'text', role: 'body', text: 'Points', label: 'Points' } },
  ])
    applyEvent(nodes, op);
  return nodes;
}

test('documents, sheets and decks become blocks with stable ids and readable labels', () => {
  const d = blocksOf('document', docNodes());
  assert.deepEqual(d.blocks.map((b) => [b.id, b.label, b.text]), [['A', '¶ 1 · Heading', 'Title'], ['B', '¶ 2', 'First paragraph.'], ['C', '¶ 3', 'Second paragraph.']]);
  const s = blocksOf('spreadsheet', sheetNodes());
  assert.deepEqual(s.blocks.map((b) => [b.id, b.label, b.text]), [['H:cell_0', 'Budget!A1', 'Line'], ['H:cell_2', 'Budget!C1', 'Plan'], ['T:cell_0', 'Budget!A2', 'Travel'], ['T:cell_2', 'Budget!C2 · Travel, Plan', '24000']]);
  const k = blocksOf('presentation', deckNodes());
  assert.deepEqual(k.blocks.map((b) => [b.id, b.label]), [['T1', 'Slide 1 · Title'], ['B1', 'Slide 1 · Body']]);
  assert.ok(k.slots['S1:notes'], 'empty notes are a place to add to');
  // A deleted node is not a block.
  const nodes = docNodes();
  applyEvent(nodes, { type: 'NodeDeleted', node_id: 'B' });
  assert.deepEqual(blocksOf('document', nodes).blocks.map((b) => b.id), ['A', 'C']);
});

test('the client’s edits become engine operations that produce exactly those edits', () => {
  for (const [kind, make, edits] of [
    ['document', docNodes, [
      { blockId: 'B', op: 'edit', before: 'First paragraph.', after: 'First paragraph, edited.' },
      { blockId: 'new1', op: 'add', afterId: 'B', label: '¶ 3', after: 'Inserted between.' },
      { blockId: 'C', op: 'remove', before: 'Second paragraph.' },
    ]],
    ['spreadsheet', sheetNodes, [{ blockId: 'T:cell_2', op: 'edit', before: '24000', after: '26800' }]],
    ['presentation', deckNodes, [
      { blockId: 'T1', op: 'edit', before: 'Q3', after: 'Q3 review' },
      { blockId: 'S1:notes', op: 'add', afterId: 'B1', label: 'Slide 1 · Notes', after: 'Say hello.' },
    ]],
  ]) {
    const nodes = make();
    const before = blocksOf(kind, nodes);
    const ops = editsToOps(edits, { kind, nodes, slots: before.slots, newId: id });
    assert.ok(ops.length, kind);
    // The operations reference only known nodes, and the source map is not changed.
    assert.deepEqual(blocksOf(kind, nodes).blocks, before.blocks);
    const after = structuredClone(nodes);
    for (const op of ops) applyEvent(after, op);
    const got = blocksOf(kind, after).blocks.map((b) => b.text);
    const want = applyEdits(before.blocks, edits).map((b) => b.text);
    assert.deepEqual(got, want, kind);
  }
  // A paragraph edit keeps the heading and writes both the rich content and the plain label.
  const ops = editsToOps([{ blockId: 'A', op: 'edit', before: 'Title', after: 'New title' }], { kind: 'document', nodes: docNodes(), slots: blocksOf('document', docNodes()).slots, newId: id });
  assert.deepEqual(ops.map((o) => o.field), ['content', 'label']);
  assert.equal(ops[0].value.type, 'heading');
  // An edit of a block that is no longer there is reported, not guessed.
  assert.throws(() => editsToOps([{ blockId: 'ZZ', op: 'edit', before: 'x', after: 'y' }], { kind: 'document', nodes: docNodes(), slots: {}, newId: id }), Unsupported);
});

test('events group into revisions by commit message, else by person and moment', () => {
  const at = (s) => new Date(1_800_000_000_000 + s * 1000).toISOString();
  const ev = (seq, actor, s, type = 'FieldEdited') => ({ id: `e${seq}`, seq, type, actor_id: actor, payload: { type }, content_hash: [1, 2], prev_chain_hash: [0], chain_hash: [171, 205], created_at: at(s) });
  const events = [ev(1, 'a', 0), ev(2, 'a', 0), ev(3, 'a', 0), ev(4, 'b', 1), ev(5, 'b', 30), ev(6, 'b', 30, 'CommentAdded')];
  const runs = runsOf(events, [{ from: 1, to: 2 }]);
  assert.deepEqual(runs.map((r) => [r.from, r.to]), [[1, 2], [3, 3], [4, 4], [5, 5], [6, 6]]);
  assert.equal(describeRun(runs[4], [], {}), 'Comment added');
  assert.equal(describeRun(runs[1], [{ blockId: 'x', op: 'edit', before: 'a', after: 'b' }], { x: '¶ 2' }), 'Edit ¶ 2');
  assert.equal(hex([171, 205]), 'abcd');
  assert.equal(filenameOf('Budget 2027', 'spreadsheet'), 'budget-2027.xlsx');
  assert.equal(blockOfKey('T:cell_2', { 'T:cell_2': {} }), 'T:cell_2');
  assert.equal(blockOfKey('B:content', {}), 'B');
});

// ---- load and reconcile against a fake engine ---------------------------------------------------

const P = '0190f5b2-0000-7000-8000-000000000001';
const D = '0190f5b2-0000-7000-8000-000000000002';
const ME = '0190f5b2-0000-7000-8000-0000000000aa';
const OTHER = '0190f5b2-0000-7000-8000-0000000000bb';

function fakeServer() {
  const nodes = docNodes();
  const events = [];
  let seq = 0;
  const stamp = (payload, actor = OTHER) => {
    seq += 1;
    events.push({ id: `E${seq}`, seq, type: payload.type, actor_id: actor, payload, content_hash: [seq], prev_chain_hash: [seq - 1], chain_hash: [seq], created_at: new Date(1_800_000_000_000 + seq * 5000).toISOString() });
  };
  for (const n of Object.values(nodes)) stamp({ type: 'NodeCreated', node_id: n.id, node_type: n.type, parent_id: n.parent_id, pos: n.pos, fields: n.current_fields });
  const state = { starred: false, sent: [], svc: {} };
  const json = (body, status = 200) => ({ status, body });
  const t = {
    async apiGet(path) {
      if (path === 'profile') return json({ id: ME, name: 'Visitor', revision: 0 });
      if (path === 'projects') return json({ items: [{ id: P, name: 'Handbook', description: '', role: 'manager', revision: 1, starred: state.starred, star_count: 0, watch: 'ignore', policy: { protect_team_version: false, required_approvals: 1, merge_roles: 'editors' } }] });
      if (path === `projects/${P}/files`) return json({ items: [{ id: D, title: 'Handbook', kind: 'document', latest_seq: seq, role: 'manager', updated_at: new Date().toISOString() }] });
      if (path === `spaces/${P}/members`) return json({ items: [{ id: ME, email: 'v@example.org', name: 'Visitor', role: 'manager' }, { id: OTHER, email: 'o@example.org', name: 'Other', role: 'owner' }] });
      if (path === `documents/${D}`) return json({ id: D, title: 'Handbook', created_at: new Date().toISOString(), latest_seq: seq, state: { nodes } });
      if (path.startsWith(`documents/${D}/events`)) {
        const from = Number(/from_seq=(\d+)/.exec(path)?.[1] ?? 1);
        return json({ items: events.filter((e) => e.seq >= from), next_cursor: null });
      }
      if (path === `documents/${D}/verify`) return json({ ok: true });
      return json({ items: [] });
    },
    async apiSend(method, path, body, ifMatch) {
      state.sent.push([method, path, body, ifMatch]);
      if (path === `projects/${P}/star`) {
        state.starred = method === 'PUT';
        return json({ starred: state.starred });
      }
      if (path === `documents/${D}/batch`) {
        const out = [];
        for (const op of body.ops) {
          applyEvent(nodes, op);
          stamp(op, ME);
          out.push(events[events.length - 1]);
        }
        return json({ events: out });
      }
      return json({ ok: true });
    },
    async svcGet(path) {
      if (path.startsWith('state/')) return json({ value: state.svc[path.slice(6)] ?? null });
      if (path.startsWith('notes')) return json({ items: [] });
      return json(null, 404);
    },
    async svcSend(method, path, body) {
      state.sent.push([method, `svc:${path}`, body]);
      if (path.startsWith('state/')) state.svc[path.slice(6)] = body.value;
      return json({ id: 1 });
    },
  };
  return { t, state, nodes };
}

test('a live workspace loads into the client model, and the server’s copy has no pending change', async () => {
  const { t } = fakeServer();
  const { db, ctx } = await load(t, { identity: ME, visitor: null });
  assert.equal(db.me, ME);
  assert.equal(db.projects[0].name, 'Handbook');
  assert.deepEqual(db.docs[0].blocks.map((b) => b.text), ['Title', 'First paragraph.', 'Second paragraph.']);
  assert.equal(B.mainSeq(db, D), 4);
  assert.ok(B.verify(db, D).every((v) => v.ok));
  assert.equal(changed(ctx, db, null), false);
});

test('a star, a commit pushed to main and the captured changes reach the server and the service', async () => {
  const { t, state, nodes } = fakeServer();
  const { db, ctx } = await load(t, { identity: ME, visitor: null });
  const next = structuredClone(db);
  next.projects[0].starred = true;
  next.working = [{ id: 'w1', docId: D, branch: 'main', staged: true, source: 'word', capturedAt: 1, edit: { blockId: 'B', op: 'edit', before: 'First paragraph.', after: 'First paragraph, from Word.' } }];
  B.commitStaged(next, P, 'main', 'Say where it came from', '');
  B.push(next, P, 'main');
  assert.equal(changed(ctx, next, null), true);
  const out = await reconcile(t, ctx, next, null);
  assert.deepEqual(out.errors, []);
  assert.ok(state.sent.some(([m, p]) => m === 'PUT' && p === `projects/${P}/star`));
  const batch = state.sent.find(([, p]) => p === `documents/${D}/batch`);
  assert.equal(batch[2].base_seq, 4);
  assert.equal(nodes.B.current_fields.label, 'First paragraph, from Word.');
  assert.ok(state.sent.some(([m, p, b]) => p === 'svc:notes' && b.summary === 'Say where it came from' && b.from_seq === 5 && b.to_seq === 6));
  // A commit made and pushed before the save is not kept as unpushed.
  assert.ok(!state.svc.local?.length);
  const again = await load(t, { identity: ME, visitor: null });
  assert.equal(again.db.projects[0].starred, true);
  assert.equal(again.db.docs[0].blocks[1].text, 'First paragraph, from Word.');
});

test('an unpushed commit is kept by the service and comes back after a reload', async () => {
  const { t, state } = fakeServer();
  const { db, ctx } = await load(t, { identity: ME, visitor: null });
  const next = structuredClone(db);
  next.working = [{ id: 'w1', docId: D, branch: 'main', staged: true, source: 'word', capturedAt: 1, edit: { blockId: 'C', op: 'edit', before: 'Second paragraph.', after: 'Second, not pushed.' } }];
  B.commitStaged(next, P, 'main', 'Not yet', '');
  await reconcile(t, ctx, next, null);
  assert.equal(state.svc.local.length, 1);
  assert.ok(!state.sent.some(([, p]) => p === `documents/${D}/batch`), 'nothing reaches main before a push');
  const again = await load(t, { identity: ME, visitor: null });
  assert.equal(B.aheadOf(again.db, P, 'main'), 1);
  assert.equal(again.db.docs[0].blocks[2].text, 'Second, not pushed.');
});

test('local ids are replaced everywhere once the server has named the thing', () => {
  assert.deepEqual(renameIds({ a: 'draft_x', b: ['draft_x', 'draft_xy'] }, { draft_x: 'uuid-1' }), { a: 'uuid-1', b: ['uuid-1', 'draft_xy'] });
  assert.deepEqual(compareBlocks([{ id: 'a', label: 'a', text: '1' }], [{ id: 'a', label: 'a', text: '2' }]).map((e) => e.op), ['edit']);
});
