/** Stable WordApi 1.3 subset; tags travel with saved DOCX content controls. */
import type { Block } from './protocol';
import { bindIdentical } from './protocol';
interface Collection<T> {
  items: T[];
  load: (fields: string) => void;
}
interface Control {
  tag: string;
  text: string;
  title: string;
  insertText: (text: string, where: 'Replace') => unknown;
}
interface Paragraph {
  text: string;
  isListItem: boolean;
  contentControls: Collection<Control>;
  getRange: (where: 'Content') => { insertContentControl: () => Control };
  insertText: (text: string, where: 'Replace') => unknown;
}
interface Context {
  document: { body: { paragraphs: Collection<Paragraph>; tables: Collection<unknown> } };
  sync: () => Promise<void>;
}
export interface WordRuntime {
  run: <T>(callback: (context: Context) => Promise<T>) => Promise<T>;
}
export interface OfficeDocument {
  getFilePropertiesAsync: (
    callback: (result: {
      status: string;
      value?: { url: string };
      error?: { message: string };
    }) => void
  ) => void;
}
/** Query current properties rather than the URL captured when the pane opened. */
export function wordFileUrl(document: OfficeDocument): Promise<string> {
  return new Promise((resolve, reject) => {
    document.getFilePropertiesAsync((result) => {
      if (result.status !== 'succeeded') {
        reject(
          new Error(result.error?.message || 'Could not identify the current Word working copy.')
        );
      } else if (!result.value?.url) {
        reject(
          new Error(
            'Save this Word file before connecting so its working copy has a durable queue.'
          )
        );
      } else resolve(result.value.url);
    });
  });
}
const prefix = 'dynodoc.block.';
export class WordAdapter {
  constructor(
    private word: WordRuntime,
    private guard: () => Promise<void> = async () => {}
  ) {}
  private async paragraphs(ctx: Context) {
    const { paragraphs, tables } = ctx.document.body;
    paragraphs.load('items/text,items/isListItem');
    tables.load('items');
    await ctx.sync();
    if (tables.items.length || paragraphs.items.some((p) => p.isListItem))
      throw new Error(
        'This add-in supports body paragraphs only. Use file push for tables or lists.'
      );
    for (const p of paragraphs.items) p.contentControls.load('items/tag,items/text');
    await ctx.sync();
    return paragraphs.items;
  }
  async bind(remote: Block[]) {
    await this.guard();
    return this.word.run(async (ctx) => {
      const paragraphs = await this.paragraphs(ctx);
      bindIdentical(
        paragraphs.map((p) => ({ text: p.text.replace(/\r$/, '') })),
        remote
      );
      const tagged = paragraphs.map((p, i) => {
        const tagged = p.contentControls.items.filter((c) => c.tag.startsWith(prefix));
        if (tagged.length && (tagged.length !== 1 || tagged[0].tag !== prefix + remote[i].id))
          throw new Error(
            'This file already has different block identities. Do not remap it by position.'
          );
        return tagged;
      });
      await this.guard();
      paragraphs.forEach((p, i) => {
        if (!tagged[i].length) {
          const c = p.getRange('Content').insertContentControl();
          c.tag = prefix + remote[i].id;
          c.title = 'Dynodoc paragraph';
        }
      });
      await ctx.sync();
      return remote;
    });
  }
  async read() {
    await this.guard();
    return this.word.run(async (ctx) => {
      const paragraphs = await this.paragraphs(ctx);
      await this.guard();
      return paragraphs.map((p) => {
        const controls = p.contentControls.items.filter((c) => c.tag.startsWith(prefix));
        if (controls.length !== 1)
          throw new Error(
            'A paragraph lost its identity or a new paragraph was added. Use file push for structural changes.'
          );
        return { id: controls[0].tag.slice(prefix.length), text: p.text.replace(/\r$/, '') };
      });
    });
  }
  async apply(expected: Block[], next: Block[]) {
    await this.guard();
    return this.word.run(async (ctx) => {
      const paragraphs = await this.paragraphs(ctx);
      if (paragraphs.length !== expected.length || next.length !== expected.length)
        throw new Error('Paragraph structure changed while preparing the pull.');
      // Validate the whole document before queuing any write.
      const controls = paragraphs.map((p, i) => {
        const controls = p.contentControls.items.filter((c) => c.tag.startsWith(prefix));
        if (
          controls.length !== 1 ||
          controls[0].tag !== prefix + expected[i].id ||
          p.text.replace(/\r$/, '') !== expected[i].text ||
          next[i].id !== expected[i].id
        )
          throw new Error(
            'The file changed after the pull preview. Capture again before applying.'
          );
        return controls[0];
      });
      await this.guard();
      controls.forEach((c, i) => {
        if (expected[i].text !== next[i].text) c.insertText(next[i].text, 'Replace');
      });
      await ctx.sync();
    });
  }
}
