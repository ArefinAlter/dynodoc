import { describe, expect, it } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { zipSync, strToU8, unzipSync, strFromU8 } from 'fflate';
import { emptyState, applyPayload } from '$lib/materializer';
import { rootOp, itemOp } from './files';
import { writeWorkbook } from './xlsx-export';
import { readWorkbook } from './xlsx-import';
import { readDocx } from './docx-import';
import { writeDocx } from './docx-export';
import {
  DocumentNote,
  DocumentContents,
  documentNotes,
  contentsEntries
} from './document-references';
import { sheetStructure } from './sheet-structure';
async function blobBytes(blob: Blob): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(new Uint8Array(r.result as ArrayBuffer));
    r.onerror = reject;
    r.readAsArrayBuffer(blob);
  });
}
function book() {
  const state = emptyState();
  applyPayload(state, rootOp('root', 'Book'));
  applyPayload(state, {
    type: 'NodeCreated',
    node_id: 'sheet',
    node_type: 'section',
    parent_id: 'root',
    pos: '0',
    fields: { label: 'Sales', kind: 'sheet', columns: 3 }
  });
  [
    ['Month', 'Revenue', 'Costs'],
    ['Jan', '20', '10'],
    ['Feb', '30', '15']
  ].forEach((row, i) =>
    applyPayload(
      state,
      itemOp('sheet', i, Object.fromEntries(row.map((v, c) => [`cell_${c}`, v])), `row${i}`)
    )
  );
  return state;
}
describe('Office advanced interchange', () => {
  it('gives copied note references independent identities without changing their bodies', () => {
    const editor = new Editor({
      extensions: [StarterKit, DocumentNote],
      content: {
        type: 'doc',
        content: [
          {
            type: 'paragraph',
            content: [
              {
                type: 'documentNote',
                attrs: { id: 'original', kind: 'footnote', body: 'Evidence' }
              }
            ]
          }
        ]
      }
    });
    editor.commands.insertContentAt(2, {
      type: 'documentNote',
      attrs: { id: 'original', kind: 'footnote', body: 'Evidence' }
    });
    const notes = documentNotes(editor.state.doc);
    expect(new Set(notes.map((n) => n.id)).size).toBe(2);
    expect(notes[0].id).toBe('original');
    expect(notes.map((n) => n.body)).toEqual(['Evidence', 'Evidence']);
    editor.destroy();
  });
  it('round trips native charts, workbook names, page setup, print area and protection', async () => {
    const state = book();
    state.nodes.root.current_fields.named_ranges = [
      { name: 'Revenue', expression: '=Sales!$B$2:$B$3' }
    ];
    state.nodes.sheet.current_fields.charts = [
      {
        id: 'sales',
        title: 'Revenue & costs',
        type: 'column',
        range: 'A1:C3',
        x: 0,
        y: 60,
        width: 560,
        height: 320
      }
    ];
    state.nodes.sheet.current_fields.print_area = 'A1:C3';
    state.nodes.sheet.current_fields.sheet_protected = true;
    state.nodes.sheet.current_fields.page_setup = { size: 'A4', landscape: true, margin: 12.7 };
    const buffer = await writeWorkbook(state),
      parts = unzipSync(new Uint8Array(buffer));
    expect(strFromU8(parts['xl/charts/chart1.xml'])).toContain('&apos;Sales&apos;!$B$2:$B$3');
    const sheets = await readWorkbook(buffer);
    expect(sheets[0].charts?.[0]).toMatchObject({
      title: 'Revenue & costs',
      type: 'column',
      range: 'A1:C3',
      x: 0,
      y: 60
    });
    expect(sheets[0].names).toEqual([{ name: 'Revenue', expression: '=Sales!$B$2:$B$3' }]);
    expect(sheets[0].page).toMatchObject({ size: 'A4', landscape: true, margin: 12.7 });
    expect(sheets[0].printArea?.replace(/\$/g, '')).toBe('A1:C3');
    expect(sheets[0].protected).toBe(true);
  });
  it('rebases chart and pivot source metadata and invalidates a deleted pivot field', () => {
    const state = book();
    state.nodes.sheet.current_fields.charts = [
      { id: 'c', title: 'Chart', type: 'line', range: 'A1:C3', x: 0, y: 0, width: 500, height: 300 }
    ];
    state.nodes.sheet.current_fields.print_area = 'A1:C3';
    applyPayload(state, {
      type: 'NodeCreated',
      node_id: 'pivot',
      node_type: 'section',
      parent_id: 'root',
      pos: '1',
      fields: {
        label: 'Pivot',
        kind: 'sheet',
        columns: 3,
        pivot_source: {
          sheetId: 'sheet',
          range: 'A1:C3',
          row: 0,
          column: null,
          value: 1,
          aggregate: 'sum'
        }
      }
    });
    sheetStructure(state, 'sheet', 'insert-column', 0).forEach((op) => applyPayload(state, op));
    expect(state.nodes.sheet.current_fields.charts).toMatchObject([{ range: 'B1:D3' }]);
    expect(state.nodes.pivot.current_fields.pivot_source).toMatchObject({
      range: 'B1:D3',
      row: 1,
      value: 2
    });
    sheetStructure(state, 'sheet', 'delete-column', 2).forEach((op) => applyPayload(state, op));
    expect(state.nodes.pivot.current_fields.pivot_source).toMatchObject({ range: '' });
  });
  it('retains actual footnote and endnote bodies through DOCX and editor serialization', async () => {
    const source = zipSync({
      'word/document.xml': strToU8(
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Evidence</w:t><w:footnoteReference w:id="1"/><w:endnoteReference w:id="2"/></w:r></w:p></w:body></w:document>'
      ),
      'word/footnotes.xml': strToU8(
        '<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:id="1"><w:p><w:r><w:t>Source বাংলা</w:t></w:r></w:p></w:footnote></w:footnotes>'
      ),
      'word/endnotes.xml': strToU8(
        '<w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:endnote w:id="2"><w:p><w:r><w:t>Appendix note</w:t></w:r></w:p></w:endnote></w:endnotes>'
      )
    });
    const imported = readDocx(source),
      editor = new Editor({
        extensions: [StarterKit, DocumentNote, DocumentContents],
        content: { type: 'doc', content: imported.blocks }
      });
    expect(documentNotes(editor.state.doc).map((n) => n.body)).toEqual([
      'Source বাংলা',
      'Appendix note'
    ]);
    const state = emptyState();
    applyPayload(state, rootOp('root', 'Notes'));
    applyPayload(state, itemOp('root', 0, { content: editor.getJSON().content![0] }, 'p'));
    const bytes = await blobBytes(await writeDocx(state, 'Notes')),
      parts = unzipSync(bytes);
    expect(strFromU8(parts['word/footnotes.xml'])).toContain('Source বাংলা');
    expect(strFromU8(parts['word/endnotes.xml'])).toContain('Appendix note');
    const reread = readDocx(bytes);
    expect(
      reread.blocks[0].content?.filter((n) => n.type === 'documentNote').map((n) => n.attrs?.body)
    ).toEqual(['Source বাংলা', 'Appendix note']);
    editor.destroy();
  });
  it('generates contents from heading styles and exports a native TOC field', async () => {
    const editor = new Editor({
      extensions: [StarterKit, DocumentNote, DocumentContents],
      content: {
        type: 'doc',
        content: [
          { type: 'heading', attrs: { level: 1 }, content: [{ type: 'text', text: 'Methods' }] },
          { type: 'paragraph', content: [{ type: 'text', text: 'Details' }] }
        ]
      }
    });
    const entries = contentsEntries(editor);
    expect(entries.map((e) => e.text)).toEqual(['Methods']);
    const state = emptyState();
    applyPayload(state, rootOp('root', 'Contents'));
    applyPayload(
      state,
      itemOp('root', 0, { content: { type: 'tableOfContents', attrs: { entries } } }, 'toc')
    );
    const parts = unzipSync(await blobBytes(await writeDocx(state, 'Contents')));
    expect(strFromU8(parts['word/document.xml'])).toContain('TOC');
    expect(strFromU8(parts['word/document.xml'])).toContain('Methods');
    editor.destroy();
  });
});
