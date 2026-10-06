/** Shared by the Word task pane and the Google Docs sidebar. No editor credentials
 * are part of a checkpoint, ledger or portable bundle. Capture is opt-in. */
export interface Node {
  id: string;
  type: string;
  deleted: boolean;
  pos: string;
  parent_id: string | null;
  current_fields: Record<string, unknown>;
}
export interface Checkpoint {
  format: 'dynodoc.checkpoint';
  version: 1;
  document_id: string;
  title: string;
  kind: string;
  through_seq: number;
  chain_hash: string;
  state: { nodes: Record<string, Node> };
  proposals?: { id: string; status: string }[];
}
export interface Block {
  id: string;
  text: string;
}
export interface Operation {
  type: 'FieldEdited';
  node_id: string;
  field: 'content';
  value: { type: 'paragraph'; content: { type: 'text'; text: string }[] };
}
export interface Change {
  id: string;
  observed_at: string;
  operation: Operation;
}
export interface Push {
  name: string;
  note: string;
  submit: boolean;
  bundle: {
    format: 'dynodoc.change-bundle';
    version: 1;
    bundle_id: string;
    client_id: string;
    document_id: string;
    base_seq: number;
    base_chain_hash: string;
    capture: { host: 'word' | 'google-docs'; mode: 'observed_snapshot'; document_name: string };
    changes: Change[];
  };
}
export interface Receipt {
  id: string;
  bundle_id: string;
  digest: string;
  uploaded_by: string;
  submitted: boolean;
}
export interface Ledger {
  version: 1;
  client_id: string;
  checkpoint: Checkpoint;
  seen: Block[];
  changes: Change[];
  pending: Push | null;
  awaiting: Receipt | null;
  sentBlocks: Block[] | null;
  archives: { checkpoint: Checkpoint; changes: Change[] }[];
}
export interface Store {
  load: () => Promise<Ledger | null>;
  save: (ledger: Ledger) => Promise<void>;
}
const copy = <T>(x: T): T => JSON.parse(JSON.stringify(x));
function operation(block: Block): Operation {
  return {
    type: 'FieldEdited',
    node_id: block.id,
    field: 'content',
    value: { type: 'paragraph', content: block.text ? [{ type: 'text', text: block.text }] : [] }
  };
}
export function projectCheckpoint(c: Checkpoint): Block[] {
  if (
    c.format !== 'dynodoc.checkpoint' ||
    c.version !== 1 ||
    c.kind !== 'document' ||
    !Number.isSafeInteger(c.through_seq) ||
    c.through_seq < 0 ||
    !/^[0-9a-f]{64}$/.test(c.chain_hash)
  )
    throw new Error('Use a valid document checkpoint from Dynodoc.');
  const nodes = Object.values(c.state.nodes).filter((n) => !n.deleted),
    roots = nodes.filter((n) => n.type === 'form');
  if (roots.length !== 1) throw new Error('This connector needs a document with one root.');
  const items = nodes.filter((n) => n.type !== 'form');
  if (items.length > 2000)
    throw new Error('This development connector supports up to 2,000 paragraphs.');
  const sorted = items.sort((a, b) =>
    a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : a.id.localeCompare(b.id)
  );
  return sorted.map((n) => {
    const content = n.current_fields.content as
      | {
          type?: string;
          attrs?: Record<string, unknown>;
          content?: { type?: string; text?: string; marks?: unknown[] }[];
        }
      | undefined;
    if (
      n.type !== 'item' ||
      n.parent_id !== roots[0].id ||
      content?.type !== 'paragraph' ||
      Object.keys(content.attrs || {}).length ||
      (content.content || []).some(
        (x) => x.type !== 'text' || typeof x.text !== 'string' || x.marks?.length
      )
    )
      throw new Error(
        'This first connector supports plain paragraphs only. Use file push for tables, lists, images or formatted blocks.'
      );
    return { id: n.id, text: (content.content || []).map((x) => x.text).join('') };
  });
}
export function bindIdentical(local: { text: string }[], remote: Block[]): Block[] {
  if (local.length !== remote.length || local.some((x, i) => x.text !== remote[i].text))
    throw new Error(
      'The local file differs from this revision. Use file push to choose its starting version before connecting.'
    );
  return copy(remote);
}
export function diff(base: Block[], local: Block[]): { id: string; base: string; local: string }[] {
  if (
    base.length !== local.length ||
    local.some((x, i) => x.id !== base[i].id) ||
    new Set(local.map((x) => x.id)).size !== local.length
  )
    throw new Error(
      'Paragraphs were inserted, deleted, moved or duplicated. Structural sync is not supported yet; export your ledger and use file push.'
    );
  return local
    .filter((x, i) => x.text !== base[i].text)
    .map((x) => ({ id: x.id, base: base.find((b) => b.id === x.id)!.text, local: x.text }));
}
export function pullPlan(base: Block[], local: Block[], remote: Block[]) {
  diff(base, local);
  diff(base, remote);
  return base.map((b, i) => ({
    id: b.id,
    base: b.text,
    local: local[i].text,
    remote: remote[i].text,
    conflict:
      local[i].text !== b.text && remote[i].text !== b.text && local[i].text !== remote[i].text,
    text: local[i].text === b.text ? remote[i].text : local[i].text
  }));
}
export class CaptureSession {
  private tail: Promise<unknown> = Promise.resolve();
  private serial<T>(work: () => Promise<T>): Promise<T> {
    const next = this.tail.then(work);
    this.tail = next.catch(() => undefined);
    return next;
  }
  private constructor(
    public ledger: Ledger,
    private store: Store,
    readonly host: 'word' | 'google-docs'
  ) {}
  static async connect(
    store: Store,
    checkpoint: Checkpoint,
    local: Block[],
    host: 'word' | 'google-docs'
  ) {
    const existing = await store.load();
    if (existing) {
      if (existing.version !== 1 || existing.checkpoint.document_id !== checkpoint.document_id)
        throw new Error('Saved capture belongs to another file. Export it before reconnecting.');
      return new CaptureSession(existing, store, host);
    }
    bindIdentical(local, projectCheckpoint(checkpoint));
    const ledger: Ledger = {
      version: 1,
      client_id: crypto.randomUUID(),
      checkpoint: copy(checkpoint),
      seen: copy(local),
      changes: [],
      pending: null,
      awaiting: null,
      sentBlocks: null,
      archives: []
    };
    await store.save(ledger);
    return new CaptureSession(ledger, store, host);
  }
  private async persist(next: Ledger) {
    await this.store.save(copy(next));
    this.ledger = next;
  }
  capture(local: Block[]) {
    const snapshot = copy(local);
    return this.serial(() => this.captureNow(snapshot));
  }
  private async captureNow(local: Block[]) {
    const changes = diff(this.ledger.seen, local);
    if (!changes.length) return;
    const next = copy(this.ledger);
    const observed_at = new Date().toISOString();
    for (const change of changes)
      next.changes.push({
        id: crypto.randomUUID(),
        observed_at,
        operation: operation({ id: change.id, text: change.local })
      });
    next.seen = copy(local);
    await this.persist(next);
  }
  prepare(name: string, note: string, submit: boolean): Promise<Push> {
    return this.serial(() => this.prepareNow(name, note, submit));
  }
  private async prepareNow(name: string, note: string, submit: boolean): Promise<Push> {
    if (this.ledger.pending) return copy(this.ledger.pending);
    if (this.ledger.awaiting)
      throw new Error(
        'A previous upload is awaiting review. Pull its merged result before sending another request.'
      );
    if (!this.ledger.changes.length) throw new Error('No captured changes to send.');
    const next = copy(this.ledger),
      c = next.checkpoint;
    next.pending = {
      name,
      note,
      submit,
      bundle: {
        format: 'dynodoc.change-bundle',
        version: 1,
        bundle_id: crypto.randomUUID(),
        client_id: next.client_id,
        document_id: c.document_id,
        base_seq: c.through_seq,
        base_chain_hash: c.chain_hash,
        capture: { host: this.host, mode: 'observed_snapshot', document_name: c.title },
        changes: copy(next.changes)
      }
    };
    next.sentBlocks = copy(next.seen);
    await this.persist(next);
    return copy(next.pending);
  }
  acknowledge(receipt: Receipt) {
    return this.serial(() => this.acknowledgeNow(receipt));
  }
  private async acknowledgeNow(receipt: Receipt) {
    if (
      !this.ledger.pending ||
      receipt.bundle_id !== this.ledger.pending.bundle.bundle_id ||
      !/^[0-9a-f]{64}$/.test(receipt.digest) ||
      !receipt.id ||
      !receipt.uploaded_by
    )
      throw new Error('The server receipt does not match the queued upload. Keep it for retry.');
    const next = copy(this.ledger);
    next.awaiting = copy(receipt);
    next.pending = null;
    await this.persist(next);
  }
  plan(checkpoint: Checkpoint, local: Block[]) {
    if (
      checkpoint.document_id !== this.ledger.checkpoint.document_id ||
      checkpoint.through_seq < this.ledger.checkpoint.through_seq ||
      (checkpoint.through_seq === this.ledger.checkpoint.through_seq &&
        checkpoint.chain_hash !== this.ledger.checkpoint.chain_hash)
    )
      throw new Error('Checkpoint belongs to another history.');
    const remote = projectCheckpoint(checkpoint);
    return pullPlan(projectCheckpoint(this.ledger.checkpoint), local, remote);
  }
  /** Hosts call only after reading again and applying the previewed text. A failed
   * host write leaves the previous checkpoint and observations intact. */
  acceptPull(checkpoint: Checkpoint, actual: Block[]) {
    return this.serial(() => this.acceptPullNow(checkpoint, actual));
  }
  private async acceptPullNow(checkpoint: Checkpoint, actual: Block[]) {
    if (this.ledger.pending)
      throw new Error(
        'An upload has no receipt yet. Retry it before advancing its starting revision.'
      );
    this.plan(checkpoint, actual);
    const remote = projectCheckpoint(checkpoint);
    if (this.ledger.awaiting) {
      const status = checkpoint.proposals?.find((p) => p.id === this.ledger.awaiting!.id)?.status;
      if (!status || status === 'open' || status === 'draft')
        throw new Error('The uploaded request is still awaiting review.');
    }
    const next = copy(this.ledger);
    next.archives.push({ checkpoint: copy(next.checkpoint), changes: copy(next.changes) });
    next.checkpoint = copy(checkpoint);
    next.awaiting = null;
    next.sentBlocks = null;
    next.seen = copy(actual);
    // Retain observations as a rebase bundle only when local text differs from the
    // new team revision. The old acknowledged bundle stays immutable on the server.
    const observed_at = new Date().toISOString();
    next.changes = diff(remote, actual).map((d) => ({
      id: crypto.randomUUID(),
      observed_at,
      operation: operation({ id: d.id, text: d.local })
    }));
    await this.persist(next);
  }
  canApply(checkpoint: Checkpoint) {
    if (this.ledger.pending) throw new Error('Retry the upload before pulling.');
    if (this.ledger.awaiting) {
      const status = checkpoint.proposals?.find((p) => p.id === this.ledger.awaiting!.id)?.status;
      if (!status || status === 'open' || status === 'draft')
        throw new Error('The uploaded request is still awaiting review.');
    }
  }
  export() {
    return copy(this.ledger);
  }
}
/** IndexedDB's read/write transaction prevents two panes overwriting one queue. */
export function browserStore(key: string): Store {
  let previous: string | null = null;
  const database = new Promise<IDBDatabase>((resolve, reject) => {
    const req = indexedDB.open('dynodoc-provenance', 1);
    req.onupgradeneeded = () => req.result.createObjectStore('ledgers');
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
  return {
    load: async () => {
      const db = await database;
      return new Promise<Ledger | null>((resolve, reject) => {
        const tx = db.transaction('ledgers', 'readonly'),
          req = tx.objectStore('ledgers').get(key);
        req.onsuccess = () => {
          previous = req.result ?? null;
          resolve(previous ? JSON.parse(previous) : null);
        };
        req.onerror = () => reject(req.error);
      });
    },
    save: async (value) => {
      const db = await database;
      return new Promise<void>((resolve, reject) => {
        const tx = db.transaction('ledgers', 'readwrite'),
          store = tx.objectStore('ledgers'),
          req = store.get(key),
          next = JSON.stringify(value);
        let failure: Error | null = null;
        req.onsuccess = () => {
          if ((req.result ?? null) !== previous) {
            failure = new Error(
              'Another pane changed the capture queue. Reconnect before continuing.'
            );
            tx.abort();
            return;
          }
          store.put(next, key);
        };
        tx.oncomplete = () => {
          previous = next;
          resolve();
        };
        tx.onabort = () =>
          reject(failure || tx.error || new Error('Could not save capture locally'));
        tx.onerror = () => reject(tx.error);
      });
    }
  };
}
export async function connectorRequest<T>(
  origin: string,
  documentId: string,
  token: string,
  action: 'checkpoint' | 'bundles',
  body?: Push
): Promise<T> {
  const url = new URL(origin);
  if (
    url.protocol !== 'https:' &&
    !(url.protocol === 'http:' && ['localhost', '127.0.0.1'].includes(url.hostname))
  )
    throw new Error('Use an HTTPS Dynodoc address.');
  const response = await fetch(`${url.origin}/api/connectors/documents/${documentId}/${action}`, {
    method: body ? 'POST' : 'GET',
    credentials: 'omit',
    headers: {
      authorization: `Bearer ${token}`,
      ...(body ? { 'content-type': 'application/json' } : {})
    },
    body: body ? JSON.stringify(body) : undefined
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || `Request failed (${response.status})`);
  return value;
}
