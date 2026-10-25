// The agent's tool-access rules: a closed table, two access levels, tools switched off in Settings,
// and no tool that does what only a person may do.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TOOLS, NEVER, PERSON_ONLY, ACCESS_ORDER, mayCall, accessRank } from '../src/lib/agentTools.ts';

const all = TOOLS.map((t) => t.name);

test('the table is closed: unique names, known access levels, an endpoint each', () => {
  assert.equal(new Set(all).size, all.length);
  for (const t of TOOLS) {
    assert.ok(ACCESS_ORDER.includes(t.access), t.name);
    assert.ok(t.endpoint.length > 0);
  }
  assert.deepEqual(all, [
    'history.read', 'history.verify', 'diff.get', 'documents.list', 'documents.outline', 'requests.list', 'requests.summarise', 'review.propose_comment', 'commit.draft_summary',
  ]);
});

test('no tool approves, merges, declines, withdraws, commits, pushes, or changes roles or rules', () => {
  for (const name of all) {
    const verb = name.split('.')[1];
    for (const word of PERSON_ONLY) assert.ok(!verb.startsWith(word), `${name} looks like ${word}`);
  }
  assert.ok(NEVER.some((n) => /merge/.test(n)));
  assert.ok(NEVER.some((n) => /key/.test(n)));
});

test('read only allows the read tools and refuses the propose tools, saying why', () => {
  for (const t of TOOLS) {
    const r = mayCall(t.name, 'read', all);
    assert.equal(r.ok, t.access === 'read', t.name);
    if (!r.ok) assert.match(r.reason, /needs Propose access/);
  }
});

test('propose allows every tool in the table', () => {
  for (const t of TOOLS) assert.equal(mayCall(t.name, 'propose', all).ok, true, t.name);
});

test('a tool switched off in Settings is refused at any access', () => {
  const enabled = all.filter((n) => n !== 'diff.get');
  for (const a of ACCESS_ORDER) {
    const r = mayCall('diff.get', a, enabled);
    assert.equal(r.ok, false);
    assert.match(r.reason, /switched off/);
  }
});

test('a name outside the table is refused, whatever the access or the enabled list says', () => {
  for (const name of ['requests.merge', 'review.approve', 'commit.push', 'drafts.withdraw', 'roles.set', 'keys.read', '']) {
    const r = mayCall(name, 'propose', [...all, name]);
    assert.equal(r.ok, false, name);
    assert.equal(r.reason, 'there is no such tool');
  }
});

test('access levels are ordered', () => {
  assert.ok(accessRank('read') < accessRank('propose'));
});
