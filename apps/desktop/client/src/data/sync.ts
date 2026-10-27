// Syncing a project on this computer with the same project on a server (decision 020).
//
// Both sides are Dynodoc engines, and a file is the same thing on both: nodes with fields. Each
// node has an id that the engine never reuses, so a file pushed from here to a server (or pulled
// the other way) keeps its node ids, and from then on the two copies can be compared node by node.
// This module does the comparing and the writing:
//
//   - a file the other side does not have is created there, from the nodes of the side that has it;
//   - for a file both have, the last time they agreed is remembered (the revision of each side), the
//     content of each side at that time is read (`documents/{id}/state?through_seq=`), and what each
//     side changed since is worked out per node and per field (a three-way merge);
//   - changes that do not touch the same field of the same node go to the other side as ordinary
//     batches of operations, the same ones the web workspace writes; the history of each side stays
//     its own hash chain and verifies on its own;
//   - changes to the same field on both sides (or a node deleted on one side and edited on the
//     other) are conflicts: nothing is written for that file until a person chooses a side for each.
//
// What does not travel: the other side's revision messages, comments, change requests and
// discussions (they stay where they were made), and the original files uploaded with a document. A
// server whose rules do not let the person write to the main line gets the changes as a change
// request instead (`sendForReview`), never by another route.
//
// No network here: every request goes through a Transport (the app's commands, or a fake in tests).
import type { Reply, Transport } from './remote.ts';
import type { Nodes, RNode } from './remote-map.ts';

export type Op = Record<string, unknown>;

export interface LinkFile {
  remoteDocId: string;
  /** The revision of each side when the two last had the same content. */
  localSeq: number;
  remoteSeq: number;
  /** Changes of this file were sent to the server as a change request that is not merged yet. */
  pending?: { requestId: string; localSeq: number; at: number };
}

export interface Link {
  accountId: string;
  remoteProjectId: string;
  remoteName: string;
  linkedAt: number;
  lastSyncedAt: number | null;
  /** By the id of the file on this computer. */
  files: Record<string, LinkFile>;
}

export type Links = Record<string, Link>;

export class SyncError extends Error {
  status: number;
  constructor(message: string, status = 0) {
    super(message);
    this.status = status;
  }
}

// ---- reading ---------------------------------------------------------------------------------------------------

function body(r: Reply, what: string): any {
  if (r.status >= 200 && r.status < 300) return r.body;
  const said = r.body && typeof r.body === 'object' && typeof r.body.error === 'string' ? r.body.error : '';
  const text =
    r.status === 401 ? 'Your session on the server has ended. Sign in again.' :
    r.status === 403 ? (said && said !== 'forbidden' ? said : `Your role does not allow this (${what}).`) :
    r.status === 409 ? 'It changed on the other side while syncing. Sync again.' :
    r.status === 429 ? 'The server asks to slow down. Try again in a moment.' :
    said || `Could not ${what} (${r.status}).`;
  throw new SyncError(text, r.status);
}

export interface FileInfo {
  id: string;
  title: string;
  kind: string;
  latest: number;
}

const MAX_FILES = 500;

/** The files of a project, all pages. */
export async function listFiles(t: Transport, projectId: string): Promise<FileInfo[]> {
  const out: FileInfo[] = [];
  for (let offset = 0; offset < MAX_FILES; ) {
    const page = body(await t.apiGet(`projects/${projectId}/files${offset ? `?offset=${offset}` : ''}`), 'list the files');
    const items: any[] = page.items ?? [];
    const known = new Set(out.map((f) => f.id));
    const fresh = items.filter((f) => !known.has(String(f.id)));
    for (const f of fresh) out.push({ id: String(f.id), title: String(f.title ?? 'Untitled'), kind: String(f.kind ?? f.settings?.kind ?? 'document'), latest: Number(f.latest_seq ?? 0) });
    // A short page is the last one; a page with nothing new means the server ignored the offset.
    if (!fresh.length || items.length < 20) break;
    offset += items.length;
  }
  return out;
}

export interface Doc {
  nodes: Nodes;
  seq: number;
  title: string;
  kind: string;
}

export async function readDoc(t: Transport, id: string): Promise<Doc> {
  const d = body(await t.apiGet(`documents/${id}`), 'read a file');
  return { nodes: (d.state?.nodes ?? {}) as Nodes, seq: Number(d.latest_seq ?? 0), title: String(d.title ?? 'Untitled'), kind: String(d.settings?.kind ?? 'document') };
}

async function nodesThrough(t: Transport, id: string, seq: number): Promise<Nodes> {
  const d = body(await t.apiGet(`documents/${id}/state?through_seq=${seq}`), 'read an earlier revision');
  return (d.state?.nodes ?? {}) as Nodes;
}

// ---- comparing -------------------------------------------------------------------------------------------------

const fieldsOf = (n: RNode): Record<string, unknown> => n.current_fields ?? n.fields ?? {};

/** JSON with sorted keys, so equal values compare equal whatever order they were written in. */
export function canon(v: unknown): string {
  if (v === undefined) return 'null';
  if (v === null || typeof v !== 'object') return JSON.stringify(v);
  if (Array.isArray(v)) return `[${v.map(canon).join(',')}]`;
  const o = v as Record<string, unknown>;
  return `{${Object.keys(o).sort().map((k) => `${JSON.stringify(k)}:${canon(o[k])}`).join(',')}}`;
}

const same = (a: unknown, b: unknown): boolean => canon(a) === canon(b);

function depth(nodes: Nodes, n: RNode): number {
  let d = 0;
  let cur: RNode | undefined = n;
  const seen = new Set<string>();
  while (cur && cur.parent_id && !seen.has(cur.id)) {
    seen.add(cur.id);
    d += 1;
    cur = nodes[cur.parent_id];
  }
  return d;
}

/** The operations that turn the nodes `from` into the nodes `to`: new nodes (parents first),
 *  deletions and restorations, moves, and changed fields. Nodes are never removed from a file, so
 *  a node only in `from` is left alone. */
export function diffNodes(from: Nodes, to: Nodes): Op[] {
  const created: RNode[] = [];
  const edits: Op[] = [];
  for (const n of Object.values(to)) {
    const old = from[n.id];
    if (!old) {
      if (!n.deleted) created.push(n);
      continue;
    }
    if (Boolean(n.deleted) !== Boolean(old.deleted)) edits.push({ type: n.deleted ? 'NodeDeleted' : 'NodeRestored', node_id: n.id });
    if ((n.parent_id ?? null) !== (old.parent_id ?? null) || n.pos !== old.pos) edits.push({ type: 'NodeMoved', node_id: n.id, new_parent_id: n.parent_id ?? null, new_pos: n.pos });
    const a = fieldsOf(old);
    const b = fieldsOf(n);
    for (const key of [...new Set([...Object.keys(a), ...Object.keys(b)])].sort()) {
      if (!same(a[key], b[key])) edits.push({ type: 'FieldEdited', node_id: n.id, field: key, value: key in b ? b[key] : null });
    }
  }
  created.sort((x, y) => depth(to, x) - depth(to, y) || (x.pos < y.pos ? -1 : x.pos > y.pos ? 1 : x.id < y.id ? -1 : 1));
  const makes: Op[] = created.map((n) => ({ type: 'NodeCreated', node_id: n.id, node_type: n.type, parent_id: n.parent_id ?? null, pos: n.pos, fields: { ...fieldsOf(n) } }));
  return [...makes, ...edits];
}

interface Change {
  node: string;
  value: unknown;
}

/** What `now` changed against `base`, by key: `new:<node>`, `del:<node>`, `move:<node>` and
 *  `f:<node>:<field>`. */
export function changesOf(base: Nodes, now: Nodes): Map<string, Change> {
  const out = new Map<string, Change>();
  for (const n of Object.values(now)) {
    const old = base[n.id];
    if (!old) {
      if (!n.deleted) out.set(`new:${n.id}`, { node: n.id, value: { type: n.type, parent: n.parent_id ?? null, pos: n.pos, fields: fieldsOf(n) } });
      continue;
    }
    if (Boolean(n.deleted) !== Boolean(old.deleted)) out.set(`del:${n.id}`, { node: n.id, value: Boolean(n.deleted) });
    if ((n.parent_id ?? null) !== (old.parent_id ?? null) || n.pos !== old.pos) out.set(`move:${n.id}`, { node: n.id, value: { parent: n.parent_id ?? null, pos: n.pos } });
    const a = fieldsOf(old);
    const b = fieldsOf(n);
    for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
      if (!same(a[key], b[key])) out.set(`f:${n.id}:${key}`, { node: n.id, value: key in b ? b[key] : null });
    }
  }
  return out;
}

export interface Conflict {
  /** What is in conflict: `f:<node>:<field>`, `move:<node>`, `del:<node>` (both deleted or restored it
   *  differently) or `node:<node>` (one side deleted it, the other changed it). */
  key: string;
  nodeId: string;
  label: string;
  local: string;
  remote: string;
  pick?: 'local' | 'remote';
}

export type Picks = Record<string, 'local' | 'remote'>;

function clone(nodes: Nodes): Nodes {
  const out: Nodes = {};
  for (const [k, v] of Object.entries(nodes)) out[k] = { ...v, current_fields: { ...fieldsOf(v) } };
  return out;
}

function put(nodes: Nodes, key: string, c: Change): void {
  const [kind, id, ...rest] = key.split(':');
  if (kind === 'new') {
    const v = c.value as { type: string; parent: string | null; pos: string; fields: Record<string, unknown> };
    nodes[id] = { id, type: v.type, parent_id: v.parent, pos: v.pos, current_fields: { ...v.fields }, deleted: false };
  } else if (!nodes[id]) {
    return;
  } else if (kind === 'del') {
    nodes[id] = { ...nodes[id], deleted: Boolean(c.value) };
  } else if (kind === 'move') {
    const v = c.value as { parent: string | null; pos: string };
    nodes[id] = { ...nodes[id], parent_id: v.parent, pos: v.pos };
  } else if (kind === 'f') {
    nodes[id] = { ...nodes[id], current_fields: { ...fieldsOf(nodes[id]), [rest.join(':')]: c.value } };
  }
}

const keysOfNode = (m: Map<string, Change>, id: string): string[] => [...m.entries()].filter(([, c]) => c.node === id).map(([k]) => k);

/** The nodes both sides agree on after taking both sides' changes, and what stays in conflict.
 *  `base` is the content both sides had when they last agreed; `picks` are a person's choices. */
export function merge3(base: Nodes, local: Map<string, Change>, remote: Map<string, Change>, picks: Picks, describe: (nodeId: string, side: 'local' | 'remote', key: string) => string, labelOf: (nodeId: string) => string): { merged: Nodes; conflicts: Conflict[] } {
  const conflicts: Conflict[] = [];
  const skipLocal = new Set<string>();
  const skipRemote = new Set<string>();
  // A node one side deleted and the other changed: the person chooses which node survives whole.
  const nodeConflicts = new Set<string>();
  const deletedOn = (m: Map<string, Change>) => [...m.entries()].filter(([k, c]) => k.startsWith('del:') && c.value === true).map(([, c]) => c.node);
  for (const [mine, theirs] of [[local, remote], [remote, local]] as const) {
    for (const id of deletedOn(mine)) {
      const touched = keysOfNode(theirs, id).filter((k) => k.startsWith('f:') || k.startsWith('move:'));
      if (touched.length && !theirs.has(`del:${id}`)) nodeConflicts.add(id);
    }
  }
  for (const id of nodeConflicts) {
    const key = `node:${id}`;
    const pick = picks[key];
    conflicts.push({ key, nodeId: id, label: labelOf(id), local: describe(id, 'local', key), remote: describe(id, 'remote', key), pick });
    if (pick === 'local') for (const k of keysOfNode(remote, id)) skipRemote.add(k);
    else if (pick === 'remote') for (const k of keysOfNode(local, id)) skipLocal.add(k);
    else {
      for (const k of keysOfNode(remote, id)) skipRemote.add(k);
      for (const k of keysOfNode(local, id)) skipLocal.add(k);
    }
  }
  const merged = clone(base);
  const keys = new Set([...local.keys(), ...remote.keys()]);
  const ordered = [...keys].sort((a, b) => (a.startsWith('new:') ? 0 : 1) - (b.startsWith('new:') ? 0 : 1));
  for (const key of ordered) {
    const l = skipLocal.has(key) ? undefined : local.get(key);
    const r = skipRemote.has(key) ? undefined : remote.get(key);
    if (l && r && !same(l.value, r.value)) {
      const pick = picks[key];
      conflicts.push({ key, nodeId: l.node, label: labelOf(l.node), local: describe(l.node, 'local', key), remote: describe(l.node, 'remote', key), pick });
      if (pick === 'local') put(merged, key, l);
      else if (pick === 'remote') put(merged, key, r);
      continue;
    }
    const one = l ?? r;
    if (one) put(merged, key, one);
  }
  return { merged, conflicts };
}

// ---- describing a node to a person -----------------------------------------------------------------------------

function plain(v: unknown): string {
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  if (!v || typeof v !== 'object') return '';
  const o = v as { text?: string; content?: unknown[]; type?: string };
  if (typeof o.text === 'string') return o.text;
  return Array.isArray(o.content) ? o.content.map(plain).join(o.type === 'doc' ? '\n' : '') : '';
}

export function nodeText(n: RNode | undefined): string {
  if (!n) return '';
  const f = fieldsOf(n);
  const label = typeof f.label === 'string' ? f.label : '';
  const text = label || plain(f.text) || plain(f.content) || Object.entries(f).filter(([k]) => k.startsWith('cell_')).map(([, v]) => plain(v)).filter(Boolean).join('  ');
  return text.replace(/\s+/g, ' ').trim();
}

function clip(s: string, n = 160): string {
  return s.length > n ? `${s.slice(0, n - 1)}…` : s;
}

// ---- the plan --------------------------------------------------------------------------------------------------

export type Action = 'same' | 'send' | 'receive' | 'both' | 'conflict' | 'add-remote' | 'add-local' | 'missing-remote' | 'missing-local' | 'waiting' | 'needs-review';

export interface Item {
  action: Action;
  title: string;
  kind: string;
  localId?: string;
  remoteId?: string;
  /** What each side's file looked like when the plan was read. */
  local?: Doc;
  remote?: Doc;
  toRemote: Op[];
  toLocal: Op[];
  conflicts: Conflict[];
  note?: string;
}

export interface Plan {
  items: Item[];
  /** How many files in each state. */
  counts: Record<Action, number>;
}

const ACTIONS: Action[] = ['same', 'send', 'receive', 'both', 'conflict', 'add-remote', 'add-local', 'missing-remote', 'missing-local', 'waiting', 'needs-review'];

function counted(items: Item[]): Plan {
  const counts = Object.fromEntries(ACTIONS.map((a) => [a, 0])) as Record<Action, number>;
  for (const i of items) counts[i.action] += 1;
  return { items, counts };
}

export interface Sides {
  local: Transport;
  remote: Transport;
  localProjectId: string;
}

/** Reads both sides and says, file by file, what a sync would do. Writes nothing. `picks` are the
 *  choices made for conflicts so far (by file, then by conflict key). */
export async function planSync(sides: Sides, link: Link, picks: Record<string, Picks> = {}): Promise<Plan> {
  const [lf, rf] = await Promise.all([listFiles(sides.local, sides.localProjectId), listFiles(sides.remote, link.remoteProjectId)]);
  const items: Item[] = [];
  const claimed = new Set<string>();
  for (const f of lf) {
    const m = link.files[f.id];
    if (!m) {
      const doc = await readDoc(sides.local, f.id);
      items.push({ action: 'add-remote', title: f.title, kind: doc.kind, localId: f.id, local: doc, toRemote: diffNodes({}, doc.nodes), toLocal: [], conflicts: [] });
      continue;
    }
    claimed.add(m.remoteDocId);
    const r = rf.find((x) => x.id === m.remoteDocId);
    if (!r) {
      items.push({ action: 'missing-remote', title: f.title, kind: f.kind, localId: f.id, remoteId: m.remoteDocId, toRemote: [], toLocal: [], conflicts: [], note: 'It is no longer on the server (deleted, moved to another project, or you lost access). It stays here and is not sent again.' });
      continue;
    }
    items.push(await planPair(sides, f, r, m, picks[f.id] ?? {}));
  }
  for (const r of rf) {
    if (claimed.has(r.id)) continue;
    const mapped = Object.entries(link.files).find(([, m]) => m.remoteDocId === r.id);
    if (mapped) {
      items.push({ action: 'missing-local', title: r.title, kind: r.kind, localId: mapped[0], remoteId: r.id, toRemote: [], toLocal: [], conflicts: [], note: 'You removed it from this computer. It stays on the server and is not brought back.' });
      continue;
    }
    const doc = await readDoc(sides.remote, r.id);
    items.push({ action: 'add-local', title: r.title, kind: doc.kind, remoteId: r.id, remote: doc, toRemote: [], toLocal: diffNodes({}, doc.nodes), conflicts: [] });
  }
  return counted(items);
}

async function planPair(sides: Sides, lf: FileInfo, rf: FileInfo, m: LinkFile, picks: Picks): Promise<Item> {
  const base: Item = { action: 'same', title: lf.title, kind: lf.kind, localId: lf.id, remoteId: rf.id, toRemote: [], toLocal: [], conflicts: [] };
  const localMoved = lf.latest !== m.localSeq;
  const remoteMoved = rf.latest !== m.remoteSeq;
  if (!localMoved && !remoteMoved) return base;
  const [l, r] = await Promise.all([readDoc(sides.local, lf.id), readDoc(sides.remote, rf.id)]);
  const lBase = localMoved ? await nodesThrough(sides.local, lf.id, m.localSeq) : l.nodes;
  const rBase = remoteMoved ? await nodesThrough(sides.remote, rf.id, m.remoteSeq) : r.nodes;
  const lc = changesOf(lBase, l.nodes);
  const rc = changesOf(rBase, r.nodes);
  const label = (id: string) => nodeText(l.nodes[id] ?? r.nodes[id]) ? clip(nodeText(l.nodes[id] ?? r.nodes[id]), 60) : `${(l.nodes[id] ?? r.nodes[id])?.type ?? 'item'}`;
  const describe = (id: string, side: 'local' | 'remote', key: string): string => {
    const nodes = side === 'local' ? l.nodes : r.nodes;
    const n = nodes[id];
    if (key.startsWith('node:')) return n?.deleted ? 'Deleted here' : clip(nodeText(n)) || 'Changed here';
    if (key.startsWith('del:')) return n?.deleted ? 'Deleted' : 'Kept';
    if (key.startsWith('move:')) return 'Moved to a different place';
    const field = key.split(':').slice(2).join(':');
    const v = n ? fieldsOf(n)[field] : undefined;
    return clip(plain(v) || (v === undefined || v === null ? '(empty)' : canon(v)));
  };
  const { merged, conflicts } = merge3(lBase, lc, rc, picks, describe, label);
  const out: Item = { ...base, local: l, remote: r, conflicts };
  if (conflicts.some((c) => !c.pick)) return { ...out, action: 'conflict', note: `${conflicts.filter((c) => !c.pick).length} change${conflicts.filter((c) => !c.pick).length === 1 ? ' is' : 's are'} in conflict. Choose a side for each.` };
  const toRemote = diffNodes(r.nodes, merged);
  const toLocal = diffNodes(l.nodes, merged);
  out.toRemote = toRemote;
  out.toLocal = toLocal;
  if (m.pending && m.pending.localSeq === lf.latest && toRemote.length && !toLocal.length) return { ...out, action: 'waiting', note: 'The changes are waiting for review on the server.' };
  out.action = toRemote.length && toLocal.length ? 'both' : toRemote.length ? 'send' : toLocal.length ? 'receive' : 'same';
  return out;
}

// ---- writing ---------------------------------------------------------------------------------------------------

const MAX_OPS_PER_BATCH = 400;
const MAX_BYTES_PER_BATCH = 300 * 1024;

export function chunkOps(ops: Op[]): Op[][] {
  const out: Op[][] = [];
  let cur: Op[] = [];
  let bytes = 0;
  for (const op of ops) {
    const n = JSON.stringify(op).length;
    if (cur.length && (cur.length >= MAX_OPS_PER_BATCH || bytes + n > MAX_BYTES_PER_BATCH)) {
      out.push(cur);
      cur = [];
      bytes = 0;
    }
    cur.push(op);
    bytes += n;
  }
  if (cur.length) out.push(cur);
  return out;
}

/** Writes operations to a file's main line in batches. Returns the last revision. */
export async function writeOps(t: Transport, docId: string, ops: Op[], baseSeq: number): Promise<number> {
  let seq = baseSeq;
  for (const part of chunkOps(ops)) {
    const res = body(await t.apiSend('POST', `documents/${docId}/batch`, { ops: part, base_seq: seq }), 'write the changes');
    const events = (res.events ?? []) as { seq: number }[];
    if (events.length) seq = events[events.length - 1].seq;
  }
  return seq;
}

async function createDoc(t: Transport, projectId: string, title: string, kind: string, ops: Op[]): Promise<{ id: string; seq: number }> {
  const doc = body(await t.apiSend('POST', 'documents', { title: title.slice(0, 200) || 'Untitled' }), 'create a file');
  const id = String(doc.id);
  body(await t.apiSend('POST', `documents/${id}/metadata`, { settings: { kind } }), 'describe the file');
  const seq = await writeOps(t, id, ops, 0);
  body(await t.apiSend('POST', `documents/${id}/location`, { space_id: projectId }), 'put the file in the project');
  return { id, seq };
}

export interface Done {
  sent: number;
  received: number;
  created: number;
  unchanged: number;
  held: { title: string; why: string }[];
  /** Files needing a person: conflicts and files the server wants a review for. */
  needs: number;
}

/** Does what a plan says, for every file that has nothing in conflict, and updates the link's marks
 *  (a new object is not made: `link` is changed). Files in conflict, or that the server's rules
 *  want reviewed, are left as they are. A failure for one file is reported and does not stop the
 *  others. */
export interface Notes {
  host: string;
  /** Called after a write, to keep a message for the revisions it made (the engine records changes, not messages). */
  onNote?: (docId: string, fromSeq: number, toSeq: number, summary: string, description: string, on: 'local' | 'remote') => Promise<void>;
}

export async function applyPlan(sides: Sides, link: Link, plan: Plan, o: Notes = { host: 'the server' }): Promise<Done> {
  const done: Done = { sent: 0, received: 0, created: 0, unchanged: 0, held: [], needs: 0 };
  for (const item of plan.items) {
    try {
      if (item.action === 'same' && item.localId && item.remoteId) {
        // Nothing to write; the two files agree at their latest revisions.
        const m = link.files[item.localId];
        if (m) {
          const [lf, rf] = [item.local?.seq, item.remote?.seq];
          if (lf !== undefined) m.localSeq = lf;
          if (rf !== undefined) m.remoteSeq = rf;
        }
        done.unchanged += 1;
      } else if (item.action === 'add-remote' && item.local && item.localId) {
        const made = await createDoc(sides.remote, link.remoteProjectId, item.title, item.kind, item.toRemote);
        link.files[item.localId] = { remoteDocId: made.id, localSeq: item.local.seq, remoteSeq: made.seq };
        await o.onNote?.(made.id, 1, made.seq, 'Sent from a computer', `Created from the file "${item.title}" on another computer.`, 'remote');
        done.created += 1;
      } else if (item.action === 'add-local' && item.remote && item.remoteId) {
        const made = await createDoc(sides.local, sides.localProjectId, item.title, item.kind, item.toLocal);
        link.files[made.id] = { remoteDocId: item.remoteId, localSeq: made.seq, remoteSeq: item.remote.seq };
        await o.onNote?.(made.id, 1, made.seq, `Received from ${o.host}`.slice(0, 200), `Copied from the file "${item.title}" on ${o.host}.`, 'local');
        done.created += 1;
      } else if ((item.action === 'send' || item.action === 'receive' || item.action === 'both') && item.localId && item.remoteId && item.local && item.remote) {
        const m = link.files[item.localId];
        let rSeq = item.remote.seq;
        let lSeq = item.local.seq;
        if (item.toRemote.length) {
          try {
            rSeq = await writeOps(sides.remote, item.remoteId, item.toRemote, item.remote.seq);
          } catch (e) {
            if (e instanceof SyncError && e.status === 403) {
              done.needs += 1;
              item.action = 'needs-review';
              item.note = 'The server’s rules do not let you change this file directly. Send the changes for review.';
              done.held.push({ title: item.title, why: item.note });
              continue;
            }
            throw e;
          }
          done.sent += 1;
          await o.onNote?.(item.remoteId, item.remote.seq + 1, rSeq, 'Synced from a computer', `${item.toRemote.length} change${item.toRemote.length === 1 ? '' : 's'} from the copy on another computer.`, 'remote');
        }
        if (item.toLocal.length) {
          lSeq = await writeOps(sides.local, item.localId, item.toLocal, item.local.seq);
          done.received += 1;
          await o.onNote?.(item.localId, item.local.seq + 1, lSeq, `Synced from ${o.host}`.slice(0, 200), `${item.toLocal.length} change${item.toLocal.length === 1 ? '' : 's'} from ${o.host}.`, 'local');
        }
        m.localSeq = lSeq;
        m.remoteSeq = rSeq;
        delete m.pending;
      } else if (item.action === 'conflict') {
        done.needs += 1;
        done.held.push({ title: item.title, why: item.note ?? 'In conflict.' });
      } else if (item.action === 'waiting' || item.action === 'missing-remote' || item.action === 'missing-local') {
        done.held.push({ title: item.title, why: item.note ?? '' });
      }
    } catch (e) {
      done.held.push({ title: item.title, why: e instanceof Error ? e.message : String(e) });
    }
  }
  link.lastSyncedAt = Date.now();
  return done;
}

/** Sends what changed in a file to the server as a change request (for a server whose rules do not
 *  allow writing to the main line). Nothing is written to either main line. */
export async function sendForReview(sides: Sides, link: Link, item: Item, who: string): Promise<{ requestId: string }> {
  if (!item.localId || !item.remoteId || !item.remote || !item.local) throw new SyncError('That file is not linked.');
  const m = link.files[item.localId];
  const made = body(
    await sides.remote.apiSend('POST', `documents/${item.remoteId}/change-requests`, {
      name: `Changes from ${who}`.slice(0, 120),
      note: 'Sent by a sync from a computer. Review the changes, then merge.',
      base_seq: item.remote.seq,
      ops: item.toRemote,
      submit: true,
    }),
    'send the changes for review',
  );
  const requestId = String(made.id ?? made.request?.id ?? '');
  if (m) m.pending = { requestId, localSeq: item.local.seq, at: Date.now() };
  return { requestId };
}

/** Starts a link: nothing is read or written yet. */
export function newLink(accountId: string, remoteProjectId: string, remoteName: string): Link {
  return { accountId, remoteProjectId, remoteName, linkedAt: Date.now(), lastSyncedAt: null, files: {} };
}
