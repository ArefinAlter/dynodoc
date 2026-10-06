import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { it, expect } from 'vitest';
function setup() {
  const properties = new Map<string, string>();
  let fail = false;
  const props = {
    getProperty: (key: string) => properties.get(key) ?? null,
    setProperty: (key: string, value: string) => {
      if (fail && key.includes('.queue.')) throw new Error('Quota');
      properties.set(key, value);
    },
    deleteProperty: (key: string) => properties.delete(key)
  };
  const context = {
    DocumentApp: {
      getActiveDocument: () => ({
        getId: () => 'local-doc',
        getActiveTab: () => ({ getId: () => 'tab-a', asDocumentTab: () => ({}) })
      })
    },
    PropertiesService: { getUserProperties: () => props },
    Utilities: {
      getUuid: () => crypto.randomUUID(),
      newBlob: (text: string) => ({ getBytes: () => new TextEncoder().encode(text) })
    },
    LockService: { getUserLock: () => ({ waitLock: () => {}, releaseLock: () => {} }) }
  } as unknown as Record<string, unknown> & {
    saveLedger: (payload: unknown, generation: string | null) => string;
    loadLedger: () => { generation: string | null; payload: unknown };
  };
  runInNewContext(readFileSync('../../extensions/google-docs/Code.gs', 'utf8'), context);
  return {
    context,
    properties,
    fail: () => {
      fail = true;
    }
  };
}
it('refuses a queue or host operation after the user switches Google Docs tabs', () => {
  const { context } = setup();
  const load = context.loadLedger as unknown as (scope: string) => unknown;
  expect(() => load('dynodoc.local-doc.another-tab')).toThrow('active tab changed');
});
it('persists Unicode queues privately and rejects concurrent sidebar overwrites', () => {
  const { context, properties } = setup(),
    payload = { observations: '🙂 multilingual '.repeat(400) },
    generation = context.saveLedger(payload, null);
  expect(context.loadLedger().payload).toEqual(payload);
  expect(() => context.saveLedger({ different: true }, null)).toThrow('Another sidebar');
  expect([...properties.keys()].every((k) => k.startsWith('dynodoc.local-doc.tab-a'))).toBe(true);
  context.saveLedger({ next: true }, generation);
  expect([...properties.keys()].filter((k) => k.includes(generation))).toHaveLength(0);
});
it('retains the previous manifest and queue after a failed chunk write', () => {
  const { context, fail } = setup(),
    generation = context.saveLedger({ saved: true }, null);
  fail();
  expect(() => context.saveLedger({ later: true }, generation)).toThrow('Quota');
  expect(context.loadLedger().payload).toEqual({ saved: true });
});
