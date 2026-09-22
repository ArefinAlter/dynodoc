import { ulid } from 'ulid';
import { Node, type Editor } from '@tiptap/core';
import type { Node as PMNode } from '@tiptap/pm/model';
import { Plugin } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
export function documentNotes(doc: PMNode) {
  const notes: { pos: number; id: string; kind: string; body: string; number: number }[] = [];
  const counters: Record<string, number> = { footnote: 0, endnote: 0 };
  doc.descendants((node, pos) => {
    if (node.type.name === 'documentNote') {
      const kind = node.attrs.kind === 'endnote' ? 'endnote' : 'footnote';
      notes.push({
        pos,
        id: String(node.attrs.id),
        kind,
        body: String(node.attrs.body || ''),
        number: ++counters[kind]
      });
    }
  });
  return notes;
}
export const DocumentNote = Node.create({
  name: 'documentNote',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  addAttributes() {
    return { id: { default: '' }, kind: { default: 'footnote' }, body: { default: '' } };
  },
  parseHTML() {
    return [
      {
        tag: 'sup[data-document-note]',
        getAttrs: (el) => ({
          id: el.getAttribute('data-note-id'),
          kind: el.getAttribute('data-document-note'),
          body: el.getAttribute('data-note-body')
        })
      }
    ];
  },
  renderHTML({ node }) {
    return [
      'sup',
      {
        'data-document-note': node.attrs.kind,
        'data-note-id': node.attrs.id,
        'data-note-body': node.attrs.body,
        title: node.attrs.body,
        tabindex: '0'
      },
      '†'
    ];
  },
  addProseMirrorPlugins() {
    return [
      new Plugin({
        appendTransaction(transactions, _old, state) {
          if (!transactions.some((t) => t.docChanged)) return null;
          const seen = new Set<string>(),
            tr = state.tr;
          state.doc.descendants((node, pos) => {
            if (node.type.name !== 'documentNote') return;
            let id = String(node.attrs.id || '');
            if (!id || seen.has(id)) {
              id = ulid();
              tr.setNodeMarkup(pos, undefined, { ...node.attrs, id });
            }
            seen.add(id);
          });
          return tr.docChanged ? tr : null;
        },
        props: {
          decorations(state) {
            return DecorationSet.create(
              state.doc,
              documentNotes(state.doc).map((n) =>
                Decoration.node(n.pos, n.pos + 1, {
                  'data-note-number': String(n.number),
                  'aria-label': `${n.kind} ${n.number}: ${n.body}`
                })
              )
            );
          }
        }
      })
    ];
  }
});
export function contentsEntries(editor: Editor) {
  const entries: { text: string; level: number; id: string }[] = [];
  editor.state.doc.descendants((node) => {
    if (node.type.name === 'heading' && Number(node.attrs.level) <= 3)
      entries.push({
        text: node.textContent,
        level: Number(node.attrs.level),
        id: String(node.attrs.blockId || '')
      });
  });
  return entries;
}
export const DocumentContents = Node.create({
  name: 'tableOfContents',
  group: 'block',
  atom: true,
  addAttributes() {
    return { entries: { default: [] } };
  },
  parseHTML() {
    return [
      {
        tag: 'nav[data-contents]',
        getAttrs: (el) => {
          try {
            return { entries: JSON.parse(el.getAttribute('data-entries') || '[]') };
          } catch {
            return { entries: [] };
          }
        }
      }
    ];
  },
  renderHTML({ node }) {
    const entries = Array.isArray(node.attrs.entries) ? node.attrs.entries.slice(0, 300) : [];
    return [
      'nav',
      { 'data-contents': 'true', 'data-entries': JSON.stringify(entries) },
      ['strong', {}, 'Contents'],
      ...entries.map((e: { text: string; level: number; id: string }) => [
        'div',
        { style: `margin-left:${Math.max(0, Math.min(2, e.level - 1)) * 16}px` },
        ['a', { 'data-heading-id': e.id, href: '#' + e.id }, String(e.text)]
      ])
    ];
  }
});
/** Browser print keeps numbered note bodies even though per-page footnote placement is DOCX-only. */
export function referencedHtml(editor: Editor) {
  const host = document.createElement('div');
  host.innerHTML = editor.getHTML();
  const notes = documentNotes(editor.state.doc);
  host.querySelectorAll('[data-document-note]').forEach((el, i) => {
    el.textContent = String(notes[i]?.number || i + 1);
  });
  for (const kind of ['footnote', 'endnote']) {
    const items = notes.filter((n) => n.kind === kind);
    if (!items.length) continue;
    const section = document.createElement('section'),
      title = document.createElement('h2');
    title.textContent = kind === 'footnote' ? 'Footnotes' : 'Endnotes';
    section.append(title);
    const list = document.createElement('ol');
    for (const note of items) {
      const item = document.createElement('li');
      item.textContent = note.body;
      list.append(item);
    }
    section.append(list);
    host.append(section);
  }
  return host.innerHTML;
}
