import { describe, it, expect, vi } from 'vitest';
import { webcrypto } from 'node:crypto';
import {
  CaptureSession,
  type ParagraphValue,
  projectCheckpoint,
  type Checkpoint,
  type Ledger,
  type Store
} from './protocol';
import {
  exportSidecar,
  parseSidecar,
  recoveryLedger,
  restoreCapture,
  type Sidecar
} from './sidecar';
vi.stubGlobal('crypto', webcrypto);
const root = '01ARZ3NDEKTSV4RRFFQ69G5FAV',
  paragraph = '01ARZ3NDEKTSV4RRFFQ69G5FAW';
const doc = '7034d450-77cf-44f0-8eec-63f892643f8e';
function checkpoint(): Checkpoint {
  // Include actual engine materialization metadata, not only the client projection.
  return {
    format: 'dynodoc.checkpoint',
    version: 1,
    document_id: doc,
    title: 'Report',
    kind: 'document',
    through_seq: 3,
    chain_hash: 'a'.repeat(64),
    state: {
      nodes: {
        [root]: {
          id: root,
          type: 'form',
          parent_id: null,
          pos: '0',
          deleted: false,
          current_fields: {},
          var_name: null
        },
        [paragraph]: {
          id: paragraph,
          type: 'item',
          parent_id: root,
          pos: '1',
          deleted: false,
          current_fields: {
            content: { type: 'paragraph', content: [{ type: 'text', text: 'Original' }] }
          },
          var_name: 'intro'
        }
      },
      comments: [],
      suggestions: {},
      removed_choices: []
    },
    proposals: []
  } as Checkpoint;
}
function memory(initial: Ledger | null = null) {
  let saved = initial;
  const store: Store = {
    load: async () => (saved ? structuredClone(saved) : null),
    save: async (value) => {
      saved = structuredClone(value);
    }
  };
  return { store, read: () => saved };
}
const edited = (text: string) => [{ id: paragraph, text }];
async function source(host: 'word' | 'google-docs' = 'word') {
  const m = memory(),
    c = checkpoint(),
    s = await CaptureSession.connect(m.store, c, projectCheckpoint(c), host);
  await s.capture(edited('Captured'));
  const push = await s.prepare('Offline edits', 'Keep this message', true);
  await s.capture(edited('Later typing'));
  return { s, push, sidecar: exportSidecar(s) };
}
describe('portable provenance recovery', () => {
  it.each(['word', 'google-docs'] as const)(
    'restores %s offline retries with exact IDs, message and late observations',
    async (host) => {
      const { push, sidecar } = await source(host),
        m = memory();
      const restored = await restoreCapture(
        m.store,
        parseSidecar(JSON.stringify(sidecar)),
        checkpoint(),
        edited('Later typing'),
        host
      );
      expect(await restored.prepare('Changed form', 'Different intent', false)).toEqual(push);
      expect(restored.ledger.client_id).toBe(sidecar.ledger.client_id);
      expect(restored.ledger.changes).toEqual(sidecar.ledger.changes);
      await restored.capture(edited('Typing on the restored device'));
      expect(restored.ledger.changes).toHaveLength(3);
      expect(await restored.prepare('Next', '', true)).toEqual(push);
      expect(JSON.stringify(exportSidecar(restored))).not.toContain('credential');
    }
  );
  it('restores acknowledged uploads and archives against a newer team checkpoint', async () => {
    const { s, push } = await source();
    await s.acknowledge({
      id: 'fe257224-bcdd-4261-aa37-778f836fa8e2',
      bundle_id: push.bundle.bundle_id,
      digest: 'b'.repeat(64),
      uploaded_by: doc,
      submitted: true,
      ...{ revision: 1, edits: 1, relation: { unrelated: false } }
    });
    const remote = checkpoint();
    remote.through_seq = 4;
    remote.chain_hash = 'c'.repeat(64);
    remote.state.nodes[paragraph].current_fields.content = {
      type: 'paragraph',
      content: [{ type: 'text', text: 'Captured' }]
    };
    remote.proposals = [{ id: s.ledger.awaiting!.id, status: 'merged' }];
    const restored = await restoreCapture(
      memory().store,
      exportSidecar(s),
      remote,
      edited('Later typing'),
      'word',
      checkpoint()
    );
    expect(restored.ledger.checkpoint.through_seq).toBe(3);
    restored.canApply(remote);
    await restored.acceptPull(remote, edited('Later typing'));
    const saved = parseSidecar(JSON.stringify(exportSidecar(restored)));
    expect(saved.ledger.archives[0].changes).toEqual(s.ledger.changes);
    expect((saved.ledger.changes[0].operation.value as ParagraphValue).content[0].text).toBe(
      'Later typing'
    );
  });
  it('never overwrites an existing queue, including an empty connected ledger', async () => {
    const { sidecar } = await source(),
      m = memory();
    const existing = await CaptureSession.connect(
      m.store,
      checkpoint(),
      projectCheckpoint(checkpoint()),
      'word'
    );
    const before = existing.export();
    await expect(
      restoreCapture(m.store, sidecar, checkpoint(), edited('Later typing'), 'word')
    ).rejects.toThrow('already has a saved queue');
    expect(m.read()).toEqual(before);
  });
  it('verifies an older base against server text, IDs and hash before restoration', async () => {
    const { sidecar } = await source(),
      remote = checkpoint();
    remote.through_seq = 4;
    expect(() => recoveryLedger(sidecar, remote, 'word')).toThrow('retrieved from Dynodoc');
    const wrong = checkpoint();
    wrong.chain_hash = 'b'.repeat(64);
    expect(() => recoveryLedger(sidecar, remote, 'word', wrong)).toThrow('retrieved from Dynodoc');
    const forged = structuredClone(sidecar);
    forged.ledger.checkpoint.state.nodes[paragraph].current_fields.content = {
      type: 'paragraph',
      content: [{ type: 'text', text: 'Forged starting text' }]
    };
    expect(() => recoveryLedger(forged, remote, 'word', checkpoint())).toThrow(
      'retrieved from Dynodoc'
    );
    expect(recoveryLedger(sidecar, remote, 'word', checkpoint())).toEqual(sidecar.ledger);
  });
  it('retains the original export and does not create a queue when storage fails', async () => {
    const { sidecar } = await source(),
      before = JSON.stringify(sidecar),
      m = memory();
    m.store.save = async () => {
      throw new Error('Quota');
    };
    await expect(
      restoreCapture(m.store, sidecar, checkpoint(), edited('Later typing'), 'word')
    ).rejects.toThrow('Quota');
    expect(m.read()).toBeNull();
    expect(JSON.stringify(sidecar)).toBe(before);
  });
  it('refuses another editor, document, hash, stale head or missing block identities', async () => {
    const { sidecar } = await source(),
      c = checkpoint();
    expect(() => recoveryLedger(sidecar, c, 'google-docs')).toThrow('original editor');
    expect(() =>
      recoveryLedger(sidecar, { ...c, document_id: crypto.randomUUID() }, 'word')
    ).toThrow('another document');
    expect(() => recoveryLedger(sidecar, { ...c, chain_hash: 'b'.repeat(64) }, 'word')).toThrow(
      'another document'
    );
    expect(() => recoveryLedger(sidecar, { ...c, through_seq: 2 }, 'word')).toThrow(
      'another document'
    );
    await expect(
      restoreCapture(memory().store, sidecar, c, [{ id: root, text: 'Later typing' }], 'word')
    ).rejects.toThrow('Structural sync');
  });
  it.each([
    ['secret field', (s: Sidecar) => Object.assign(s.ledger, { token: 'secret' })],
    [
      'altered frozen intent',
      (s: Sidecar) => {
        (s.ledger.pending!.bundle.changes[0].operation.value as ParagraphValue).content[0].text =
          'Forged';
      }
    ],
    [
      'missing observation',
      (s: Sidecar) => {
        s.ledger.changes.pop();
      }
    ],
    [
      'duplicate observation',
      (s: Sidecar) => {
        s.ledger.changes[1].id = s.ledger.changes[0].id;
      }
    ],
    [
      'wrong base',
      (s: Sidecar) => {
        s.ledger.pending!.bundle.base_chain_hash = 'b'.repeat(64);
      }
    ],
    [
      'changed client',
      (s: Sidecar) => {
        s.ledger.pending!.bundle.client_id = crypto.randomUUID();
      }
    ],
    [
      'unknown node',
      (s: Sidecar) => {
        s.ledger.changes[1].operation.node_id = root;
      }
    ],
    [
      'sent text mismatch',
      (s: Sidecar) => {
        s.ledger.sentBlocks![0].text = 'Wrong';
      }
    ],
    [
      'simultaneous receipt',
      (s: Sidecar) => {
        s.ledger.awaiting = {} as Ledger['awaiting'];
      }
    ],
    [
      'non-string block',
      (s: Sidecar) => {
        Object.assign(s.ledger.seen[0], { text: 42 });
      }
    ]
  ])('rejects %s before any recovery write', async (_name, change) => {
    const { sidecar } = await source();
    change(sidecar);
    expect(() => parseSidecar(JSON.stringify(sidecar))).toThrow('Invalid provenance sidecar');
  });
  it('accepts equivalent JSON key order without rebuilding frozen upload intent', async () => {
    const { sidecar, push } = await source();
    const operation = sidecar.ledger.pending!.bundle.changes[0].operation;
    sidecar.ledger.pending!.bundle.changes[0].operation = {
      value: operation.value,
      field: operation.field,
      node_id: operation.node_id,
      type: operation.type
    };
    const s = await restoreCapture(
      memory().store,
      sidecar,
      checkpoint(),
      edited('Later typing'),
      'word'
    );
    expect(await s.prepare('Next', '', false)).toEqual(push);
  });
  it('keeps observations after a frozen upload when total history exceeds the per-upload limit', async () => {
    const { sidecar, push } = await source();
    const late = sidecar.ledger.changes[1];
    sidecar.ledger.changes.push(
      ...Array.from({ length: 20000 }, () => ({ ...late, id: crypto.randomUUID() }))
    );
    const decoded = parseSidecar(JSON.stringify(sidecar));
    expect(decoded.ledger.changes).toHaveLength(20002);
    expect(decoded.ledger.pending).toEqual(push);
  });
  it('rejects unsupported and incomplete files with a recoverable error', () => {
    for (const text of [
      '{',
      'null',
      '{}',
      JSON.stringify({ format: 'dynodoc.provenance-sidecar', version: 99 })
    ])
      expect(() => parseSidecar(text)).toThrow('Invalid provenance sidecar');
  });
});
