import { byPosition } from './api';
import { applyPayload } from '$lib/materializer';
import { rebaseSheetMetadata } from './sheet-references';
import { FormulaEngine } from './formula';
import { ulid } from 'ulid';
import type { DocumentState, EventPayload, MaterializedNode } from '$lib/api/types';
import { rowsForSheet } from './files';
import { cellFormat, columnName, sheetLayout, validationError, workbookInput } from './sheet-model';
import { parseAddress, type CellRange } from './sheet-operations';

export { workbookNames, namedRangeOps } from './workbook-names';
import { workbookNames } from './workbook-names';
export function checkRange(
  state: DocumentState,
  sheet: MaterializedNode,
  address: string
): CellRange {
  const range = parseAddress(address);
  const rows = rowsForSheet(state, sheet.id);
  if (!range || range.r2 >= rows.length || range.c2 >= Number(sheet.current_fields.columns || 12))
    throw new Error('Enter a range inside this sheet, such as A1:C10.');
  return range;
}

/** Sort a contiguous set of whole rows, preserving IDs, formatting and formula references. */
export function sortSheet(
  state: DocumentState,
  sheet: MaterializedNode,
  column: number,
  descending = false,
  header = false
): EventPayload[] {
  if (sheetLayout(sheet.current_fields.sheet_layout).merges.length)
    throw new Error('Unmerge cells before sorting.');
  const rows = rowsForSheet(state, sheet.id);
  const hf = FormulaEngine.buildFromSheets(workbookInput(state), workbookNames(state));
  try {
    const id = hf.getSheetId(String(sheet.current_fields.label || sheet.id))!;
    const values = hf.getSheetValues(id);
    const compare = (a: number, b: number) => {
      const x = values[a]?.[column],
        y = values[b]?.[column];
      if (x === null || x === undefined || x === '')
        return y === null || y === undefined || y === '' ? a - b : 1;
      if (y === null || y === undefined || y === '') return -1;
      const result =
        typeof x === 'number' && typeof y === 'number'
          ? x - y
          : String(x).localeCompare(String(y), undefined, { numeric: true, sensitivity: 'base' });
      return result * (descending ? -1 : 1) || a - b;
    };
    const order = rows.map((_, i) => i);
    const sorted = header ? [0, ...order.slice(1).sort(compare)] : order.sort(compare);
    const permutation = Array(rows.length).fill(0);
    sorted.forEach((old, index) => (permutation[old] = index));
    hf.setRowOrder(id, permutation);
    const ops: EventPayload[] = sorted.flatMap((old, index) =>
      old === index
        ? []
        : [
            {
              type: 'NodeMoved',
              node_id: rows[old].id,
              new_parent_id: sheet.id,
              new_pos: String(index).padStart(8, '0')
            }
          ]
    );
    for (const other of Object.values(state.nodes).filter(
      (n) => !n.deleted && n.current_fields.kind === 'sheet'
    )) {
      const originalRows = rowsForSheet(state, other.id);
      const reordered = other.id === sheet.id ? sorted.map((i) => originalRows[i]) : originalRows;
      const data = hf.getSheetSerialized(
        hf.getSheetId(String(other.current_fields.label || other.id))!
      );
      reordered.forEach((row, r) => {
        for (let c = 0; c < Number(other.current_fields.columns || 12); c++) {
          const old = String(row.current_fields[`cell_${c}`] ?? '');
          const next = data[r]?.[c];
          if (old.startsWith('=') && typeof next === 'string' && next !== old)
            ops.push({ type: 'FieldEdited', node_id: row.id, field: `cell_${c}`, value: next });
        }
      });
    }
    const root = Object.values(state.nodes).find((n) => !n.deleted && n.type === 'form');
    const names = hf
      .getAllNamedExpressionsSerialized()
      .map(({ name, expression }) => ({ name, expression }));
    if (root && JSON.stringify(names) !== JSON.stringify(workbookNames(state)))
      ops.push({ type: 'FieldEdited', node_id: root.id, field: 'named_ranges', value: names });
    if (ops.length > 5000)
      throw new Error('This sort would change more than 5,000 fields. Split the workbook first.');
    return ops;
  } finally {
    hf.destroy();
  }
}

export interface SheetChart {
  id: string;
  title: string;
  type: 'column' | 'bar' | 'line' | 'pie';
  range: string;
  x: number;
  y: number;
  width: number;
  height: number;
}
export function sheetCharts(value: unknown): SheetChart[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter(
      (c): c is SheetChart =>
        c &&
        typeof c.id === 'string' &&
        typeof c.title === 'string' &&
        ['column', 'bar', 'line', 'pie'].includes(c.type) &&
        typeof c.range === 'string' &&
        !!parseAddress(c.range)
    )
    .slice(0, 12)
    .map((c) => ({
      ...c,
      title: c.title.slice(0, 150),
      x: Math.max(0, Math.min(5000, Number.isFinite(Number(c.x)) ? Number(c.x) : 160)),
      y: Math.max(0, Math.min(5000, Number.isFinite(Number(c.y)) ? Number(c.y) : 60)),
      width: Math.max(360, Math.min(1000, Number(c.width) || 560)),
      height: Math.max(240, Math.min(700, Number(c.height) || 320))
    }));
}
export function chartData(chart: SheetChart, values: unknown[][]) {
  const range = parseAddress(chart.range);
  if (!range) return { labels: [] as string[], series: [] as { name: string; values: number[] }[] };
  const end = Math.min(range.r2, range.r1 + 200);
  const labels = Array.from({ length: Math.max(0, end - range.r1) }, (_, i) =>
    String(values[range.r1 + i + 1]?.[range.c1] ?? '')
  );
  const series = Array.from({ length: Math.max(0, Math.min(8, range.c2 - range.c1)) }, (_, i) => ({
    name: String(values[range.r1]?.[range.c1 + i + 1] ?? columnName(range.c1 + i + 1)),
    values: labels.map((_, r) => {
      const v = values[range.r1 + r + 1]?.[range.c1 + i + 1];
      return typeof v === 'number' && Number.isFinite(v) ? v : 0;
    })
  }));
  return { labels, series };
}

export interface PivotDefinition {
  sheetId: string;
  range: string;
  row: number;
  column: number | null;
  value: number;
  aggregate: 'sum' | 'count' | 'average' | 'min' | 'max';
}
export function pivotValues(
  values: unknown[][],
  definition: PivotDefinition
): (string | number)[][] {
  const x = parseAddress(definition.range);
  if (!x || x.r2 <= x.r1) throw new Error('Select a header row and at least one data row.');
  if (
    [
      definition.row,
      definition.value,
      ...(definition.column === null ? [] : [definition.column])
    ].some((c) => c < x.c1 || c > x.c2)
  )
    throw new Error('Pivot fields must be inside the source range.');
  const groups = new Map<string, Map<string, unknown[]>>(),
    columns = new Set<string>();
  for (let r = x.r1 + 1; r <= Math.min(x.r2, 2999); r++) {
    const source = values[r] || [];
    if (source.slice(x.c1, x.c2 + 1).every((v) => v === null || v === undefined || v === ''))
      continue;
    const key = String(source[definition.row] ?? '(blank)'),
      column =
        definition.column === null
          ? definition.aggregate.toUpperCase()
          : String(source[definition.column] ?? '(blank)');
    columns.add(column);
    if (!groups.has(key)) groups.set(key, new Map());
    const group = groups.get(key)!;
    if (!group.has(column)) group.set(column, []);
    group.get(column)!.push(source[definition.value]);
  }
  if (columns.size > 98 || groups.size > 2998)
    throw new Error('The pivot result exceeds the supported sheet size.');
  const cols = [...columns].sort();
  const aggregate = (items: unknown[]) => {
    const numbers = items.filter((v): v is number => typeof v === 'number' && Number.isFinite(v));
    if (definition.aggregate === 'count')
      return items.filter((v) => v !== null && v !== undefined && v !== '').length;
    if (!numbers.length) return 0;
    if (definition.aggregate === 'min') return Math.min(...numbers);
    if (definition.aggregate === 'max') return Math.max(...numbers);
    const sum = numbers.reduce((a, b) => a + b, 0);
    return definition.aggregate === 'average' ? sum / numbers.length : sum;
  };
  const output: (string | number)[][] = [
    [String(values[x.r1]?.[definition.row] || 'Row'), ...cols, 'Grand total']
  ];
  for (const [name, group] of [...groups].sort(([a], [b]) => a.localeCompare(b)))
    output.push([
      name,
      ...cols.map((c) => aggregate(group.get(c) || [])),
      aggregate([...group.values()].flat())
    ]);
  output.push([
    'Grand total',
    ...cols.map((c) => aggregate([...groups.values()].flatMap((g) => g.get(c) || []))),
    aggregate([...groups.values()].flatMap((g) => [...g.values()].flat()))
  ]);
  return output;
}
export function pivotOps(
  state: DocumentState,
  definition: PivotDefinition,
  values: unknown[][],
  target?: MaterializedNode
): { ops: EventPayload[]; id: string } {
  const data = pivotValues(values, definition),
    id = target?.id || ulid();
  const root = Object.values(state.nodes).find((n) => !n.deleted && n.type === 'form');
  if (!root) throw new Error('Workbook root is missing.');
  const sheets = Object.values(state.nodes).filter(
    (n) => !n.deleted && n.current_fields.kind === 'sheet'
  );
  let name = 'PivotTable';
  for (let i = 1; sheets.some((s) => s.current_fields.label === name); i++)
    name = `PivotTable ${i}`;
  const ops: EventPayload[] = target
    ? []
    : [
        {
          type: 'NodeCreated',
          node_id: id,
          node_type: 'section',
          parent_id: root.id,
          pos: (sheets.sort((a, b) => byPosition(a, b)).at(-1)?.pos || '') + 'V',
          fields: { kind: 'sheet', label: name }
        }
      ];
  ops.push(
    { type: 'FieldEdited', node_id: id, field: 'pivot_source', value: definition },
    { type: 'FieldEdited', node_id: id, field: 'columns', value: Math.max(12, data[0].length) },
    {
      type: 'FieldEdited',
      node_id: id,
      field: 'sheet_layout',
      value: {
        ...sheetLayout(target?.current_fields.sheet_layout),
        frozenRows: 1,
        widths: { 0: 180 }
      }
    }
  );
  const existing = target ? rowsForSheet(state, id) : [];
  const byKey = new Map(existing.map((n) => [String(n.current_fields.pivot_key), n]));
  const retained = new Set<string>();
  data.forEach((row, index) => {
    const key = index === 0 ? 'header' : index === data.length - 1 ? 'total' : `row:${row[0]}`;
    const node = byKey.get(key),
      nodeId = node?.id || ulid();
    retained.add(nodeId);
    const fields: Record<string, unknown> = { pivot_key: key };
    row.forEach((v, c) => {
      fields[`cell_${c}`] = String(v);
      fields[`format_${c}`] = {
        bold: index === 0 || index === data.length - 1,
        background: index === 0 ? '#107c41' : index === data.length - 1 ? '#e2f0d9' : '#ffffff',
        color: index === 0 ? '#ffffff' : '#202124'
      };
    });
    if (!node)
      ops.push({
        type: 'NodeCreated',
        node_id: nodeId,
        node_type: 'item',
        parent_id: id,
        pos: String(index).padStart(8, '0'),
        fields
      });
    else {
      if (node.pos !== String(index).padStart(8, '0'))
        ops.push({
          type: 'NodeMoved',
          node_id: nodeId,
          new_parent_id: id,
          new_pos: String(index).padStart(8, '0')
        });
      for (let c = row.length; c < Number(target?.current_fields.columns || 12); c++) {
        fields[`cell_${c}`] = '';
        fields[`format_${c}`] = {};
      }
      for (const [field, value] of Object.entries(fields))
        if (JSON.stringify(node.current_fields[field]) !== JSON.stringify(value))
          ops.push({ type: 'FieldEdited', node_id: nodeId, field, value });
    }
  });
  for (const row of existing)
    if (!retained.has(row.id)) ops.push({ type: 'NodeDeleted', node_id: row.id });
  if (ops.length > 5000)
    throw new Error('This pivot needs more than 5,000 edits. Use a smaller source range.');
  return { ops, id };
}

export function splitColumn(
  state: DocumentState,
  sheet: MaterializedNode,
  range: CellRange,
  delimiter: string
): EventPayload[] {
  if (!delimiter) throw new Error('Choose a delimiter.');
  const rows = rowsForSheet(state, sheet.id),
    ops: EventPayload[] = [];
  for (let r = range.r1; r <= range.r2; r++) {
    const row = rows[r],
      raw = String(row?.current_fields[`cell_${range.c1}`] ?? '');
    if (raw.startsWith('=')) throw new Error('Convert source formulas to values before splitting.');
    const parts = raw.split(delimiter);
    if (range.c1 + parts.length > Number(sheet.current_fields.columns || 12))
      throw new Error('Add enough destination columns first.');
    parts.forEach((value, i) => {
      const c = range.c1 + i,
        old = String(row.current_fields[`cell_${c}`] ?? '');
      if (i > 0 && old !== '')
        throw new Error('The destination contains data. Clear it or choose empty columns first.');
      const problem = validationError(value, cellFormat(row, c));
      if (problem) throw new Error(problem);
      if (value !== old)
        ops.push({ type: 'FieldEdited', node_id: row.id, field: `cell_${c}`, value });
    });
  }
  if (ops.length > 4000) throw new Error('Split up to 4,000 cells at a time.');
  return ops;
}

export function removeDuplicateRows(
  state: DocumentState,
  sheet: MaterializedNode,
  range: CellRange,
  header = true
): EventPayload[] {
  if (sheetLayout(sheet.current_fields.sheet_layout).merges.length)
    throw new Error('Unmerge cells before removing duplicate rows.');
  const rows = rowsForSheet(state, sheet.id),
    seen = new Set<string>(),
    removed = new Set<number>();
  const hf = FormulaEngine.buildFromSheets(workbookInput(state), workbookNames(state));
  try {
    const id = hf.getSheetId(String(sheet.current_fields.label || sheet.id))!,
      values = hf.getSheetValues(id);
    for (let r = range.r1 + (header ? 1 : 0); r <= range.r2; r++) {
      const key = JSON.stringify(
        Array.from({ length: range.c2 - range.c1 + 1 }, (_, i) => values[r]?.[range.c1 + i] ?? '')
      );
      if (seen.has(key)) removed.add(r);
      else seen.add(key);
    }
    if (!removed.size) return [];
    if (removed.size >= rows.length) throw new Error('Keep at least one row.');
    hf.removeRows(id, ...[...removed].map((i) => [i, 1] as [number, number]));
    const ops: EventPayload[] = [...removed].map((i) => ({
      type: 'NodeDeleted',
      node_id: rows[i].id
    }));
    for (const other of Object.values(state.nodes).filter(
      (n) => !n.deleted && n.current_fields.kind === 'sheet'
    )) {
      const remaining = rowsForSheet(state, other.id).filter(
        (_, i) => other.id !== sheet.id || !removed.has(i)
      );
      const data = hf.getSheetSerialized(
        hf.getSheetId(String(other.current_fields.label || other.id))!
      );
      remaining.forEach((row, r) => {
        for (let c = 0; c < Number(other.current_fields.columns || 12); c++) {
          const old = String(row.current_fields[`cell_${c}`] ?? ''),
            next = data[r]?.[c];
          if (old.startsWith('=') && typeof next === 'string' && next !== old)
            ops.push({ type: 'FieldEdited', node_id: row.id, field: `cell_${c}`, value: next });
        }
      });
    }
    const metadataState = JSON.parse(JSON.stringify(state)) as DocumentState;
    for (const index of [...removed].sort((a, b) => b - a)) {
      const changes = rebaseSheetMetadata(metadataState, sheet.id, 'row', index, -1);
      changes.forEach((op) => applyPayload(metadataState, op));
      ops.push(...changes);
    }
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
      throw new Error('This operation changes more than 5,000 fields. Use a smaller range.');
    return ops;
  } finally {
    hf.destroy();
  }
}
