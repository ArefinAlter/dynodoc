// @vitest-environment node
// Apps Script storage behaviour that Marketplace users depend on: locks are always
// released, a full Google property store fails with an explanation (not a raw
// Apps Script exception), and a failed save never damages the previous queue.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runInNewContext } from 'node:vm';
import { describe, expect, it } from 'vitest';

const extensions = resolve(process.cwd(), '../../extensions');

function setup(options: { lockTimeout?: boolean; quota?: number } = {}) {
  const properties = new Map<string, string>();
  const locks = { acquired: 0, released: 0 };
  let writes = 0;
  const props = {
    getProperty: (key: string) => properties.get(key) ?? null,
    setProperty: (key: string, value: string) => {
      writes += 1;
      if (options.quota !== undefined && writes > options.quota)
        throw new Error('Exception: You have exceeded the property storage quota.');
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
    LockService: {
      getUserLock: () => ({
        waitLock: () => {
          if (options.lockTimeout) throw new Error('Exception: Lock timeout');
          locks.acquired += 1;
        },
        releaseLock: () => {
          locks.released += 1;
        }
      })
    }
  } as unknown as Record<string, unknown> & {
    saveLedger: (payload: unknown, generation: string | null, scope?: string) => string;
    loadLedger: () => { generation: string | null; payload: unknown };
    connectDynodoc: (documentId: string, token: string) => unknown;
    pushDynodoc: (body: unknown) => unknown;
  };
  runInNewContext(readFileSync(resolve(extensions, 'google-docs/Code.gs'), 'utf8'), context);
  runInNewContext(readFileSync(resolve(extensions, 'google-docs/Common.gs'), 'utf8'), context);
  return { context, properties, locks };
}

describe('saveLedger', () => {
  it('releases the lock after a save and after any failure inside it', () => {
    const { context, locks } = setup();
    const generation = context.saveLedger({ saved: true }, null);
    expect(locks).toEqual({ acquired: 1, released: 1 });
    expect(() => context.saveLedger({ stale: true }, null)).toThrow('Another sidebar');
    expect(locks).toEqual({ acquired: 2, released: 2 });
    // The tab check runs after the lock is taken; it used to leak the lock.
    expect(() =>
      context.saveLedger({ other: true }, generation, 'dynodoc.local-doc.tab-b')
    ).toThrow('active tab changed');
    expect(locks).toEqual({ acquired: 3, released: 3 });
  });

  it('explains a lock timeout and does not release a lock it never held', () => {
    const { context, locks } = setup({ lockTimeout: true });
    expect(() => context.saveLedger({ saved: true }, null)).toThrow(
      'Another Dynodoc sidebar is saving'
    );
    expect(locks).toEqual({ acquired: 0, released: 0 });
  });

  it('turns a full Google property store into a clear message and keeps the old queue', () => {
    // Two writes succeed (one chunk and its manifest); the next save runs out of space.
    const { context, properties } = setup({ quota: 2 });
    const generation = context.saveLedger({ saved: true }, null);
    const before = new Map(properties);
    let message = '';
    try {
      context.saveLedger({ later: 'x'.repeat(5000) }, generation);
    } catch (error) {
      message = (error as Error).message;
    }
    expect(message).toContain("Google's saved-data limit for this add-on has been reached");
    expect(message).toContain('Export your provenance');
    expect(message).toContain('previous saved queue is intact');
    expect(properties).toEqual(before);
    expect(context.loadLedger().payload).toEqual({ saved: true });
  });

  it('leaves unrelated storage errors untouched', () => {
    const { context } = setup();
    (context.PropertiesService as { getUserProperties: () => unknown }).getUserProperties = () => ({
      getProperty: () => null,
      setProperty: () => {
        throw new Error('Service unavailable');
      },
      deleteProperty: () => {}
    });
    expect(() => context.saveLedger({ saved: true }, null)).toThrow('Service unavailable');
  });

  it('stops at the documented 100 KB per-file limit with an export instruction', () => {
    const { context } = setup();
    expect(() => context.saveLedger({ big: 'x'.repeat(100_001) }, null)).toThrow(
      /100 KB limit.*previous saved queue is intact/
    );
  });
});

describe('error wording', () => {
  it('uses correct articles in connection errors', () => {
    const { context } = setup();
    expect(() => context.connectDynodoc('not-an-id', 'not-a-key')).toThrow(
      'file ID and an editor connection key'
    );
    expect(() => context.pushDynodoc({ bundle: { capture: { host: 'word' } } })).toThrow(
      'Use an editor change bundle.'
    );
  });
});
