// The Branch menu's availability: what Dynodoc supports is enabled only when it applies; what it
// does not have (rename, squash, stash) is never enabled.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { branchMenu, commitTarget, canPropose, canWriteMain } from '../src/lib/branchMenu.ts';

const by = (list) => Object.fromEntries(list.map((a) => [a.id, a]));

test('the menu has GitHub Desktop’s items in its order', () => {
  const ids = branchMenu({ kind: 'main', role: 'editor' }).map((a) => a.id);
  assert.deepEqual(ids, ['new', 'rename', 'delete', 'discard', 'stash', 'update', 'compare', 'merge', 'squash', 'rebase', 'request', 'view']);
});

test('rename, squash and stash are shown, never enabled, with a reason', () => {
  const contexts = [
    { kind: 'main', role: 'owner', uncommitted: 3 },
    { kind: 'draft', state: 'private', role: 'owner', behind: 2, ahead: 0 },
    { kind: 'draft', state: 'in_review', role: 'owner', requestSatisfied: true },
  ];
  for (const c of contexts) {
    const m = by(branchMenu(c));
    for (const id of ['rename', 'squash', 'stash']) {
      assert.equal(m[id].supported, false);
      assert.equal(m[id].enabled, false);
      assert.ok(m[id].note.length > 10);
    }
  }
});

test('on main: new draft yes; withdraw, update, compare, rebase, merge, request no', () => {
  const m = by(branchMenu({ kind: 'main', role: 'editor', uncommitted: 0 }));
  assert.equal(m.new.enabled, true);
  for (const id of ['delete', 'update', 'compare', 'rebase', 'merge', 'request', 'view', 'discard']) assert.equal(m[id].enabled, false, id);
});

test('discard needs captured changes', () => {
  assert.equal(by(branchMenu({ kind: 'main', role: 'editor', uncommitted: 2 })).discard.enabled, true);
  assert.equal(by(branchMenu({ kind: 'main', role: 'editor', uncommitted: 0 })).discard.enabled, false);
});

test('a viewer cannot start a draft', () => {
  const m = by(branchMenu({ kind: 'main', role: 'viewer' }));
  assert.equal(m.new.enabled, false);
  assert.match(m.new.note, /role/);
  assert.equal(canPropose('viewer'), false);
  assert.equal(canPropose('reviewer'), true);
});

test('a private draft of mine: withdraw, compare; update and rebase only when behind; send for review only when pushed', () => {
  const behind = by(branchMenu({ kind: 'draft', state: 'private', role: 'editor', behind: 2, ahead: 0, mine: true }));
  assert.equal(behind.delete.enabled, true);
  assert.equal(behind.compare.enabled, true);
  assert.equal(behind.update.enabled, true);
  assert.equal(behind.rebase.enabled, true);
  assert.equal(behind.request.enabled, true);
  assert.equal(behind.merge.enabled, false);
  assert.match(behind.merge.note, /review first/);
  const current = by(branchMenu({ kind: 'draft', state: 'private', role: 'editor', behind: 0, ahead: 1, mine: true }));
  assert.equal(current.update.enabled, false);
  assert.equal(current.rebase.enabled, false);
  assert.equal(current.request.enabled, false);
  assert.match(current.request.note, /Push/);
});

test('in review: merge only when the rules are met; view the request', () => {
  const waiting = by(branchMenu({ kind: 'draft', state: 'in_review', role: 'manager', requestSatisfied: false }));
  assert.equal(waiting.merge.enabled, false);
  assert.match(waiting.merge.note, /rules/);
  assert.equal(waiting.view.enabled, true);
  assert.equal(waiting.request.enabled, false);
  const ready = by(branchMenu({ kind: 'draft', state: 'in_review', role: 'manager', requestSatisfied: true }));
  assert.equal(ready.merge.enabled, true);
});

test('someone else’s draft cannot be withdrawn, updated or rebased by me', () => {
  const m = by(branchMenu({ kind: 'draft', state: 'in_review', role: 'owner', behind: 3, mine: false }));
  assert.equal(m.delete.enabled, false);
  assert.equal(m.update.enabled, false);
  assert.equal(m.rebase.enabled, false);
  assert.equal(m.compare.enabled, true);
});

test('closed drafts offer nothing that changes them', () => {
  for (const state of ['merged', 'withdrawn']) {
    const m = by(branchMenu({ kind: 'draft', state, role: 'owner', behind: 4, ahead: 0 }));
    for (const id of ['delete', 'update', 'rebase', 'request', 'merge']) assert.equal(m[id].enabled, false, `${state} ${id}`);
  }
});

test('commit target: main is writable by editors only when not protected; drafts by their author', () => {
  assert.equal(commitTarget({ kind: 'main', role: 'editor', protectedMain: false }).ok, true);
  assert.equal(commitTarget({ kind: 'main', role: 'editor', protectedMain: true }).ok, false);
  assert.equal(commitTarget({ kind: 'main', role: 'approver', protectedMain: false }).ok, false);
  assert.equal(commitTarget({ kind: 'draft', state: 'private', role: 'reviewer', mine: true }).ok, true);
  assert.equal(commitTarget({ kind: 'draft', state: 'private', role: 'editor', mine: false }).ok, false);
  assert.equal(commitTarget({ kind: 'draft', state: 'merged', role: 'editor', mine: true }).ok, false);
  assert.equal(canWriteMain('owner', true), false);
});
