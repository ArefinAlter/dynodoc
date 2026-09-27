import Papa from 'papaparse';
import type { ImportedSheet } from './xlsx-import';
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
export async function importFile(file: File, signal?: AbortSignal): Promise<ImportResult> {
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
  if (ext === 'pptx') {
    const { importPptx } = await import('./pptx-import');
    return importPptx(new Uint8Array(await file.arrayBuffer()), file.name);
  }
  const rootId = ulid();
  const warnings: string[] = [];
  const ops: EventPayload[] = [rootOp(rootId, file.name.replace(/\.[^.]+$/, ''))];
  if (ext === 'csv' || ext === 'xlsx') {
    const sheets: ImportedSheet[] = [];
    if (ext === 'csv') {
      const parsed = Papa.parse<string[]>(await file.text(), { skipEmptyLines: 'greedy' });
      if (parsed.errors.length) warnings.push(...parsed.errors.map((e) => e.message));
      const { importRedcap } = await import('./research-files');
      const dictionary = importRedcap(parsed.data, file.name);
      if (dictionary) return dictionary;
      sheets.push({ name: 'Sheet 1', rows: parsed.data });
    } else {
      const buffer = await file.arrayBuffer();
      if (typeof Worker !== 'undefined' && typeof window !== 'undefined') {
        const { convertWorkbook } = await import('./office-import');
        sheets.push(...(await convertWorkbook(buffer, signal)));
      } else {
        const { readWorkbook } = await import('./xlsx-import');
        sheets.push(...(await readWorkbook(buffer)));
      }
      warnings.push(
        'Values, formulas, common cell formats, notes, validation rules, merged cells, widths and frozen panes are imported. Numeric cell-value conditional formatting is imported. Rectangular same-sheet column, bar, line and pie charts and workbook names are imported. Other charts, macros, native pivot definitions, formula-based conditional formatting and external data connections are not reproduced. Review theme colors, custom number formats and formulas; the original remains available.'
      );
    }
    const { importXlsForm } = await import('./research-files');
    const form = importXlsForm(sheets, file.name);
    if (form) return form;
    if (sheets[0]?.names?.length)
      ops.push({
        type: 'FieldEdited',
        node_id: rootId,
        field: 'named_ranges',
        value: sheets[0].names
      });
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
          charts: sheet.charts || [],
          page_setup: sheet.page,
          print_area: sheet.printArea || '',
          columns: Math.max(12, ...sheet.rows.map((r) => r.length))
        }
      });
      const rowIds: string[] = [];
      sheet.rows.forEach((row, i) => {
        const fields: Record<string, unknown> = {};
        row.forEach((cell, c) => {
          fields[`cell_${c}`] = cell;
          if (sheet.formats) fields[`format_${c}`] = sheet.formats[i]?.[c] || {};
        });
        if (sheet.heights) fields.row_height = sheet.heights[i] || 28;
        if (sheet.hidden) fields.hidden = sheet.hidden[i] || false;
        const id = ulid();
        rowIds.push(id);
        ops.push(itemOp(sheetId, i, fields, id));
      });
      if (sheet.layout) {
        const node = ops.find((op) => op.type === 'NodeCreated' && op.node_id === sheetId);
        if (node?.type === 'NodeCreated')
          node.fields = {
            ...(node.fields as object),
            sheet_layout: {
              ...sheet.layout,
              merges: sheet.layout.merges.map((merge) => ({
                ...merge,
                fromRow: rowIds[Number(merge.fromRow)],
                toRow: rowIds[Number(merge.toRow)]
              }))
            }
          };
      }
      if (sheet.protected)
        ops.push({ type: 'FieldEdited', node_id: sheetId, field: 'sheet_protected', value: true });
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
    const result = await convertDocx(buffer, signal);
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
  throw new Error('Choose a DOCX, DOC, XLSX, CSV, PPTX, TXT, or Markdown file.');
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
  const { writeWorkbook } = await import('./xlsx-export');
  download(
    await writeWorkbook(state),
    name + '.xlsx',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
  );
}

/** A download that records its origin can later be uploaded back as reviewable changes. */
export async function exportDocument(
  state: DocumentState,
  name: string,
  origin?: { documentId: string; seq: number }
) {
  const { writeDocx } = await import('./docx-export');
  const { downloadName } = await import('./file-push');
  download(
    await writeDocx(state, name, origin),
    origin ? downloadName(name, origin.seq) : name + '.docx',
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
  );
}

export async function exportPresentation(state: DocumentState, name: string) {
  const { writePptx } = await import('./pptx-export');
  download(
    await writePptx(state, name),
    name + '.pptx',
    'application/vnd.openxmlformats-officedocument.presentationml.presentation'
  );
}
