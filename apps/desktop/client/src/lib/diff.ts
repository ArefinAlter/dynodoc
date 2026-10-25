// The diff model for documents, spreadsheets and presentations. Dynodoc compares structured
// blocks, not lines of a file: a paragraph of a document, a cell of a sheet, or a text frame of a
// slide. A change edits, adds or removes one block. This module turns a base list of blocks and a
// list of changes into hunks with context (Git GUI's "context lines"), then into rows for a
// unified or a side-by-side view, with a word-level diff inside an edited block. Pure functions,
// no DOM; tested with `node --test` (client/test/diff.test.mjs).

export interface Block {
  id: string;
  /** What a person calls the block: "¶ 4", "Budget!C7", "Slide 3 · Title". */
  label: string;
  text: string;
}

export type EditOp = 'edit' | 'add' | 'remove';

export interface Edit {
  blockId: string;
  op: EditOp;
  /** For edit and remove: the text before. */
  before?: string;
  /** For edit and add: the text after. */
  after?: string;
  /** For add: the block the new one follows ("" or absent: at the start). */
  afterId?: string;
  /** For add: the new block's label. */
  label?: string;
}

export type SegKind = 'same' | 'add' | 'del';
export interface Seg {
  t: SegKind;
  s: string;
}

/** Words, runs of spaces and single punctuation marks, so a diff reads as words change. */
export function tokenize(text: string): string[] {
  const out: string[] = [];
  const re = /(\s+|[A-Za-z0-9À-ɏঀ-৿'’]+|[^\sA-Za-z0-9À-ɏঀ-৿'’])/gu;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) out.push(m[0]);
  return out;
}

function pushSeg(out: Seg[], t: SegKind, s: string): void {
  if (!s) return;
  const last = out[out.length - 1];
  if (last && last.t === t) last.s += s;
  else out.push({ t, s });
}

/** Word-level difference of two texts (longest common subsequence of tokens). Deletions come
 *  before insertions at the same place. Very long texts fall back to "all removed, all added". */
export function wordDiff(a: string, b: string): Seg[] {
  if (a === b) return a ? [{ t: 'same', s: a }] : [];
  const x = tokenize(a);
  const y = tokenize(b);
  const out: Seg[] = [];
  if (x.length * y.length > 250_000) {
    pushSeg(out, 'del', a);
    pushSeg(out, 'add', b);
    return out;
  }
  const n = x.length, m = y.length;
  // L[i][j] = LCS length of x[i..] and y[j..], in one flat array.
  const L = new Uint32Array((n + 1) * (m + 1));
  const at = (i: number, j: number) => i * (m + 1) + j;
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      L[at(i, j)] = x[i] === y[j] ? L[at(i + 1, j + 1)] + 1 : Math.max(L[at(i + 1, j)], L[at(i, j + 1)]);
    }
  }
  let i = 0, j = 0;
  let del = '', add = '';
  const flush = () => {
    pushSeg(out, 'del', del);
    pushSeg(out, 'add', add);
    del = '';
    add = '';
  };
  while (i < n && j < m) {
    if (x[i] === y[j]) {
      flush();
      pushSeg(out, 'same', x[i]);
      i++;
      j++;
    } else if (L[at(i + 1, j)] >= L[at(i, j + 1)]) {
      del += x[i++];
    } else {
      add += y[j++];
    }
  }
  while (i < n) del += x[i++];
  while (j < m) add += y[j++];
  flush();
  return out;
}

/** The text of one side of a word diff. */
export function sideText(segs: Seg[], side: 'before' | 'after'): string {
  return segs.filter((g) => g.t === 'same' || g.t === (side === 'before' ? 'del' : 'add')).map((g) => g.s).join('');
}

/** The blocks after the changes. Unknown block ids and adds after a missing block are ignored
 *  (an add whose anchor is gone goes to the end), never thrown. */
export function applyEdits(base: Block[], edits: Edit[]): Block[] {
  const byId = new Map(edits.filter((e) => e.op !== 'add').map((e) => [e.blockId, e]));
  const adds = edits.filter((e) => e.op === 'add');
  const known = new Set([...base.map((b) => b.id), ...adds.map((e) => e.blockId)]);
  const out: Block[] = [];
  const placed = new Set<string>();
  const addAfter = (anchor: string): void => {
    for (const e of adds) {
      if ((e.afterId ?? '') !== anchor || placed.has(e.blockId)) continue;
      placed.add(e.blockId);
      out.push({ id: e.blockId, label: e.label ?? e.blockId, text: e.after ?? '' });
      addAfter(e.blockId);
    }
  };
  addAfter('');
  for (const b of base) {
    const e = byId.get(b.id);
    if (!e) out.push(b);
    else if (e.op === 'edit') out.push({ ...b, text: e.after ?? '' });
    addAfter(b.id);
  }
  for (const e of adds) if (e.afterId && !known.has(e.afterId) && !placed.has(e.blockId)) out.push({ id: e.blockId, label: e.label ?? e.blockId, text: e.after ?? '' });
  return out;
}

export type RowKind = 'context' | 'edit' | 'add' | 'del';

export interface Row {
  kind: RowKind;
  blockId: string;
  label: string;
  /** Position in the base (1-based); absent for an added block. */
  oldNo?: number;
  /** Position in the result (1-based); absent for a removed block. */
  newNo?: number;
  before?: string;
  after?: string;
  /** For an edit: the word diff. */
  segs?: Seg[];
}

export interface Hunk {
  /** "@@ ¶ 3 – ¶ 7 @@" style header: the first and last block in the hunk. */
  header: string;
  rows: Row[];
}

/** Every block in order, with what happened to it. */
export function rows(base: Block[], edits: Edit[]): Row[] {
  const byId = new Map(edits.filter((e) => e.op !== 'add').map((e) => [e.blockId, e]));
  const adds = edits.filter((e) => e.op === 'add');
  const known = new Set([...base.map((b) => b.id), ...adds.map((e) => e.blockId)]);
  const out: Row[] = [];
  let oldNo = 0, newNo = 0;
  const placed = new Set<string>();
  const addAfter = (anchor: string): void => {
    for (const e of adds) {
      if ((e.afterId ?? '') !== anchor || placed.has(e.blockId)) continue;
      placed.add(e.blockId);
      out.push({ kind: 'add', blockId: e.blockId, label: e.label ?? e.blockId, newNo: ++newNo, after: e.after ?? '' });
      addAfter(e.blockId);
    }
  };
  addAfter('');
  for (const b of base) {
    const e = byId.get(b.id);
    oldNo++;
    if (!e) out.push({ kind: 'context', blockId: b.id, label: b.label, oldNo, newNo: ++newNo, before: b.text, after: b.text });
    else if (e.op === 'remove') out.push({ kind: 'del', blockId: b.id, label: b.label, oldNo, before: b.text });
    else {
      const after = e.after ?? '';
      out.push({ kind: 'edit', blockId: b.id, label: b.label, oldNo, newNo: ++newNo, before: b.text, after, segs: wordDiff(b.text, after) });
    }
    addAfter(b.id);
  }
  for (const e of adds) {
    if (e.afterId && !known.has(e.afterId) && !placed.has(e.blockId)) out.push({ kind: 'add', blockId: e.blockId, label: e.label ?? e.blockId, newNo: ++newNo, after: e.after ?? '' });
  }
  return out;
}

/** Hunks: changed rows with up to `context` unchanged blocks around them; hunks that touch or
 *  overlap are joined. A negative context is treated as 0; Infinity shows the whole file. */
export function hunks(base: Block[], edits: Edit[], context = 3): Hunk[] {
  const all = rows(base, edits);
  const ctx = Math.max(0, context);
  const keep = new Array<boolean>(all.length).fill(false);
  all.forEach((r, i) => {
    if (r.kind === 'context') return;
    const lo = Math.max(0, i - ctx);
    const hi = Number.isFinite(ctx) ? Math.min(all.length - 1, i + ctx) : all.length - 1;
    for (let k = Number.isFinite(ctx) ? lo : 0; k <= hi; k++) keep[k] = true;
  });
  const out: Hunk[] = [];
  let current: Row[] = [];
  const close = () => {
    if (!current.length) return;
    const first = current[0].label, last = current[current.length - 1].label;
    out.push({ header: first === last ? `@@ ${first} @@` : `@@ ${first} – ${last} @@`, rows: current });
    current = [];
  };
  all.forEach((r, i) => {
    if (keep[i]) current.push(r);
    else close();
  });
  close();
  return out;
}

export interface Line {
  sign: ' ' | '-' | '+';
  label: string;
  segs: Seg[];
  oldNo?: number;
  newNo?: number;
  blockId: string;
}

/** Unified view: an edit is a "-" line then a "+" line, each with its own word marks. */
export function unified(hunk: Hunk): Line[] {
  const out: Line[] = [];
  for (const r of hunk.rows) {
    if (r.kind === 'context') out.push({ sign: ' ', label: r.label, segs: [{ t: 'same', s: r.before ?? '' }], oldNo: r.oldNo, newNo: r.newNo, blockId: r.blockId });
    else if (r.kind === 'del') out.push({ sign: '-', label: r.label, segs: [{ t: 'del', s: r.before ?? '' }], oldNo: r.oldNo, blockId: r.blockId });
    else if (r.kind === 'add') out.push({ sign: '+', label: r.label, segs: [{ t: 'add', s: r.after ?? '' }], newNo: r.newNo, blockId: r.blockId });
    else {
      const segs = r.segs ?? [];
      out.push({ sign: '-', label: r.label, segs: segs.filter((g) => g.t !== 'add'), oldNo: r.oldNo, blockId: r.blockId });
      out.push({ sign: '+', label: r.label, segs: segs.filter((g) => g.t !== 'del'), newNo: r.newNo, blockId: r.blockId });
    }
  }
  return out;
}

export interface SplitSide {
  label: string;
  no: number;
  segs: Seg[];
}
export interface SplitLine {
  kind: RowKind;
  blockId: string;
  left?: SplitSide;
  right?: SplitSide;
}

/** Side-by-side view: the base at the left, the result at the right, one row per block. */
export function split(hunk: Hunk): SplitLine[] {
  return hunk.rows.map((r) => {
    const segs = r.segs ?? [];
    const left: SplitSide | undefined =
      r.oldNo === undefined ? undefined : { label: r.label, no: r.oldNo, segs: r.kind === 'edit' ? segs.filter((g) => g.t !== 'add') : [{ t: r.kind === 'del' ? 'del' : 'same', s: r.before ?? '' }] };
    const right: SplitSide | undefined =
      r.newNo === undefined ? undefined : { label: r.label, no: r.newNo, segs: r.kind === 'edit' ? segs.filter((g) => g.t !== 'del') : [{ t: r.kind === 'add' ? 'add' : 'same', s: r.after ?? '' }] };
    return { kind: r.kind, blockId: r.blockId, left, right };
  });
}

export interface Stats {
  edited: number;
  added: number;
  removed: number;
  /** Words added and removed inside edits and whole blocks. */
  wordsAdded: number;
  wordsRemoved: number;
}

function words(text: string): number {
  return tokenize(text).filter((t) => /\S/.test(t) && /[\p{L}\p{N}]/u.test(t)).length;
}

export function stats(edits: Edit[]): Stats {
  const out: Stats = { edited: 0, added: 0, removed: 0, wordsAdded: 0, wordsRemoved: 0 };
  for (const e of edits) {
    if (e.op === 'add') {
      out.added++;
      out.wordsAdded += words(e.after ?? '');
    } else if (e.op === 'remove') {
      out.removed++;
      out.wordsRemoved += words(e.before ?? '');
    } else {
      out.edited++;
      for (const g of wordDiff(e.before ?? '', e.after ?? '')) {
        if (g.t === 'add') out.wordsAdded += words(g.s);
        if (g.t === 'del') out.wordsRemoved += words(g.s);
      }
    }
  }
  return out;
}

/** Two versions of the same blocks compared (a branch against main, a version against another):
 *  the edits that turn `from` into `to`. Blocks are matched by id. */
export function compareBlocks(from: Block[], to: Block[]): Edit[] {
  const toById = new Map(to.map((b) => [b.id, b]));
  const fromIds = new Set(from.map((b) => b.id));
  const edits: Edit[] = [];
  let before = '';
  for (const b of from) {
    const t = toById.get(b.id);
    // A removal remembers where the block stood and its label, so it can be undone in place.
    const anchor = before;
    before = b.id;
    if (!t) edits.push({ blockId: b.id, op: 'remove', before: b.text, afterId: anchor, label: b.label });
    else if (t.text !== b.text) edits.push({ blockId: b.id, op: 'edit', before: b.text, after: t.text });
  }
  let previous = '';
  for (const t of to) {
    if (!fromIds.has(t.id)) edits.push({ blockId: t.id, op: 'add', after: t.text, afterId: previous, label: t.label });
    // An added block follows the block before it in `to`, whether that one is old or new.
    previous = t.id;
  }
  return edits;
}
