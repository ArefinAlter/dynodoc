import Papa from 'papaparse';
import { ulid } from 'ulid';
import type { EventPayload, DocumentState } from '$lib/api/types';
import type { Kind } from './api';
export interface ImportResult {
  kind: Kind;
  ops: EventPayload[];
  warnings: string[];
  filename: string;
  rootId: string;
}
export interface TextBlock {
  type: string;
  text?: string;
  attrs?: Record<string, unknown>;
  content?: TextBlock[];
  marks?: { type: string; attrs?: Record<string, unknown> }[];
}
export function textOf(block: TextBlock): string {
  return block.text ?? (block.content || []).map(textOf).join(block.type === 'doc' ? '\n' : '');
}
export function rootOp(id: string, label: string): EventPayload {
  return { type: 'NodeCreated', node_id: id, node_type: 'form', pos: '000000', fields: { label } };
}
export function itemOp(
  parent: string,
  pos: number,
  fields: Record<string, unknown>,
  node_id = ulid()
): EventPayload {
  return {
    type: 'NodeCreated',
    node_id,
    node_type: 'item',
    parent_id: parent,
    pos: String(pos).padStart(8, '0'),
    fields
  };
}
export function paragraph(text: string): TextBlock {
  return { type: 'paragraph', content: text ? [{ type: 'text', text }] : [] };
}
export async function importFile(file: File): Promise<ImportResult> {
  if (file.size > 10 * 1024 * 1024)
    throw new Error('Please use a file smaller than 10 MB for this demo.');
  const ext = file.name.split('.').pop()?.toLowerCase();
  const rootId = ulid();
  const warnings: string[] = [];
  const ops: EventPayload[] = [rootOp(rootId, file.name.replace(/\.[^.]+$/, ''))];
  if (ext === 'csv' || ext === 'xlsx') {
    const sheets: { name: string; rows: string[][] }[] = [];
    if (ext === 'csv') {
      const parsed = Papa.parse<string[]>(await file.text(), { skipEmptyLines: 'greedy' });
      if (parsed.errors.length) warnings.push(...parsed.errors.map((e) => e.message));
      const { importRedcap } = await import('./research-files');
      const dictionary = importRedcap(parsed.data, file.name);
      if (dictionary) return dictionary;
      sheets.push({ name: 'Sheet 1', rows: parsed.data });
    } else {
      const { default: ExcelJS } = await import('exceljs');
      const book = new ExcelJS.Workbook();
      await book.xlsx.load(await file.arrayBuffer());
      for (const sheet of book.worksheets) {
        if (sheet.rowCount > 3001 || sheet.columnCount > 100)
          throw new Error('This demo supports up to 3,000 rows and 100 columns per workbook.');
        const rows: string[][] = [];
        sheet.eachRow({ includeEmpty: true }, (row) => {
          const cells: string[] = [];
          for (let col = 1; col <= Math.min(sheet.columnCount, 100); col++) {
            const cell = row.getCell(col);
            cells.push(cell.formula ? '=' + cell.formula : cell.text);
          }
          rows.push(cells);
        });
        sheets.push({ name: sheet.name, rows });
      }
      warnings.push(
        'Cell values and formulas are imported. Charts, macros, external links, and workbook formatting are not preserved in the editable copy. The original upload remains available.'
      );
    }
    const { importXlsForm } = await import('./research-files');
    const form = importXlsForm(sheets, file.name);
    if (form) return form;
    const total = sheets.reduce((sum, s) => sum + s.rows.length, 0);
    if (total > 3000)
      throw new Error(
        'This demo supports up to 3,000 rows per import. Split this file into smaller tables.'
      );
    for (const sheet of sheets) {
      if (sheet.rows.some((r) => r.length > 100))
        throw new Error('This demo supports up to 100 columns.');
      const sheetId = ulid();
      ops.push({
        type: 'NodeCreated',
        node_id: sheetId,
        node_type: 'section',
        parent_id: rootId,
        pos: String(ops.length).padStart(8, '0'),
        fields: {
          label: sheet.name,
          kind: 'sheet',
          columns: Math.max(12, ...sheet.rows.map((r) => r.length))
        }
      });
      sheet.rows.forEach((row, i) => {
        const fields: Record<string, unknown> = {};
        row.forEach((cell, c) => (fields[`cell_${c}`] = cell));
        ops.push(itemOp(sheetId, i, fields));
      });
    }
    return { kind: 'spreadsheet', ops, warnings, filename: file.name, rootId };
  }
  if (ext === 'docx' || ext === 'doc') {
    let buffer = await file.arrayBuffer();
    if (ext === 'doc') {
      const data = new FormData();
      data.set('file', file);
      const response = await fetch('/api/convert-doc', { method: 'POST', body: data });
      if (!response.ok)
        throw new Error((await response.json()).error || 'Legacy Word conversion is unavailable.');
      buffer = await response.arrayBuffer();
    }
    const mammoth = await import('mammoth');
    const result = await mammoth.convertToHtml(
      { arrayBuffer: buffer },
      { convertImage: mammoth.images.imgElement(async () => ({ src: '' })) }
    );
    warnings.push(...result.messages.map((m) => m.message));
    warnings.push(
      'Text, headings, lists, and tables are imported. Page layout, embedded objects, images, and tracked-change fidelity require review. The original upload remains available.'
    );
    const parsed = new DOMParser().parseFromString(result.value, 'text/html');
    // Convert through the editor schema later; never inject unsanitized HTML into the DOM.
    const { generateJSON } = await import('@tiptap/core');
    const { default: StarterKit } = await import('@tiptap/starter-kit');
    const { TableKit } = await import('@tiptap/extension-table');
    parsed.querySelectorAll('script,style,iframe,object,img').forEach((el) => el.remove());
    parsed.querySelectorAll('a').forEach((a) => {
      if (!/^https?:\/\//i.test(a.getAttribute('href') || '')) a.removeAttribute('href');
    });
    const content = generateJSON(parsed.body.innerHTML, [StarterKit, TableKit]) as TextBlock;
    for (const [i, block] of (content.content || []).entries()) {
      const id = ulid();
      block.attrs = { ...block.attrs, blockId: id };
      ops.push(itemOp(rootId, i, { label: textOf(block), content: block }, id));
    }
    return { kind: 'document', ops, warnings, filename: file.name, rootId };
  }
  if (ext === 'txt' || ext === 'md') {
    (await file.text())
      .split(/\n/)
      .forEach((text, i) => ops.push(itemOp(rootId, i, { label: text, content: paragraph(text) })));
    return { kind: 'document', ops, warnings, filename: file.name, rootId };
  }
  throw new Error('Choose a DOCX, DOC, XLSX, CSV, TXT, or Markdown file.');
}
export function download(data: BlobPart, name: string, type: string) {
  const url = URL.createObjectURL(new Blob([data], { type }));
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}
export function rowsForSheet(state: DocumentState, sheetId: string) {
  return Object.values(state.nodes)
    .filter((n) => n.parent_id === sheetId && !n.deleted)
    .sort((a, b) => (a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : 0));
}
export async function exportSpreadsheet(
  state: DocumentState,
  name: string,
  format: 'csv' | 'xlsx',
  activeSheet?: string
) {
  const sheets = Object.values(state.nodes)
    .filter((n) => !n.deleted && n.type === 'section' && n.current_fields.kind === 'sheet')
    .sort((a, b) => (a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : 0));
  const grid = (id: string) =>
    rowsForSheet(state, id).map((row) => {
      const indices = Object.keys(row.current_fields)
        .filter((k) => /^cell_\d+$/.test(k))
        .map((k) => Number(k.slice(5)));
      return Array.from({ length: Math.max(0, ...indices) + 1 }, (_, i) =>
        String(row.current_fields[`cell_${i}`] ?? '')
      );
    });
  if (format === 'csv') {
    const id = activeSheet || sheets[0]?.id;
    if (!id) return;
    download(
      Papa.unparse(grid(id), { escapeFormulae: true }),
      name + '.csv',
      'text/csv;charset=utf-8'
    );
    return;
  }
  const { default: ExcelJS } = await import('exceljs');
  const book = new ExcelJS.Workbook();
  sheets.forEach((s, i) => {
    const label = String(s.current_fields.label || `Sheet ${i + 1}`)
      .replace(/[\\/*?:[\]]/g, '')
      .slice(0, 25);
    let worksheet;
    try {
      worksheet = book.addWorksheet(label);
    } catch {
      worksheet = book.addWorksheet(`Sheet ${i + 1}`);
    }
    worksheet.addRows(
      grid(s.id).map((row) =>
        row.map((value) => (value.startsWith('=') ? { formula: value.slice(1) } : value))
      )
    );
    worksheet.columns.forEach((col) => (col.width = 22));
  });
  download(
    (await book.xlsx.writeBuffer()) as ArrayBuffer,
    name + '.xlsx',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
  );
}
export async function exportDocument(state: DocumentState, name: string) {
  const { Document, Packer, Paragraph, TextRun, HeadingLevel, Table, TableRow, TableCell } =
    await import('docx');
  const nodes = Object.values(state.nodes)
    .filter((n) => !n.deleted && n.type === 'item')
    .sort((a, b) => (a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : 0));
  function runs(block: TextBlock): InstanceType<typeof TextRun>[] {
    if (block.text !== undefined) {
      const marks = block.marks || [];
      return [
        new TextRun({
          text: block.text,
          bold: marks.some((m) => m.type === 'bold'),
          italics: marks.some((m) => m.type === 'italic'),
          underline: marks.some((m) => m.type === 'underline') ? {} : undefined,
          strike: marks.some((m) => m.type === 'strike')
        })
      ];
    }
    return (block.content || []).flatMap(runs);
  }
  function blocks(
    block: TextBlock
  ): (InstanceType<typeof Paragraph> | InstanceType<typeof Table>)[] {
    if (block.type === 'table')
      return [
        new Table({
          rows: (block.content || []).map(
            (row) =>
              new TableRow({
                children: (row.content || []).map(
                  (cell) => new TableCell({ children: (cell.content || []).flatMap(blocks) })
                )
              })
          )
        })
      ];
    if (['bulletList', 'orderedList', 'listItem', 'blockquote'].includes(block.type))
      return (block.content || []).flatMap(blocks);
    const level = Number(block.attrs?.level) || 1;
    const heading =
      block.type === 'heading'
        ? [
            HeadingLevel.HEADING_1,
            HeadingLevel.HEADING_2,
            HeadingLevel.HEADING_3,
            HeadingLevel.HEADING_4,
            HeadingLevel.HEADING_5,
            HeadingLevel.HEADING_6
          ][level - 1]
        : undefined;
    return [new Paragraph({ children: runs(block), heading })];
  }
  const doc = new Document({
    sections: [
      {
        children: nodes.flatMap((n) =>
          blocks(
            (n.current_fields.content as TextBlock) ||
              paragraph(String(n.current_fields.label || ''))
          )
        )
      }
    ]
  });
  download(
    await Packer.toBlob(doc),
    name + '.docx',
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
  );
}
