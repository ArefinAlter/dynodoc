/** Portable recovery data is untrusted input, never a credential or an authorship proof. */
import {
  CaptureSession,
  projectCheckpoint,
  diff,
  hosts,
  blockKey,
  operationText,
  assertHostKind,
  type Host,
  type Block,
  type Checkpoint,
  type Change,
  type Ledger,
  type Store
} from './protocol';

export interface Sidecar {
  format: 'dynodoc.provenance-sidecar';
  version: 1;
  host: Host;
  ledger: Ledger;
}
export const SIDECAR_MAX_BYTES = 16 * 1024 * 1024;
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const ulid = /^[0-7][0-9A-HJKMNP-TV-Z]{25}$/;
const hash = /^[0-9a-f]{64}$/;
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value));
function invalid(): never {
  throw new Error(
    'Invalid provenance sidecar. Keep the original export and do not overwrite your queue.'
  );
}
function object(value: unknown, keys?: string[]): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) invalid();
  const result = value as Record<string, unknown>;
  if (keys && Object.keys(result).some((key) => !keys.includes(key))) invalid();
  return result;
}
function string(value: unknown, pattern?: RegExp): string {
  if (typeof value !== 'string' || (pattern && !pattern.test(value))) invalid();
  return value;
}
function array(value: unknown, limit: number): unknown[] {
  if (!Array.isArray(value) || value.length > limit) invalid();
  return value;
}
function checkpoint(value: unknown): Checkpoint {
  const c = object(value, [
    'format',
    'version',
    'document_id',
    'title',
    'kind',
    'through_seq',
    'chain_hash',
    'state',
    'proposals'
  ]);
  string(c.document_id, uuid);
  string(c.title);
  const state = object(c.state, ['nodes', 'comments', 'suggestions', 'removed_choices']);
  const nodes = object(state.nodes);
  if (state.comments !== undefined) array(state.comments, 20000);
  if (state.suggestions !== undefined) object(state.suggestions);
  if (state.removed_choices !== undefined)
    for (const id of array(state.removed_choices, 10000)) string(id, ulid);
  if (Object.keys(nodes).length > 10000) invalid();
  for (const [id, value] of Object.entries(nodes)) {
    const node = object(value, [
      'id',
      'type',
      'deleted',
      'pos',
      'parent_id',
      'current_fields',
      'var_name'
    ]);
    if (string(node.id, ulid) !== id || typeof node.deleted !== 'boolean') invalid();
    string(node.type);
    string(node.pos);
    if (node.parent_id !== null) string(node.parent_id, ulid);
    if (node.var_name !== undefined && node.var_name !== null) string(node.var_name);
    object(node.current_fields);
  }
  if (c.proposals !== undefined)
    for (const value of array(c.proposals, 100)) {
      const proposal = object(value, ['id', 'status']);
      string(proposal.id, uuid);
      if (!['draft', 'open', 'merged', 'declined', 'withdrawn'].includes(string(proposal.status)))
        invalid();
    }
  const result = c as unknown as Checkpoint;
  projectCheckpoint(result);
  return result;
}
function blocks(value: unknown, base: Block[]): Block[] {
  const result = array(value, 2000).map((value) => {
    const b = object(value, ['id', 'text', 'field']);
    return {
      id: string(b.id, ulid),
      text: string(b.text),
      ...(b.field !== undefined ? { field: string(b.field) } : {})
    };
  });
  diff(base, result);
  return result;
}
function changes(value: unknown, base: Block[], limit = 20000): Change[] {
  const known = new Set(base.map(blockKey)),
    ids = new Set<string>();
  return array(value, limit).map((value) => {
    const c = object(value, ['id', 'observed_at', 'operation']),
      id = string(c.id, uuid);
    if (ids.has(id)) invalid();
    ids.add(id);
    const time = string(c.observed_at);
    if (
      !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/.test(time) ||
      !Number.isFinite(Date.parse(time))
    )
      invalid();
    const op = object(c.operation, ['type', 'node_id', 'field', 'value']);
    if (
      op.type !== 'FieldEdited' ||
      !known.has(blockKey({ id: string(op.node_id, ulid), field: string(op.field) }))
    )
      invalid();
    if (op.field !== 'content') {
      if (!(op.field === 'text' || /^cell_\d{1,2}$/.test(string(op.field)))) invalid();
      string(op.value);
      return c as unknown as Change;
    }
    const paragraph = object(op.value, ['type', 'content']);
    if (paragraph.type !== 'paragraph') invalid();
    for (const value of array(paragraph.content, 20000)) {
      const text = object(value, ['type', 'text']);
      if (text.type !== 'text') invalid();
      string(text.text);
    }
    return c as unknown as Change;
  });
}
function replay(base: Block[], changes: Change[]): Block[] {
  const text = new Map(base.map((b) => [blockKey(b), b.text]));
  for (const change of changes)
    text.set(
      blockKey({ id: change.operation.node_id, field: change.operation.field }),
      operationText(change.operation)
    );
  return base.map((b) => ({ ...b, text: text.get(blockKey(b))! }));
}
function equal(a: unknown, b: unknown): boolean {
  // All compared values here are projected/typed, with object-key order normalized below.
  if (Array.isArray(a) && Array.isArray(b))
    return a.length === b.length && a.every((v, i) => equal(v, b[i]));
  if (a && b && typeof a === 'object' && typeof b === 'object') {
    const left = Object.entries(a),
      right = Object.keys(b);
    return (
      left.length === right.length &&
      left.every(
        ([key, value]) => Object.hasOwn(b, key) && equal(value, (b as Record<string, unknown>)[key])
      )
    );
  }
  return a === b;
}
export function parseSidecar(text: string): Sidecar {
  if (new TextEncoder().encode(text).length > SIDECAR_MAX_BYTES)
    throw new Error('Provenance recovery files must be at most 16 MiB.');
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    invalid();
  }
  const s = object(raw, ['format', 'version', 'host', 'ledger']);
  if (
    s.format !== 'dynodoc.provenance-sidecar' ||
    s.version !== 1 ||
    !hosts.includes(string(s.host) as Host)
  )
    invalid();
  const l = object(s.ledger, [
    'version',
    'client_id',
    'checkpoint',
    'seen',
    'changes',
    'pending',
    'awaiting',
    'sentBlocks',
    'archives'
  ]);
  if (l.version !== 1) invalid();
  string(l.client_id, uuid);
  const c = checkpoint(l.checkpoint),
    base = projectCheckpoint(c),
    seen = blocks(l.seen, base),
    // The 20,000 server limit applies to a frozen upload, not all later typing.
    // The sidecar byte cap bounds the complete recoverable observation history.
    observed = changes(l.changes, base, Number.MAX_SAFE_INTEGER);
  assertHostKind(s.host as Host, c.kind);
  if (!equal(replay(base, observed), seen)) invalid();
  for (const value of array(l.archives, 1000)) {
    const archive = object(value, ['checkpoint', 'changes']),
      previous = checkpoint(archive.checkpoint);
    if (previous.document_id !== c.document_id || previous.through_seq > c.through_seq) invalid();
    changes(archive.changes, projectCheckpoint(previous), Number.MAX_SAFE_INTEGER);
  }
  if (l.pending !== null) {
    const p = object(l.pending, ['name', 'note', 'submit', 'bundle']),
      b = object(p.bundle, [
        'format',
        'version',
        'bundle_id',
        'client_id',
        'document_id',
        'base_seq',
        'base_chain_hash',
        'capture',
        'changes'
      ]);
    if (
      !string(p.name).trim() ||
      string(p.name).length > 200 ||
      string(p.note).length > 4000 ||
      typeof p.submit !== 'boolean'
    )
      invalid();
    string(b.bundle_id, uuid);
    if (
      b.format !== 'dynodoc.change-bundle' ||
      b.version !== 1 ||
      b.client_id !== l.client_id ||
      b.document_id !== c.document_id ||
      b.base_seq !== c.through_seq ||
      b.base_chain_hash !== c.chain_hash
    )
      invalid();
    const capture = object(b.capture, ['host', 'mode', 'document_name']);
    if (capture.host !== s.host || capture.mode !== 'observed_snapshot') invalid();
    string(capture.document_name);
    const sent = changes(b.changes, base);
    if (
      !sent.length ||
      !equal(sent, observed.slice(0, sent.length)) ||
      !equal(replay(base, sent), blocks(l.sentBlocks, base)) ||
      l.awaiting !== null
    )
      invalid();
  } else if (l.awaiting !== null) {
    const receipt = object(l.awaiting, [
      'id',
      'bundle_id',
      'digest',
      'uploaded_by',
      'submitted',
      'revision',
      'edits',
      'relation'
    ]);
    string(receipt.id, uuid);
    string(receipt.bundle_id, uuid);
    string(receipt.digest, hash);
    string(receipt.uploaded_by, uuid);
    if (typeof receipt.submitted !== 'boolean' || !observed.length) invalid();
    for (const key of ['revision', 'edits'])
      if (
        receipt[key] !== undefined &&
        (!Number.isSafeInteger(receipt[key]) || (receipt[key] as number) < 0)
      )
        invalid();
    if (receipt.relation !== undefined) object(receipt.relation);
    const sent = blocks(l.sentBlocks, base);
    // Acknowledged uploads are a prefix of observations; late typing stays separate.
    const current = clone(base);
    let matches = false;
    const index = new Map(current.map((b, i) => [blockKey(b), i]));
    for (const change of observed) {
      current[
        index.get(blockKey({ id: change.operation.node_id, field: change.operation.field }))!
      ].text = operationText(change.operation);
      if (equal(current, sent)) {
        matches = true;
        break;
      }
    }
    if (!matches) invalid();
  } else if (l.sentBlocks !== null) invalid();
  return clone(s as unknown as Sidecar);
}
export function exportSidecar(session: CaptureSession): Sidecar {
  return {
    format: 'dynodoc.provenance-sidecar',
    version: 1,
    host: session.host,
    ledger: session.export()
  };
}
/** Validate before host identity binding; restoring never rewrites paragraph text. */
export function recoveryLedger(
  sidecar: Sidecar,
  remote: Checkpoint,
  host: Host,
  verifiedBase: Checkpoint = remote
): Ledger {
  const s = parseSidecar(JSON.stringify(sidecar)),
    c = s.ledger.checkpoint;
  projectCheckpoint(remote);
  if (s.host !== host)
    throw new Error(
      'Restore this queue in its original editor. Use reviewed team checkpoints to exchange changes between Word and Google Docs.'
    );
  if (
    c.document_id !== remote.document_id ||
    c.through_seq > remote.through_seq ||
    (c.through_seq === remote.through_seq && c.chain_hash !== remote.chain_hash)
  )
    throw new Error('Recovery belongs to another document or history.');
  if (
    verifiedBase.document_id !== c.document_id ||
    verifiedBase.through_seq !== c.through_seq ||
    verifiedBase.chain_hash !== c.chain_hash ||
    !equal(projectCheckpoint(verifiedBase), projectCheckpoint(c))
  )
    throw new Error(
      'The saved starting revision must match a checkpoint retrieved from Dynodoc before recovery.'
    );
  diff(projectCheckpoint(c), projectCheckpoint(remote));
  return s.ledger;
}
export async function restoreCapture(
  store: Store,
  sidecar: Sidecar,
  remote: Checkpoint,
  local: Block[],
  host: Host,
  verifiedBase: Checkpoint = remote
): Promise<CaptureSession> {
  const ledger = recoveryLedger(sidecar, remote, host, verifiedBase);
  if (await store.load())
    throw new Error(
      'This working copy already has a saved queue. Export it and use a fresh copy; recovery cannot replace it.'
    );
  // Additional typing after the export is captured separately once restoration succeeds.
  diff(ledger.seen, local);
  await store.save(ledger);
  return CaptureSession.connect(store, remote, local, host);
}
