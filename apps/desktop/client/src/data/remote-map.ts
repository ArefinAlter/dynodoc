// How a real Dynodoc server's data maps to the client's model, and back. Pure functions, no
// network, tested with `node --test` (client/test/remote-map.test.mjs).
//
// The engine stores a file as nodes (engine-core materializer): a root `form`, and under it
//  - a text document: `item` paragraphs, whose plain text is the `label` field (the web editor
//    writes it beside the rich `content`);
//  - a spreadsheet: `section` sheets (`kind: 'sheet'`), `item` rows ordered by `pos`, cells in
//    `cell_<column>` fields;
//  - a presentation: `section` slides (`kind: 'slide'`), `item` shapes with a `role` and a `text`
//    field, speaker notes in the slide's `notes` field.
// The client compares BLOCKS (lib/diff.ts): one paragraph, one non-empty cell, one shape's text,
// one slide's notes. A block's id is the node id, or `<node>:<field>` for a cell or the notes.
import type { Block, Edit } from '../lib/diff.ts';
import type { FileKind } from './types.ts';

export interface RNode {
  id: string;
  parent_id?: string | null;
  type: string;
  pos: string;
  current_fields?: Record<string, unknown>;
  fields?: Record<string, unknown>;
  deleted?: boolean;
}

export type Nodes = Record<string, RNode>;

export interface REvent {
  id: string;
  seq: number;
  type: string;
  actor_id: string;
  payload: Record<string, unknown>;
  content_hash: number[] | string;
  prev_chain_hash: number[] | string;
  chain_hash: number[] | string;
  created_at: string;
}

const fieldsOf = (n: RNode): Record<string, unknown> => n.current_fields ?? n.fields ?? {};
const byPos = (a: RNode, b: RNode) => (a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : a.id < b.id ? -1 : 1);

export function hex(v: number[] | string | undefined | null): string {
  if (!v) return '';
  if (typeof v === 'string') return v;
  return v.map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** Plain text of a TipTap/ProseMirror node. */
export function textOf(v: unknown): string {
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  if (!v || typeof v !== 'object') return '';
  const o = v as { text?: string; content?: unknown[]; type?: string };
  if (typeof o.text === 'string') return o.text;
  return Array.isArray(o.content) ? o.content.map(textOf).join(o.type === 'doc' ? '\n' : '') : '';
}

function children(nodes: Nodes, parent: string | null | undefined): RNode[] {
  return Object.values(nodes).filter((n) => !n.deleted && (n.parent_id ?? null) === (parent ?? null)).sort(byPos);
}

export function rootOf(nodes: Nodes): RNode | undefined {
  return children(nodes, null)[0];
}

export const columnLetter = (i: number): string => {
  let s = '';
  let n = i;
  do {
    s = String.fromCharCode(65 + (n % 26)) + s;
    n = Math.floor(n / 26) - 1;
  } while (n >= 0);
  return s;
};

const cellIndex = (field: string): number | null => {
  const m = /^cell_(\d{1,4})$/.exec(field);
  return m ? Number(m[1]) : null;
};

const cellText = (v: unknown): string => (v === null || v === undefined ? '' : typeof v === 'object' ? textOf(v) : String(v));

/** Where a block lives in the engine's nodes: the node, the field that holds its text, and
 *  whether it is a node of its own (a paragraph or a shape) or a field of a node (a cell, notes). */
export interface Slot {
  node: string;
  field: string;
  own: boolean;
  kind: 'paragraph' | 'cell' | 'shape' | 'notes';
  parent?: string;
  pos?: string;
  heading?: boolean;
}

export interface Mapped {
  blocks: Block[];
  slots: Record<string, Slot>;
}

/** The blocks of a file's nodes, in reading order. */
export function blocksOf(kind: FileKind | string, nodes: Nodes): Mapped {
  const blocks: Block[] = [];
  const slots: Record<string, Slot> = {};
  const root = rootOf(nodes);
  if (!root) return { blocks, slots };
  const top = children(nodes, root.id);
  if (kind === 'spreadsheet') {
    for (const sheet of top) {
      const sf = fieldsOf(sheet);
      if (sf.kind !== 'sheet') continue;
      const name = String(sf.label ?? 'Sheet');
      const rows = children(nodes, sheet.id);
      const header = rows[0] ? fieldsOf(rows[0]) : {};
      rows.forEach((row, r) => {
        const f = fieldsOf(row);
        const cells = Object.keys(f)
          .map((k) => [k, cellIndex(k)] as const)
          .filter((x): x is readonly [string, number] => x[1] !== null)
          .sort((a, b) => a[1] - b[1]);
        for (const [field, c] of cells) {
          const text = cellText(f[field]);
          if (!text) continue;
          const rowName = c > 0 && r > 0 ? cellText(f.cell_0) : '';
          const colName = c > 0 && r > 0 ? cellText(header[field]) : '';
          const what = [rowName, colName].filter(Boolean).join(', ');
          const id = `${row.id}:${field}`;
          blocks.push({ id, label: `${name}!${columnLetter(c)}${r + 1}${what ? ` · ${what}` : ''}`, text });
          slots[id] = { node: row.id, field, own: false, kind: 'cell', parent: sheet.id };
        }
      });
    }
    return { blocks, slots };
  }
  if (kind === 'presentation') {
    let n = 0;
    for (const slide of top) {
      const sf = fieldsOf(slide);
      if (sf.kind !== 'slide') continue;
      n += 1;
      let last = '';
      for (const shape of children(nodes, slide.id)) {
        const f = fieldsOf(shape);
        if (f.kind && f.kind !== 'text') continue;
        const role = String(f.role ?? 'body');
        const part = role === 'title' ? 'Title' : role === 'subtitle' ? 'Subtitle' : role === 'body' ? 'Body' : 'Text';
        blocks.push({ id: shape.id, label: `Slide ${n} · ${part}`, text: cellText(f.text) });
        slots[shape.id] = { node: shape.id, field: 'text', own: true, kind: 'shape', parent: slide.id, pos: shape.pos };
        last = shape.id;
      }
      const notes = cellText(sf.notes);
      if (notes) {
        const id = `${slide.id}:notes`;
        blocks.push({ id, label: `Slide ${n} · Notes`, text: notes });
        slots[id] = { node: slide.id, field: 'notes', own: false, kind: 'notes', parent: root.id };
      } else {
        // An empty notes field is still a place a block can be added (see editsToOps).
        slots[`${slide.id}:notes`] = { node: slide.id, field: 'notes', own: false, kind: 'notes', parent: root.id };
      }
      void last;
    }
    return { blocks, slots };
  }
  // Documents (and anything else): every item under the root, in order, depth first.
  let i = 0;
  const walk = (parent: string) => {
    for (const node of children(nodes, parent)) {
      const f = fieldsOf(node);
      if (node.type === 'item' || node.type === 'paragraph' || node.type === 'clause') {
        i += 1;
        const content = f.content as { type?: string } | undefined;
        const heading = content?.type === 'heading';
        const text = typeof f.label === 'string' ? f.label : textOf(f.content);
        blocks.push({ id: node.id, label: `¶ ${i}${heading ? ' · Heading' : ''}`, text });
        slots[node.id] = { node: node.id, field: 'label', own: true, kind: 'paragraph', parent: parent, pos: node.pos, heading };
      }
      walk(node.id);
    }
  };
  walk(root.id);
  return { blocks, slots };
}

/** Applies one event's payload to a node map (the subset of the engine's operations that
 *  changes what the client shows; comments and suggestions change no block). */
export function applyEvent(nodes: Nodes, payload: Record<string, unknown>): void {
  const type = payload.type;
  const id = String(payload.node_id ?? '');
  if (type === 'NodeCreated') {
    nodes[id] = {
      id,
      parent_id: (payload.parent_id as string | undefined) ?? null,
      type: String(payload.node_type),
      pos: String(payload.pos ?? ''),
      current_fields: { ...((payload.fields as Record<string, unknown>) ?? {}) },
      deleted: false,
    };
  } else if (type === 'FieldEdited' && nodes[id]) {
    nodes[id].current_fields = { ...fieldsOf(nodes[id]), [String(payload.field)]: payload.value };
  } else if (type === 'NodeMoved' && nodes[id]) {
    nodes[id] = { ...nodes[id], parent_id: (payload.new_parent_id as string | undefined) ?? nodes[id].parent_id, pos: String(payload.new_pos ?? nodes[id].pos) };
  } else if (type === 'NodeDeleted' && nodes[id]) {
    nodes[id] = { ...nodes[id], deleted: true };
  } else if (type === 'NodeRestored' && nodes[id]) {
    nodes[id] = { ...nodes[id], deleted: false };
  }
}

export function cloneNodes(nodes: Nodes): Nodes {
  const out: Nodes = {};
  for (const [k, v] of Object.entries(nodes)) out[k] = { ...v, current_fields: { ...fieldsOf(v) } };
  return out;
}

const EDITING = new Set(['NodeCreated', 'FieldEdited', 'RichTextPatched', 'NodeMoved', 'NodeDeleted', 'NodeRestored']);

/** A revision the way the client shows one: a run of events. */
export interface Run {
  from: number;
  to: number;
  actor: string;
  at: number;
  events: REvent[];
}

/** Groups the event log into revisions: the ranges the commit messages name, and the rest by
 *  person and moment (one request of the web editor writes its events within a second). */
export function runsOf(events: REvent[], ranges: { from: number; to: number }[]): Run[] {
  const sorted = [...events].sort((a, b) => a.seq - b.seq);
  const out: (Run & { key: string; last: number })[] = [];
  for (const e of sorted) {
    const at = Date.parse(e.created_at);
    const range = ranges.find((r) => e.seq >= r.from && e.seq <= r.to);
    const key = range ? `range:${range.from}:${range.to}` : `actor:${e.actor_id}:${EDITING.has(e.type) ? 'edit' : 'other'}`;
    const cur = out[out.length - 1];
    if (cur && cur.key === key && (range || at - cur.last < 2000)) {
      cur.to = e.seq;
      cur.last = at;
      cur.events.push(e);
    } else out.push({ key, last: at, from: e.seq, to: e.seq, actor: e.actor_id, at, events: [e] });
  }
  return out.map(({ key: _k, last: _l, ...run }) => run);
}

/** What a run without a commit message is called. */
export function describeRun(run: Run, edits: Edit[], labels: Record<string, string>): string {
  const types = new Set(run.events.map((e) => e.type));
  if (types.has('CommentAdded')) return 'Comment added';
  if (types.has('CommentReplied')) return 'Comment replied to';
  if (types.has('CommentResolved')) return 'Discussion resolved or reopened';
  if (types.has('SuggestionProposed')) return 'Suggestion proposed';
  if (!edits.length) return run.events.length === 1 ? `${run.events[0].type.replace(/([a-z])([A-Z])/g, '$1 $2').toLowerCase()}` : `${run.events.length} changes`;
  if (edits.length === 1) {
    const e = edits[0];
    const label = labels[e.blockId] ?? e.label ?? 'a block';
    return e.op === 'edit' ? `Edit ${label}` : e.op === 'add' ? `Add ${label}` : `Remove ${label}`;
  }
  return `Edit ${edits.length} blocks`;
}

// ---- from the client's edits back to engine operations ---------------------------------------------

export interface OpsContext {
  kind: FileKind | string;
  nodes: Nodes;
  slots: Record<string, Slot>;
  /** Makes a new node id (a ULID). */
  newId: () => string;
}

/** The web editor's midpoint between two position keys (frontend lib/workspace/changes.ts). */
export function between(left: string, right?: string): string {
  let prefix = '';
  for (let i = 0; i < 200; i++) {
    const a = left.charCodeAt(i) || 32;
    const b = (right ?? '').charCodeAt(i) || 126;
    if (b - a > 1) return prefix + String.fromCharCode(Math.floor((a + b) / 2));
    prefix += String.fromCharCode(a);
  }
  return left + 'P';
}

function paragraphContent(text: string, existing: unknown, heading: boolean): Record<string, unknown> {
  const base = existing && typeof existing === 'object' ? { ...(existing as Record<string, unknown>) } : { type: heading ? 'heading' : 'paragraph', ...(heading ? { attrs: { level: 2 } } : {}) };
  return { ...base, content: text ? [{ type: 'text', text }] : [] };
}

export class Unsupported extends Error {}

/** The engine operations that make `edits` true on the given nodes. Each op is checked against
 *  the node map as it goes, so a block that is no longer there is reported, not guessed. */
export function editsToOps(edits: Edit[], ctx: OpsContext): Record<string, unknown>[] {
  const nodes = cloneNodes(ctx.nodes);
  const slots = { ...ctx.slots };
  const ops: Record<string, unknown>[] = [];
  const push = (op: Record<string, unknown>) => {
    ops.push(op);
    applyEvent(nodes, op);
  };
  for (const e of edits) {
    const slot = slots[e.blockId];
    if (e.op === 'edit') {
      if (!slot || !nodes[slot.node] || nodes[slot.node].deleted) throw new Unsupported(`${e.label ?? 'A block'} is no longer in the file.`);
      const f = fieldsOf(nodes[slot.node]);
      const text = e.after ?? '';
      if (slot.kind === 'paragraph') {
        push({ type: 'FieldEdited', node_id: slot.node, field: 'content', value: paragraphContent(text, f.content, Boolean(slot.heading)) });
        push({ type: 'FieldEdited', node_id: slot.node, field: 'label', value: text });
      } else if (slot.kind === 'shape') {
        push({ type: 'FieldEdited', node_id: slot.node, field: 'text', value: text });
        push({ type: 'FieldEdited', node_id: slot.node, field: 'label', value: text.replace(/\s+/g, ' ').trim().slice(0, 120) });
        if (f.role === 'title' && slot.parent) push({ type: 'FieldEdited', node_id: slot.parent, field: 'label', value: text.replace(/\s+/g, ' ').trim().slice(0, 120) || 'Untitled slide' });
      } else {
        push({ type: 'FieldEdited', node_id: slot.node, field: slot.field, value: text });
      }
    } else if (e.op === 'remove') {
      if (!slot || !nodes[slot.node]) continue; // already gone
      if (slot.own) push({ type: 'NodeDeleted', node_id: slot.node });
      else push({ type: 'FieldEdited', node_id: slot.node, field: slot.field, value: '' });
      delete slots[e.blockId];
    } else {
      // add
      const text = e.after ?? '';
      const existing = slots[e.blockId];
      if (existing && !existing.own) {
        // A cell or the notes: the field exists in the node; give it a value.
        push({ type: 'FieldEdited', node_id: existing.node, field: existing.field, value: text });
        continue;
      }
      const after = e.afterId ? slots[e.afterId] : undefined;
      if (ctx.kind === 'document' || !['spreadsheet', 'presentation'].includes(String(ctx.kind))) {
        const root = rootOf(nodes);
        if (!root) throw new Unsupported('The file has no content to add to.');
        const siblings = children(nodes, after?.parent ?? root.id);
        const i = after ? siblings.findIndex((s) => s.id === after.node) : -1;
        const left = i >= 0 ? siblings[i].pos : '';
        const right = siblings[i + 1]?.pos;
        const id = ctx.newId();
        const heading = /Heading/.test(e.label ?? '');
        push({ type: 'NodeCreated', node_id: id, node_type: 'item', parent_id: after?.parent ?? root.id, pos: between(left, right), fields: { label: text, content: paragraphContent(text, undefined, heading) } });
        slots[e.blockId] = { node: id, field: 'label', own: true, kind: 'paragraph', parent: after?.parent ?? root.id, heading };
      } else if (ctx.kind === 'presentation') {
        const slide = after?.kind === 'notes' ? after.node : after?.parent;
        if (!slide || !nodes[slide]) throw new Unsupported('New text needs a slide to go on.');
        const shapes = children(nodes, slide);
        const i = after ? shapes.findIndex((s) => s.id === after.node) : -1;
        const id = ctx.newId();
        push({ type: 'NodeCreated', node_id: id, node_type: 'item', parent_id: slide, pos: between(i >= 0 ? shapes[i].pos : '', shapes[i + 1]?.pos), fields: { kind: 'text', role: 'body', text, x: 6.25, y: 60, w: 87.5, h: 20, style: { size: 20 }, label: text.slice(0, 120) } });
        slots[e.blockId] = { node: id, field: 'text', own: true, kind: 'shape', parent: slide };
      } else {
        throw new Unsupported('A new cell is added in the spreadsheet itself.');
      }
    }
  }
  return ops;
}

/** The ids of the engine nodes an edit touches (for a merge's selected changes). */
export function nodeOf(blockId: string): string {
  const i = blockId.indexOf(':');
  return i < 0 ? blockId : blockId.slice(0, i);
}

/** A conflict key from the engine (`<node>:<field>` or `<node>:$block`) as a client block id. */
export function blockOfKey(key: string, slots: Record<string, Slot>): string {
  const [node, field] = key.split(':');
  if (field && slots[`${node}:${field}`]) return `${node}:${field}`;
  return node;
}

const EXT: Record<string, string> = { document: 'docx', spreadsheet: 'xlsx', presentation: 'pptx' };

/** A file name for a title, as the editors would save it. */
export function filenameOf(title: string, kind: string): string {
  const base = title.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'untitled';
  return `${base}.${EXT[kind] ?? 'docx'}`;
}

/** Stored role names in the client and the engine are the same; the agent policy uses the
 *  product names (contributor, reviewer) for the stored reviewer and approver. */
export const POLICY_ROLE_TO_CLIENT: Record<string, string> = { viewer: 'viewer', contributor: 'reviewer', reviewer: 'approver', editor: 'editor', manager: 'manager', owner: 'owner' };
export const CLIENT_ROLE_TO_POLICY: Record<string, string> = { viewer: 'viewer', reviewer: 'contributor', approver: 'reviewer', editor: 'editor', manager: 'manager', owner: 'owner' };
