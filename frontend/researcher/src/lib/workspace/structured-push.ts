/**
 * Push a locally edited Excel workbook or PowerPoint deck back into a document: the
 * spreadsheet and presentation counterparts of `planPush` for Word files.
 *
 * Office files carry no per-row or per-shape markers, so the file is lined up with the
 * revision it started from by content. Unchanged rows and slides anchor the alignment,
 * edited ones are recognised in place or by similarity and keep their node IDs and
 * positions, and only fields that really differ are written. Values and styles are
 * compared in a canonical form that ignores what exporting and re-saving change on
 * their own, so an untouched download produces no changes.
 */
import { ulid } from 'ulid';
import type { DocumentState, MaterializedNode, NodeType } from '$lib/api/types';
import { applyPayload, emptyState } from '$lib/materializer';
import { byPosition } from './api';
import { between, cloneState, stateChanges } from './changes';
import { stableJson } from './equality';
import {
  cachedSimilarity,
  increasing,
  matchEqual,
  similarity,
  type BlockChange,
  type PushPlan
} from './file-push';
import type { ImportResult } from './files';
import { bounded, pageSetup, safeColor, safeFont } from './office';
import {
  inferLayout,
  readDeck,
  resolvedStyle,
  shapeLabel,
  slideLabel,
  themeName,
  themes,
  titleOf,
  type ShapeStyle,
  type Slide,
  type SlideShape,
  type ThemeName
} from './presentation';
import {
  cellFormat,
  columnName,
  conditionalRules,
  numberFormat,
  sheetLayout,
  typedValue,
  type CellFormat,
  type MergeRange,
  type SheetLayout
} from './sheet-model';

/** The uploaded file as the importer read it, or a state already folded from it. */
export type PushSource = ImportResult | DocumentState;

/** Fold an import into a state once, so several candidate revisions can reuse it. */
export function pushState(
  incoming: PushSource,
  kind?: 'spreadsheet' | 'presentation'
): DocumentState {
  if ('nodes' in incoming) return incoming;
  if (kind && incoming.kind !== kind)
    throw new Error(
      kind === 'spreadsheet'
        ? 'This file does not contain a workbook to compare.'
        : 'This file does not contain slides to compare.'
    );
  const state = emptyState();
  for (const op of incoming.ops) applyPayload(state, op);
  return state;
}

// ---------------------------------------------------------------------------------
// Shared alignment

interface Tree {
  root?: MaterializedNode;
  children: (id: string) => MaterializedNode[];
}
function tree(state: DocumentState): Tree {
  const map = new Map<string, MaterializedNode[]>();
  let root: MaterializedNode | undefined;
  for (const n of Object.values(state.nodes)) {
    if (n.deleted) continue;
    if (!root && n.type === 'form') root = n;
    if (!n.parent_id) continue;
    const list = map.get(n.parent_id);
    if (list) list.push(n);
    else map.set(n.parent_id, [n]);
  }
  for (const list of map.values()) list.sort(byPosition);
  return { root, children: (id) => map.get(id) || [] };
}
function removeTree(state: DocumentState, from: Tree, id: string) {
  for (const child of from.children(id)) removeTree(state, from, child.id);
  state.nodes[id].deleted = true;
}
function addNode(
  state: DocumentState,
  id: string,
  parent: string,
  type: NodeType,
  pos: string,
  fields: Record<string, unknown>
) {
  state.nodes[id] = {
    id,
    parent_id: parent,
    type,
    pos,
    current_fields: fields,
    var_name: null,
    deleted: false
  };
}

interface Unit {
  /** Everything compared: content and formatting. */
  key: string;
  /** Content alone, so a unit whose formatting changed still matches exactly. */
  loose: string;
  /** Has no content of its own (an empty row or slide), so it never anchors. */
  blank: boolean;
}
interface Alignment {
  /** Base index paired with each file unit, or -1. */
  pairs: number[];
  /** File units whose pair is out of order. */
  moved: Set<number>;
}
/**
 * Pair a file's units with the base revision's, as `planPush` does for paragraphs
 * without bookmarks: content found exactly once on both sides anchors the alignment,
 * and anchors out of order are moves. Between anchors equal units, then units with
 * equal content, are matched in order (Myers); the rest is paired in place when a gap
 * is the same size on both sides and bounded by matches, otherwise by similarity.
 */
function align(
  base: Unit[],
  file: Unit[],
  similar: (f: number, b: number) => number,
  inPlace: (f: number, b: number) => boolean,
  threshold = 0.45
): Alignment {
  const pairs = new Array<number>(file.length).fill(-1),
    taken = new Array<boolean>(base.length).fill(false);
  const link = (f: number, b: number) => {
    pairs[f] = b;
    taken[b] = true;
  };
  const scores = new Map<number, number>();
  const score = (f: number, b: number) => {
    const k = f * (base.length + 1) + b;
    if (!scores.has(k)) scores.set(k, similar(f, b));
    return scores.get(k)!;
  };
  const once = (units: Unit[]) => {
    const seen = new Map<string, number>();
    units.forEach((u, i) => {
      if (!u.blank) seen.set(u.key, seen.has(u.key) ? -1 : i);
    });
    return seen;
  };
  const inBase = once(base);
  const anchors = [...once(file)]
    .filter(([key, f]) => f >= 0 && (inBase.get(key) ?? -1) >= 0)
    .map(([key, f]) => [f, inBase.get(key)!] as [number, number])
    .sort((x, y) => x[0] - y[0]);
  const kept = increasing(anchors.map((a) => a[1]));
  const moved = new Set<number>();
  anchors.forEach(([f, b], i) => {
    link(f, b);
    if (!kept.has(i)) moved.add(f);
  });
  const gaps = (fixed: [number, number][]) => {
    const bounds = [[-1, -1], ...fixed, [file.length, base.length]];
    const out: [number[], number[]][] = [];
    for (let g = 0; g + 1 < bounds.length; g++) {
      const fs: number[] = [],
        bs: number[] = [];
      for (let f = bounds[g][0] + 1; f < bounds[g + 1][0]; f++) if (pairs[f] < 0) fs.push(f);
      for (let b = bounds[g][1] + 1; b < bounds[g + 1][1]; b++) if (!taken[b]) bs.push(b);
      if (fs.length && bs.length) out.push([fs, bs]);
    }
    return out;
  };
  const steady = () =>
    pairs.map((b, f) => [f, b] as [number, number]).filter(([f, b]) => b >= 0 && !moved.has(f));
  // Blank units get keys that never match, so they are paired by position below.
  for (const field of ['key', 'loose'] as const)
    for (const [fs, bs] of gaps(steady())) {
      const a = fs.map((f) => (file[f].blank ? `\u0000f${f}` : file[f][field])),
        b = bs.map((i) => (base[i].blank ? `\u0000b${i}` : base[i][field]));
      const present = new Set(a);
      if (b.some((k) => present.has(k))) for (const [x, y] of matchEqual(a, b)) link(fs[x], bs[y]);
    }
  const fixed = steady();
  for (const [fs, bs] of gaps(fixed)) {
    // As many rows between two unchanged ones on both sides: edited where they stand.
    if (fixed.length && fs.length === bs.length) {
      fs.forEach((f, i) => {
        if (inPlace(f, bs[i]) || score(f, bs[i]) >= threshold) link(f, bs[i]);
      });
      continue;
    }
    let last = -1;
    fs.forEach((f, i) => {
      let best = -1,
        top = 0;
      for (let y = last + 1; y < Math.min(bs.length, last + 60); y++) {
        const s = score(f, bs[y]);
        if (s > top) {
          top = s;
          best = y;
        }
      }
      if (best < 0 || top < threshold) return;
      // Leave it for a later file unit that matches it better (an edited row below an
      // inserted one).
      if (fs.slice(i + 1, i + 60).some((g) => score(g, bs[best]) > top)) return;
      link(f, bs[best]);
      last = best;
    });
  }
  return { pairs, moved };
}

/**
 * Position keys for a parent's children in file order: children that stayed in order
 * keep their keys; new and moved ones go between their neighbours. `fences` are keys of
 * children that stay where they are without appearing in the file.
 */
function place(
  nodes: (MaterializedNode | undefined)[],
  steady: (i: number) => boolean,
  fences: string[] = []
) {
  const walls = [...fences].sort();
  let left = '';
  return nodes.map((node, i) => {
    const keeps = !!node && steady(i) && node.pos > left;
    let pos = node && keeps ? node.pos : '';
    if (!keeps) {
      let next = walls.find((p) => p > left);
      for (let j = i + 1; j < nodes.length; j++)
        if (nodes[j] && steady(j)) {
          if (next === undefined || nodes[j]!.pos < next) next = nodes[j]!.pos;
          break;
        }
      pos = between(left, next !== undefined && next > left ? next : undefined);
    }
    left = pos;
    return { pos, keeps };
  });
}
function clip(text: string, max = 60) {
  const s = text.replace(/\s+/g, ' ').trim();
  return s.length > max ? s.slice(0, max - 1) + '…' : s;
}
const same = (a: unknown, b: unknown) => stableJson(a ?? null) === stableJson(b ?? null);

// ---------------------------------------------------------------------------------
// Spreadsheets

/** Height the importer records for rows without a set height. */
const ROW_HEIGHT = 28;
const cellKey = /^(?:cell|format)_(\d{1,2})$/;
interface SheetRow extends Unit {
  node: MaterializedNode;
  /** Canonical values by column, without trailing empty cells. */
  values: string[];
}

const columnIndex = (letters: string) =>
  [...letters].reduce((v, ch) => v * 26 + ch.charCodeAt(0) - 64, 0) - 1;
/**
 * A formula with relative references written as offsets from its own cell (R1C1 style),
 * so a formula Excel rewrote because rows were inserted above it compares equal.
 */
function formulaKey(formula: string, row: number, col: number) {
  return formula
    .replace(/("(?:[^"]|"")*")|[^"]+/g, (part, text) => text || part.toUpperCase())
    .replace(
      /("(?:[^"]|"")*")|('(?:[^']|'')*')|(\$?)([A-Z]{1,3})(\$?)(\d{1,7})(?![\w(])/g,
      (match, text, sheet, fixedCol, letters, fixedRow, digits, at: number, whole: string) => {
        if (text || sheet || /[\w.]/.test(whole[at - 1] || '')) return match;
        const c = columnIndex(letters),
          r = Number(digits) - 1;
        return (fixedRow ? `R${r}` : `R[${r - row}]`) + (fixedCol ? `C${c}` : `C[${c - col}]`);
      }
    );
}
/**
 * The value Excel stores: "12.50" and "12.5" are one number, dates compare as instants
 * and formulas by what they refer to from the cell at `row`, `col`.
 */
function valueKey(text: string, f: CellFormat, row = 0, col = 0): string {
  const value = typedValue(text, f);
  if (value instanceof Date) return value.toISOString();
  if (value && typeof value === 'object')
    return '=' + formulaKey((value as { formula: string }).formula, row, col);
  return String(value);
}
/**
 * The format Excel shows, without the defaults every exported cell carries (font, size,
 * "General", a type the value already implies). `text` is the cell's stored text.
 */
function formatKey(f: CellFormat, text: string): Record<string, unknown> {
  const key: Record<string, unknown> = {};
  for (const flag of ['bold', 'italic', 'underline', 'strike', 'wrap', 'border'] as const)
    if (f[flag]) key[flag] = true;
  if (safeFont(f.fontFamily) !== safeFont('')) key.fontFamily = safeFont(f.fontFamily);
  const size = bounded(f.fontSize, 6, 96, 11);
  if (size !== 11) key.fontSize = size;
  for (const name of ['color', 'background'] as const)
    if (safeColor(f[name])) key[name] = safeColor(f[name]).toLowerCase();
  if (['left', 'center', 'right'].includes(f.align || '')) key.align = f.align;
  if (['top', 'middle', 'bottom'].includes(f.vertical || '')) key.vertical = f.vertical;
  if (numberFormat(f) !== 'General') key.numFmt = numberFormat(f);
  const typed = typedValue(text, f);
  if (typed instanceof Date) key.type = 'date';
  else if (typeof typed === 'boolean') key.type = 'boolean';
  else if (f.type === 'text' && typeof typedValue(text, { ...f, type: undefined }) !== 'string')
    key.type = 'text';
  // Export writes links on values only; mailto links do not survive import.
  if (!text.startsWith('=') && /^https?:\/\//i.test(f.hyperlink || '')) key.hyperlink = f.hyperlink;
  if (f.note) key.note = String(f.note).slice(0, 10000);
  if (f.validation?.type) {
    const rule = JSON.parse(JSON.stringify(f.validation)) as Record<string, unknown>;
    if (Array.isArray(rule.formulae)) rule.formulae = rule.formulae.map(String);
    key.validation = rule;
  }
  const rules = conditionalRules(f.conditional);
  if (rules.length)
    key.conditional = rules.map((r) => ({
      ...r,
      background: r.background.toLowerCase(),
      color: r.color.toLowerCase()
    }));
  return key;
}
/**
 * Merge the file's cell format into the stored one key by key, so settings that did not
 * change keep the form the base stored them in (for example a percent preset). Returns
 * undefined when nothing differs; `text` is the text the cell will hold.
 */
function mergeFormat(
  base: CellFormat,
  text: string,
  file: CellFormat,
  fileText: string
): CellFormat | undefined {
  const target = formatKey(file, fileText);
  if (same(formatKey(base, text), target)) return undefined;
  const next: Record<string, unknown> = { ...base };
  // Two passes: dropping a date type can change the implied number format.
  for (let pass = 0; pass < 2; pass++) {
    const current = formatKey(next as CellFormat, text);
    for (const key of new Set([...Object.keys(current), ...Object.keys(target)])) {
      if (same(current[key], target[key])) continue;
      if (key === 'numFmt') {
        delete next.number;
        delete next.decimals;
      }
      if (target[key] === undefined) delete next[key];
      else next[key] = key === 'validation' ? file.validation : target[key];
    }
  }
  if (same(formatKey(next as CellFormat, text), target)) return next as CellFormat;
  return { ...target, ...(target.validation ? { validation: file.validation } : {}) };
}
function columnsOf(...nodes: MaterializedNode[]) {
  const columns = new Set<number>();
  for (const node of nodes)
    for (const k of Object.keys(node.current_fields)) {
      const m = cellKey.exec(k);
      if (m) columns.add(Number(m[1]));
    }
  return [...columns].sort((a, b) => a - b);
}
const rowHeight = (node: MaterializedNode) =>
  Math.round(bounded(node.current_fields.row_height, 1, 5000, ROW_HEIGHT) * 10) / 10;
function readRow(node: MaterializedNode, row: number): SheetRow {
  const values: string[] = [],
    formats: Record<number, unknown> = {};
  for (const c of columnsOf(node)) {
    const text = String(node.current_fields[`cell_${c}`] ?? ''),
      format = cellFormat(node, c);
    values[c] = valueKey(text, format, row, c);
    const key = formatKey(format, text);
    if (Object.keys(key).length) formats[c] = key;
  }
  const cells = Array.from(values, (v) => v || '');
  while (cells.length && !cells.at(-1)) cells.pop();
  const hidden = node.current_fields.hidden === true;
  return {
    node,
    values: cells,
    blank: !cells.length,
    key: stableJson({ cells, formats, height: rowHeight(node), hidden }),
    loose: stableJson(cells)
  };
}
/** How alike two rows are (0–1): equal cells count fully, edited text partly. */
function rowSimilarity(a: string[], b: string[], like = similarity) {
  let total = 0,
    score = 0;
  for (let c = 0; c < Math.max(a.length, b.length); c++) {
    const x = a[c] || '',
      y = b[c] || '';
    if (!x && !y) continue;
    total++;
    if (x === y) score++;
    // Numbers are equal or not; their digits' overlap means nothing.
    else if (x && y && !(Number.isFinite(Number(x)) && Number.isFinite(Number(y))))
      score += 0.8 * like(x, y);
  }
  return total ? score / total : 1;
}
const filled = (values: string[]) =>
  values
    .map((v, c) => (v ? c : -1))
    .filter((c) => c >= 0)
    .join(',');
const texts = (node: MaterializedNode) => {
  const out = columnsOf(node).reduce<string[]>((list, c) => {
    list[c] = String(node.current_fields[`cell_${c}`] ?? '');
    return list;
  }, []);
  const cells = Array.from(out, (v) => v || '');
  while (cells.length && !cells.at(-1)) cells.pop();
  return cells;
};
/** Sheet names as the XLSX export writes them: Excel forbids some characters and repeats. */
function excelNames(sheets: MaterializedNode[]) {
  const used = new Set<string>();
  return sheets.map((s, i) => {
    let name = String(s.current_fields.label || `Sheet ${i + 1}`)
      .replace(/[\\/*?:[\]]/g, '')
      .slice(0, 31);
    if (!name.trim() || used.has(name.toLowerCase())) name = `Sheet ${i + 1}`;
    used.add(name.toLowerCase());
    return name;
  });
}
const sheetRef = (name: string) =>
  /^[\p{L}_][\p{L}\p{N}_.]*$/u.test(name) ? name : `'${name.replace(/'/g, "''")}'`;
/** `Sales!A5:D5  "Tariq | 12 | …"`: wide rows show their first cells and `focus` cells. */
function rowSummary(sheet: string, row: number, cells: string[], focus = new Set<number>()) {
  const range = cells.length > 1 ? `A${row}:${columnName(cells.length - 1)}${row}` : `A${row}`;
  const parts: string[] = [];
  let skipped = false;
  cells.forEach((text, c) => {
    if (cells.length <= 8 || c < 3 || focus.has(c)) {
      if (skipped) parts.push('…');
      skipped = false;
      parts.push(clip(text, 40));
    } else skipped = true;
  });
  if (skipped) parts.push('…');
  return `${sheetRef(sheet)}!${range}  "${parts.join(' | ')}"`;
}

/**
 * Fields that make a paired row (base row `y`, file row `x`) match the file: the columns
 * whose values or formats changed, plus formulas whose references only moved with the
 * row, which are rewritten so they stay correct at the row's new place.
 */
function rowPatch(base: MaterializedNode, y: number, file: MaterializedNode, x: number) {
  const patch: Record<string, unknown> = {},
    edited = new Set<number>(),
    styled = new Set<number>();
  for (const c of columnsOf(base, file)) {
    const baseText = String(base.current_fields[`cell_${c}`] ?? ''),
      fileText = String(file.current_fields[`cell_${c}`] ?? '');
    const baseFormat = cellFormat(base, c),
      fileFormat = cellFormat(file, c);
    let text = baseText;
    if (valueKey(baseText, baseFormat, y, c) === valueKey(fileText, fileFormat, x, c)) {
      if (valueKey(baseText, baseFormat, x, c) !== valueKey(fileText, fileFormat, x, c)) {
        text = fileText;
        patch[`cell_${c}`] = text;
      }
    } else {
      // Keep a stored date-only value date-only when Excel returns it as midnight UTC.
      text =
        /^\d{4}-\d{2}-\d{2}$/.test(baseText) &&
        /^\d{4}-\d{2}-\d{2}T00:00:00(\.000)?Z$/.test(fileText)
          ? fileText.slice(0, 10)
          : fileText;
      patch[`cell_${c}`] = text;
      edited.add(c);
    }
    const format = mergeFormat(baseFormat, text, fileFormat, fileText);
    if (format) {
      patch[`format_${c}`] = format;
      styled.add(c);
    }
  }
  const resized = rowHeight(base) !== rowHeight(file),
    shown = (base.current_fields.hidden === true) !== (file.current_fields.hidden === true);
  if (resized) patch.row_height = file.current_fields.row_height;
  if (shown) patch.hidden = file.current_fields.hidden === true;
  // A row whose formulas only follow it to a new place has not changed for the reviewer.
  const changed = edited.size > 0 || styled.size > 0 || resized || shown;
  return { patch, edited, styled, changed };
}
/** A row from the file as a new node's fields, without formatting defaults. */
function newRowFields(file: MaterializedNode) {
  const fields: Record<string, unknown> = {};
  for (const c of columnsOf(file)) {
    const text = String(file.current_fields[`cell_${c}`] ?? '');
    if (text) fields[`cell_${c}`] = text;
    const format = mergeFormat({}, text, cellFormat(file, c), text);
    if (format) fields[`format_${c}`] = format;
  }
  if (rowHeight(file) !== ROW_HEIGHT) fields.row_height = file.current_fields.row_height;
  if (file.current_fields.hidden === true) fields.hidden = true;
  return fields;
}

const width = (layout: SheetLayout, c: number) =>
  Math.round((layout.widths[String(c)] || 64) * 10) / 10;
const pageKey = (value: unknown) => {
  const p = pageSetup(value);
  return {
    size: p.size,
    landscape: p.landscape,
    margin: Math.round(p.margin * 10) / 10,
    header: p.header.trim(),
    footer: p.footer.trim(),
    pageNumbers: p.pageNumbers
  };
};
const areaKey = (value: unknown) =>
  String(value || '')
    .replace(/\$/g, '')
    .toUpperCase();
const chartsKey = (value: unknown) =>
  (Array.isArray(value) ? (value as Record<string, unknown>[]) : []).map((c) => ({
    title: String(c?.title ?? ''),
    type: c?.type,
    range: areaKey(c?.range),
    box: [c?.x, c?.y, c?.width, c?.height].map((n) => Math.round(Number(n) || 0))
  }));
const namesKey = (value: unknown) =>
  (Array.isArray(value) ? (value as Record<string, unknown>[]) : [])
    .filter((n) => typeof n?.name === 'string' && typeof n?.expression === 'string')
    .map((n) => ({ name: n.name, expression: n.expression }));

/**
 * Sheet settings the importer reads (layout, page setup, print area, charts,
 * protection) that differ in the file, with before/after descriptions. `ids` maps the
 * file's row IDs to the IDs they will have; `live` holds the rows that remain.
 */
function sheetPatch(
  base: MaterializedNode,
  file: MaterializedNode,
  rows: {
    ids: Map<string, string>;
    live: Set<string>;
    baseAt: Map<string, number>;
    fileAt: Map<string, number>;
    /** Columns the file's rows fill. */
    used: number;
  }
) {
  const bf = base.current_fields,
    ff = file.current_fields;
  const patch: Record<string, unknown> = {},
    aspects: [string, string][] = [];
  const bl = sheetLayout(bf.sheet_layout),
    fl = sheetLayout(ff.sheet_layout);
  const layout: Record<string, unknown> =
    bf.sheet_layout && typeof bf.sheet_layout === 'object'
      ? { ...(bf.sheet_layout as Record<string, unknown>) }
      : {};
  let relaid = false;
  const count = Math.min(100, Math.max(12, Number(bf.columns) || 12, Number(ff.columns) || 12));
  const widened = Array.from({ length: count }, (_, c) => c).filter(
    (c) => width(bl, c) !== width(fl, c)
  );
  if (widened.length) {
    layout.widths = {
      ...bl.widths,
      ...Object.fromEntries(widened.map((c) => [String(c), width(fl, c)]))
    };
    relaid = true;
    const list = (l: SheetLayout) =>
      'column widths ' + widened.map((c) => `${columnName(c)} ${width(l, c)}`).join(', ');
    aspects.push([list(bl), list(fl)]);
  }
  const hiddenList = (l: SheetLayout) =>
    [...l.hiddenColumns]
      .sort((a, b) => a - b)
      .map(columnName)
      .join(', ') || 'none';
  if (hiddenList(bl) !== hiddenList(fl)) {
    layout.hiddenColumns = [...fl.hiddenColumns].sort((a, b) => a - b);
    relaid = true;
    aspects.push([`hidden columns ${hiddenList(bl)}`, `hidden columns ${hiddenList(fl)}`]);
  }
  if (bl.frozenRows !== fl.frozenRows || bl.frozenColumns !== fl.frozenColumns) {
    layout.frozenRows = fl.frozenRows;
    layout.frozenColumns = fl.frozenColumns;
    relaid = true;
    const frozen = (l: SheetLayout) => `frozen ${l.frozenRows} rows, ${l.frozenColumns} columns`;
    aspects.push([frozen(bl), frozen(fl)]);
  }
  // Merges name their rows by ID; compare them once file rows carry their final IDs.
  const merges = fl.merges
    .map((m) => ({
      ...m,
      fromRow: rows.ids.get(m.fromRow) || '',
      toRow: rows.ids.get(m.toRow) || ''
    }))
    .filter((m) => m.fromRow && m.toRow);
  const mergeKeys = (list: MergeRange[]) =>
    list
      .filter((m) => rows.live.has(m.fromRow) && rows.live.has(m.toRow))
      .map((m) => `${m.fromRow}|${m.toRow}|${m.fromCol}|${m.toCol}`)
      .sort()
      .join(';');
  if (mergeKeys(bl.merges) !== mergeKeys(merges)) {
    layout.merges = merges;
    relaid = true;
    const cells = (list: MergeRange[], at: Map<string, number>) =>
      'merged ' +
      (list
        .filter((m) => at.has(m.fromRow) && at.has(m.toRow))
        .slice(0, 6)
        .map(
          (m) =>
            `${columnName(m.fromCol)}${at.get(m.fromRow)}:${columnName(m.toCol)}${at.get(m.toRow)}`
        )
        .join(', ') || 'none');
    aspects.push([cells(bl.merges, rows.baseAt), cells(merges, rows.fileAt)]);
  }
  if (relaid) patch.sheet_layout = layout;
  // Widen the sheet for data beyond its columns; trailing empty columns never narrow it.
  if (rows.used > (Number(bf.columns) || 12)) patch.columns = Math.min(100, rows.used);
  if (!same(pageKey(bf.page_setup), pageKey(ff.page_setup))) {
    patch.page_setup = { ...pageSetup(bf.page_setup), ...pageKey(ff.page_setup) };
    const page = (v: unknown) => {
      const p = pageKey(v);
      return (
        `page ${p.size} ${p.landscape ? 'landscape' : 'portrait'}, margin ${p.margin} mm` +
        (p.header ? `, header “${p.header}”` : '') +
        (p.footer ? `, footer “${p.footer}”` : '')
      );
    };
    aspects.push([page(bf.page_setup), page(ff.page_setup)]);
  }
  if (areaKey(bf.print_area) !== areaKey(ff.print_area)) {
    patch.print_area = String(ff.print_area || '');
    aspects.push([
      `print area ${areaKey(bf.print_area) || 'none'}`,
      `print area ${areaKey(ff.print_area) || 'none'}`
    ]);
  }
  if (!same(chartsKey(bf.charts), chartsKey(ff.charts))) {
    // Chart IDs are editor identities the file does not carry; keep them by order.
    const previous = Array.isArray(bf.charts) ? (bf.charts as { id?: string }[]) : [];
    patch.charts = (Array.isArray(ff.charts) ? (ff.charts as { id?: string }[]) : []).map(
      (chart, i) => ({ ...chart, id: previous[i]?.id || chart.id })
    );
    const list = (v: unknown) =>
      'charts ' +
      (chartsKey(v)
        .map((c) => `“${c.title}” ${c.range}`)
        .join(', ') || 'none');
    aspects.push([list(bf.charts), list(ff.charts)]);
  }
  if ((bf.sheet_protected === true) !== (ff.sheet_protected === true)) {
    patch.sheet_protected = ff.sheet_protected === true;
    const state = (v: unknown) => (v === true ? 'protected' : 'not protected');
    aspects.push([state(bf.sheet_protected), state(ff.sheet_protected)]);
  }
  return { patch, aspects };
}

/**
 * Compare a pushed Excel file with the revision it started from. Sheets pair by name,
 * then in order; rows pair by content and keep their IDs and positions.
 */
export function planSheetPush(base: DocumentState, incoming: PushSource): PushPlan {
  const file = pushState(incoming, 'spreadsheet');
  const was = tree(base),
    now = tree(file);
  const root = was.root;
  if (!root) throw new Error('This document has no workbook to compare with.');
  const sheetsOf = (t: Tree) =>
    (t.root ? t.children(t.root.id) : []).filter(
      (n) => n.type === 'section' && n.current_fields.kind === 'sheet'
    );
  const baseSheets = sheetsOf(was),
    fileSheets = sheetsOf(now);
  if (!fileSheets.length) throw new Error('The file has no readable sheets.');
  const state = cloneState(base);
  const changes: BlockChange[] = [],
    warnings: string[] = [];
  let unchanged = 0;

  // Sheets pair by the name Excel shows, then the remaining ones in order.
  const names = excelNames(baseSheets);
  const sheetPairs = fileSheets.map(() => -1),
    named = new Set<number>();
  fileSheets.forEach((sheet, f) => {
    const b = names.findIndex(
      (name, i) => !named.has(i) && name === String(sheet.current_fields.label ?? '')
    );
    if (b >= 0) {
      sheetPairs[f] = b;
      named.add(b);
    }
  });
  const rest = baseSheets.map((_, b) => b).filter((b) => !named.has(b));
  sheetPairs.forEach((b, f) => {
    if (b < 0 && rest.length) sheetPairs[f] = rest.shift()!;
  });
  const pairedSheets = sheetPairs.map((_, f) => f).filter((f) => sheetPairs[f] >= 0);
  const sheetOrder = increasing(pairedSheets.map((f) => sheetPairs[f]));
  const steadySheets = new Set(pairedSheets.filter((_, i) => sheetOrder.has(i)));
  const sheetSlots = place(
    sheetPairs.map((b) => (b >= 0 ? baseSheets[b] : undefined)),
    (f) => steadySheets.has(f)
  );

  fileSheets.forEach((fileSheet, f) => {
    const name = String(fileSheet.current_fields.label ?? '');
    const fileRows = now.children(fileSheet.id).map(readRow);
    let lastFilled = fileRows.length - 1;
    while (lastFilled >= 0 && fileRows[lastFilled].blank) lastFilled--;
    const ids = new Map<string, string>();

    if (sheetPairs[f] < 0) {
      // A new sheet with all its rows; trailing blank rows are export padding.
      const id = ulid();
      const rows = fileRows.slice(0, lastFilled + 1);
      rows.forEach((row) => ids.set(row.node.id, ulid()));
      const layout = sheetLayout(fileSheet.current_fields.sheet_layout);
      const fields: Record<string, unknown> = {
        ...fileSheet.current_fields,
        page_setup: {
          ...pageSetup(fileSheet.current_fields.page_setup),
          ...pageKey(fileSheet.current_fields.page_setup)
        },
        columns: Math.max(12, ...rows.map((r) => r.values.length)),
        sheet_layout: {
          ...layout,
          merges: layout.merges
            .map((m) => ({ ...m, fromRow: ids.get(m.fromRow)!, toRow: ids.get(m.toRow)! }))
            .filter((m) => m.fromRow && m.toRow)
        }
      };
      addNode(state, id, root.id, 'section', sheetSlots[f].pos, fields);
      rows.forEach((row, r) =>
        addNode(
          state,
          ids.get(row.node.id)!,
          id,
          'item',
          String(r).padStart(8, '0'),
          newRowFields(row.node)
        )
      );
      changes.push({
        kind: 'added',
        nodeId: id,
        before: '',
        after:
          `Sheet “${name}” · ${rows.length} row${rows.length === 1 ? '' : 's'}` +
          (rows[0] ? ` · ${rowSummary(name, 1, texts(rows[0].node))}` : '')
      });
      return;
    }

    const baseSheet = baseSheets[sheetPairs[f]],
      baseName = names[sheetPairs[f]];
    const baseRows = was.children(baseSheet.id).map(readRow);
    let lastBase = baseRows.length - 1;
    while (lastBase >= 0 && baseRows[lastBase].blank) lastBase--;
    // Rows edited in place need only share their filled columns, or some text.
    const like = cachedSimilarity();
    const { pairs, moved } = align(
      baseRows,
      fileRows,
      (x, y) => rowSimilarity(fileRows[x].values, baseRows[y].values, like),
      (x, y) =>
        fileRows[x].blank ||
        baseRows[y].blank ||
        filled(fileRows[x].values) === filled(baseRows[y].values) ||
        rowSimilarity(fileRows[x].values, baseRows[y].values, like) >= 0.2
    );
    const pairedBase = new Set(pairs.filter((b) => b >= 0));
    // Blank rows past the last filled one are padding: file ones are not added, base
    // ones stay where they are.
    const rows = fileRows.map((_, x) => x).filter((x) => pairs[x] >= 0 || x <= lastFilled);
    const kept = new Set(
      baseRows.map((_, y) => y).filter((y) => !pairedBase.has(y) && y > lastBase)
    );
    const slots = place(
      rows.map((x) => (pairs[x] >= 0 ? baseRows[pairs[x]].node : undefined)),
      (i) => pairs[rows[i]] >= 0 && !moved.has(rows[i]),
      [...kept].map((y) => baseRows[y].node.pos)
    );
    let touched = false;
    rows.forEach((x, i) => {
      const { pos, keeps } = slots[i],
        row = fileRows[x];
      if (pairs[x] < 0) {
        const id = ulid();
        ids.set(row.node.id, id);
        addNode(state, id, baseSheet.id, 'item', pos, newRowFields(row.node));
        changes.push({
          kind: 'added',
          nodeId: id,
          before: '',
          after: rowSummary(name, x + 1, texts(state.nodes[id]))
        });
        touched = true;
        return;
      }
      const y = pairs[x],
        node = state.nodes[baseRows[y].node.id];
      ids.set(row.node.id, node.id);
      if (!keeps) node.pos = pos;
      const { patch, edited, styled, changed } = rowPatch(baseRows[y].node, y, row.node, x);
      if (Object.keys(patch).length) node.current_fields = { ...node.current_fields, ...patch };
      if (!changed) {
        if (keeps) unchanged++;
        else {
          changes.push({
            kind: 'moved',
            nodeId: node.id,
            before: rowSummary(baseName, y + 1, texts(baseRows[y].node)),
            after: rowSummary(name, x + 1, texts(node))
          });
          touched = true;
        }
        return;
      }
      touched = true;
      const focus = new Set([...edited, ...styled]);
      const notes = [
        ...(styled.size
          ? [`formatting ${[...styled].map((c) => columnName(c) + (x + 1)).join(', ')}`]
          : []),
        ...('row_height' in patch ? [`row height ${rowHeight(node)}`] : []),
        ...('hidden' in patch ? [patch.hidden ? 'hidden row' : 'shown row'] : [])
      ];
      changes.push({
        kind: keeps ? 'edited' : 'moved',
        nodeId: node.id,
        before: rowSummary(
          keeps ? name : baseName,
          keeps ? x + 1 : y + 1,
          texts(baseRows[y].node),
          focus
        ),
        after:
          rowSummary(name, x + 1, texts(node), focus) +
          (notes.length ? `  · ${notes.join(' · ')}` : ''),
        formatting: !edited.size
      });
    });
    baseRows.forEach((row, y) => {
      if (pairedBase.has(y) || kept.has(y)) return;
      state.nodes[row.node.id].deleted = true;
      changes.push({
        kind: 'removed',
        nodeId: row.node.id,
        before: rowSummary(baseName, y + 1, texts(row.node)),
        after: ''
      });
      touched = true;
    });

    const live = new Set(
      Object.values(state.nodes)
        .filter((n) => !n.deleted && n.parent_id === baseSheet.id)
        .map((n) => n.id)
    );
    const { patch, aspects } = sheetPatch(baseSheet, fileSheet, {
      ids,
      live,
      baseAt: new Map(baseRows.map((r, y) => [r.node.id, y + 1])),
      fileAt: new Map(rows.map((x) => [ids.get(fileRows[x].node.id)!, x + 1])),
      used: Math.max(0, ...rows.map((x) => fileRows[x].values.length))
    });
    const bf = baseSheet.current_fields,
      ff = fileSheet.current_fields;
    if (baseName !== name) patch.label = name;
    const sheetNode = state.nodes[baseSheet.id];
    if (!sheetSlots[f].keeps) sheetNode.pos = sheetSlots[f].pos;
    if (Object.keys(patch).length)
      sheetNode.current_fields = { ...sheetNode.current_fields, ...patch };
    if (baseName !== name || aspects.length || !sheetSlots[f].keeps)
      changes.push({
        kind: sheetSlots[f].keeps ? 'edited' : 'moved',
        nodeId: baseSheet.id,
        before: [`Sheet “${baseName}”`, ...aspects.map((a) => a[0])].join(' · '),
        after: [`Sheet “${name}”`, ...aspects.map((a) => a[1])].join(' · '),
        formatting: baseName === name
      });
    if (touched && bf.sheet_protected === true && ff.sheet_protected === true)
      warnings.push(
        `Sheet “${name}” is protected. Its row changes can be merged only after an editor removes the protection.`
      );
  });
  baseSheets.forEach((sheet, b) => {
    if (sheetPairs.includes(b)) return;
    const rows = was.children(sheet.id);
    removeTree(state, was, sheet.id);
    changes.push({
      kind: 'removed',
      nodeId: sheet.id,
      before:
        `Sheet “${names[b]}” · ${rows.length} row${rows.length === 1 ? '' : 's'}` +
        (rows[0] ? ` · ${rowSummary(names[b], 1, texts(rows[0]))}` : ''),
      after: ''
    });
  });

  // Workbook names live on the root.
  const fileRoot = now.root;
  if (
    fileRoot &&
    !same(
      namesKey(root.current_fields.named_ranges),
      namesKey(fileRoot.current_fields.named_ranges)
    )
  ) {
    state.nodes[root.id].current_fields = {
      ...state.nodes[root.id].current_fields,
      named_ranges: namesKey(fileRoot.current_fields.named_ranges)
    };
    const list = (v: unknown) =>
      'Named ranges: ' +
      (namesKey(v)
        .map((n) => `${n.name} ${n.expression}`)
        .join(', ') || 'none');
    changes.push({
      kind: 'edited',
      nodeId: root.id,
      before: list(root.current_fields.named_ranges),
      after: list(fileRoot.current_fields.named_ranges)
    });
  }
  return { state, ops: stateChanges(base, state), changes, unchanged, anchored: 0, warnings };
}

/** Fraction (0–1) of the file's filled rows found unchanged in a candidate revision. */
export function sheetMatchScore(state: DocumentState, incoming: PushSource) {
  const rowsOf = (s: DocumentState) => {
    const t = tree(s);
    return (t.root ? t.children(t.root.id) : [])
      .filter((n) => n.current_fields.kind === 'sheet')
      .flatMap((sheet) => t.children(sheet.id).map(readRow))
      .filter((row) => !row.blank);
  };
  const counts = new Map<string, number>();
  for (const row of rowsOf(state)) counts.set(row.key, (counts.get(row.key) || 0) + 1);
  const rows = rowsOf(pushState(incoming, 'spreadsheet'));
  let score = 0;
  for (const row of rows) {
    const c = counts.get(row.key) || 0;
    if (c) {
      score++;
      counts.set(row.key, c - 1);
    }
  }
  return rows.length ? score / rows.length : 0;
}

// ---------------------------------------------------------------------------------
// Presentations

/** Shapes the PPTX export writes: it drops empty plain text boxes and pictures without data. */
const exported = (s: SlideShape) =>
  s.kind === 'image' ? !!s.src : s.kind === 'shape' || !!s.role || !!s.text.trim();
const meaningful = (s: SlideShape) => (s.kind === 'image' ? !!s.src : !!s.text.trim());
const normal = (text: string) => text.replace(/\r\n?/g, '\n');
/** Short content fingerprint so image data never becomes part of a comparison key. */
function fingerprint(src = '') {
  const data = src.slice(src.indexOf(',') + 1);
  let h = 0x811c9dc5;
  for (let i = 0; i < data.length; i++) h = Math.imul(h ^ data.charCodeAt(i), 0x01000193);
  return `${data.length}:${(h >>> 0).toString(16)}`;
}
function shapeContent(s: SlideShape) {
  return s.kind === 'image'
    ? ['image', s.role || '', fingerprint(s.src), s.alt || '']
    : ['box', s.role || '', normal(s.text)];
}
/**
 * How a shape looks once theme defaults are resolved, so explicit colours written on
 * export equal the theme defaults they came from. Empty text has no visible style.
 */
function shapeLook(s: SlideShape, theme: ThemeName): Record<string, unknown> {
  if (s.kind === 'image') return {};
  const r = resolvedStyle(s, theme);
  if (!s.text.trim()) return { fill: r.fill || '' };
  return {
    fill: r.fill || '',
    size: r.size,
    color: r.color.toLowerCase(),
    align: r.align,
    bold: !!s.style.bold,
    italic: !!s.style.italic,
    underline: !!s.style.underline,
    bullets: !!s.style.bullets
  };
}
const GEOMETRY = ['x', 'y', 'w', 'h'] as const;
/** PPTX stores geometry in EMU; differences within half a percent are rounding. */
const shifted = (a: SlideShape, b: SlideShape) =>
  GEOMETRY.filter((k) => Math.abs(a[k] - b[k]) > 0.5);
function shapeScore(a: SlideShape, b: SlideShape) {
  if ((a.kind === 'image') !== (b.kind === 'image')) return 0;
  if (a.role && b.role && a.role !== b.role) return 0;
  const identical = same(shapeContent(a), shapeContent(b));
  const text =
    a.kind === 'image'
      ? fingerprint(a.src) === fingerprint(b.src)
        ? 1
        : 0
      : similarity(a.text, b.text);
  const distance = GEOMETRY.reduce((sum, k) => sum + Math.abs(a[k] - b[k]), 0);
  const role = a.role && a.role === b.role ? 0.35 : !a.role && !b.role ? 0.1 : 0;
  return (identical ? 1 : 0) + role + 0.4 * text + 0.25 * Math.max(0, 1 - distance / 60);
}
/** Shapes on a paired slide pair by content, role and position, best matches first. */
function pairShapes(base: SlideShape[], file: SlideShape[]) {
  const scored: [number, number, number][] = [];
  file.forEach((s, f) =>
    base.forEach((t, b) => {
      const score = shapeScore(t, s);
      if (score >= 0.35) scored.push([score, f, b]);
    })
  );
  scored.sort((x, y) => y[0] - x[0] || x[1] - y[1] || x[2] - y[2]);
  const pairs = file.map(() => -1),
    taken = new Set<number>();
  for (const [, f, b] of scored)
    if (pairs[f] < 0 && !taken.has(b)) {
      pairs[f] = b;
      taken.add(b);
    }
  return pairs;
}
/**
 * Whether the file restyled a shape. The base style is judged on the file's text, since
 * an empty text box carries no visible style through export.
 */
const restyled = (base: SlideShape, file: SlideShape, theme: ThemeName) =>
  !same(shapeLook({ ...base, text: file.text }, theme), shapeLook(file, theme));
/** Style that makes `base` look like `file`; keys equal to the theme default are dropped. */
function mergeStyle(base: SlideShape, file: SlideShape, theme: ThemeName): ShapeStyle {
  const current = shapeLook({ ...base, text: file.text }, theme),
    target = shapeLook(file, theme);
  const resolved = resolvedStyle(file, theme),
    defaults = resolvedStyle({ ...base, style: {} }, theme);
  const next: Record<string, unknown> = { ...base.style };
  for (const key of Object.keys(target)) {
    if (same(current[key], target[key])) continue;
    if (key === 'size' || key === 'color' || key === 'align' || key === 'fill') {
      const value = key === 'color' ? resolved.color.toLowerCase() : resolved[key];
      const fallback = key === 'color' ? defaults.color.toLowerCase() : defaults[key];
      if (value === undefined || value === fallback) delete next[key];
      else next[key] = value;
    } else if (target[key]) next[key] = true;
    else delete next[key];
  }
  return next as ShapeStyle;
}
/** A shape from the file as a new node's fields, without explicit theme defaults. */
function newShapeFields(s: SlideShape, theme: ThemeName) {
  const { id: _id, pos: _pos, ...fields } = s;
  void _id;
  void _pos;
  const style = mergeStyle({ ...s, style: {} }, s, theme);
  // An empty placeholder's only meaningful style is whether typed text gets bullets.
  if (!s.text.trim() && s.style.bullets) style.bullets = true;
  return { ...fields, style, label: shapeLabel(s) };
}
const roleNames = { title: 'Title', subtitle: 'Subtitle', body: 'Body' } as const;
function shapeSummary(n: number, s: SlideShape, detail = '') {
  const name = s.role
    ? roleNames[s.role]
    : s.kind === 'image'
      ? 'Picture'
      : s.kind === 'shape'
        ? 'Shape'
        : 'Text box';
  const content = s.kind === 'image' ? s.alt || 'Picture' : s.text;
  return `Slide ${n} · ${name}  "${clip(content, 120)}"${detail}`;
}
const geometryText = (s: SlideShape) =>
  ` · at ${Math.round(s.x)}%, ${Math.round(s.y)}% · ${Math.round(s.w)}×${Math.round(s.h)}%`;
function lookText(s: SlideShape, theme: ThemeName) {
  const look = shapeLook(s, theme);
  const parts = [
    look.size ? `${look.size} pt` : '',
    String(look.color || ''),
    String(look.align || ''),
    look.fill ? `fill ${look.fill}` : '',
    ...(['bold', 'italic', 'underline', 'bullets'] as const).filter((k) => look[k])
  ].filter(Boolean);
  return parts.length ? ` · ${parts.join(', ')}` : '';
}
interface SlideUnit extends Unit {
  slide: Slide;
  shapes: SlideShape[];
  text: string;
}
function readSlide(slide: Slide): SlideUnit {
  const shapes = slide.shapes.filter(exported);
  const text = [...shapes.map((s) => (s.kind === 'image' ? s.alt || '' : s.text)), slide.notes]
    .filter((t) => t.trim())
    .join('\n');
  return {
    slide,
    shapes,
    text,
    blank: !shapes.some(meaningful) && !slide.notes.trim(),
    key: stableJson([shapes.map(shapeContent), slide.notes.trim()]),
    loose: stableJson(shapes.map((s) => (s.kind === 'image' ? fingerprint(s.src) : normal(s.text))))
  };
}
function slideSummary(n: number, unit: SlideUnit, detail = '') {
  const heading =
    titleOf(unit.slide) || unit.shapes.find((s) => s.text.trim())?.text || 'Untitled slide';
  return `Slide ${n}  "${clip(heading, 80)}"${detail}`;
}

/**
 * Compare a pushed PowerPoint file with the revision it started from. Slides pair by
 * content, then by text similarity in order; shapes on paired slides pair by content,
 * role and position.
 */
export function planSlidePush(base: DocumentState, incoming: PushSource): PushPlan {
  const file = pushState(incoming, 'presentation');
  const deck = readDeck(base),
    other = readDeck(file);
  if (!deck.rootId) throw new Error('This document has no slides to compare with.');
  if (!other.slides.length) throw new Error('The file has no readable slides.');
  const rootId = deck.rootId,
    theme = deck.theme,
    background = themes[theme].background;
  const was = tree(base);
  const state = cloneState(base);
  const changes: BlockChange[] = [];
  let unchanged = 0;

  const baseSlides = deck.slides.map(readSlide),
    fileSlides = other.slides.map(readSlide);
  const score = (f: number, b: number) => similarity(fileSlides[f].text, baseSlides[b].text);
  const roles = (u: SlideUnit) => u.shapes.map((s) => s.role || s.kind).join(',');
  const { pairs, moved: movedSlides } = align(baseSlides, fileSlides, score, (f, b) => {
    const x = fileSlides[f],
      y = baseSlides[b];
    return x.blank || y.blank || roles(x) === roles(y) || score(f, b) >= 0.2;
  });
  // A slide that was both moved and edited still pairs when its text is clearly the same.
  const taken = new Set(pairs.filter((b) => b >= 0));
  fileSlides.forEach((unit, f) => {
    if (pairs[f] >= 0 || unit.blank) return;
    let best = -1,
      top = 0.6;
    baseSlides.forEach((other, b) => {
      if (taken.has(b) || other.blank) return;
      const s = score(f, b);
      if (s >= top) {
        top = s;
        best = b;
      }
    });
    if (best >= 0) {
      pairs[f] = best;
      taken.add(best);
      movedSlides.add(f);
    }
  });
  const slots = place(
    pairs.map((b) => (b >= 0 ? state.nodes[baseSlides[b].slide.id] : undefined)),
    (f) => pairs[f] >= 0 && !movedSlides.has(f)
  );

  fileSlides.forEach((unit, f) => {
    const n = f + 1,
      { pos, keeps } = slots[f],
      slide = unit.slide;
    if (pairs[f] < 0) {
      const id = ulid();
      addNode(state, id, rootId, 'section', pos, {
        label: slideLabel(titleOf(slide)),
        kind: 'slide',
        layout: slide.layout,
        notes: slide.notes,
        ...(slide.background && slide.background !== background
          ? { background: slide.background }
          : {}),
        ...(slide.hidden ? { hidden: true } : {})
      });
      unit.shapes.forEach((s, j) =>
        addNode(state, ulid(), id, 'item', String(j).padStart(4, '0'), newShapeFields(s, theme))
      );
      changes.push({
        kind: 'added',
        nodeId: id,
        before: '',
        after: slideSummary(n, unit, ` · ${unit.shapes.length} objects`)
      });
      return;
    }
    const b = pairs[f],
      baseUnit = baseSlides[b],
      baseSlide = baseUnit.slide,
      node = state.nodes[baseSlide.id];
    if (!keeps) node.pos = pos;

    // Shapes: pair, then keep z-order keys where the order is unchanged.
    const shapePairs = pairShapes(baseUnit.shapes, unit.shapes);
    const paired = shapePairs.map((_, j) => j).filter((j) => shapePairs[j] >= 0);
    const order = increasing(paired.map((j) => shapePairs[j]));
    const steady = new Set(paired.filter((_, i) => order.has(i)));
    const kept = baseSlide.shapes.filter((s) => !exported(s));
    const shapeSlots = place(
      shapePairs.map((i) => (i >= 0 ? state.nodes[baseUnit.shapes[i].id] : undefined)),
      (j) => steady.has(j),
      kept.map((s) => s.pos)
    );
    unit.shapes.forEach((s, j) => {
      const at = shapeSlots[j];
      if (shapePairs[j] < 0) {
        const id = ulid();
        addNode(state, id, baseSlide.id, 'item', at.pos, newShapeFields(s, theme));
        changes.push({ kind: 'added', nodeId: id, before: '', after: shapeSummary(n, s) });
        return;
      }
      const t = baseUnit.shapes[shapePairs[j]],
        shapeNode = state.nodes[t.id];
      if (!at.keeps) shapeNode.pos = at.pos;
      const patch: Record<string, unknown> = {};
      const edited = normal(t.text) !== normal(s.text);
      if (edited) patch.text = s.text;
      const moves = shifted(t, s);
      for (const k of moves) patch[k] = Math.round(s[k] * 100) / 100;
      const styled = restyled(t, s, theme);
      if (styled) patch.style = mergeStyle(t, s, theme);
      if (t.kind === 'image' && fingerprint(t.src) !== fingerprint(s.src)) patch.src = s.src;
      if (t.kind === 'image' && (t.alt || '') !== (s.alt || '')) patch.alt = s.alt || '';
      if (edited || 'alt' in patch) patch.label = shapeLabel({ ...t, ...patch } as SlideShape);
      if (!Object.keys(patch).length) {
        if (at.keeps) unchanged++;
        else
          changes.push({
            kind: 'moved',
            nodeId: t.id,
            before: shapeSummary(b + 1, t),
            after: shapeSummary(n, t)
          });
        return;
      }
      shapeNode.current_fields = { ...shapeNode.current_fields, ...patch };
      const after = { ...t, ...patch } as SlideShape;
      const detail = (x: SlideShape) =>
        (moves.length ? geometryText(x) : '') + (styled ? lookText(x, theme) : '');
      changes.push({
        kind: at.keeps ? 'edited' : 'moved',
        nodeId: t.id,
        before: shapeSummary(b + 1, t, detail(t)),
        after: shapeSummary(n, after, detail(s)),
        formatting: !edited && !('src' in patch) && !('alt' in patch)
      });
    });
    baseUnit.shapes.forEach((t, i) => {
      if (shapePairs.includes(i)) return;
      state.nodes[t.id].deleted = true;
      changes.push({ kind: 'removed', nodeId: t.id, before: shapeSummary(b + 1, t), after: '' });
    });

    // Slide settings: notes, background, visibility, a layout the file states.
    const patch: Record<string, unknown> = {},
      aspects: [string, string][] = [];
    if (baseSlide.notes.trim() !== slide.notes.trim()) {
      patch.notes = slide.notes;
      aspects.push([`notes “${clip(baseSlide.notes, 80)}”`, `notes “${clip(slide.notes, 80)}”`]);
    }
    const fill = (s: Slide) => (s.background || background).toLowerCase();
    if (fill(baseSlide) !== fill(slide)) {
      patch.background = fill(slide) === background ? null : fill(slide);
      aspects.push([`background ${fill(baseSlide)}`, `background ${fill(slide)}`]);
    }
    if (baseSlide.hidden !== slide.hidden) {
      patch.hidden = slide.hidden;
      aspects.push([baseSlide.hidden ? 'hidden' : 'shown', slide.hidden ? 'hidden' : 'shown']);
    }
    // A layout inferred from the shapes carries no information of its own.
    if (slide.layout !== baseSlide.layout && slide.layout !== inferLayout(slide.shapes)) {
      patch.layout = slide.layout;
      aspects.push([`layout ${baseSlide.layout}`, `layout ${slide.layout}`]);
    }
    // The slide label follows its title, as it does when the title is typed in Dynodoc.
    if (normal(titleOf(slide)) !== normal(titleOf(baseSlide)))
      node.current_fields = { ...node.current_fields, label: slideLabel(titleOf(slide)) };
    if (Object.keys(patch).length) node.current_fields = { ...node.current_fields, ...patch };
    if (aspects.length || !keeps)
      changes.push({
        kind: keeps ? 'edited' : 'moved',
        nodeId: baseSlide.id,
        before: slideSummary(b + 1, baseUnit, aspects.map((a) => ` · ${a[0]}`).join('')),
        after: slideSummary(n, unit, aspects.map((a) => ` · ${a[1]}`).join('')),
        formatting: true
      });
  });
  baseSlides.forEach((unit, b) => {
    if (taken.has(b)) return;
    removeTree(state, was, unit.slide.id);
    changes.push({
      kind: 'removed',
      nodeId: unit.slide.id,
      before: slideSummary(b + 1, unit, ` · ${unit.shapes.length} objects`),
      after: ''
    });
  });

  // Deck settings. PowerPoint files do not name a Dynodoc theme, so only an explicit
  // non-default theme in the incoming state replaces the current one.
  const rootPatch: Record<string, unknown> = {};
  if (other.size !== deck.size) rootPatch.slide_size = other.size;
  if (other.theme !== 'office' && other.theme !== theme) rootPatch.theme = themeName(other.theme);
  if (Object.keys(rootPatch).length) {
    state.nodes[rootId].current_fields = { ...state.nodes[rootId].current_fields, ...rootPatch };
    changes.push({
      kind: 'edited',
      nodeId: rootId,
      before: `Slide size ${deck.size} · theme ${themes[theme].label}`,
      after: `Slide size ${other.size} · theme ${themes[themeName(rootPatch.theme ?? theme)].label}`,
      formatting: true
    });
  }
  return { state, ops: stateChanges(base, state), changes, unchanged, anchored: 0, warnings: [] };
}

/** Fraction (0–1) of the file's text boxes and pictures found unchanged in a candidate revision. */
export function slideMatchScore(state: DocumentState, incoming: PushSource) {
  const deck = readDeck(state);
  const key = (s: SlideShape) => stableJson([shapeContent(s), shapeLook(s, deck.theme)]);
  const counts = new Map<string, number>();
  for (const slide of deck.slides)
    for (const s of slide.shapes.filter(meaningful))
      counts.set(key(s), (counts.get(key(s)) || 0) + 1);
  const shapes = readDeck(pushState(incoming, 'presentation')).slides.flatMap((slide) =>
    slide.shapes.filter(meaningful)
  );
  let score = 0;
  for (const s of shapes) {
    const c = counts.get(key(s)) || 0;
    if (c) {
      score++;
      counts.set(key(s), c - 1);
    }
  }
  return shapes.length ? score / shapes.length : 0;
}
