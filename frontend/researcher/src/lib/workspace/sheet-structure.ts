import { rebaseSheetMetadata } from './sheet-references';
import { workbookNames } from './workbook-names';
import { HyperFormula } from 'hyperformula';
import { workbookInput, sheetLayout } from './sheet-model';
import { ulid } from 'ulid';
import type { DocumentState, EventPayload } from '$lib/api/types';
import { rowsForSheet } from './files';
export type SheetOperation =
  | 'insert-row'
  | 'insert-column'
  | 'delete-row'
  | 'delete-column'
  | 'rename'
  | 'delete-sheet';
/** Rebase cell references through HyperFormula while retaining every surviving engine node ID. */
export function sheetStructure(
  state: DocumentState,
  targetId: string,
  action: SheetOperation,
  index = 0,
  name = ''
): EventPayload[] {
  const sheets = Object.values(state.nodes).filter(
    (n) => !n.deleted && n.type === 'section' && n.current_fields.kind === 'sheet'
  );
  const target = sheets.find((s) => s.id === targetId);
  if (!target) throw new Error('Choose a sheet.');
  const rows = rowsForSheet(state, targetId),
    width = Number(target.current_fields.columns) || 12;
  if (
    action === 'rename' &&
    (!name.trim() ||
      name.length > 31 ||
      /[\\/?*[\]:]/.test(name) ||
      sheets.some(
        (s) =>
          s.id !== targetId &&
          String(s.current_fields.label).toLowerCase() === name.trim().toLowerCase()
      ))
  )
    throw new Error('Use a unique sheet name of 1–31 characters, without / \\ ? * [ ] or :.');
  if (action === 'delete-sheet' && sheets.length <= 1) throw new Error('Keep at least one sheet.');
  if (action === 'insert-row' && rows.length >= 3000)
    throw new Error('A sheet supports up to 3,000 rows.');
  if (action === 'insert-column' && width >= 100)
    throw new Error('A sheet supports up to 100 columns.');
  if (action === 'delete-column' && width <= 1) throw new Error('Keep at least one column.');
  if (action === 'delete-row' && rows.length <= 1) throw new Error('Keep at least one row.');
  const hf = HyperFormula.buildFromSheets(
    workbookInput(state),
    { licenseKey: 'gpl-v3' },
    workbookNames(state)
  );
  const ids = new Map(
    sheets.map((s) => [s.id, hf.getSheetId(String(s.current_fields.label || s.id))!])
  );
  const id = ids.get(targetId)!;
  const ops: EventPayload[] = [];
  try {
    const inserted = ulid();
    if (action === 'insert-row') {
      hf.addRows(id, [index, 1]);
      ops.push({
        type: 'NodeCreated',
        node_id: inserted,
        node_type: 'item',
        parent_id: targetId,
        pos: 'pending',
        fields: {}
      });
    }
    if (action === 'delete-row') {
      if (!rows[index]) throw new Error('Choose a row.');
      hf.removeRows(id, [index, 1]);
      ops.push({ type: 'NodeDeleted', node_id: rows[index].id });
    }
    if (action === 'insert-column' || action === 'delete-column') {
      if (action === 'insert-column') hf.addColumns(id, [index, 1]);
      else hf.removeColumns(id, [index, 1]);
      ops.push({
        type: 'FieldEdited',
        node_id: targetId,
        field: 'columns',
        value: width + (action === 'insert-column' ? 1 : -1)
      });
    }
    if (action === 'rename') {
      hf.renameSheet(id, name.trim());
      ops.push({ type: 'FieldEdited', node_id: targetId, field: 'label', value: name.trim() });
    }
    if (action === 'delete-sheet') {
      hf.removeColumns(id, [0, hf.getConfig().maxColumns]);
      hf.removeSheet(id);
      ops.push(...rows.map((n) => ({ type: 'NodeDeleted', node_id: n.id }) as EventPayload), {
        type: 'NodeDeleted',
        node_id: targetId
      });
    }
    for (const s of sheets) {
      if (action === 'delete-sheet' && s.id === targetId) continue;
      const data = hf.getSheetSerialized(ids.get(s.id)!);
      const rowIds = rowsForSheet(state, s.id).map((n) => n.id);
      let columns = Number(s.current_fields.columns) || 12;
      if (s.id === targetId) {
        if (action === 'insert-row') rowIds.splice(index, 0, inserted);
        if (action === 'delete-row') rowIds.splice(index, 1);
        if (action === 'insert-column') columns++;
      }
      rowIds.forEach((nodeId, r) => {
        const fields = state.nodes[nodeId]?.current_fields || {};
        if (s.id === targetId && action === 'insert-row')
          ops.push({
            type: 'NodeMoved',
            node_id: nodeId,
            new_parent_id: targetId,
            new_pos: String(r).padStart(8, '0')
          });
        for (let c = 0; c < columns; c++) {
          const serialized = data[r]?.[c];
          const sourceColumn =
            s.id === targetId && c >= index
              ? action === 'insert-column'
                ? c - 1
                : action === 'delete-column'
                  ? c + 1
                  : c
              : c;
          const original =
            s.id === targetId && action === 'insert-column' && c === index
              ? ''
              : String(fields[`cell_${sourceColumn}`] ?? '');
          const value =
            original.startsWith('=') && typeof serialized === 'string' ? serialized : original;
          if (value !== String(fields[`cell_${c}`] ?? ''))
            ops.push({ type: 'FieldEdited', node_id: nodeId, field: `cell_${c}`, value });
          if (
            s.id === targetId &&
            c >= index &&
            (action === 'insert-column' || action === 'delete-column')
          )
            ops.push({
              type: 'FieldEdited',
              node_id: nodeId,
              field: `format_${c}`,
              value:
                action === 'insert-column' && c === index
                  ? {}
                  : fields[`format_${c + (action === 'insert-column' ? -1 : 1)}`] || {}
            });
        }
      });
    }
    const layout = sheetLayout(target.current_fields.sheet_layout);
    if (action === 'insert-column' || action === 'delete-column') {
      const delta = action === 'insert-column' ? 1 : -1;
      layout.widths = Object.fromEntries(
        Object.entries(layout.widths).flatMap(([key, value]) => {
          const c = Number(key);
          if (delta === -1 && c === index) return [];
          return [[String(c >= index ? c + delta : c), value]];
        })
      );
      layout.hiddenColumns = layout.hiddenColumns
        .filter((c) => !(delta === -1 && c === index))
        .map((c) => (c >= index ? c + delta : c));
      layout.merges = layout.merges
        .flatMap((m) => {
          if (delta === -1 && m.fromCol === m.toCol && m.fromCol === index) return [];
          return [
            {
              ...m,
              fromCol: m.fromCol >= index ? Math.max(index, m.fromCol + delta) : m.fromCol,
              toCol: m.toCol >= index ? m.toCol + delta : m.toCol
            }
          ];
        })
        .filter((m) => m.toCol >= m.fromCol);
      ops.push({ type: 'FieldEdited', node_id: targetId, field: 'sheet_layout', value: layout });
    }
    if (action === 'delete-row') {
      layout.merges = layout.merges
        .flatMap((m) => {
          if (m.fromRow === rows[index]?.id && m.toRow === rows[index]?.id) return [];
          return [
            {
              ...m,
              fromRow: m.fromRow === rows[index]?.id ? rows[index + 1]?.id : m.fromRow,
              toRow: m.toRow === rows[index]?.id ? rows[index - 1]?.id : m.toRow
            }
          ];
        })
        .filter((m) => m.fromRow && m.toRow);
      ops.push({ type: 'FieldEdited', node_id: targetId, field: 'sheet_layout', value: layout });
    }
    if (['insert-row', 'delete-row', 'insert-column', 'delete-column'].includes(action))
      ops.push(
        ...rebaseSheetMetadata(
          state,
          targetId,
          action.endsWith('row') ? 'row' : 'column',
          index,
          action.startsWith('insert') ? 1 : -1
        )
      );
    const root = Object.values(state.nodes).find((n) => !n.deleted && n.type === 'form');
    if (root && workbookNames(state).length)
      ops.push({
        type: 'FieldEdited',
        node_id: root.id,
        field: 'named_ranges',
        value: hf
          .getAllNamedExpressionsSerialized()
          .map(({ name, expression }) => ({ name, expression }))
      });
    if (ops.length > 5000)
      throw new Error('This operation changes more than 5,000 fields. Use a smaller workbook.');
    return ops;
  } finally {
    hf.destroy();
  }
}
