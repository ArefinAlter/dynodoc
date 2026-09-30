/**
 * Push a locally edited file back into a document, like `git push` from a laptop.
 *
 * The file's paragraphs are lined up with the stable block IDs of the revision it
 * started from: hidden bookmarks written on download identify blocks exactly; files
 * without them (an original upload edited by someone else) are aligned by content.
 * The result is ordinary draft changes, reviewed and merged like any other draft.
 */
import { DOMParser } from '@xmldom/xmldom';
import { ulid } from 'ulid';
import type { DocumentState, EventPayload, MaterializedNode } from '$lib/api/types';
import { byPosition } from './api';
import { readOfficeZip, xmlText } from './bounded-zip';
import { between, cloneState, stateChanges } from './changes';
import type { DocxImport } from './docx-import';
import { textOf, type TextBlock } from './files';
import { originPattern } from './office';

export interface FileOrigin {
  documentId?: string;
  seq?: number;
  exportedAt?: string;
}
/**
 * Read the document and revision Dynodoc recorded in a downloaded file: custom
 * properties in Word files, a marker in the Subject of Excel and PowerPoint files.
 */
export function readOrigin(bytes: Uint8Array): FileOrigin {
  try {
    const parts = readOfficeZip(
      bytes,
      (name) => name === 'docProps/custom.xml' || name === 'docProps/core.xml'
    );
    const elements = (name: string) => {
      const source = xmlText(parts.get(name));
      if (!source) return [];
      const root = new DOMParser().parseFromString(source, 'application/xml').documentElement;
      return Array.from(root?.getElementsByTagName('*') || []);
    };
    const origin: FileOrigin = {};
    for (const property of elements('docProps/custom.xml').filter(
      (e) => e.localName === 'property'
    )) {
      const value = (property.textContent || '').trim().slice(0, 100);
      const name = property.getAttribute('name');
      if (name === 'DynodocDocument' && /^[0-9a-f-]{36}$/i.test(value)) origin.documentId = value;
      if (name === 'DynodocRevision' && /^\d{1,12}$/.test(value)) origin.seq = Number(value);
      if (name === 'DynodocExported') origin.exportedAt = value;
    }
    if (origin.documentId || origin.seq !== undefined) return origin;
    for (const e of elements('docProps/core.xml')) {
      if (!['subject', 'keywords', 'description'].includes(e.localName || '')) continue;
      const match = originPattern.exec((e.textContent || '').slice(0, 300));
      if (match)
        return {
          documentId: match[1],
          seq: Number(match[2]),
          ...(match[3] ? { exportedAt: match[3] } : {})
        };
    }
    return origin;
  } catch {
    return {};
  }
}

/** Downloads are named `Title (v42).docx`: v42 is the revision the copy started from. */
export function downloadName(title: string, seq: number, extension = 'docx') {
  const printable = [...title].filter((c) => c.charCodeAt(0) >= 32).join('');
  const safe = printable.replace(/[\\/:*?"<>|]+/g, ' ').trim() || 'Document';
  return `${safe.slice(0, 150)} (v${seq}).${extension}`;
}
/** Revision from the naming convention, tolerating browser duplicate suffixes like " (1)". */
export function revisionFromName(filename: string): number | undefined {
  let name = filename.replace(/\.[^.]+$/, '').trim();
  name = name.replace(/\s*\(\d{1,3}\)$/, '').replace(/\s*-\s*copy$/i, '');
  const match = /(?:^|[\s_\-([])v(?:ersion)?[\s_-]?(\d{1,9})\)?\]?$/i.exec(name);
  return match ? Number(match[1]) : undefined;
}
/** Title with the revision suffix removed, used to suggest the matching document. */
export function titleFromName(filename: string) {
  return filename
    .replace(/\.[^.]+$/, '')
    .replace(/\s*\(\d{1,3}\)$/, '')
    .replace(/\s*[([]?v(?:ersion)?[\s_-]?\d{1,9}[)\]]?$/i, '')
    .replace(/[_]+/g, ' ')
    .trim();
}

type Defaults = NonNullable<DocxImport['defaults']>;
const PARAGRAPH_KEYS = [
  'textAlign',
  'dir',
  'keepWithNext',
  'pageBreakBefore',
  'indent',
  'firstLine',
  'spaceBefore',
  'spaceAfter',
  'lineHeight',
  'level'
];
function neutral(key: string, value: unknown) {
  if (value === null || value === undefined || value === '' || value === false) return true;
  if (['indent', 'firstLine', 'spaceBefore', 'spaceAfter'].includes(key)) return !Number(value);
  if (key === 'textAlign') return value === 'left' || value === 'start';
  if (key === 'dir') return value === 'ltr' || value === 'auto';
  if (key === 'start') return Number(value) === 1;
  if (key === 'colspan' || key === 'rowspan') return Number(value) === 1;
  if (key === 'verticalAlign') return value === 'top';
  return false;
}
const same = (a: unknown, b: unknown) => String(a) === String(b);

/**
 * Canonical comparison form. Word re-saves split runs, add inherited defaults and
 * renumber comments or notes; none of that is a change the author made.
 */
function canonicalNode(block: TextBlock, defaults?: Defaults): unknown {
  let type = block.type;
  if (type === 'taskList') type = 'bulletList';
  if (type === 'taskItem') type = 'listItem';
  if (type === 'tableOfContents') return { type };
  if (type === 'text') {
    const marks = (block.marks || [])
      .map((m) => {
        const attrs: Record<string, unknown> = {};
        for (const [k, v] of Object.entries(m.attrs || {})) {
          if (v === null || v === undefined || v === '') continue;
          if (m.type === 'textStyle' && defaults && same(defaults.run[k], v)) continue;
          if (['trackedChange', 'sourceComment'].includes(m.type) && (k === 'id' || k === 'date'))
            continue;
          attrs[k] = v;
        }
        return { type: m.type, attrs };
      })
      .filter((m) => m.type !== 'textStyle' || Object.keys(m.attrs).length)
      .sort((a, b) => a.type.localeCompare(b.type));
    return { type, text: block.text || '', marks };
  }
  const attrs: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(block.attrs || {})) {
    if (k === 'blockId' || k === 'id' || k === 'checked' || neutral(k, v)) continue;
    if (type === 'image' && (k === 'width' || k === 'height')) continue;
    if (PARAGRAPH_KEYS.includes(k) && defaults && same(defaults.para[k], v)) continue;
    attrs[k] = v;
  }
  const content: unknown[] = [];
  for (const child of block.content || []) {
    const next = canonicalNode(child, defaults) as { type: string; text?: string; marks?: unknown };
    const last = content.at(-1) as typeof next | undefined;
    // Merge adjacent runs with identical formatting.
    if (
      next.type === 'text' &&
      last?.type === 'text' &&
      JSON.stringify(last.marks) === JSON.stringify(next.marks)
    )
      last.text += next.text || '';
    else if (next.type !== 'text' || next.text) content.push(next);
  }
  return { type, attrs, content };
}
export function canonical(blocks: TextBlock | TextBlock[], defaults?: Defaults) {
  const list = Array.isArray(blocks) ? blocks : [blocks];
  return JSON.stringify(list.map((b) => canonicalNode(b, defaults)));
}

// Myers shortest edit script over arrays of keys; returns matched index pairs.
function myers(a: string[], b: string[], maxD = 4000): [number, number][] | null {
  const n = a.length,
    m = b.length,
    max = n + m;
  if (!max) return [];
  const offset = max + 1;
  let v = new Int32Array(2 * max + 3);
  const trace: Int32Array[] = [];
  for (let d = 0; d <= Math.min(max, maxD); d++) {
    trace.push(v.slice(offset - d - 1, offset + d + 2));
    for (let k = -d; k <= d; k += 2) {
      let x =
        k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1])
          ? v[offset + k + 1]
          : v[offset + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) {
        x++;
        y++;
      }
      v[offset + k] = x;
      if (x >= n && y >= m) {
        const pairs: [number, number][] = [];
        let cx = n,
          cy = m;
        for (let dd = d; dd > 0; dd--) {
          const prev = trace[dd],
            at = (kk: number) => prev[kk + dd + 1];
          const kk = cx - cy;
          const down = kk === -dd || (kk !== dd && at(kk - 1) < at(kk + 1));
          const pk = down ? kk + 1 : kk - 1;
          const px = at(pk),
            py = px - pk;
          while (cx > px + (down ? 0 : 1) && cy > py + (down ? 1 : 0)) {
            cx--;
            cy--;
            pairs.push([cx, cy]);
          }
          cx = px;
          cy = py;
        }
        while (cx > 0 && cy > 0) {
          cx--;
          cy--;
          pairs.push([cx, cy]);
        }
        return pairs.reverse();
      }
    }
    v = v.slice();
  }
  return null;
}
/** Longest increasing subsequence of values, returning the kept indices. */
export function increasing(values: number[]): Set<number> {
  const tails: number[] = [],
    links = new Array<number>(values.length);
  values.forEach((value, i) => {
    let lo = 0,
      hi = tails.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (values[tails[mid]] < value) lo = mid + 1;
      else hi = mid;
    }
    links[i] = lo ? tails[lo - 1] : -1;
    tails[lo] = i;
  });
  const kept = new Set<number>();
  for (let i = tails.at(-1) ?? -1; i >= 0; i = links[i]) kept.add(i);
  return kept;
}
/** Match equal keys: Myers for similar sequences, unique keys when very different. */
export function matchEqual(a: string[], b: string[]): [number, number][] {
  const exact = myers(a, b);
  if (exact) return exact;
  const count = (list: string[]) => {
    const seen = new Map<string, number[]>();
    list.forEach((key, i) => seen.set(key, [...(seen.get(key) || []), i]));
    return seen;
  };
  const left = count(a),
    right = count(b);
  const pairs = [...left.entries()]
    .filter(([key, at]) => at.length === 1 && right.get(key)?.length === 1)
    .map(([key, at]) => [at[0], right.get(key)![0]] as [number, number])
    .sort((x, y) => x[1] - y[1]);
  const kept = increasing(pairs.map((p) => p[0]));
  return pairs.filter((_, i) => kept.has(i));
}
function words(text: string) {
  return text.toLowerCase().match(/[\p{L}\p{N}]+/gu) || [];
}
function dice(x: string[], y: string[]) {
  if (!x.length || !y.length) return 0;
  const counts = new Map<string, number>();
  for (const w of x) counts.set(w, (counts.get(w) || 0) + 1);
  let common = 0;
  for (const w of y) {
    const c = counts.get(w) || 0;
    if (c) {
      common++;
      counts.set(w, c - 1);
    }
  }
  return (2 * common) / (x.length + y.length);
}
function bigrams(text: string) {
  const s = [...text.toLowerCase().replace(/\s+/g, ' ').trim()];
  return s.slice(1).map((c, i) => s[i] + c);
}
/** `dice` for token lists sorted in advance: a merge walk, without allocating. */
function diceSorted(x: string[], y: string[]) {
  if (!x.length || !y.length) return 0;
  let i = 0,
    j = 0,
    common = 0;
  while (i < x.length && j < y.length)
    if (x[i] === y[j]) {
      common++;
      i++;
      j++;
    } else if (x[i] < y[j]) i++;
    else j++;
  return (2 * common) / (x.length + y.length);
}
function compare(
  a: string,
  b: string,
  split: (text: string, pairs: boolean) => string[],
  measure = dice
) {
  if (!a.trim() && !b.trim()) return 1;
  const byWords = measure(split(a, false), split(b, false));
  if (a.length + b.length > 4000) return byWords;
  return Math.max(byWords, measure(split(a, true), split(b, true)));
}
/**
 * How alike two paragraphs are (0–1), used to recognise an edited paragraph. Word
 * overlap handles rewording; character pairs handle small edits such as "change" →
 * "changed" in short sentences, where one word is a large share of the text.
 */
export function similarity(a: string, b: string) {
  return compare(a, b, (text, pairs) => (pairs ? bigrams(text) : words(text)));
}
/**
 * `similarity` for many comparisons among the same texts (the cells of a sheet): each
 * text is split and sorted once.
 */
export function cachedSimilarity() {
  const known = [new Map<string, string[]>(), new Map<string, string[]>()];
  const split = (text: string, pairs: boolean) => {
    const cache = known[pairs ? 1 : 0];
    let tokens = cache.get(text);
    if (!tokens) cache.set(text, (tokens = (pairs ? bigrams(text) : words(text)).sort()));
    return tokens;
  };
  return (a: string, b: string) => compare(a, b, split, diceSorted);
}

/** Inline word-level difference for side-by-side review. */
export function wordDiff(before: string, after: string) {
  const tokenize = (s: string) => s.match(/\s+|[\p{L}\p{N}’']+|[^\s\p{L}\p{N}]/gu) || [];
  const a = tokenize(before),
    b = tokenize(after);
  const pairs = matchEqual(a, b);
  const out: { kind: 'same' | 'added' | 'removed'; text: string }[] = [];
  const push = (kind: 'same' | 'added' | 'removed', text: string) => {
    if (!text) return;
    const last = out.at(-1);
    if (last?.kind === kind) last.text += text;
    else out.push({ kind, text });
  };
  let i = 0,
    j = 0;
  for (const [x, y] of [...pairs, [a.length, b.length] as [number, number]]) {
    push('removed', a.slice(i, x).join(''));
    push('added', b.slice(j, y).join(''));
    if (x < a.length) push('same', a[x]);
    i = x + 1;
    j = y + 1;
  }
  return out;
}

export interface BlockChange {
  kind: 'added' | 'edited' | 'removed' | 'moved';
  nodeId: string;
  before: string;
  after: string;
  /** Only formatting differs; the words are the same. */
  formatting?: boolean;
}
export interface PushPlan {
  state: DocumentState;
  ops: EventPayload[];
  changes: BlockChange[];
  unchanged: number;
  /** Blocks identified exactly through Dynodoc bookmarks rather than by content. */
  anchored: number;
  warnings: string[];
}
interface BaseItem {
  node: MaterializedNode;
  block: TextBlock;
  keys: string[];
}
interface Unit {
  blocks: TextBlock[];
  anchor?: string;
  tab?: string;
  key: string;
  text: string;
}
const stable = /^[0-9A-HJKMNP-TV-Z]{26}$/;
function blockOf(node: MaterializedNode): TextBlock {
  return (
    (node.current_fields.content as TextBlock | undefined) || {
      type: 'paragraph',
      content: node.current_fields.label
        ? [{ type: 'text', text: String(node.current_fields.label) }]
        : []
    }
  );
}
/** Document items in the order Word export writes them: main text, then each tab. */
export function exportOrder(state: DocumentState): MaterializedNode[] {
  const live = Object.values(state.nodes).filter((n) => !n.deleted);
  const root = live.find((n) => n.type === 'form');
  if (!root) return [];
  const tabs = live
    .filter((n) => n.type === 'section' && n.current_fields.kind === 'document_tab')
    .sort((a, b) => byPosition(a, b));
  return [root, ...tabs].flatMap((parent) =>
    live
      .filter((n) => n.type === 'item' && n.parent_id === parent.id)
      .sort((a, b) => byPosition(a, b))
  );
}
/** How many of the file's blocks already appear, unchanged, in a candidate revision. */
export function matchScore(state: DocumentState, incoming: DocxImport) {
  const counts = new Map<string, number>();
  for (const node of exportOrder(state)) {
    const key = canonical(blockOf(node), incoming.defaults);
    counts.set(key, (counts.get(key) || 0) + 1);
  }
  let score = 0;
  for (const block of incoming.blocks) {
    const key = canonical(block, incoming.defaults),
      c = counts.get(key) || 0;
    if (c && textOf(block).trim()) {
      score++;
      counts.set(key, c - 1);
    }
  }
  return score;
}

/** Keep formatting the author set; drop defaults Word re-applied to an edited block. */
function cleanEdited(block: TextBlock, base: TextBlock, defaults?: Defaults): TextBlock {
  const usedKeys = new Set<string>();
  const visit = (b: TextBlock) => {
    for (const m of b.marks || []) for (const k of Object.keys(m.attrs || {})) usedKeys.add(k);
    for (const k of Object.keys(b.attrs || {})) usedKeys.add(k);
    (b.content || []).forEach(visit);
  };
  visit(base);
  const strip = (b: TextBlock): TextBlock => {
    const next: TextBlock = { ...b };
    if (b.attrs) {
      const attrs: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(b.attrs)) {
        if (v === null || v === undefined) continue;
        if (defaults && same(defaults.para[k], v) && !usedKeys.has(k)) continue;
        attrs[k] = v;
      }
      next.attrs = attrs;
    }
    if (b.marks)
      next.marks = b.marks
        .map((m) => {
          if (m.type !== 'textStyle' || !defaults) return m;
          const attrs = Object.fromEntries(
            Object.entries(m.attrs || {}).filter(
              ([k, v]) => v !== null && !(same(defaults.run[k], v) && !usedKeys.has(k))
            )
          );
          return { ...m, attrs };
        })
        .filter((m) => m.type !== 'textStyle' || Object.keys(m.attrs || {}).length);
    if (b.content) next.content = b.content.map(strip);
    return next;
  };
  let result = strip(block);
  // Word files cannot carry checklist state; keep checkboxes when the list shape is intact.
  if (base.type === 'taskList' && result.type === 'bulletList')
    result = {
      ...result,
      type: 'taskList',
      content: (result.content || []).map((item, i) => ({
        ...item,
        type: 'taskItem',
        attrs: { checked: Boolean(base.content?.[i]?.attrs?.checked) }
      }))
    };
  // Keep attributes Word export does not write (they cannot have been edited in Word).
  const kept = Object.fromEntries(
    Object.entries(base.attrs || {}).filter(([k]) => !PARAGRAPH_KEYS.includes(k))
  );
  return { ...result, attrs: { ...kept, ...result.attrs } };
}

/**
 * Compare a pushed Word file with the revision it started from.
 * `roundtrip` is that revision exported and read back, which shows exactly what an
 * untouched paragraph looks like after passing through Word format.
 */
export function planPush(
  base: DocumentState,
  incoming: DocxImport,
  roundtrip?: DocxImport
): PushPlan {
  const warnings: string[] = [];
  const defaults = incoming.defaults;
  const root = Object.values(base.nodes).find((n) => !n.deleted && n.type === 'form');
  if (!root) throw new Error('This document has no main text to compare with.');
  const order = exportOrder(base);
  const index = new Map(order.map((n, i) => [n.id, i]));
  const tabs = new Set(
    Object.values(base.nodes)
      .filter((n) => !n.deleted && n.current_fields.kind === 'document_tab')
      .map((n) => n.id)
  );
  // Untouched blocks as they look after export: anchored blocks plus any continuation
  // paragraphs (for example the later paragraphs of a quotation).
  const exported = new Map<string, TextBlock[]>();
  if (roundtrip) {
    let current = '';
    roundtrip.blocks.forEach((block, i) => {
      const anchor = roundtrip.anchors?.[i] || '';
      const id = anchor.startsWith('_ddt') || anchor === '_ddcomments' ? '' : anchor.slice(3);
      if (anchor) current = index.has(id) ? id : '';
      if (current) exported.set(current, [...(exported.get(current) || []), block]);
    });
  }
  const items: BaseItem[] = order.map((node) => {
    const block = blockOf(node);
    const keys = [canonical(block, defaults)];
    const out = exported.get(node.id);
    if (out) keys.push(canonical(out, roundtrip?.defaults || defaults));
    return { node, block, keys };
  });

  // Group the file's blocks into units, following tab markers and dropping the
  // exported comments appendix.
  const units: Unit[] = [];
  let tab = '';
  for (let i = 0; i < incoming.blocks.length; i++) {
    const block = incoming.blocks[i],
      anchor = incoming.anchors?.[i] || '';
    if (anchor === '_ddcomments') break;
    if (anchor.startsWith('_ddt')) {
      const id = anchor.slice(4);
      if (tabs.has(id)) {
        tab = id;
        continue;
      }
    }
    const id = anchor.startsWith('_dd') && !anchor.startsWith('_ddt') ? anchor.slice(3) : '';
    const unit: Unit = {
      blocks: [block],
      anchor: stable.test(id) && index.has(id) ? id : undefined,
      tab: tab || undefined,
      key: '',
      text: ''
    };
    // Continuation paragraphs of a multi-paragraph block stay with their block.
    const continuation = unit.anchor ? exported.get(unit.anchor) : undefined;
    while (
      continuation &&
      unit.blocks.length < continuation.length &&
      i + 1 < incoming.blocks.length &&
      !incoming.anchors?.[i + 1] &&
      canonical(incoming.blocks[i + 1], defaults) ===
        canonical(continuation[unit.blocks.length], roundtrip?.defaults || defaults)
    )
      unit.blocks.push(incoming.blocks[++i]);
    unit.key = canonical(unit.blocks, defaults);
    unit.text = unit.blocks.map(textOf).join('\n');
    units.push(unit);
  }
  if (!units.length) throw new Error('The file has no readable text.');

  // 1. Exact identity from bookmarks; duplicates (copied paragraphs) count as new.
  const pairOfUnit = new Array<number>(units.length).fill(-1);
  const pairOfItem = new Array<number>(items.length).fill(-1);
  units.forEach((unit, u) => {
    const b = unit.anchor ? index.get(unit.anchor)! : -1;
    if (b >= 0 && pairOfItem[b] < 0) {
      pairOfUnit[u] = b;
      pairOfItem[b] = u;
    }
  });
  const anchoredUnits = units.map((_, u) => u).filter((u) => pairOfUnit[u] >= 0);
  const inPlace = increasing(anchoredUnits.map((u) => pairOfUnit[u]));
  const moved = new Set(anchoredUnits.filter((_, i) => !inPlace.has(i)));
  const fixed = anchoredUnits.filter((u) => !moved.has(u));

  // 2. Within each gap between fixed blocks, match unchanged blocks exactly, then
  // recognise edited blocks by similarity, preserving order.
  const bounds = [[-1, -1], ...fixed.map((u) => [u, pairOfUnit[u]]), [units.length, items.length]];
  for (let g = 0; g + 1 < bounds.length; g++) {
    const [u0, b0] = bounds[g],
      [u1, b1] = bounds[g + 1];
    const gapUnits: number[] = [],
      gapItems: number[] = [];
    for (let u = u0 + 1; u < u1; u++) if (pairOfUnit[u] < 0) gapUnits.push(u);
    for (let b = b0 + 1; b < b1; b++) if (pairOfItem[b] < 0) gapItems.push(b);
    if (!gapUnits.length || !gapItems.length) continue;
    const present = new Set(gapUnits.map((u) => units[u].key));
    const itemKey = (b: number) => items[b].keys.find((k) => present.has(k)) ?? items[b].keys[0];
    const exact = matchEqual(
      gapUnits.map((u) => units[u].key),
      gapItems.map(itemKey)
    );
    for (const [x, y] of exact) {
      pairOfUnit[gapUnits[x]] = gapItems[y];
      pairOfItem[gapItems[y]] = gapUnits[x];
    }
    let lastItem = -1;
    const openUnits = gapUnits.filter((u) => pairOfUnit[u] < 0),
      openItems = gapItems.filter((b) => pairOfItem[b] < 0);
    // One paragraph replaced in place is an edit of it unless the wording is unrelated.
    if (
      openUnits.length === 1 &&
      openItems.length === 1 &&
      similarity(textOf(items[openItems[0]].block), units[openUnits[0]].text) >= 0.3
    ) {
      pairOfUnit[openUnits[0]] = openItems[0];
      pairOfItem[openItems[0]] = openUnits[0];
      continue;
    }
    for (const u of gapUnits) {
      if (pairOfUnit[u] >= 0) {
        lastItem = gapItems.indexOf(pairOfUnit[u]);
        continue;
      }
      let best = -1,
        score = 0;
      for (let y = lastItem + 1; y < Math.min(gapItems.length, lastItem + 60); y++) {
        const b = gapItems[y];
        if (pairOfItem[b] >= 0) break;
        const s = similarity(textOf(items[b].block), units[u].text);
        if (s > score) {
          score = s;
          best = y;
        }
      }
      const threshold = words(units[u].text).length < 4 ? 0.6 : 0.45;
      if (best >= 0 && score >= threshold) {
        pairOfUnit[u] = gapItems[best];
        pairOfItem[gapItems[best]] = u;
        lastItem = best;
      }
    }
  }

  // 3. Rebuild the document: keep positions of blocks that stayed in order and place
  // new or moved blocks between their neighbours.
  const state = cloneState(base);
  const changes: BlockChange[] = [];
  let unchanged = 0;
  const parentOf = (u: number): string => {
    const b = pairOfUnit[u];
    return units[u].tab || (b >= 0 ? items[b].node.parent_id || root.id : '') || '';
  };
  const parents = units.map((_, u) => parentOf(u));
  for (let u = 0; u < units.length; u++)
    if (!parents[u]) parents[u] = (u ? parents[u - 1] : '') || root.id;
  const inOrder = (u: number) =>
    pairOfUnit[u] >= 0 &&
    !moved.has(u) &&
    parents[u] === (items[pairOfUnit[u]].node.parent_id || root.id);
  const lastPos = new Map<string, string>();
  for (let u = 0; u < units.length; u++) {
    const unit = units[u],
      parent = parents[u],
      b = pairOfUnit[u],
      left = lastPos.get(parent) ?? '';
    const keeps = inOrder(u) && items[b].node.pos > left;
    let pos = keeps ? items[b].node.pos : '';
    if (!keeps) {
      let next: string | undefined;
      for (let v = u + 1; v < units.length; v++)
        if (parents[v] === parent && inOrder(v)) {
          next = items[pairOfUnit[v]].node.pos;
          break;
        }
      pos = between(left, next !== undefined && next > left ? next : undefined);
    }
    lastPos.set(parent, pos);
    const content: TextBlock =
      unit.blocks.length > 1 ? { type: 'blockquote', content: unit.blocks } : unit.blocks[0];
    if (b < 0) {
      const id = ulid(),
        cleaned = cleanEdited(content, { type: 'paragraph' }, defaults),
        block = { ...cleaned, attrs: { ...cleaned.attrs, blockId: id } };
      state.nodes[id] = {
        id,
        parent_id: parent,
        type: 'item',
        pos,
        current_fields: { label: textOf(block), content: block },
        var_name: null,
        deleted: false
      };
      changes.push({ kind: 'added', nodeId: id, before: '', after: textOf(block) });
      continue;
    }
    const item = items[b],
      node = state.nodes[item.node.id];
    if (!keeps) {
      node.pos = pos;
      node.parent_id = parent;
    }
    if (item.keys.includes(unit.key) || item.block.type === 'tableOfContents') {
      if (keeps) unchanged++;
      else
        changes.push({
          kind: 'moved',
          nodeId: node.id,
          before: textOf(item.block),
          after: textOf(item.block)
        });
      continue;
    }
    const cleaned = cleanEdited(content, item.block, defaults),
      edited = { ...cleaned, attrs: { ...cleaned.attrs, blockId: node.id } };
    node.current_fields = { ...node.current_fields, content: edited, label: textOf(edited) };
    changes.push({
      kind: keeps ? 'edited' : 'moved',
      nodeId: node.id,
      before: textOf(item.block),
      after: textOf(edited),
      formatting: textOf(item.block) === textOf(edited)
    });
  }
  items.forEach((item, b) => {
    if (pairOfItem[b] >= 0 || item.block.type === 'tableOfContents') return;
    state.nodes[item.node.id].deleted = true;
    changes.push({ kind: 'removed', nodeId: item.node.id, before: textOf(item.block), after: '' });
  });
  if (tabs.size && !incoming.anchors?.some((a) => a?.startsWith('_ddt')))
    warnings.push(
      'This file does not mark document tabs. New paragraphs are placed next to the text they follow.'
    );
  if (incoming.blocks.some((b) => JSON.stringify(b).includes('"inlineMath"')))
    warnings.push(
      'Equations in edited paragraphs return as linear text; review them before merging.'
    );
  return {
    state,
    ops: stateChanges(base, state),
    changes,
    unchanged,
    anchored: anchoredUnits.length,
    warnings
  };
}
