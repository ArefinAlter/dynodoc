import { stableJson } from './equality';
import { textOf, type TextBlock } from './files';

export type TextOp =
  | { kind: 'retain'; count: number; style?: number | null }
  | { kind: 'delete'; count: number }
  | { kind: 'insert'; text: string; style: number };
export type RichTextPatch = {
  version: number;
  before: string;
  after: string;
  styles: Record<string, unknown>[];
  edits: (
    | { kind: 'text'; path: number[]; ops: TextOp[] }
    | { kind: 'props'; path: number[]; value: TextBlock }
    | { kind: 'replace'; path: number[]; value: TextBlock }
  )[];
};
const MAX_SCALARS = 100_000;
// Svelte's live document state contains proxies, which structuredClone rejects.
// This protocol is JSON-only; serialization also detaches every nested proxy.
function cloneJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value));
}

/** Replay canonical server events. SHA-256 pre/postconditions are verified by the
 * Rust authority before append and during replay. Browser replay also validates
 * paths, style references, bounds and full input consumption. */
export function applyRichTextPatch(before: unknown, patch: RichTextPatch): TextBlock {
  if (patch.version !== 1 || patch.edits.length > 10_000 || patch.styles.length > 10_000)
    throw new Error('Unsupported rich-text change. Reload the document.');
  const root = { content: [cloneJson(before) as TextBlock] };
  for (const edit of patch.edits) {
    if (edit.path.length > 64) throw new Error('Rich-text path is too deep.');
    let parent = root as TextBlock;
    let index = 0;
    for (const step of edit.path) {
      parent = parent.content?.[index] as TextBlock;
      index = step;
      if (!Number.isSafeInteger(index) || index < 0) throw new Error('Invalid rich-text path.');
    }
    const target = parent?.content?.[index];
    if (!target) throw new Error('Missing rich-text container. Reload the document.');
    if (edit.kind === 'replace') {
      parent.content![index] = cloneJson(edit.value);
      continue;
    }
    if (edit.kind === 'props') {
      if ('content' in edit.value) throw new Error('Invalid rich-text properties.');
      parent.content![index] = {
        ...cloneJson(edit.value),
        ...(target.content === undefined ? {} : { content: target.content })
      };
      continue;
    }
    if (!['paragraph', 'heading', 'codeBlock'].includes(target.type))
      throw new Error('Invalid rich-text container.');
    const styles: Record<string, unknown>[] = [];
    const lookup = new Map<string, number>();
    const intern = (metadata: Record<string, unknown>) => {
      const key = stableJson(metadata);
      const found = lookup.get(key);
      if (found !== undefined) return found;
      const index = styles.length;
      lookup.set(key, index);
      styles.push(metadata);
      return index;
    };
    const input: { ch: string; style: number }[] = [];
    for (const child of target.content || []) {
      if (child.type !== 'text' || typeof child.text !== 'string' || child.content)
        throw new Error('Invalid rich-text run.');
      const metadata = { ...child };
      delete metadata.text;
      const style = intern(metadata);
      for (const ch of child.text) input.push({ ch, style });
      if (input.length > MAX_SCALARS) throw new Error('Rich-text container exceeds limit.');
    }
    const added = patch.styles.map((style) => {
      if (style.type !== 'text' || 'text' in style || 'content' in style)
        throw new Error('Invalid rich-text style.');
      return intern(style);
    });
    const styleAt = (index: number) => {
      if (!Number.isSafeInteger(index) || index < 0 || added[index] === undefined)
        throw new Error('Unknown rich-text style.');
      return added[index];
    };
    const output: { ch: string; style: number }[] = [];
    let cursor = 0;
    for (const op of edit.ops) {
      if (op.kind === 'insert') {
        const style = styleAt(op.style);
        if (op.text.length > MAX_SCALARS * 2) throw new Error('Rich-text insert exceeds limit.');
        for (const ch of op.text) output.push({ ch, style });
      } else {
        if (!Number.isSafeInteger(op.count) || op.count < 0 || cursor + op.count > input.length)
          throw new Error('Rich-text change is out of bounds. Reload the document.');
        const end = cursor + op.count;
        if (op.kind === 'retain') {
          const style = op.style == null ? undefined : styleAt(op.style);
          for (let i = cursor; i < end; i++)
            output.push({ ch: input[i].ch, style: style ?? input[i].style });
        }
        cursor = end;
      }
      if (output.length > MAX_SCALARS) throw new Error('Rich-text result exceeds limit.');
    }
    if (cursor !== input.length)
      throw new Error('Incomplete rich-text change. Reload the document.');
    const content: TextBlock[] = [];
    let lastStyle = -1;
    for (const token of output) {
      if (token.style === lastStyle) content[content.length - 1].text += token.ch;
      else {
        content.push({ ...cloneJson(styles[token.style]), type: 'text', text: token.ch });
        lastStyle = token.style;
      }
    }
    target.content = content;
  }
  return root.content[0];
}

export function richTextLabel(content: TextBlock): string {
  return textOf(content);
}

export function describeRichTextPatch(patch: RichTextPatch): string {
  let text = false,
    formatting = false;
  for (const edit of patch.edits) {
    if (edit.kind === 'replace') return 'Document content updated';
    if (edit.kind === 'props') formatting = true;
    if (edit.kind === 'text')
      for (const op of edit.ops) {
        if (op.kind === 'retain' && op.style != null) formatting = true;
        if (op.kind !== 'retain') text = true;
      }
  }
  return text && formatting
    ? 'Wording and formatting updated'
    : formatting
      ? 'Formatting updated'
      : 'Wording updated';
}
