// The scripted backend follows Dynodoc's rules on the sample: append-only history whose chain
// verifies, commits per file, drafts, conflicts, review rules before a merge, recovery drafts.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { seed } from '../src/data/sample.ts';
import * as B from '../src/data/backend.ts';
import { plan, execute } from '../src/data/agent.ts';
import { TOOLS } from '../src/lib/agentTools.ts';

test('the sample history verifies, and an edited revision does not', () => {
  const db = seed(1_800_000_000_000);
  for (const d of db.docs) assert.ok(B.verify(db, d.id).every((v) => v.ok), d.filename);
  const c = B.branchCommits(db, 'd_handbook', 'main')[3];
  c.edits = [{ ...c.edits[0], after: 'Tampered.' }];
  const res = B.verify(db, 'd_handbook');
  assert.equal(res.find((v) => v.commitId === c.id).ok, false);
});

test('the sample is neutral: example.org addresses only', () => {
  const db = seed();
  for (const p of db.people) if (!p.agent) assert.match(p.email, /@example\.org$/);
});

test('sample budgets add up', () => {
  const db = seed();
  const d = db.docs.find((x) => x.id === 'd_budget');
  const num = (id) => Number(d.blocks.find((b) => b.id === id).text.split(' ')[0].replace(/,/g, ''));
  assert.equal(num('C4') + num('C5') + num('C6') + num('C7') + num('C8') + num('C9'), num('C10'));
  assert.equal(num('D4') + num('D5') + num('D6') + num('D7') + num('D8') + num('D9'), num('D10'));
});

test('commit makes one revision per file, local until pushed, and clears the staged changes', () => {
  const db = seed();
  const staged = B.working(db, 'p_handbook', 'main').filter((w) => w.staged);
  const files = new Set(staged.map((w) => w.docId)).size;
  const before = B.mainSeq(db, 'd_budget');
  const out = B.commitStaged(db, 'p_handbook', 'main', 'Training budget and travel note', '');
  assert.equal(out.length, files);
  assert.ok(out.every((c) => c.local));
  assert.equal(B.mainSeq(db, 'd_budget'), before + 1);
  assert.equal(B.working(db, 'p_handbook', 'main').filter((w) => w.staged).length, 0);
  assert.ok(B.aheadOf(db, 'p_handbook', 'main') >= 1);
  assert.equal(B.push(db, 'p_handbook', 'main'), out.length);
  assert.equal(B.aheadOf(db, 'p_handbook', 'main'), 0);
  assert.ok(B.verify(db, 'd_budget').every((v) => v.ok));
  assert.throws(() => B.commitStaged(db, 'p_handbook', 'main', 'x', ''), /Nothing is staged/);
});

test('a request cannot be merged before the rules are met, then merges as a person’s action', () => {
  const db = seed();
  const r = db.requests.find((x) => x.id === 'cr14');
  let st = B.reviewState(db, r);
  assert.equal(st.required, 2);
  assert.equal(st.approvals, 1);
  assert.equal(st.canMerge, false);
  assert.throws(() => B.merge(db, r.id), /1 of 2/);
  B.review(db, r.id, 'approved', 'Fine.');
  st = B.reviewState(db, r);
  assert.equal(st.canMerge, true);
  const seqBefore = B.mainSeq(db, r.docId);
  const c = B.merge(db, r.id);
  assert.equal(c.kind, 'merge');
  assert.equal(c.seq, seqBefore + 1);
  assert.equal(r.status, 'merged');
  assert.match(db.docs.find((d) => d.id === r.docId).blocks.find((b) => b.id === 'p10').text, /16 weeks/);
});

test('an author cannot approve their own request', () => {
  const db = seed();
  db.me = 'u_priya';
  assert.throws(() => B.review(db, 'cr14', 'approved', ''), /own request/);
});

test('conflicts must be chosen before a merge; the choice decides the merged text', () => {
  const db = seed();
  const r = db.requests.find((x) => x.id === 'cr15');
  assert.deepEqual(r.conflicts.map((c) => c.blockId).sort(), ['D10', 'D7']);
  db.projects[0].rules.requiredApprovals = 0;
  assert.throws(() => B.merge(db, r.id), /conflict/);
  r.conflicts.forEach((c) => (c.choice = 'team'));
  B.merge(db, r.id);
  assert.equal(db.docs.find((d) => d.id === 'd_budget').blocks.find((b) => b.id === 'D7').text, '21,400');
});

test('rebase needs a choice for each conflict and then builds on the latest main', () => {
  const db = seed();
  const d = db.drafts.find((x) => x.id === 'dr_travel');
  assert.equal(B.behind(db, d), 1);
  assert.throws(() => B.rebase(db, d.id, {}), /Choose a version/);
  B.rebase(db, d.id, { D7: 'draft', D10: 'draft' });
  assert.equal(B.behind(db, d), 0);
  assert.equal(d.blocks.find((b) => b.id === 'D7').text, '26,800');
  assert.deepEqual(B.conflictsOf(db, d), []);
  assert.ok(B.verify(db, 'd_budget').every((v) => v.ok));
});

test('revert makes a recovery draft that undoes the revision; main is untouched', () => {
  const db = seed();
  const c = B.branchCommits(db, 'd_handbook', 'main').find((x) => x.summary === 'Fix heading case');
  const mainBefore = JSON.stringify(db.docs.find((d) => d.id === 'd_handbook').blocks);
  const d = B.recoveryDraft(db, c.id, 'revert');
  assert.equal(d.state, 'private');
  assert.equal(d.blocks.find((b) => b.id === 'p13').text, 'Documents And Review');
  assert.equal(JSON.stringify(db.docs.find((x) => x.id === 'd_handbook').blocks), mainBefore);
});

test('blocksAt rebuilds an older revision', () => {
  const db = seed();
  const r1 = B.blocksAt(db, 'd_handbook', 1);
  assert.equal(r1.find((b) => b.id === 'p4').text, 'Core hours are 10:00 to 16:00.');
  assert.equal(r1.some((b) => b.id === 'p7'), false);
  assert.equal(B.blocksAt(db, 'd_handbook', 0).length, 0);
});

test('withdraw closes the draft and its request, kept in history', () => {
  const db = seed();
  B.withdraw(db, 'dr_remote');
  assert.equal(db.drafts.find((d) => d.id === 'dr_remote').state, 'withdrawn');
  assert.ok(db.commits.some((c) => c.branch === 'dr_remote'));
  db.me = 'u_jordan';
  assert.throws(() => B.withdraw(db, 'dr_parental'), /author/);
});

test('the scripted agent goes through the access check: a read-only thread is refused the propose tools', () => {
  const db = seed();
  const all = TOOLS.map((t) => t.name);
  const p = plan(db, 'p_handbook', 'main', 'Draft a commit summary for my staged changes');
  const read = execute(p, 'read', all);
  assert.equal(read.calls[0].refused !== undefined, true);
  assert.match(p.finish(read.refused).text, /could not draft/);
  const propose = execute(p, 'propose', all);
  assert.equal(propose.refused.length, 0);
  assert.equal(p.finish(propose.refused).card.kind, 'summary');
});

test('asking the agent to merge calls a tool that does not exist and is refused', () => {
  const db = seed();
  const p = plan(db, 'p_handbook', 'main', 'Merge #14 for me');
  const out = execute(p, 'propose', TOOLS.map((t) => t.name));
  const merge = out.calls.find((c) => c.tool === 'requests.merge');
  assert.equal(merge.refused, 'there is no such tool');
  assert.equal(db.requests.find((r) => r.id === 'cr14').status, 'open');
});
