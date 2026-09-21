import { describe, expect, it } from 'vitest';
import { zipSync, strToU8 } from 'fflate';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { TextStyleKit } from '@tiptap/extension-text-style';
import { OfficeParagraph, PageBreak } from './office-extensions';
import { TrackedChange, reviewChanges, resolveReview } from './tracked-changes';
import { readDocx } from './docx-import';
import { writeDocx } from './docx-export';
import { readOfficeZip, xmlText } from './bounded-zip';
import { rootOp, itemOp } from './files';
import { applyPayload } from '$lib/materializer';
import type { DocumentState } from '$lib/api/types';
import { WorkbookCalculator } from './workbook-calculator';
import { fillRange, mergeRange } from './sheet-operations';
import { sheetStructure } from './sheet-structure';
import { cellFormat, readCell, writeCell } from './sheet-model';
import ExcelJS from 'exceljs';

const empty = (): DocumentState => ({
  nodes: {},
  comments: [],
  suggestions: {},
  removed_choices: []
});
const xml = (body: string) =>
  `<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${body}</w:body></w:document>`;
function blobBuffer(blob: Blob): Promise<ArrayBuffer> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result as ArrayBuffer);
    r.onerror = reject;
    r.readAsArrayBuffer(blob);
  });
}
describe('Office document interoperability', () => {
  it('imports inherited styles, Bengali/Arabic, direction, text revisions and real table spans', () => {
    const bytes = zipSync({
      'word/document.xml': strToU8(
        xml(
          '<w:p><w:pPr><w:pStyle w:val="Custom"/><w:bidi/><w:spacing w:after="240"/></w:pPr><w:r><w:rPr><w:i/></w:rPr><w:t>বাংলা العربية</w:t></w:r><w:ins w:id="7" w:author="Sam"><w:r><w:t> new</w:t></w:r></w:ins><w:del w:id="8" w:author="Maya"><w:r><w:delText>old</w:delText></w:r></w:del></w:p><w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Merged</w:t></w:r></w:p></w:tc></w:tr></w:tbl>'
        )
      ),
      'word/styles.xml': strToU8(
        '<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:styleId="Base"><w:rPr><w:rFonts w:ascii="Noto Sans Bengali"/><w:color w:val="336699"/><w:sz w:val="28"/></w:rPr></w:style><w:style w:styleId="Custom"><w:basedOn w:val="Base"/><w:rPr><w:b/></w:rPr></w:style></w:styles>'
      )
    });
    const result = readDocx(bytes),
      p = result.blocks[0];
    expect(p.attrs).toMatchObject({ dir: 'rtl', spaceAfter: 12 });
    expect(p.content?.[0].marks).toEqual(
      expect.arrayContaining([
        { type: 'bold' },
        { type: 'italic' },
        {
          type: 'textStyle',
          attrs: { fontFamily: 'Noto Sans Bengali', fontSize: '14pt', color: '#336699' }
        }
      ])
    );
    expect(p.content?.[2].marks).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'trackedChange',
          attrs: expect.objectContaining({ kind: 'delete', author: 'Maya' })
        })
      ])
    );
    expect(result.blocks[1].content?.[0].content?.[0].attrs?.colspan).toBe(2);
  });
  it('round-trips supported formatting, links, review marks, page settings and lists through real DOCX bytes', async () => {
    const state = empty();
    applyPayload(state, rootOp('root', 'Example'));
    state.nodes.root.current_fields.page_setup = {
      size: 'Letter',
      landscape: true,
      margin: 12.7,
      header: 'Working copy',
      footer: 'Team',
      pageNumbers: true,
      language: 'bn'
    };
    applyPayload(
      state,
      itemOp(
        'root',
        0,
        {
          content: {
            type: 'paragraph',
            attrs: { dir: 'rtl', spaceAfter: 12 },
            content: [
              {
                type: 'text',
                text: 'مرحبا বাংলা',
                marks: [
                  { type: 'bold' },
                  {
                    type: 'textStyle',
                    attrs: { fontFamily: 'Arial', fontSize: '16pt', color: '#336699' }
                  },
                  { type: 'link', attrs: { href: 'https://example.test' } },
                  {
                    type: 'trackedChange',
                    attrs: { id: 'x', kind: 'insert', author: 'Sam', date: '2026-09-21T00:00:00Z' }
                  }
                ]
              }
            ]
          }
        },
        'p'
      )
    );
    applyPayload(
      state,
      itemOp(
        'root',
        1,
        {
          content: {
            type: 'orderedList',
            attrs: { start: 3 },
            content: [
              {
                type: 'listItem',
                content: [{ type: 'paragraph', content: [{ type: 'text', text: 'One' }] }]
              }
            ]
          }
        },
        'list'
      )
    );
    applyPayload(state, itemOp('root', 2, { content: { type: 'pageBreak' } }, 'break'));
    const bytes = new Uint8Array(await blobBuffer(await writeDocx(state, 'Example'))),
      result = readDocx(bytes);
    expect(result.page).toMatchObject({
      size: 'Letter',
      landscape: true,
      header: 'Working copy',
      footer: expect.stringContaining('Team')
    });
    expect(result.blocks[0].attrs?.dir).toBe('rtl');
    expect(result.blocks[0].content?.[0].marks).toEqual(
      expect.arrayContaining([
        { type: 'textStyle', attrs: { fontFamily: 'Arial', fontSize: '16pt', color: '#336699' } },
        { type: 'link', attrs: { href: 'https://example.test' } }
      ])
    );
    expect(result.blocks.some((b) => b.type === 'orderedList')).toBe(true);
    expect(result.blocks.some((b) => b.type === 'pageBreak')).toBe(true);
  });
  it('rejects entity declarations, excessive nesting and actual archive expansion', () => {
    expect(() => xmlText(strToU8('<!DOCTYPE x [<!ENTITY x "hello">]><x/>'))).toThrow(/entity/);
    expect(() => xmlText(strToU8('<x>'.repeat(130) + '</x>'.repeat(130)))).toThrow(/deeply/);
    const bomb = zipSync({ 'word/document.xml': new Uint8Array(33 * 1024 * 1024).fill(65) });
    expect(() => readOfficeZip(bomb, () => true)).toThrow(/expanded/);
  });
});

describe('tracked text review', () => {
  function editor(text = 'Original 😀 text') {
    return new Editor({
      element: document.createElement('div'),
      extensions: [
        StarterKit,
        TextStyleKit,
        OfficeParagraph,
        PageBreak,
        TrackedChange.configure({ enabled: true, author: 'Sam' })
      ],
      content: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] }
    });
  }
  it('retains deletions and resolves replacements in either direction', () => {
    const e = editor();
    e.commands.insertContentAt({ from: 1, to: 10 }, 'Revised ');
    expect(
      reviewChanges(e.state.doc)
        .map((c) => c.kind)
        .sort()
    ).toEqual(['delete', 'insert']);
    resolveReview(e, false);
    expect(e.getText()).toBe('Original 😀 text');
    e.commands.insertContentAt({ from: 1, to: 10 }, 'Revised ');
    resolveReview(e, true);
    expect(e.getText()).toBe('Revised 😀 text');
    e.destroy();
  });
  it('does not restore a deleted pending insertion and refuses structural edits', () => {
    const e = editor('One');
    e.commands.insertContentAt(4, ' two');
    e.commands.deleteRange({ from: 4, to: 8 });
    expect(e.getText()).toBe('One');
    expect(reviewChanges(e.state.doc)).toHaveLength(0);
    expect(e.commands.insertContentAt(4, { type: 'pageBreak' })).toBe(true); // command accepted; filter refuses transaction
    expect(e.state.doc.childCount).toBe(1);
    e.destroy();
  });
  it('updates review state after native composition without splitting its intermediate text', async () => {
    const e = editor('বাংলা');
    e.view.dom.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
    e.commands.insertContentAt(6, ' ভাষা');
    expect(reviewChanges(e.state.doc)).toHaveLength(0);
    e.view.dom.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(reviewChanges(e.state.doc)).toHaveLength(1);
    e.destroy();
  });
});

describe('spreadsheet fidelity and operations', () => {
  function state() {
    const s = empty();
    applyPayload(s, rootOp('root', 'Book'));
    applyPayload(s, {
      type: 'NodeCreated',
      node_id: 'sheet',
      node_type: 'section',
      parent_id: 'root',
      pos: '0',
      fields: { kind: 'sheet', label: 'Sheet 1', columns: 3 }
    });
    applyPayload(s, itemOp('sheet', 0, { cell_0: '2', cell_1: '=A1*2' }, 'a'));
    applyPayload(s, itemOp('sheet', 1, { cell_0: '3' }, 'b'));
    return s;
  }
  it('retains XLSX numbers, dates, formats, validation and notes', async () => {
    const book = new ExcelJS.Workbook(),
      sheet = book.addWorksheet('Data'),
      c = sheet.getCell('A1');
    c.value = 42;
    c.font = { name: 'Arial', size: 14, bold: true, color: { argb: 'FF336699' } };
    c.numFmt = '0.00';
    c.note = 'Review this';
    c.dataValidation = { type: 'whole', operator: 'between', formulae: [1, 50] };
    const parsed = readCell(c);
    expect(parsed.value).toBe('42');
    expect(parsed.format).toMatchObject({
      type: 'number',
      bold: true,
      color: '#336699',
      note: 'Review this'
    });
    writeCell(sheet.getCell('B1'), parsed.value, parsed.format);
    expect(sheet.getCell('B1').value).toBe(42);
    expect(sheet.getCell('B1').dataValidation.formulae).toEqual([1, 50]);
    const bytes = await book.xlsx.writeBuffer(),
      reloaded = new ExcelJS.Workbook();
    await reloaded.xlsx.load(bytes);
    expect(readCell(reloaded.worksheets[0].getCell('B1')).format.bold).toBe(true);
  });
  it('fills relative formulas and refuses destructive merges', () => {
    const s = state(),
      ops = fillRange(s, s.nodes.sheet, { r1: 0, r2: 1, c1: 1, c2: 1 }, 'down');
    ops.forEach((op) => applyPayload(s, op));
    expect(s.nodes.b.current_fields.cell_1).toBe('=A2*2');
    expect(() => mergeRange(s, s.nodes.sheet, { r1: 0, r2: 1, c1: 0, c2: 1 })).toThrow(
      /upper-left/
    );
  });
  it('reuses the formula graph while retaining text cells and rebases layouts', () => {
    const calculator = new WorkbookCalculator();
    expect(calculator.calculate({ Data: [['2', '=A1*2']] }).Data[0][1]).toBe(4);
    expect(calculator.calculate({ Data: [['3', '=A1*2']] }).Data[0][1]).toBe(6);
    calculator.destroy();
    const s = state();
    s.nodes.a.current_fields.cell_2 = '00123';
    s.nodes.a.current_fields.format_2 = { type: 'text' };
    s.nodes.sheet.current_fields.sheet_layout = {
      widths: { 2: 160 },
      hiddenColumns: [2],
      merges: []
    };
    sheetStructure(s, 'sheet', 'insert-column', 1).forEach((op) => applyPayload(s, op));
    expect(s.nodes.a.current_fields.cell_3).toBe('00123');
    expect(cellFormat(s.nodes.a, 3).type).toBe('text');
    expect(s.nodes.sheet.current_fields.sheet_layout).toMatchObject({
      widths: { 3: 160 },
      hiddenColumns: [3]
    });
  });
});
