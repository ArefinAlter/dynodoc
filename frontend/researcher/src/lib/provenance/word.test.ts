import { it, expect } from 'vitest';
import { WordAdapter, wordFileUrl, type WordRuntime } from './word';
function host() {
  const paragraphs = ['First', 'Second'].map((text) => {
    const controls: {
      tag: string;
      text: string;
      title: string;
      insertText: (text: string) => void;
    }[] = [];
    const p = {
      text,
      isListItem: false,
      contentControls: { items: controls, load: () => {} },
      getRange: () => ({
        insertContentControl: () => {
          const c = {
            tag: '',
            title: '',
            get text() {
              return p.text;
            },
            insertText: (text: string) => {
              p.text = text;
            }
          };
          controls.push(c);
          return c;
        }
      }),
      insertText: (text: string) => {
        p.text = text;
      }
    };
    return p;
  });
  const tables: unknown[] = [];
  const word: WordRuntime = {
    run: async (callback) =>
      callback({
        document: {
          body: {
            paragraphs: { items: paragraphs, load: () => {} },
            tables: { items: tables, load: () => {} }
          }
        },
        sync: async () => {}
      })
  };
  return { adapter: new WordAdapter(word), paragraphs, tables };
}
it('binds content-control IDs and updates the Word paragraphs through the native adapter contract', async () => {
  const { adapter, paragraphs } = host(),
    base = [
      { id: 'a', text: 'First' },
      { id: 'b', text: 'Second' }
    ];
  await adapter.bind(base);
  expect(paragraphs[0].contentControls.items[0].tag).toBe('dynodoc.block.a');
  paragraphs[0].text = 'Local Word edit';
  expect((await adapter.read())[0]).toEqual({ id: 'a', text: 'Local Word edit' });
  const current = await adapter.read();
  await adapter.apply(current, [current[0], { id: 'b', text: 'Pulled from Docs' }]);
  expect(paragraphs[1].text).toBe('Pulled from Docs');
});
it('validates every paragraph before writing and refuses lost or duplicate anchors', async () => {
  const { adapter, paragraphs } = host(),
    base = [
      { id: 'a', text: 'First' },
      { id: 'b', text: 'Second' }
    ];
  await adapter.bind(base);
  paragraphs[1].text = 'Typed after preview';
  await expect(
    adapter.apply(base, [{ id: 'a', text: 'Would change first' }, base[1]])
  ).rejects.toThrow('after the pull preview');
  expect(paragraphs[0].text).toBe('First');
  paragraphs[0].contentControls.items.push(paragraphs[0].contentControls.items[0]);
  await expect(adapter.read()).rejects.toThrow('identity');
});
it('refuses table bodies instead of flattening them', async () => {
  const { adapter, tables } = host();
  tables.push({ table: true });
  await expect(adapter.bind([])).rejects.toThrow('tables or lists');
});
it('validates all identities before adding recovery tags to any paragraph', async () => {
  const { adapter, paragraphs } = host();
  paragraphs[1].contentControls.items.push({
    tag: 'dynodoc.block.wrong',
    text: 'Second',
    title: '',
    insertText: () => {}
  });
  await expect(
    adapter.bind([
      { id: 'a', text: 'First' },
      { id: 'b', text: 'Second' }
    ])
  ).rejects.toThrow('different block identities');
  expect(paragraphs[0].contentControls.items).toHaveLength(0);
});
it('checks the working copy again immediately before queuing pull writes', async () => {
  const { paragraphs, tables } = host();
  let checks = 0;
  const adapter = new WordAdapter(
    {
      run: async (callback) =>
        callback({
          document: {
            body: {
              paragraphs: { items: paragraphs, load: () => {} },
              tables: { items: tables, load: () => {} }
            }
          },
          sync: async () => {}
        })
    },
    async () => {
      if (++checks === 4) throw new Error('Working copy changed');
    }
  );
  const base = [
    { id: 'a', text: 'First' },
    { id: 'b', text: 'Second' }
  ];
  await adapter.bind(base); // two checks: before reading and before tagging
  await expect(adapter.apply(base, [{ id: 'a', text: 'Would write' }, base[1]])).rejects.toThrow(
    'Working copy changed'
  );
  expect(paragraphs[0].text).toBe('First');
});
it('uses current Office file properties and refuses unsaved or failed lookups', async () => {
  await expect(
    wordFileUrl({
      getFilePropertiesAsync: (callback) =>
        callback({ status: 'succeeded', value: { url: 'file:///renamed.docx' } })
    })
  ).resolves.toBe('file:///renamed.docx');
  await expect(
    wordFileUrl({
      getFilePropertiesAsync: (callback) => callback({ status: 'succeeded', value: { url: '' } })
    })
  ).rejects.toThrow('Save this Word file');
  await expect(
    wordFileUrl({
      getFilePropertiesAsync: (callback) =>
        callback({ status: 'failed', error: { message: 'Host unavailable' } })
    })
  ).rejects.toThrow('Host unavailable');
});
