import type { DocumentState, EventPayload } from '$lib/api/types';
import { parseAddress, rangeAddress } from './sheet-operations';
import { sheetCharts, type PivotDefinition } from './sheet-analysis';

/** Keep chart, print, table and pivot sources attached during structural edits. */
export function rebaseSheetMetadata(
  state: DocumentState,
  sheetId: string,
  axis: 'row' | 'column',
  index: number,
  delta: 1 | -1
): EventPayload[] {
  const ops: EventPayload[] = [];
  const address = (value: unknown) => {
    const range = typeof value === 'string' ? parseAddress(value) : null;
    if (!range) return '';
    const first = axis === 'row' ? 'r1' : 'c1',
      last = axis === 'row' ? 'r2' : 'c2';
    if (delta === -1 && range[first] === index && range[last] === index) return '';
    if (range[first] >= index) range[first] = Math.max(index, range[first] + delta);
    if (range[last] >= index) range[last] += delta;
    return range[last] >= range[first] ? rangeAddress(range) : '';
  };
  const sheet = state.nodes[sheetId];
  const charts = sheetCharts(sheet.current_fields.charts);
  if (charts.length)
    ops.push({
      type: 'FieldEdited',
      node_id: sheetId,
      field: 'charts',
      value: charts.flatMap((c) => {
        const range = address(c.range);
        return range ? [{ ...c, range }] : [];
      })
    });
  for (const field of ['print_area', 'table_range'])
    if (sheet.current_fields[field])
      ops.push({
        type: 'FieldEdited',
        node_id: sheetId,
        field,
        value: address(sheet.current_fields[field])
      });
  for (const node of Object.values(state.nodes)) {
    const pivot = node.current_fields.pivot_source as PivotDefinition | undefined;
    if (node.deleted || !pivot || pivot.sheetId !== sheetId) continue;
    const next = { ...pivot, range: address(pivot.range) };
    if (axis === 'column')
      for (const field of ['row', 'column', 'value'] as const) {
        const value = next[field];
        if (value !== null) {
          if (delta === -1 && value === index) next.range = '';
          next[field] = value >= index ? Math.max(index, value + delta) : value;
        }
      }
    ops.push({ type: 'FieldEdited', node_id: node.id, field: 'pivot_source', value: next });
  }
  return ops;
}
