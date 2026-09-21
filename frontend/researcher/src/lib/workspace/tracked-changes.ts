import { Mark, type Editor } from '@tiptap/core';
import { Fragment, type Node as PMNode } from '@tiptap/pm/model';
import { Plugin, PluginKey, type Transaction } from '@tiptap/pm/state';
import { ulid } from 'ulid';

export interface ReviewChange {
  id: string;
  kind: 'insert' | 'delete';
  author: string;
  date: string;
  text: string;
}
const key = new PluginKey('dynodocTrackedText');
export const REVIEW_META = 'dynodocReview';

function paragraphs(doc: PMNode) {
  const nodes: { node: PMNode; pos: number; id: string }[] = [];
  doc.descendants((node, pos) => {
    if (node.isTextblock) {
      nodes.push({ node, pos, id: String(node.attrs.blockId || pos) });
      return false;
    }
  });
  return nodes;
}
function sameStructure(before: PMNode, after: PMNode): boolean {
  if (before.eq(after)) return true;
  if (before.type !== after.type) return false;
  if (before.isTextblock)
    return (
      before.content.content.every((n) => n.isText) && after.content.content.every((n) => n.isText)
    );
  if (before.childCount !== after.childCount) return false;
  for (let i = 0; i < before.childCount; i++)
    if (!sameStructure(before.child(i), after.child(i))) return false;
  return true;
}

/** A review layer for text edits, not a replacement for the version-control engine.
 * Structural edits are explicitly refused while tracking is on. Native IME
 * composition is allowed to finish before applying review marks to its result. */
export const TrackedChange = Mark.create({
  name: 'trackedChange',
  inclusive: false,
  addOptions: () => ({
    enabled: false,
    author: 'Editor',
    onNotice: (message: string) => {
      void message;
    }
  }),
  addStorage() {
    return { enabled: this.options.enabled, author: this.options.author };
  },
  addAttributes: () => ({
    id: { default: '' },
    kind: { default: 'insert' },
    author: { default: 'Editor' },
    date: { default: '' }
  }),
  parseHTML: () => [
    {
      tag: 'span[data-review-id]',
      getAttrs: (el) => ({
        id: el.getAttribute('data-review-id'),
        kind: el.getAttribute('data-review-kind'),
        author: el.getAttribute('data-author'),
        date: el.getAttribute('data-date')
      })
    }
  ],
  renderHTML: ({ mark }) => [
    'span',
    {
      'data-review-id': mark.attrs.id,
      'data-review-kind': mark.attrs.kind,
      'data-author': mark.attrs.author,
      'data-date': mark.attrs.date,
      class: mark.attrs.kind === 'delete' ? 'review-delete' : 'review-insert',
      title: `${mark.attrs.kind === 'delete' ? 'Deleted' : 'Inserted'} by ${mark.attrs.author}`
    },
    0
  ],
  addProseMirrorPlugins() {
    const storage = this.storage,
      notice = this.options.onNotice;
    let compositionBase: PMNode | null = null;
    return [
      new Plugin({
        key,
        props: {
          handleDOMEvents: {
            compositionstart(view) {
              if (storage.enabled) compositionBase = view.state.doc;
              return false;
            },
            compositionend(view) {
              const base = compositionBase;
              compositionBase = null;
              if (base)
                setTimeout(() => {
                  if (!view.isDestroyed)
                    view.dispatch(view.state.tr.setMeta('reviewCompositionBase', base));
                }, 0);
              return false;
            }
          }
        },
        filterTransaction(tr, state) {
          if (
            !storage.enabled ||
            !tr.docChanged ||
            tr.getMeta(REVIEW_META) ||
            tr.getMeta('history$') ||
            tr.getMeta('preventUpdate')
          )
            return true;
          if (!sameStructure(state.doc, tr.doc)) {
            notice(
              'Track text edits supports wording within existing paragraphs. Turn it off for page breaks, tables, paragraph splits or other structural edits; those changes still appear in version history.'
            );
            return false;
          }
          return true;
        },
        appendTransaction(transactions, oldState, newState) {
          if (
            !storage.enabled ||
            compositionBase ||
            transactions.some(
              (t) => t.getMeta(REVIEW_META) || t.getMeta('history$') || t.getMeta('preventUpdate')
            )
          )
            return;
          const base = transactions
            .find((t) => t.getMeta('reviewCompositionBase'))
            ?.getMeta('reviewCompositionBase') as PMNode | undefined;
          if (!base && !transactions.some((t) => t.docChanged)) return;
          const before = paragraphs(base || oldState.doc),
            after = paragraphs(newState.doc);
          if (before.length !== after.length) return;
          const tr = newState.tr;
          for (let i = after.length - 1; i >= 0; i--) {
            const a = before[i],
              b = after[i];
            if (a.id !== b.id || a.node.textContent === b.node.textContent) continue;
            // Work in Unicode scalars, then map back to ProseMirror's UTF-16 positions.
            const oldChars = Array.from(a.node.textContent),
              newChars = Array.from(b.node.textContent);
            let prefix = 0,
              suffix = 0;
            while (
              prefix < oldChars.length &&
              prefix < newChars.length &&
              oldChars[prefix] === newChars[prefix]
            )
              prefix++;
            while (
              suffix < oldChars.length - prefix &&
              suffix < newChars.length - prefix &&
              oldChars.at(-1 - suffix) === newChars.at(-1 - suffix)
            )
              suffix++;
            const offset = newChars.slice(0, prefix).join('').length;
            const end = b.pos + 1 + newChars.slice(0, newChars.length - suffix).join('').length;
            const start = b.pos + 1 + offset;
            const neighbor = newState.doc
              .resolve(start)
              .nodeBefore?.marks.find(
                (m) =>
                  m.type.name === 'trackedChange' &&
                  m.attrs.kind === 'insert' &&
                  m.attrs.author === storage.author
              );
            const attrs = {
              id: neighbor?.attrs.id || ulid(),
              author: storage.author,
              date: neighbor?.attrs.date || new Date().toISOString()
            };
            const mark = newState.schema.marks.trackedChange;
            if (end > start) tr.addMark(start, end, mark.create({ ...attrs, kind: 'insert' }));
            const removed = a.node.content.cut(
              offset,
              oldChars.slice(0, oldChars.length - suffix).join('').length
            );
            const restore: PMNode[] = [];
            removed.forEach((n) => {
              // Deleting a pending insertion cancels it. Pending deletions remain deletions.
              const previous = n.marks.find((m) => m.type === mark);
              if (previous?.attrs.kind === 'insert') return;
              restore.push(
                previous ? n : n.mark([...n.marks, mark.create({ ...attrs, kind: 'delete' })])
              );
            });
            if (restore.length) tr.insert(start, Fragment.fromArray(restore));
          }
          if (tr.steps.length) return tr.setMeta(REVIEW_META, true);
        }
      })
    ];
  }
});

export function reviewChanges(doc: PMNode): ReviewChange[] {
  const changes = new Map<string, ReviewChange>();
  doc.descendants((node) => {
    const mark = node.marks.find((m) => m.type.name === 'trackedChange');
    if (!mark || !node.isText) return;
    const k = `${mark.attrs.id}:${mark.attrs.kind}`;
    const prev = changes.get(k);
    changes.set(k, { ...mark.attrs, text: (prev?.text || '') + node.text } as ReviewChange);
  });
  return [...changes.values()];
}
export function resolveReview(editor: Editor, accept: boolean, id?: string) {
  let tr: Transaction = editor.state.tr;
  const ranges: { from: number; to: number; kind: string }[] = [];
  editor.state.doc.descendants((node, pos) => {
    const mark = node.marks.find(
      (m) => m.type.name === 'trackedChange' && (!id || m.attrs.id === id)
    );
    if (node.isText && mark)
      ranges.push({ from: pos, to: pos + node.nodeSize, kind: mark.attrs.kind });
  });
  for (const range of ranges.reverse()) {
    if ((range.kind === 'delete') === accept) tr = tr.delete(range.from, range.to);
    else tr = tr.removeMark(range.from, range.to, editor.schema.marks.trackedChange);
  }
  editor.view.dispatch(tr.setMeta(REVIEW_META, true));
}
