// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import ExcelJS from 'exceljs';
import { applyPayload } from '$lib/materializer';
import type { DocumentState } from '$lib/api/types';
import { itemOp, rootOp, rowsForSheet } from './files';
import { WorkbookCalculator } from './workbook-calculator';
import { workbookInput, conditionalStyle } from './sheet-model';
import { workbookNames } from './workbook-names';
import { fillRange } from './sheet-operations';
import { namedRangeOps, pivotValues, removeDuplicateRows, sortSheet } from './sheet-analysis';
import corpus from './formula/fixtures/excel-corpus.json';
import {
  FormulaEngine,
  FormulaError,
  formatGeneral,
  parseConstant,
  registeredFunctionNames,
  tryParse,
  type CellContent,
  type Scalar
} from './formula';

type Expected = null | number | string | boolean | { e: string } | { a: Expected[][] } | { x: 1 };

const project = (...parts: string[]) => resolve(process.cwd(), ...parts);
const fixture = (name: string) => readFileSync(project('src/lib/workspace/formula/fixtures', name));

function norm(v: unknown): string {
  if (v === null || v === undefined) return 'blank';
  if (v instanceof FormulaError) return v.value;
  if (typeof v === 'number') return `n:${v}`;
  if (typeof v === 'boolean') return `b:${v}`;
  return `s:${String(v)}`;
}
/** Complex numbers travel as text; compare their parts numerically (Excel's last digits differ). */
function complexParts(text: string): [number, number] | null {
  const m = /^([+-]?[\d.]+(?:E[+-]?\d+)?)?(?:([+-])([\d.]+(?:E[+-]?\d+)?)?[ij])?$/.exec(text);
  if (!m || !/[ij]$/.test(text)) return null;
  const im = (m[3] === undefined ? 1 : Number(m[3])) * (m[2] === '-' ? -1 : 1);
  return [m[1] === undefined ? 0 : Number(m[1]), m[2] === undefined ? 0 : im];
}
function same(got: Scalar, want: Expected, tolerance = 1e-9): boolean {
  if (want !== null && typeof want === 'object') {
    if ('e' in want) return got instanceof FormulaError && got.value === want.e;
    return false;
  }
  if (typeof got === 'string' && typeof want === 'string') {
    const a = complexParts(got),
      b = complexParts(want);
    if (a && b) return Math.abs(a[0] - b[0]) < 1e-12 && Math.abs(a[1] - b[1]) < 1e-12;
  }
  const g = got === null ? 0 : got;
  const w = want === null ? 0 : want;
  if (typeof w === 'number' && typeof g === 'number')
    return Math.abs(g - w) <= tolerance * Math.max(1, Math.abs(w));
  return g === w;
}
function build(
  sheets: Record<string, CellContent[][]>,
  names: { name: string; expression: string }[] = []
) {
  return FormulaEngine.buildFromSheets(sheets, names, {
    now: () => corpus.options.now,
    dateOrder: 'MDY'
  });
}
const value = (e: FormulaEngine, sheet: string, row: number, col: number) =>
  e.getCellValue({ sheet: e.getSheetId(sheet)!, row, col });

describe('formula engine against Microsoft Excel', () => {
  it('reproduces the Excel-evaluated corpus', () => {
    const sheets: Record<string, CellContent[][]> = {};
    for (const [name, rows] of Object.entries(corpus.grid))
      sheets[name] = (rows as CellContent[][]).map((r) => r.map((v) => v ?? null));
    while (sheets.Data.length < 20) sheets.Data.push([]);
    const engine = build(sheets, corpus.names);
    const sheet = engine.getSheetId('Data')!;
    const at = (r: number, c: number) => ({ sheet, row: 19 + r, col: 9 + c });
    const failures: string[] = [];
    let checked = 0;
    for (const [formula, want, tolerance] of corpus.cases as [string, Expected, number?][]) {
      engine.setCellContents(at(0, 0), formula);
      checked++;
      if (want && typeof want === 'object' && 'x' in want) {
        if (!(engine.getCellValue(at(0, 0)) instanceof FormulaError))
          failures.push(`${formula}: expected an error`);
        continue;
      }
      if (want && typeof want === 'object' && 'a' in want) {
        want.a.forEach((row, r) =>
          row.forEach((cell, c) => {
            const got = engine.getCellValue(at(r, c));
            if (!same(got, cell, tolerance))
              failures.push(`${formula} [${r},${c}]: got ${norm(got)} want ${norm(cell as never)}`);
          })
        );
        continue;
      }
      const got = engine.getCellValue(at(0, 0));
      if (!same(got, want, tolerance))
        failures.push(`${formula}: got ${norm(got)} want ${JSON.stringify(want)}`);
    }
    expect(checked).toBeGreaterThanOrEqual(300);
    expect(failures).toEqual([]);
  });

  async function parity(buffer: Buffer | Uint8Array) {
    const book = new ExcelJS.Workbook();
    await book.xlsx.load(buffer as never);
    const sheets: Record<string, CellContent[][]> = {};
    const expected: {
      sheet: string;
      row: number;
      col: number;
      formula: string;
      result: unknown;
    }[] = [];
    for (const ws of book.worksheets) {
      const rows: CellContent[][] = [];
      ws.eachRow({ includeEmpty: false }, (row, r) => {
        row.eachCell({ includeEmpty: false }, (cell, c) => {
          rows[r - 1] ??= [];
          const v = cell.value as unknown;
          let content: CellContent;
          if (
            v &&
            typeof v === 'object' &&
            !(v instanceof Date) &&
            ('formula' in v || 'sharedFormula' in v)
          ) {
            const f = v as { formula?: string; result?: unknown };
            content = '=' + (f.formula ?? cell.formula);
            expected.push({
              sheet: ws.name,
              row: r - 1,
              col: c - 1,
              formula: content,
              result: f.result
            });
          } else if (v instanceof Date) content = v.getTime() / 86400000 + 25569;
          else content = (v as CellContent) ?? null;
          rows[r - 1][c - 1] = content;
        });
      });
      sheets[ws.name] = rows;
    }
    const names = (book.definedNames.model as { name: string; ranges: string[] }[]).map((n) => ({
      name: n.name,
      expression: '=' + n.ranges[0]
    }));
    const engine = build(sheets, names);
    const bad: string[] = [];
    for (const e of expected) {
      const got = engine.getCellValue({
        sheet: engine.getSheetId(e.sheet)!,
        row: e.row,
        col: e.col
      });
      const want =
        e.result && typeof e.result === 'object' && 'error' in e.result
          ? { e: (e.result as { error: string }).error }
          : (e.result as Expected);
      if (!same(got, want ?? null))
        bad.push(
          `${e.sheet}!R${e.row + 1}C${e.col + 1} ${e.formula}: got ${norm(got)} want ${JSON.stringify(want)}`
        );
    }
    return { total: expected.length, mismatches: bad };
  }

  it('recalculates every formula of the Excel-written parity workbook to its cached value', async () => {
    const result = await parity(fixture('formula-parity.xlsx'));
    expect(result.total).toBeGreaterThanOrEqual(200);
    expect(result.mismatches).toEqual([]);
  });

  it('recalculates the bundled example workbooks to their cached values', async () => {
    const result = await parity(readFileSync(project('static/examples/fieldwork-tracker.xlsx')));
    expect(result.total).toBe(6);
    expect(result.mismatches).toEqual([]);
    const survey = await parity(
      readFileSync(project('static/examples/community-questionnaire.xlsx'))
    );
    expect(survey.mismatches).toEqual([]);
  });
});

describe('formula syntax', () => {
  const calc = (formula: string, cells: CellContent[][] = []) => {
    const e = build({ S: [[formula], ...cells] });
    return e.getCellValue({ sheet: 0, row: 0, col: 0 });
  };
  it('follows Excel operator precedence and associativity', () => {
    expect(calc('=-2^2')).toBe(4);
    expect(calc('=2^3^2')).toBe(64);
    expect(calc('=2*3+4')).toBe(10);
    expect(calc('=2+3*4')).toBe(14);
    expect(calc('=10-2-3')).toBe(5);
    expect(calc('=2^-2')).toBe(0.25);
    expect(calc('=50%*200')).toBe(100);
    expect(calc('="a"&1+2')).toBe('a3');
    expect(calc('=1<2=TRUE')).toBe(true);
  });
  it('accepts lowercase, whitespace and sheet-qualified references', () => {
    const e = build({
      Data: [[2, '=data!a1*2', "=SUM( 'my sheet'!A1 : A2 )"]],
      'My Sheet': [[5], [6]]
    });
    expect(value(e, 'Data', 0, 1)).toBe(4);
    expect(value(e, 'Data', 0, 2)).toBe(11);
  });
  it('reports parse failures and unknown names as error values, never exceptions', () => {
    expect(calc('=1+')).toBeInstanceOf(FormulaError);
    expect((calc('=1+') as FormulaError).value).toBe('#ERROR!');
    expect((calc('=NOPE(1)') as FormulaError).value).toBe('#NAME?');
    expect((calc('=SUM(') as FormulaError).value).toBe('#ERROR!');
    expect((calc('=1/0') as FormulaError).value).toBe('#DIV/0!');
    expect((calc('=SUM(1,2)+') as FormulaError).type).toBe('ERROR');
    expect((calc('=IF(1)') as FormulaError).type).toBe('ERROR');
    expect((calc('="a"+1') as FormulaError).value).toBe('#VALUE!');
    expect((calc('=C1:C2 D1:D2') as FormulaError).value).toBe('#NULL!');
    expect((calc('=SQRT(-1)') as FormulaError).value).toBe('#NUM!');
    expect((calc('=NA()') as FormulaError).value).toBe('#N/A');
  });
  it('parses constants the way cell content is typed', () => {
    expect(parseConstant('12')).toBe(12);
    expect(parseConstant(' 12 ')).toBe(12);
    expect(parseConstant('50%')).toBe(0.5);
    expect(parseConstant('$5')).toBe(5);
    expect(parseConstant('1e3')).toBe(1000);
    expect(parseConstant('true')).toBe(true);
    expect(parseConstant("'123")).toBe('123');
    expect(parseConstant('1,000')).toBe('1,000');
    expect(parseConstant('2026-01-05')).toBe('2026-01-05');
    expect(parseConstant('')).toBeNull();
    expect((parseConstant('#N/A') as FormulaError).value).toBe('#N/A');
    expect(parseConstant('abc')).toBe('abc');
  });
  it('keeps the original text of a formula when its references do not change', () => {
    const e = build({ S: [[1, '=sum( a1 , 2 )']] });
    expect(e.getCellSerialized({ sheet: 0, row: 0, col: 1 })).toBe('=sum( a1 , 2 )');
    e.addRows(0, [1, 1]);
    expect(e.getCellSerialized({ sheet: 0, row: 0, col: 1 })).toBe('=sum( a1 , 2 )');
  });
  it('lists every registered function once, in capitals', () => {
    const names = registeredFunctionNames();
    expect(names.length).toBeGreaterThan(300);
    expect(new Set(names).size).toBe(names.length);
    for (const required of [
      'SUM',
      'VLOOKUP',
      'XLOOKUP',
      'INDEX',
      'MATCH',
      'IFERROR',
      'SUMIFS',
      'COUNTIFS',
      'TEXT',
      'DATE',
      'PMT'
    ])
      expect(names).toContain(required);
    expect(names.every((n) => n === n.toUpperCase())).toBe(true);
  });
});

describe('functions newer than the reference Excel build', () => {
  const calc = (f: string) => {
    const e = build({ S: [[f]] });
    return e.getCellValue({ sheet: 0, row: 0, col: 0 });
  };
  const grid = (f: string, rows = 3, cols = 3) => {
    const e = build({ S: [[f]] });
    return Array.from({ length: rows }, (_, r) =>
      Array.from({ length: cols }, (_, c) => norm(e.getCellValue({ sheet: 0, row: r, col: c })))
    );
  };
  it('splits and slices text', () => {
    expect(calc('=TEXTBEFORE("a-b-c","-")')).toBe('a');
    expect(calc('=TEXTAFTER("a-b-c","-")')).toBe('b-c');
    expect(calc('=TEXTBEFORE("a-b-c","-",2)')).toBe('a-b');
    expect(calc('=TEXTAFTER("a-b-c","-",-1)')).toBe('c');
    expect((calc('=TEXTBEFORE("abc","-")') as FormulaError).value).toBe('#N/A');
    expect(grid('=TEXTSPLIT("a,b;c,d",",",";")', 2, 2)).toEqual([
      ['s:a', 's:b'],
      ['s:c', 's:d']
    ]);
  });
  it('offers the HyperFormula extras and pooling helpers', () => {
    expect(calc('=ISBINARY("1010")')).toBe(true);
    expect(calc('=ISBINARY("1012")')).toBe(false);
    expect(grid('=MAXPOOL({1,2,3,4;5,6,7,8},2)', 1, 2)).toEqual([['n:6', 'n:8']]);
    expect(grid('=MEDIANPOOL({1,2,3,4;5,6,7,8},2)', 1, 2)).toEqual([['n:3.5', 'n:5.5']]);
    expect(calc('=SPLIT("a b c",1)')).toBe('b');
  });
  it('computes Bessel functions to full precision', () => {
    const close = (f: string, want: number) => {
      const v = calc(f) as number;
      expect(Math.abs(v - want)).toBeLessThan(1e-12 * Math.max(1, Math.abs(want)));
    };
    close('=BESSELJ(1,0)', 0.7651976865579666);
    close('=BESSELJ(2.5,2)', 0.4460590584396172);
    close('=BESSELY(1,0)', 0.08825696421567697);
    close('=BESSELI(1,0)', 1.2660658777520082);
    close('=BESSELK(1,0)', 0.42102443824070834);
    close('=BESSELK(1,1)', 0.6019072301972346);
  });
  it('shapes arrays', () => {
    expect(grid('=VSTACK({1,2},{3,4})', 2, 2)).toEqual([
      ['n:1', 'n:2'],
      ['n:3', 'n:4']
    ]);
    expect(grid('=HSTACK({1;2},{3;4})', 2, 2)).toEqual([
      ['n:1', 'n:3'],
      ['n:2', 'n:4']
    ]);
    expect(grid('=TAKE({1,2,3;4,5,6},1)', 1, 3)).toEqual([['n:1', 'n:2', 'n:3']]);
    expect(grid('=DROP({1,2,3;4,5,6},1)', 1, 3)).toEqual([['n:4', 'n:5', 'n:6']]);
    expect(grid('=CHOOSECOLS({1,2,3;4,5,6},3,1)', 2, 2)).toEqual([
      ['n:3', 'n:1'],
      ['n:6', 'n:4']
    ]);
    expect(grid('=TOCOL({1,2;3,4})', 4, 1)).toEqual([['n:1'], ['n:2'], ['n:3'], ['n:4']]);
    expect(grid('=EXPAND({1,2},2,3,0)', 2, 3)).toEqual([
      ['n:1', 'n:2', 'n:0'],
      ['n:0', 'n:0', 'n:0']
    ]);
    expect(calc('=COUNTUNIQUE({1,1,2,"a","A"})')).toBe(3);
    expect(grid('=ARRAY_CONSTRAIN({1,2,3;4,5,6},1,2)', 1, 2)).toEqual([['n:1', 'n:2']]);
  });
});

describe('structural edits rebase references like the previous engine', () => {
  const workbook = () =>
    build(
      {
        Data: [
          ['=B2', '=B$3', '=$B3', '=SUM(A1:A3)', '=Other!A1'],
          [1, 2, 3],
          [4, 5, 6],
          [7, 8, 9]
        ],
        'My Sheet': [['=Data!A2', '=Data!B3', '=SUM(Data!A1:A3)']],
        Other: [[1]]
      },
      [
        { name: 'Rng', expression: '=Data!$A$2:$A$4' },
        { name: 'Pt', expression: "='My Sheet'!$A$1" }
      ]
    );
  const text = (e: FormulaEngine, sheet: string) =>
    e.getSheetSerialized(e.getSheetId(sheet)!).map((r) => r.map((c) => (c === null ? null : c)));
  const names = (e: FormulaEngine) =>
    e.getAllNamedExpressionsSerialized().map((n) => [n.name, n.expression]);

  it('shifts and expands references when rows are inserted', () => {
    const e = workbook();
    e.addRows(e.getSheetId('Data')!, [1, 1]);
    expect(text(e, 'Data')[0]).toEqual(['=B3', '=B$4', '=$B4', '=SUM(A1:A4)', '=Other!A1']);
    expect(text(e, 'My Sheet')).toEqual([['=Data!A3', '=Data!B4', '=SUM(Data!A1:A4)']]);
    expect(names(e)).toEqual([
      ['Rng', '=Data!$A$3:$A$5'],
      ['Pt', "='My Sheet'!$A$1"]
    ]);
  });
  it('turns deleted cells into #REF! and shrinks ranges', () => {
    const e = workbook();
    e.removeRows(e.getSheetId('Data')!, [1, 1]);
    expect(text(e, 'Data')[0]).toEqual(['=#REF!', '=B$2', '=$B2', '=SUM(A1:A2)', '=Other!A1']);
    expect(text(e, 'My Sheet')).toEqual([['=#REF!', '=Data!B2', '=SUM(Data!A1:A2)']]);
    expect(names(e)[0]).toEqual(['Rng', '=Data!$A$2:$A$3']);
    expect(value(e, 'Data', 0, 0)).toBeInstanceOf(FormulaError);
    expect((value(e, 'Data', 0, 0) as FormulaError).value).toBe('#REF!');
    e.removeColumns(e.getSheetId('Data')!, [0, 1]);
    expect(names(e)[0]).toEqual(['Rng', '=#REF!']);
  });
  it('shifts columns, renames sheets with quoting and removes sheets', () => {
    const e = workbook();
    e.addColumns(e.getSheetId('Data')!, [1, 2]);
    expect(text(e, 'Data')[0]).toEqual([
      '=D2',
      null,
      null,
      '=D$3',
      '=$D3',
      '=SUM(A1:A3)',
      '=Other!A1'
    ]);
    const f = workbook();
    f.renameSheet(f.getSheetId('My Sheet')!, 'Q1 Plan');
    expect(names(f)[1]).toEqual(['Pt', "='Q1 Plan'!$A$1"]);
    f.renameSheet(f.getSheetId('Other')!, 'Totals');
    expect(text(f, 'Data')[0][4]).toBe('=Totals!A1');
    f.removeSheet(f.getSheetId('Data')!);
    expect(text(f, 'Q1 Plan')).toEqual([['=#REF!', '=#REF!', '=SUM(#REF!)']]);
  });
  it('keeps relative offsets of formulas that move with a row order (sort)', () => {
    const e = build({
      Data: [
        ['h', 'v', 'd'],
        ['b', 4, '=B2*2'],
        ['a', 2, '=B3+B2'],
        ['c', 9, '=$B$2+B4'],
        ['d', 1, '=Data!B2+Other!A1']
      ],
      Other: [['=Data!B2', '=Data!$B$3']]
    });
    e.setRowOrder(0, [0, 4, 1, 2, 3]);
    expect(text(e, 'Data')).toEqual([
      ['h', 'v', 'd'],
      ['a', 2, '=B2+B1'],
      ['c', 9, '=$B$2+B3'],
      ['d', 1, '=Data!B1+#REF!'],
      ['b', 4, '=B5*2']
    ]);
    // formulas outside the sorted rows keep their addresses
    expect(text(e, 'Other')).toEqual([['=Data!B2', '=Data!$B$3']]);
  });
  it('translates formulas when a cell is copied', () => {
    const e = build({
      S: [
        [1, 2],
        ['=A1*2', '=$A1+B$1+Other!C3']
      ],
      Other: []
    });
    e.copyCell({ sheet: 0, row: 1, col: 0 }, { sheet: 0, row: 2, col: 0 });
    expect(e.getCellSerialized({ sheet: 0, row: 2, col: 0 })).toBe('=A2*2');
    e.copyCell({ sheet: 0, row: 1, col: 1 }, { sheet: 0, row: 2, col: 2 });
    expect(e.getCellSerialized({ sheet: 0, row: 2, col: 2 })).toBe('=$A2+C$1+Other!D4');
  });
  it('rejects names that are relative or malformed and supports sheet scope', () => {
    const e = build({
      Data: [
        [1, 2],
        [3, 4]
      ],
      Other: [['=Local', '=Global+1']]
    });
    expect(() => e.addNamedExpression('Bad name', '=Data!$A$1')).toThrow(/not a valid name/);
    expect(() => e.addNamedExpression('Rel', '=A1')).toThrow(/absolute/);
    expect(() => e.addNamedExpression('A1', '=Data!$A$1')).toThrow(/not a valid name/);
    e.addNamedExpression('Global', '=Data!$A$1');
    e.addNamedExpression('Local', '=Data!$B$2', e.getSheetId('Other')!);
    expect(value(e, 'Other', 0, 0)).toBe(4);
    expect(value(e, 'Other', 0, 1)).toBe(2);
    e.setCellContents({ sheet: e.getSheetId('Data')!, row: 0, col: 2 }, '=Local');
    expect((value(e, 'Data', 0, 2) as FormulaError).value).toBe('#NAME?');
  });
});

describe('calculation graph', () => {
  it('marks circular references and everything that depends on them', () => {
    const e = build({
      S: [
        ['=B1', '=A1', '=A1+1', '=IFERROR(A1,"rec")'],
        ['=A2', '=SUM(A1:B1)']
      ]
    });
    for (const c of [0, 1, 2]) expect((value(e, 'S', 0, c) as FormulaError).value).toBe('#CYCLE!');
    expect(value(e, 'S', 0, 3)).toBe('rec');
    expect((value(e, 'S', 1, 0) as FormulaError).value).toBe('#CYCLE!');
    expect((value(e, 'S', 1, 1) as FormulaError).value).toBe('#CYCLE!');
    e.setCellContents({ sheet: 0, row: 0, col: 1 }, 5);
    expect(value(e, 'S', 0, 0)).toBe(5);
    expect(value(e, 'S', 0, 2)).toBe(6);
  });
  it('evaluates long dependency chains without recursion limits', () => {
    const rows: CellContent[][] = [[1]];
    for (let i = 1; i < 6000; i++) rows.push([`=A${i}+1`]);
    const e = build({ S: rows });
    expect(value(e, 'S', 5999, 0)).toBe(6000);
    e.setCellContents({ sheet: 0, row: 0, col: 0 }, 11);
    expect(value(e, 'S', 5999, 0)).toBe(6010);
  });
  it('resolves a chain that points forward through thousands of cells', () => {
    const rows: CellContent[][] = [];
    for (let i = 0; i < 5000; i++) rows.push([`=A${i + 2}+1`]);
    rows.push([7]);
    const e = build({ S: rows });
    expect(value(e, 'S', 0, 0)).toBe(5007);
    e.setCellContents({ sheet: 0, row: 5000, col: 0 }, 0);
    expect(value(e, 'S', 0, 0)).toBe(5000);
  });
  it('matches a fresh build after random incremental edits', () => {
    let seed = 7;
    const rnd = () => (seed = (seed * 1664525 + 1013904223) % 4294967296) / 4294967296;
    const pick = <T>(xs: T[]) => xs[Math.floor(rnd() * xs.length)];
    const cell = () => `${pick(['A', 'B', 'C', 'D'])}${1 + Math.floor(rnd() * 6)}`;
    const content = (): CellContent => {
      const x = rnd();
      if (x < 0.3) return Math.floor(rnd() * 10);
      if (x < 0.4) return null;
      if (x < 0.5) return 'text';
      const f = pick([
        () => `=${cell()}+${cell()}`,
        () => `=SUM(${cell()}:${cell()})`,
        () => `=IF(${cell()}>3,${cell()},0)`,
        () => `=COUNT(${cell()}:${cell()})*2`,
        () => `=MAX(${cell()},Other!${cell()})`,
        () => `=SUM(${cell()}:${cell()}*2)`,
        () => `=ROWS(${cell()}:${cell()})`
      ]);
      return f();
    };
    const rows = () => Array.from({ length: 6 }, () => Array.from({ length: 4 }, content));
    const sheets = { S: rows(), Other: rows() };
    const live = build(sheets);
    for (let step = 0; step < 120; step++) {
      const target = pick(['S', 'Other']);
      const r = Math.floor(rnd() * 6),
        c = Math.floor(rnd() * 4);
      const v = content();
      live.setCellContents({ sheet: live.getSheetId(target)!, row: r, col: c }, v);
      sheets[target as 'S' | 'Other'][r][c] = v;
      if (step % 10 === 0) {
        const fresh = build(JSON.parse(JSON.stringify(sheets)) as Record<string, CellContent[][]>);
        for (const name of ['S', 'Other'])
          expect(live.getSheetValues(live.getSheetId(name)!).map((row) => row.map(norm))).toEqual(
            fresh.getSheetValues(fresh.getSheetId(name)!).map((row) => row.map(norm))
          );
      }
    }
  });
  it('spills array results into free cells and reports blocked spills', () => {
    const e = build({
      S: [
        ['=SEQUENCE(3)', null, '=B1:B3'],
        [null, 7],
        [null, 8]
      ]
    });
    expect([0, 1, 2].map((r) => value(e, 'S', r, 0))).toEqual([1, 2, 3]);
    e.setCellContents({ sheet: 0, row: 1, col: 0 }, 'block');
    expect((value(e, 'S', 0, 0) as FormulaError).value).toBe('#SPILL!');
    e.setCellContents({ sheet: 0, row: 1, col: 0 }, null);
    expect(value(e, 'S', 2, 0)).toBe(3);
  });
  it('recalculates volatile functions on each edit and honours the injected clock', () => {
    let now = 46000.25;
    const e = FormulaEngine.buildFromSheets({ S: [['=NOW()', '=TODAY()', '=RAND()', 1]] }, [], {
      now: () => now,
      random: () => 0.25
    });
    expect(value(e, 'S', 0, 0)).toBe(46000.25);
    expect(value(e, 'S', 0, 1)).toBe(46000);
    expect(value(e, 'S', 0, 2)).toBe(0.25);
    now = 46001.5;
    e.setCellContents({ sheet: 0, row: 0, col: 3 }, 2);
    expect(value(e, 'S', 0, 0)).toBe(46001.5);
  });
});

describe('safety and bounds', () => {
  it('survives random garbage and random formulas without throwing', () => {
    let seed = 20261014;
    const rnd = () => (seed = (seed * 1664525 + 1013904223) % 4294967296) / 4294967296;
    const pick = <T>(xs: T[]) => xs[Math.floor(rnd() * xs.length)];
    const atoms = [
      '1',
      '2.5',
      '"x"',
      '""',
      'TRUE',
      'A1',
      'B$2',
      '$C3',
      'A1:B3',
      'Z9',
      '#N/A',
      '#REF!',
      '{1,2;3,4}',
      'Sheet2!A1',
      "'Q1 Data'!A1:A3",
      'Missing',
      'A:A',
      '2:3'
    ];
    const fns = [
      'SUM',
      'IF',
      'AND',
      'INDEX',
      'MATCH',
      'VLOOKUP',
      'LEN',
      'LEFT',
      'TEXT',
      'ROUND',
      'OFFSET',
      'INDIRECT',
      'SORT',
      'FILTER',
      'TRANSPOSE',
      'SEQUENCE',
      'COUNTIF',
      'SUMIF',
      'DATE',
      'MID',
      'XLOOKUP',
      'TEXTJOIN',
      'MMULT',
      'NOPE',
      'PMT',
      'IFERROR'
    ];
    const ops = ['+', '-', '*', '/', '^', '&', '=', '<>', '<', '>=', ' ', ':', ','];
    const gen = (depth: number): string => {
      const x = rnd();
      if (depth <= 0 || x < 0.35) return pick(atoms);
      if (x < 0.6) return `${gen(depth - 1)}${pick(ops)}${gen(depth - 1)}`;
      if (x < 0.65) return `(${gen(depth - 1)})`;
      if (x < 0.7) return `-${gen(depth - 1)}`;
      if (x < 0.75) return `${gen(depth - 1)}%`;
      const n = Math.floor(rnd() * 4);
      return `${pick(fns)}(${Array.from({ length: n }, () => gen(depth - 1)).join(',')})`;
    };
    const e = build({
      S: [
        [1, 2, 3],
        [4, 'a', 6],
        ['=1+1', null, 9]
      ],
      'Q1 Data': [[5], [6], [7]],
      Sheet2: [[1]]
    });
    const target = { sheet: 0, row: 5, col: 5 };
    const junk = '=+-*/^&<>=(),;:{}[]!@#$%"\' ABCabc019.\\';
    let evaluated = 0;
    for (let i = 0; i < 12000; i++) {
      let f: string;
      if (i % 4 === 0) {
        let t = '=';
        const len = Math.floor(rnd() * 30);
        for (let k = 0; k < len; k++) t += junk[Math.floor(rnd() * junk.length)];
        f = t;
      } else f = '=' + gen(1 + Math.floor(rnd() * 4));
      e.setCellContents(target, f);
      const v = e.getCellValue(target);
      expect(
        ['number', 'string', 'boolean'].includes(typeof v) ||
          v === null ||
          v instanceof FormulaError
      ).toBe(true);
      evaluated++;
    }
    expect(evaluated).toBe(12000);
    // parser level: no exceptions either
    for (let i = 0; i < 2000; i++) {
      const r = tryParse('=' + gen(5));
      expect('ast' in r || 'error' in r).toBe(true);
    }
  });
  it('bounds nesting, text length and array size', () => {
    const deep = '=' + '('.repeat(5000) + '1' + ')'.repeat(5000);
    const e = build({
      S: [[deep, '=REPT("ab",20000)', '=SEQUENCE(2000000)', '=SUM(SEQUENCE(1000,1000))']]
    });
    expect(value(e, 'S', 0, 0)).toBeInstanceOf(FormulaError);
    expect((value(e, 'S', 0, 1) as FormulaError).value).toBe('#VALUE!');
    expect(value(e, 'S', 0, 2)).toBeInstanceOf(FormulaError);
    expect(value(e, 'S', 0, 3)).toBe(500000500000);
    const nested = '=' + 'IF(TRUE,'.repeat(60) + '1' + ')'.repeat(60);
    expect(value(build({ S: [[nested]] }), 'S', 0, 0)).toBe(1);
  });
  it('stops runaway calculations with the work budget', () => {
    const e = FormulaEngine.buildFromSheets({ S: [['=SUM(SEQUENCE(1000,1000))', '=1+1']] }, [], {
      maxWork: 10_000
    });
    expect((value(e, 'S', 0, 0) as FormulaError).value).toBe('#ERROR!');
    expect(value(e, 'S', 0, 1)).toBe(2);
  });
  it('keeps object prototypes out of names, sheets and function lookups', () => {
    const sheets = Object.fromEntries([
      ['__proto__', [['=constructor', '=toString(1)', '=__proto__', '=hasOwnProperty']]],
      ['constructor', [[1]]],
      ['my sheet', [[2]]]
    ]) as Record<string, CellContent[][]>;
    const e = FormulaEngine.buildFromSheets(sheets, [
      { name: '__proto__', expression: '=constructor!$A$1' }
    ]);
    const row = e.getSheetValues(e.getSheetId('__proto__')!)[0];
    expect(row.map((v) => (v instanceof FormulaError ? v.value : v))).toEqual([
      '#NAME?',
      '#NAME?',
      1,
      '#NAME?'
    ]);
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
    expect(Object.prototype.hasOwnProperty.call(Object.prototype, 'value')).toBe(false);
  });
});

describe('spreadsheet workflows that e2e/office.spec.ts drives through the editor', () => {
  const empty = (): DocumentState => ({
    nodes: {},
    comments: [],
    suggestions: {},
    removed_choices: []
  });
  function book(label: string, rows: string[][], columns = 6) {
    const state = empty();
    applyPayload(state, rootOp('root', 'Book'));
    applyPayload(state, {
      type: 'NodeCreated',
      node_id: 'sheet',
      node_type: 'section',
      parent_id: 'root',
      pos: '0',
      fields: { kind: 'sheet', label, columns }
    });
    rows.forEach((row, i) =>
      applyPayload(
        state,
        itemOp('sheet', i, Object.fromEntries(row.map((v, c) => [`cell_${c}`, v])), `r${i}`)
      )
    );
    return state;
  }
  const computed = (state: DocumentState) => {
    const calculator = new WorkbookCalculator();
    const values = calculator.calculate(workbookInput(state), workbookNames(state));
    calculator.destroy();
    return values;
  };

  it('uses a named expression, then refreshes a pivot after an edit', () => {
    const state = book('Sheet 1', [
      ['Region', 'Sales'],
      ['East', '10'],
      ['West', '20'],
      ['East', '30']
    ]);
    namedRangeOps(state, 'SalesTotal', "=SUM('Sheet 1'!$B$2:$B$4)").forEach((op) =>
      applyPayload(state, op)
    );
    applyPayload(state, {
      type: 'FieldEdited',
      node_id: 'r0',
      field: 'cell_2',
      value: '=SalesTotal'
    });
    const values = computed(state);
    expect(values['Sheet 1'][0][2]).toBe(60);
    const definition = {
      sheetId: 'sheet',
      range: 'A1:B4',
      row: 0,
      column: null,
      value: 1,
      aggregate: 'sum' as const
    };
    expect(pivotValues(values['Sheet 1'], definition)[1]).toEqual(['East', 40, 40]);
    applyPayload(state, { type: 'FieldEdited', node_id: 'r1', field: 'cell_1', value: '15' });
    expect(pivotValues(computed(state)['Sheet 1'], definition)[1]).toEqual(['East', 45, 45]);
    expect(computed(state)['Sheet 1'][0][2]).toBe(65);
  });
  it('styles a cell from the value its formula calculates', () => {
    const state = book('Sheet 1', [['12', '=A1*2']]);
    const format = {
      conditional: [
        { operator: 'greaterThan' as const, value: 20, background: '#ffc7ce', color: '#9c0006' }
      ]
    };
    expect(conditionalStyle(computed(state)['Sheet 1'][0][1], format).background).toBe('#ffc7ce');
    applyPayload(state, { type: 'FieldEdited', node_id: 'r0', field: 'cell_0', value: '4' });
    expect(conditionalStyle(computed(state)['Sheet 1'][0][1], format).background).toBeUndefined();
  });
  it('fills a formula down with relative references and calculates the result', () => {
    const state = book('Sheet 1', [['2', '=A1*2'], ['3']]);
    fillRange(state, state.nodes.sheet, { r1: 0, r2: 1, c1: 1, c2: 1 }, 'down').forEach((op) =>
      applyPayload(state, op)
    );
    expect(state.nodes.r1.current_fields.cell_1).toBe('=A2*2');
    expect(computed(state)['Sheet 1'][1][1]).toBe(6);
  });
  it('removes duplicates, sorts with a header and keeps row formulas', () => {
    const state = book('Sheet 1', [
      ['Name', 'Value', 'Double'],
      ['West', '20', '=B2*2'],
      ['East', '10', '=B3*2'],
      ['East', '10', '=B4*2']
    ]);
    removeDuplicateRows(state, state.nodes.sheet, { r1: 0, r2: 3, c1: 0, c2: 2 }).forEach((op) =>
      applyPayload(state, op)
    );
    sortSheet(state, state.nodes.sheet, 0, false, true).forEach((op) => applyPayload(state, op));
    const rows = rowsForSheet(state, 'sheet');
    expect(rows.map((r) => r.current_fields.cell_0)).toEqual(['Name', 'East', 'West']);
    const values = computed(state)['Sheet 1'];
    expect(values[1][2]).toBe(20);
    expect(values[2][2]).toBe(40);
  });
  it('splits text and keeps text-typed numbers as text', () => {
    const state = book('Sheet 1', [['00123', '=A1&"x"']]);
    applyPayload(state, {
      type: 'FieldEdited',
      node_id: 'r0',
      field: 'format_0',
      value: { type: 'text' }
    });
    const values = computed(state)['Sheet 1'];
    expect(values[0][0]).toBe('00123');
    expect(values[0][1]).toBe('00123x');
  });
  it('restricts a name to a valid, absolute definition through the editor operation', () => {
    const state = book('Data', [['1', '2']]);
    expect(() => namedRangeOps(state, 'A1', '=Data!$A$1')).toThrow(/Cell addresses are reserved/);
    expect(() => namedRangeOps(state, 'Rel', '=A1')).toThrow(/absolute/);
    expect(() => namedRangeOps(state, 'Open', 'Data!$A$1')).toThrow(/Start the expression with =/);
  });
});

describe('numbers leaving the engine', () => {
  const values = (...formulas: string[]) => {
    const e = build({ S: [formulas] });
    return formulas.map((_, c) => value(e, 'S', 0, c));
  };
  it('are trimmed to 15 significant digits so binary noise never reaches the editor', () => {
    expect(values('=0.1+0.2', '=1.1*3', '=100*1.1', '=1/3')).toEqual([
      0.3, 3.3, 110, 0.333333333333333
    ]);
  });
  it('keep whole numbers and tiny differences intact', () => {
    expect(values('=123456789012345', '=(0.1+0.2-0.3)*1')[0]).toBe(123456789012345);
    expect(values('=(0.1+0.2-0.3)*1')[0]).toBeCloseTo(5.55e-17, 18);
  });
  it('formats like Excel General in a default-width cell', () => {
    expect(formatGeneral(1 / 3)).toBe('0.333333333');
    expect(formatGeneral(-1 / 3)).toBe('-0.33333333');
    expect(formatGeneral(0.1 + 0.2)).toBe('0.3');
    expect(formatGeneral(1234567.891234)).toBe('1234567.891');
    expect(formatGeneral(123456789012)).toBe('1.23457E+11');
    expect(formatGeneral(1e-10)).toBe('1E-10');
    expect(formatGeneral(0)).toBe('0');
  });
});
