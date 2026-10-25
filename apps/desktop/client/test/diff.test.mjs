// The diff model: word diffs, applying changes, hunks with context, unified and split rows.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { tokenize, wordDiff, sideText, applyEdits, rows, hunks, unified, split, stats, compareBlocks } from '../src/lib/diff.ts';

const base = [
  { id: 'p1', label: '¶ 1', text: 'Welcome to the team' },
  { id: 'p2', label: '¶ 2', text: 'Core hours are 10:00 to 16:00.' },
  { id: 'p3', label: '¶ 3', text: 'Remote work' },
  { id: 'p4', label: '¶ 4', text: 'Two days a week.' },
  { id: 'p5', label: '¶ 5', text: 'Leave' },
  { id: 'p6', label: '¶ 6', text: '22 days.' },
  { id: 'p7', label: '¶ 7', text: 'Expenses' },
];

test('tokenize keeps words, spaces and marks, and joins back to the text', () => {
  const text = 'Core hours: 10:00 to 15:00, café — বাংলা ok.';
  const t = tokenize(text);
  assert.equal(t.join(''), text);
  assert.ok(t.includes('café'));
  assert.ok(t.includes('বাংলা'));
});

test('wordDiff marks only the changed words, and each side rebuilds its text', () => {
  const a = 'Core hours are 10:00 to 16:00.';
  const b = 'Core hours are 10:00 to 15:00 in your time zone.';
  const d = wordDiff(a, b);
  assert.equal(sideText(d, 'before'), a);
  assert.equal(sideText(d, 'after'), b);
  assert.deepEqual(d.filter((g) => g.t === 'del').map((g) => g.s.trim()), ['16']);
  assert.ok(d.some((g) => g.t === 'add' && g.s.includes('in your time zone')));
  assert.deepEqual(wordDiff('same', 'same'), [{ t: 'same', s: 'same' }]);
  assert.deepEqual(wordDiff('', ''), []);
  assert.deepEqual(wordDiff('', 'new'), [{ t: 'add', s: 'new' }]);
});

test('wordDiff on cell values', () => {
  const d = wordDiff('9,000', '12,000');
  assert.equal(sideText(d, 'before'), '9,000');
  assert.equal(sideText(d, 'after'), '12,000');
  assert.ok(d.some((g) => g.t === 'del') && d.some((g) => g.t === 'add'));
});

test('a very long pair falls back to remove-all, add-all', () => {
  const a = 'x '.repeat(800);
  const b = 'y '.repeat(800);
  const d = wordDiff(a, b);
  assert.deepEqual(d.map((g) => g.t), ['del', 'add']);
});

test('applyEdits edits, removes and inserts in place, including chains of added blocks', () => {
  const out = applyEdits(base, [
    { blockId: 'p2', op: 'edit', before: base[1].text, after: 'Core hours are 10:00 to 15:00.' },
    { blockId: 'p6', op: 'remove', before: '22 days.' },
    { blockId: 'n1', op: 'add', afterId: 'p4', label: '¶ 4a', after: 'Up to four weeks abroad.' },
    { blockId: 'n2', op: 'add', afterId: 'n1', label: '¶ 4b', after: 'Tell your manager.' },
    { blockId: 'n0', op: 'add', afterId: '', label: '¶ 0', after: 'Title' },
    { blockId: 'zz', op: 'edit', before: 'x', after: 'y' },
  ]);
  assert.deepEqual(out.map((b) => b.id), ['n0', 'p1', 'p2', 'p3', 'p4', 'n1', 'n2', 'p5', 'p7']);
  assert.equal(out[2].text, 'Core hours are 10:00 to 15:00.');
  assert.equal(base[1].text, 'Core hours are 10:00 to 16:00.', 'the base is not changed');
});

test('an add whose anchor is missing goes to the end; a self-anchored add does not loop', () => {
  const out = applyEdits(base.slice(0, 2), [
    { blockId: 'n1', op: 'add', afterId: 'gone', after: 'A' },
    { blockId: 'n2', op: 'add', afterId: 'n2', after: 'B' },
  ]);
  assert.equal(out[out.length - 1].id, 'n1');
  assert.ok(out.length <= 4);
});

test('rows number the base and the result like line numbers', () => {
  const r = rows(base.slice(0, 3), [
    { blockId: 'p1', op: 'remove', before: base[0].text },
    { blockId: 'x', op: 'add', afterId: 'p2', after: 'New' },
  ]);
  assert.deepEqual(r.map((x) => [x.kind, x.oldNo, x.newNo]), [
    ['del', 1, undefined],
    ['context', 2, 1],
    ['add', undefined, 2],
    ['context', 3, 3],
  ]);
});

test('hunks keep `context` unchanged blocks around each change and join near ones', () => {
  const edits = [
    { blockId: 'p2', op: 'edit', before: base[1].text, after: 'Core hours are 10:00 to 15:00.' },
    { blockId: 'p6', op: 'edit', before: '22 days.', after: '25 days.' },
  ];
  const none = hunks(base, edits, 0);
  assert.equal(none.length, 2);
  assert.deepEqual(none.map((h) => h.rows.map((r) => r.blockId)), [['p2'], ['p6']]);
  assert.equal(none[0].header, '@@ ¶ 2 @@');
  const one = hunks(base, edits, 1);
  assert.deepEqual(one.map((h) => h.rows.map((r) => r.blockId)), [['p1', 'p2', 'p3'], ['p5', 'p6', 'p7']]);
  assert.equal(one[0].header, '@@ ¶ 1 – ¶ 3 @@');
  const two = hunks(base, edits, 2);
  assert.equal(two.length, 1, 'overlapping context joins the hunks');
  assert.equal(hunks(base, edits, Infinity)[0].rows.length, base.length);
  assert.equal(hunks(base, edits, -5).length, 2, 'negative context is 0');
  assert.deepEqual(hunks(base, [], 3), []);
});

test('unified: an edit is a - line then a + line, each with only its side of the words', () => {
  const [h] = hunks(base, [{ blockId: 'p6', op: 'edit', before: '22 days.', after: '25 days.' }], 1);
  const lines = unified(h);
  assert.deepEqual(lines.map((l) => l.sign), [' ', '-', '+', ' ']);
  const minus = lines[1], plus = lines[2];
  assert.ok(minus.segs.every((g) => g.t !== 'add'));
  assert.ok(plus.segs.every((g) => g.t !== 'del'));
  assert.equal(minus.segs.map((g) => g.s).join(''), '22 days.');
  assert.equal(plus.segs.map((g) => g.s).join(''), '25 days.');
  assert.equal(minus.oldNo, 6);
  assert.equal(plus.newNo, 6);
});

test('split: one row per block, left is the base and right the result', () => {
  const hs = hunks(base.slice(0, 3), [
    { blockId: 'p1', op: 'remove', before: base[0].text },
    { blockId: 'p2', op: 'edit', before: base[1].text, after: 'Core hours are flexible.' },
    { blockId: 'x', op: 'add', afterId: 'p3', label: '¶ x', after: 'Added' },
  ], 0);
  assert.equal(hs.length, 2, 'the unchanged block between them splits the hunks');
  const s = hs.flatMap(split);
  assert.deepEqual(s.map((l) => [l.kind, Boolean(l.left), Boolean(l.right)]), [
    ['del', true, false],
    ['edit', true, true],
    ['add', false, true],
  ]);
  assert.ok(s[1].left.segs.some((g) => g.t === 'del'));
  assert.ok(s[1].right.segs.some((g) => g.t === 'add'));
});

test('stats count blocks and words', () => {
  const s = stats([
    { blockId: 'a', op: 'edit', before: 'one two three', after: 'one four three five' },
    { blockId: 'b', op: 'add', after: 'six seven' },
    { blockId: 'c', op: 'remove', before: 'eight' },
  ]);
  assert.deepEqual(s, { edited: 1, added: 1, removed: 1, wordsAdded: 4, wordsRemoved: 2 });
});

test('compareBlocks finds the edits from one version to another, and applying them gives the other', () => {
  const to = [
    { id: 'p1', label: '¶ 1', text: 'Welcome to the team' },
    { id: 'n1', label: '¶ 1a', text: 'Read this first.' },
    { id: 'p2', label: '¶ 2', text: 'Core hours are flexible.' },
    { id: 'p4', label: '¶ 4', text: 'Two days a week.' },
    { id: 'p5', label: '¶ 5', text: 'Leave' },
    { id: 'p6', label: '¶ 6', text: '22 days.' },
    { id: 'p7', label: '¶ 7', text: 'Expenses' },
  ];
  const e = compareBlocks(base, to);
  assert.deepEqual(e.map((x) => [x.op, x.blockId]).sort(), [['add', 'n1'], ['edit', 'p2'], ['remove', 'p3']]);
  assert.deepEqual(applyEdits(base, e).map((b) => [b.id, b.text]), to.map((b) => [b.id, b.text]));
});
