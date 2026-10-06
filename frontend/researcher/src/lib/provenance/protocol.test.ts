import { describe, it, expect, vi } from 'vitest';
import { webcrypto } from 'node:crypto';
import {
  CaptureSession,
  type ParagraphValue,
  projectCheckpoint,
  bindIdentical,
  pullPlan,
  type Block,
  type Checkpoint,
  type Ledger,
  type Store,
  type Receipt
} from './protocol';
vi.stubGlobal('crypto', webcrypto);
const blocks: Block[] = [
  { id: 'paragraph-a', text: 'First' },
  { id: 'paragraph-b', text: 'Second' }
];
function checkpoint(texts = blocks, seq = 3): Checkpoint {
  return {
    format: 'dynodoc.checkpoint',
    version: 1,
    document_id: 'doc-a',
    title: 'Report',
    kind: 'document',
    through_seq: seq,
    chain_hash: 'a'.repeat(64),
    state: {
      nodes: Object.fromEntries(
        [
          {
            id: 'root',
            type: 'form',
            parent_id: null,
            pos: '0',
            current_fields: {},
            deleted: false
          },
          ...texts.map((b, i) => ({
            id: b.id,
            type: 'item',
            parent_id: 'root',
            pos: String(i),
            current_fields: {
              content: { type: 'paragraph', content: [{ type: 'text', text: b.text }] }
            },
            deleted: false
          }))
        ].map((n) => [n.id, n])
      )
    },
    proposals: []
  };
}
function memory() {
  let saved: Ledger | null = null;
  return {
    store: {
      load: async () => (saved ? structuredClone(saved) : null),
      save: async (value: Ledger) => {
        saved = structuredClone(value);
      }
    } satisfies Store,
    read: () => saved
  };
}
const changed = (text: string): Block[] => [{ id: 'paragraph-a', text }, blocks[1]];
function receipt(bundle: string): Receipt {
  return {
    id: 'request-a',
    bundle_id: bundle,
    digest: 'b'.repeat(64),
    uploaded_by: 'server-actor',
    submitted: true
  };
}
describe('shared Word / Google Docs capture contract', () => {
  it('maps duplicate text by stable identity and rejects uncertain first bindings', () => {
    expect(
      bindIdentical(
        [{ text: 'Same' }, { text: 'Same' }],
        [
          { id: 'a', text: 'Same' },
          { id: 'b', text: 'Same' }
        ]
      ).map((b) => b.id)
    ).toEqual(['a', 'b']);
    expect(() => bindIdentical([{ text: 'Edited' }, blocks[1]], blocks)).toThrow(
      'starting version'
    );
  });
  it('refuses rich/structural content instead of flattening it', () => {
    const c = checkpoint();
    c.state.nodes['paragraph-a'].current_fields.content = { type: 'table', content: [] };
    expect(() => projectCheckpoint(c)).toThrow('plain paragraphs');
    expect(() => pullPlan(blocks, [blocks[1], blocks[0]], blocks)).toThrow('moved');
  });
  it('keeps an identical upload through a failure and reload, including later typing', async () => {
    const m = memory(),
      session = await CaptureSession.connect(m.store, checkpoint(), blocks, 'word');
    await session.capture(changed('Word edit'));
    const pending = await session.prepare('Edit', '', true);
    await session.capture(changed('Further offline typing'));
    const reopened = await CaptureSession.connect(m.store, checkpoint(), [], 'word');
    expect(await reopened.prepare('Different form text', 'Different', false)).toEqual(pending);
    expect(reopened.ledger.changes).toHaveLength(2);
    await reopened.acknowledge(receipt(pending.bundle.bundle_id));
    expect(reopened.ledger.seen[0].text).toBe('Further offline typing');
    expect(reopened.ledger.checkpoint.through_seq).toBe(3);
    await expect(reopened.prepare('Next', '', true)).rejects.toThrow('awaiting review');
  });
  it('does not erase a queued upload on a mismatched receipt', async () => {
    const m = memory(),
      s = await CaptureSession.connect(m.store, checkpoint(), blocks, 'google-docs');
    await s.capture(changed('Docs edit'));
    const p = await s.prepare('Edit', '', true);
    await expect(s.acknowledge(receipt('wrong-id'))).rejects.toThrow('does not match');
    expect(s.ledger.pending).toEqual(p);
  });
  it('stops capture on a storage failure without advancing the saved baseline', async () => {
    const m = memory(),
      s = await CaptureSession.connect(m.store, checkpoint(), blocks, 'word');
    m.store.save = async () => {
      throw new Error('Quota');
    };
    await expect(s.capture(changed('Unsaved observation'))).rejects.toThrow('Quota');
    expect(s.ledger.seen).toEqual(blocks);
    expect(m.read()?.changes).toHaveLength(0);
  });
  it('serializes delayed persistence and simultaneous observations', async () => {
    const m = memory(),
      s = await CaptureSession.connect(m.store, checkpoint(), blocks, 'word');
    await Promise.all([s.capture(changed('One')), s.capture(changed('Two'))]);
    expect(s.ledger.changes).toHaveLength(2);
    expect(s.ledger.seen[0].text).toBe('Two');
  });
  it('surfaces overlaps and preserves independent local edits during pull', () => {
    const local = changed('Local'),
      remote = [blocks[0], { ...blocks[1], text: 'Remote' }];
    expect(pullPlan(blocks, local, remote).map((p) => p.text)).toEqual(['Local', 'Remote']);
    expect(pullPlan(blocks, local, changed('Overlap'))[0].conflict).toBe(true);
  });
  it('exchanges Word edits into a Docs checkpoint and preserves the observed history on rebase', async () => {
    const m = memory(),
      word = await CaptureSession.connect(m.store, checkpoint(), blocks, 'word');
    await word.capture(changed('Word edit'));
    const p = await word.prepare('Word edits', '', true);
    await word.acknowledge(receipt(p.bundle.bundle_id));
    const merged = checkpoint(changed('Word edit'), 4);
    merged.proposals = [{ id: 'request-a', status: 'merged' }];
    word.canApply(merged);
    await word.acceptPull(merged, changed('Word edit'));
    expect(word.ledger.changes).toHaveLength(0);
    expect(
      (word.ledger.archives[0].changes[0].operation.value as ParagraphValue).content[0].text
    ).toBe('Word edit');
    const gm = memory(),
      docs = await CaptureSession.connect(
        gm.store,
        merged,
        projectCheckpoint(merged),
        'google-docs'
      );
    await docs.capture(changed('Docs continuation'));
    const next = await docs.prepare('Docs edits', '', true);
    expect(next.bundle.base_seq).toBe(4);
    expect(next.bundle.capture.host).toBe('google-docs');
    expect(next.bundle.changes[0].operation.node_id).toBe('paragraph-a');
  });
  it('blocks stale/different history and open reviews before applying to a host', async () => {
    const m = memory(),
      s = await CaptureSession.connect(m.store, checkpoint(), blocks, 'word');
    expect(() => s.plan(checkpoint(blocks, 2), blocks)).toThrow('another history');
    await s.capture(changed('Edit'));
    const p = await s.prepare('Edit', '', true);
    await s.acknowledge(receipt(p.bundle.bundle_id));
    const open = checkpoint();
    open.proposals = [{ id: 'request-a', status: 'open' }];
    expect(() => s.canApply(open)).toThrow('awaiting review');
  });
});
