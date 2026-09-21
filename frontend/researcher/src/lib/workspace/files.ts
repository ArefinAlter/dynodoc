import Papa from 'papaparse';
import type { Worksheet } from 'exceljs';
import { readCell, writeCell, readSheetLayout, sheetLayout, cellFormat } from './sheet-model';
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
  if (block.type === 'inlineMath' || block.type === 'blockMath')
    return String(block.attrs?.latex || '');
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
    throw new Error('Please use a file smaller than 10 MB per upload.');
  const ext = file.name.split('.').pop()?.toLowerCase();
  if (ext === 'tex') {
    const { importLatex } = await import('./latex');
    return importLatex(await file.text(), file.name);
  }
  if (ext === 'zip') {
    const { importOverleaf } = await import('./latex');
    return importOverleaf(file);
  }
  const rootId = ulid();
  const warnings: string[] = [];
  const ops: EventPayload[] = [rootOp(rootId, file.name.replace(/\.[^.]+$/, ''))];
  if (ext === 'csv' || ext === 'xlsx') {
    const sheets: { name: string; rows: string[][]; source?: Worksheet }[] = [];
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
      const data = await file.arrayBuffer();
      const { readOfficeZip } = await import('./bounded-zip');
      readOfficeZip(new Uint8Array(data), () => true);
      await book.xlsx.load(data);
      for (const sheet of book.worksheets) {
        if (sheet.rowCount > 3001 || sheet.columnCount > 100)
          throw new Error('This workspace supports up to 3,000 rows and 100 columns per workbook.');
        const rows: string[][] = [];
        sheet.eachRow({ includeEmpty: true }, (row) => {
          const cells: string[] = [];
          for (let col = 1; col <= Math.min(sheet.columnCount, 100); col++) {
            const cell = row.getCell(col);
            cells.push(
              cell.isMerged && cell.master.address !== cell.address ? '' : readCell(cell).value
            );
          }
          rows.push(cells);
        });
        sheets.push({ name: sheet.name, rows, source: sheet });
      }
      warnings.push(
        'Values, formulas, common cell formats, notes, validation rules, merged cells, widths and frozen panes are imported. Charts, macros, pivot tables, conditional formatting and external data connections are not reproduced. Review theme colors, custom number formats and formulas; the original remains available.'
      );
    }
    const { importXlsForm } = await import('./research-files');
    const form = importXlsForm(sheets, file.name);
    if (form) return form;
    const total = sheets.reduce((sum, s) => sum + s.rows.length, 0);
    if (total > 3000)
      throw new Error(
        'This workspace supports up to 3,000 rows per import. Split this file into smaller tables.'
      );
    for (const sheet of sheets) {
      if (sheet.rows.some((r) => r.length > 100))
        throw new Error('This workspace supports up to 100 columns.');
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
      const rowIds: string[] = [];
      sheet.rows.forEach((row, i) => {
        const fields: Record<string, unknown> = {};
        row.forEach((cell, c) => {
          fields[`cell_${c}`] = cell;
          if (sheet.source)
            fields[`format_${c}`] = readCell(sheet.source.getCell(i + 1, c + 1)).format;
        });
        if (sheet.source) {
          fields.row_height = sheet.source.getRow(i + 1).height
            ? (sheet.source.getRow(i + 1).height! * 4) / 3
            : 28;
          fields.hidden = sheet.source.getRow(i + 1).hidden || false;
        }
        const id = ulid();
        rowIds.push(id);
        ops.push(itemOp(sheetId, i, fields, id));
      });
      if (sheet.source) {
        const node = ops.find((op) => op.type === 'NodeCreated' && op.node_id === sheetId);
        if (node?.type === 'NodeCreated')
          node.fields = {
            ...(node.fields as object),
            sheet_layout: readSheetLayout(sheet.source, rowIds)
          };
      }
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
    const { convertDocx } = await import('./office-import');
    const result = await convertDocx(buffer);
    warnings.push(...result.warnings);
    const root = ops[0];
    if (root.type === 'NodeCreated')
      root.fields = { ...(root.fields as object), page_setup: result.page };
    for (const [i, block] of result.blocks.entries()) {
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
    const rows = rowsForSheet(state, s.id),
      layout = sheetLayout(s.current_fields.sheet_layout);
    rows.forEach((row, r) => {
      const count = Math.min(100, Number(s.current_fields.columns) || 12);
      for (let c = 0; c < count; c++)
        writeCell(
          worksheet.getCell(r + 1, c + 1),
          String(row.current_fields[`cell_${c}`] ?? ''),
          cellFormat(row, c)
        );
      if (row.current_fields.row_height)
        worksheet.getRow(r + 1).height = (Number(row.current_fields.row_height) * 3) / 4;
      worksheet.getRow(r + 1).hidden = row.current_fields.hidden === true;
    });
    for (let c = 0; c < (Number(s.current_fields.columns) || 12); c++) {
      worksheet.getColumn(c + 1).width = ((layout.widths[String(c)] || 118) - 5) / 7;
      worksheet.getColumn(c + 1).hidden = layout.hiddenColumns.includes(c);
    }
    if (layout.frozenRows || layout.frozenColumns)
      worksheet.views = [
        { state: 'frozen', xSplit: layout.frozenColumns, ySplit: layout.frozenRows }
      ];
    for (const merge of layout.merges) {
      const from = rows.findIndex((r) => r.id === merge.fromRow),
        to = rows.findIndex((r) => r.id === merge.toRow);
      if (from >= 0 && to >= from)
        worksheet.mergeCells(from + 1, merge.fromCol + 1, to + 1, merge.toCol + 1);
    }
  });
  download(
    (await book.xlsx.writeBuffer()) as ArrayBuffer,
    name + '.xlsx',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
  );
}
export async function exportDocument(state: DocumentState, name: string) {
  const { writeDocx } = await import('./docx-export');
  download(
    await writeDocx(state, name),
    name + '.docx',
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
  );
}
