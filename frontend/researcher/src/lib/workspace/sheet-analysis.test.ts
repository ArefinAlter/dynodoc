import { describe, expect, it } from 'vitest';
import { emptyState, applyPayload } from '$lib/materializer';
import { itemOp, rootOp, rowsForSheet } from './files';
import {
  namedRangeOps,
  sortSheet,
  pivotValues,
  pivotOps,
  removeDuplicateRows,
  splitColumn,
  chartData
} from './sheet-analysis';
import { sheetStructure } from './sheet-structure';
import { WorkbookCalculator } from './workbook-calculator';
import { workbookInput } from './sheet-model';
import { workbookNames } from './workbook-names';

function book(values: string[][]) {
  const state = emptyState();
  applyPayload(state, rootOp('root', 'Test'));
  for (const [id, name] of [
    ['data', 'Data'],
    ['summary', 'Summary']
  ])
    applyPayload(state, {
      type: 'NodeCreated',
      node_id: id,
      node_type: 'section',
      parent_id: 'root',
      pos: id,
      fields: { kind: 'sheet', label: name, columns: 6 }
    });
  values.forEach((row, i) =>
    applyPayload(
      state,
      itemOp('data', i, Object.fromEntries(row.map((v, c) => [`cell_${c}`, v])), `r${i}`)
    )
  );
  applyPayload(state, itemOp('summary', 0, { cell_0: '=Data!B2' }, 'total'));
  return state;
}
describe('workbook analysis', () => {
  it('sorts computed values, retains row identity, rebases local formulas and retains external cell addresses', () => {
    const state = book([
      ['Name', 'Value', 'Double'],
      ['B', '4', '=B2*2'],
      ['A', '2', '=B3*2'],
      [],
      []
    ]);
    sortSheet(state, state.nodes.data, 0, false, true).forEach((op) => applyPayload(state, op));
    expect(rowsForSheet(state, 'data').map((r) => r.id)).toEqual(['r0', 'r2', 'r1', 'r3', 'r4']);
    expect(state.nodes.r2.current_fields.cell_2).toBe('=B2*2');
    expect(state.nodes.r1.current_fields.cell_2).toBe('=B3*2');
    expect(state.nodes.total.current_fields.cell_0).toBe('=Data!B2');
  });
  it('calculates named ranges and updates them after row insertion and sheet rename', () => {
    const state = book([
      ['Name', 'Value'],
      ['A', '4'],
      ['B', '2']
    ]);
    namedRangeOps(state, 'Amounts', '=Data!$B$2:$B$3').forEach((op) => applyPayload(state, op));
    state.nodes.total.current_fields.cell_0 = '=SUM(Amounts)';
    sheetStructure(state, 'data', 'insert-row', 1).forEach((op) => applyPayload(state, op));
    sheetStructure(state, 'data', 'rename', 0, 'New Data').forEach((op) => applyPayload(state, op));
    expect(workbookNames(state)[0].expression).toBe("='New Data'!$B$3:$B$4");
    const calculator = new WorkbookCalculator();
    expect(calculator.calculate(workbookInput(state), workbookNames(state)).Summary[0][0]).toBe(6);
    calculator.destroy();
  });
  it('aggregates a pivot by row and column and computes weighted average totals', () => {
    const values = [
      ['Region', 'Quarter', 'Sales'],
      ['East', 'Q1', 10],
      ['East', 'Q1', 20],
      ['West', 'Q1', 90],
      ['West', 'Q2', 40]
    ];
    const result = pivotValues(values, {
      sheetId: 'data',
      range: 'A1:C5',
      row: 0,
      column: 1,
      value: 2,
      aggregate: 'average'
    });
    expect(result).toEqual([
      ['Region', 'Q1', 'Q2', 'Grand total'],
      ['East', 15, 0, 15],
      ['West', 90, 40, 65],
      ['Grand total', 40, 40, 40]
    ]);
  });
  it('refreshes pivot results while retaining surviving group IDs', () => {
    const state = book([
      ['Region', 'Sales'],
      ['East', '10'],
      ['West', '20']
    ]);
    const definition = {
      sheetId: 'data',
      range: 'A1:B3',
      row: 0,
      column: null,
      value: 1,
      aggregate: 'sum' as const
    };
    const first = pivotOps(state, definition, [
      ['Region', 'Sales'],
      ['East', 10],
      ['West', 20]
    ]);
    first.ops.forEach((op) => applyPayload(state, op));
    const east = rowsForSheet(state, first.id).find((r) => r.current_fields.cell_0 === 'East')!;
    pivotOps(
      state,
      definition,
      [
        ['Region', 'Sales'],
        ['East', 50],
        ['North', 30]
      ],
      state.nodes[first.id]
    ).ops.forEach((op) => applyPayload(state, op));
    expect(state.nodes[east.id].current_fields.cell_1).toBe('50');
    expect(rowsForSheet(state, first.id).map((r) => r.current_fields.cell_0)).toEqual([
      'Region',
      'East',
      'North',
      'Grand total'
    ]);
  });
  it('removes duplicates and rebases references to remaining rows', () => {
    const state = book([
      ['Key', 'Value'],
      ['A', '4'],
      ['A', '4'],
      ['B', '7']
    ]);
    state.nodes.total.current_fields.cell_0 = '=Data!B4';
    removeDuplicateRows(state, state.nodes.data, { r1: 0, r2: 3, c1: 0, c2: 1 }).forEach((op) =>
      applyPayload(state, op)
    );
    expect(state.nodes.r2.deleted).toBe(true);
    expect(state.nodes.total.current_fields.cell_0).toMatch(/Data'?!B3/);
    expect(state.nodes.r3.id).toBe('r3');
  });
  it('splits text without overwriting destinations or formulas', () => {
    const state = book([['Ada,Lovelace', 'existing']]);
    const range = { r1: 0, r2: 0, c1: 0, c2: 0 };
    expect(() => splitColumn(state, state.nodes.data, range, ',')).toThrow(
      'destination contains data'
    );
    state.nodes.r0.current_fields.cell_1 = '';
    splitColumn(state, state.nodes.data, range, ',').forEach((op) => applyPayload(state, op));
    expect(state.nodes.r0.current_fields.cell_1).toBe('Lovelace');
    state.nodes.r0.current_fields.cell_0 = '=1+2';
    expect(() => splitColumn(state, state.nodes.data, range, ',')).toThrow('formulas');
  });
  it('uses computed numeric chart series and bounded categories', () => {
    const data = chartData(
      {
        id: 'c',
        title: 'Sales',
        type: 'column',
        range: 'A1:C3',
        x: 0,
        y: 0,
        width: 560,
        height: 320
      },
      [
        ['Month', 'Sales', 'Costs'],
        ['Jan', 20, 10],
        ['Feb', 30, 15]
      ]
    );
    expect(data.labels).toEqual(['Jan', 'Feb']);
    expect(data.series[0]).toEqual({ name: 'Sales', values: [20, 30] });
  });
});
