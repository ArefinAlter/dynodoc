import { workbookNames } from './workbook-names';
import { HyperFormula } from 'hyperformula';
import type { DocumentState, EventPayload, MaterializedNode } from '$lib/api/types';
import { rowsForSheet } from './files';
import {
  workbookInput,
  cellFormat,
  validationError,
  sheetLayout,
  columnName,
  type SheetLayout
} from './sheet-model';

export interface CellRange {
  r1: number;
  r2: number;
  c1: number;
  c2: number;
}
export function fillRange(
  state: DocumentState,
  sheet: MaterializedNode,
  range: CellRange,
  direction: 'down' | 'right'
): EventPayload[] {
  const rows = rowsForSheet(state, sheet.id),
    ops: EventPayload[] = [];
  if ((range.r2 - range.r1 + 1) * (range.c2 - range.c1 + 1) > 2000)
    throw new Error('Fill up to 2,000 cells at a time.');
  const hf = HyperFormula.buildFromSheets(
    workbookInput(state),
    { licenseKey: 'gpl-v3' },
    workbookNames(state)
  );
  try {
    const id = hf.getSheetId(String(sheet.current_fields.label || sheet.id))!;
    for (let r = range.r1; r <= range.r2; r++)
      for (let c = range.c1; c <= range.c2; c++) {
        const sourceR = direction === 'down' ? range.r1 : r,
          sourceC = direction === 'right' ? range.c1 : c;
        if (sourceR === r && sourceC === c) continue;
        if (!rows[r] || !rows[sourceR]) throw new Error('Select a valid range.');
        hf.copy({
          start: { sheet: id, row: sourceR, col: sourceC },
          end: { sheet: id, row: sourceR, col: sourceC }
        });
        hf.paste({ sheet: id, row: r, col: c });
        const result = hf.getCellSerialized({ sheet: id, row: r, col: c });
        const original = String(rows[sourceR].current_fields[`cell_${sourceC}`] ?? '');
        const value = original.startsWith('=') ? String(result ?? '') : original;
        const format = cellFormat(rows[sourceR], sourceC);
        const error = validationError(value, cellFormat(rows[r], c));
        if (error) throw new Error(error);
        ops.push(
          { type: 'FieldEdited', node_id: rows[r].id, field: `cell_${c}`, value },
          { type: 'FieldEdited', node_id: rows[r].id, field: `format_${c}`, value: format }
        );
      }
    return ops;
  } finally {
    hf.destroy();
  }
}
export function mergeRange(
  state: DocumentState,
  sheet: MaterializedNode,
  range: CellRange,
  split = false
): EventPayload[] {
  const rows = rowsForSheet(state, sheet.id),
    layout = sheetLayout(sheet.current_fields.sheet_layout);
  if (!rows[range.r1] || !rows[range.r2]) throw new Error('Select cells to merge.');
  const overlaps = (m: SheetLayout['merges'][number]) => {
    const first = rows.findIndex((r) => r.id === m.fromRow),
      last = rows.findIndex((r) => r.id === m.toRow);
    return first <= range.r2 && last >= range.r1 && m.fromCol <= range.c2 && m.toCol >= range.c1;
  };
  if (split) layout.merges = layout.merges.filter((m) => !overlaps(m));
  else {
    if (range.r1 === range.r2 && range.c1 === range.c2)
      throw new Error('Select more than one cell to merge.');
    if (layout.merges.some(overlaps))
      throw new Error('Unmerge overlapping cells before merging this range.');
    for (let r = range.r1; r <= range.r2; r++)
      for (let c = range.c1; c <= range.c2; c++)
        if (
          (r !== range.r1 || c !== range.c1) &&
          String(rows[r].current_fields[`cell_${c}`] ?? '') !== ''
        )
          throw new Error(
            'Only the upper-left cell may contain a value. Move other values first; merging never discards them.'
          );
    layout.merges.push({
      fromRow: rows[range.r1].id,
      toRow: rows[range.r2].id,
      fromCol: range.c1,
      toCol: range.c2
    });
  }
  return [{ type: 'FieldEdited', node_id: sheet.id, field: 'sheet_layout', value: layout }];
}
export function rangeAddress(range: CellRange) {
  return `${columnName(range.c1)}${range.r1 + 1}:${columnName(range.c2)}${range.r2 + 1}`;
}
export function parseAddress(value: string): CellRange | null {
  const match = /^([A-Z]{1,3})([1-9]\d{0,5})(?::([A-Z]{1,3})([1-9]\d{0,5}))?$/i.exec(value.trim());
  if (!match) return null;
  const index = (name: string) =>
    [...name.toUpperCase()].reduce((n, c) => n * 26 + c.charCodeAt(0) - 64, 0) - 1;
  const c1 = index(match[1]),
    c2 = index(match[3] || match[1]),
    r1 = Number(match[2]) - 1,
    r2 = Number(match[4] || match[2]) - 1;
  return { r1: Math.min(r1, r2), r2: Math.max(r1, r2), c1: Math.min(c1, c2), c2: Math.max(c1, c2) };
}
