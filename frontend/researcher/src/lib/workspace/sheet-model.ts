import type {
  Cell,
  Worksheet,
  CellValue,
  DataValidation,
  ConditionalFormattingOptions
} from 'exceljs';
import type { DocumentState, MaterializedNode } from '$lib/api/types';
import { bounded, safeColor, safeFont } from './office';

export interface CellFormat {
  decimals?: number;
  conditional?: ConditionalRule[];
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
  fontFamily?: string;
  fontSize?: number;
  color?: string;
  background?: string;
  align?: 'left' | 'center' | 'right';
  vertical?: 'top' | 'middle' | 'bottom';
  wrap?: boolean;
  number?: string;
  numFmt?: string;
  border?: boolean;
  type?: 'text' | 'number' | 'boolean' | 'date';
  hyperlink?: string;
  note?: string;
  validation?: DataValidation;
}
export interface ConditionalRule {
  operator: 'greaterThan' | 'lessThan' | 'equal' | 'between';
  value: number;
  upper?: number;
  background: string;
  color: string;
}
export function conditionalRules(value: unknown): ConditionalRule[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter(
      (rule): rule is ConditionalRule =>
        !!rule &&
        typeof rule === 'object' &&
        ['greaterThan', 'lessThan', 'equal', 'between'].includes(rule.operator) &&
        typeof rule.value === 'number' &&
        Number.isFinite(rule.value) &&
        (rule.operator !== 'between' ||
          (typeof rule.upper === 'number' &&
            Number.isFinite(rule.upper) &&
            rule.upper >= rule.value)) &&
        !!safeColor(rule.background) &&
        !!safeColor(rule.color)
    )
    .slice(0, 8);
}
export function conditionalStyle(value: unknown, format: CellFormat): CellFormat {
  if (
    value === null ||
    value === undefined ||
    typeof value === 'boolean' ||
    String(value).trim() === ''
  )
    return format;
  const number = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(number)) return format;
  const rule = conditionalRules(format.conditional).find((r) =>
    r.operator === 'greaterThan'
      ? number > r.value
      : r.operator === 'lessThan'
        ? number < r.value
        : r.operator === 'equal'
          ? number === r.value
          : number >= r.value && number <= r.upper!
  );
  return rule ? { ...format, background: rule.background, color: rule.color } : format;
}

/** Import bounded numeric cell-is rules. Formulas and unsupported rule kinds are not evaluated. */
export function readConditionalFormats(sheet: Worksheet, formats: CellFormat[][]) {
  const entries =
    (sheet.model as unknown as { conditionalFormattings?: ConditionalFormattingOptions[] })
      .conditionalFormattings || [];
  let assignments = 0;
  for (const entry of entries.slice(0, 2000)) {
    const rules = [...entry.rules]
      .sort((a, b) => a.priority - b.priority)
      .flatMap((rule) => {
        if (rule.type !== 'cellIs' || !rule.formulae?.length) return [];
        const fill = rule.style?.fill;
        return conditionalRules([
          {
            operator: rule.operator,
            value: Number(rule.formulae[0]),
            upper: rule.formulae.length > 1 ? Number(rule.formulae[1]) : undefined,
            background: fill?.type === 'pattern' ? rgb(fill.fgColor) : '#ffffff',
            color: rgb(rule.style?.font?.color) || '#202124'
          }
        ]);
      });
    if (!rules.length) continue;
    for (const ref of entry.ref.split(/\s+/).slice(0, 100)) {
      const match = /^\$?([A-Z]{1,3})\$?(\d+)(?::\$?([A-Z]{1,3})\$?(\d+))?$/.exec(ref);
      if (!match) continue;
      const col = (s: string) => [...s].reduce((v, c) => v * 26 + c.charCodeAt(0) - 64, 0) - 1;
      const r1 = Number(match[2]) - 1,
        r2 = Math.min(formats.length - 1, Number(match[4] || match[2]) - 1);
      const c1 = col(match[1]),
        c2 = Math.min(99, col(match[3] || match[1]));
      for (let r = r1; r <= r2; r++)
        for (let c = c1; c <= c2; c++) {
          if (++assignments > 300000)
            throw new Error(
              'This workbook has too many conditional formatting rules to import safely.'
            );
          const f = formats[r]?.[c];
          if (f) f.conditional = [...(f.conditional || []), ...rules].slice(0, 8);
        }
    }
  }
}
export interface MergeRange {
  fromRow: string;
  toRow: string;
  fromCol: number;
  toCol: number;
}
export interface SheetLayout {
  widths: Record<string, number>;
  hiddenColumns: number[];
  frozenRows: number;
  frozenColumns: number;
  merges: MergeRange[];
}
export function sheetLayout(value: unknown): SheetLayout {
  const v = (value && typeof value === 'object' ? value : {}) as Partial<SheetLayout>;
  return {
    widths:
      v.widths && typeof v.widths === 'object'
        ? Object.fromEntries(
            Object.entries(v.widths)
              .filter(([k]) => /^\d{1,2}$/.test(k))
              .map(([k, v]) => [k, bounded(v, 40, 500, 118)])
          )
        : {},
    hiddenColumns: Array.isArray(v.hiddenColumns)
      ? [...new Set(v.hiddenColumns.filter((c) => Number.isInteger(c) && c >= 0 && c < 100))]
      : [],
    frozenRows: bounded(v.frozenRows, 0, 10, 0),
    frozenColumns: bounded(v.frozenColumns, 0, 5, 0),
    merges: Array.isArray(v.merges)
      ? v.merges
          .filter(
            (m) =>
              typeof m.fromRow === 'string' &&
              typeof m.toRow === 'string' &&
              Number.isInteger(m.fromCol) &&
              Number.isInteger(m.toCol) &&
              m.fromCol >= 0 &&
              m.toCol >= m.fromCol &&
              m.toCol < 100
          )
          .slice(0, 5000)
      : []
  };
}
export function columnName(n: number) {
  let s = '';
  for (n++; n > 0; n = Math.floor((n - 1) / 26)) s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
  return s;
}
export function cellFormat(node: MaterializedNode, c: number): CellFormat {
  const f = node.current_fields[`format_${c}`];
  return (f && typeof f === 'object' ? f : {}) as CellFormat;
}
export function formulaInput(raw: unknown, f: CellFormat = {}): string | number | boolean {
  const v = String(raw ?? '');
  if (f.type === 'text' && !v.startsWith('=')) return `'${v}`;
  if (f.type === 'date' && Number.isFinite(Date.parse(v))) return Date.parse(v) / 86400000 + 25569;
  if (f.type === 'boolean') return v.toLowerCase() === 'true';
  return v;
}
export function workbookInput(state: DocumentState) {
  const sheets = Object.values(state.nodes)
    .filter((n) => !n.deleted && n.current_fields.kind === 'sheet')
    .sort((a, b) => a.pos.localeCompare(b.pos));
  const input: Record<string, (string | number | boolean)[][]> = {};
  for (const sheet of sheets)
    input[String(sheet.current_fields.label || sheet.id)] = Object.values(state.nodes)
      .filter((n) => !n.deleted && n.parent_id === sheet.id)
      .sort((a, b) => a.pos.localeCompare(b.pos))
      .map((n) =>
        Array.from({ length: Math.min(100, Number(sheet.current_fields.columns) || 12) }, (_, c) =>
          formulaInput(n.current_fields[`cell_${c}`], cellFormat(n, c))
        )
      );
  return input;
}
const rgb = (v?: { argb?: string }) =>
  v?.argb && /^[a-f\d]{8}$/i.test(v.argb) ? `#${v.argb.slice(2)}` : undefined;
export function readCell(cell: Cell): { value: string; format: CellFormat } {
  const value = cell.formula
    ? `=${cell.formula}`
    : cell.value instanceof Date
      ? cell.value.toISOString()
      : typeof cell.value === 'boolean'
        ? String(cell.value)
        : cell.value === null || cell.value === undefined
          ? ''
          : cell.text;
  const font = cell.font || {},
    alignment = cell.alignment || {};
  const fill = cell.fill?.type === 'pattern' ? rgb(cell.fill.fgColor) : undefined;
  const format: CellFormat = {
    bold: font.bold,
    italic: font.italic,
    underline: !!font.underline,
    strike: font.strike,
    fontFamily: font.name ? safeFont(font.name) : undefined,
    fontSize: font.size,
    color: rgb(font.color),
    background: fill,
    align: ['left', 'center', 'right'].includes(alignment.horizontal || '')
      ? (alignment.horizontal as CellFormat['align'])
      : undefined,
    vertical: ['top', 'middle', 'bottom'].includes(alignment.vertical || '')
      ? (alignment.vertical as CellFormat['vertical'])
      : undefined,
    wrap: alignment.wrapText,
    numFmt: cell.numFmt || undefined,
    border: !!(cell.border?.top || cell.border?.bottom || cell.border?.left || cell.border?.right),
    type: cell.formula
      ? undefined
      : cell.value instanceof Date
        ? 'date'
        : typeof cell.value === 'number'
          ? 'number'
          : typeof cell.value === 'boolean'
            ? 'boolean'
            : 'text',
    hyperlink: /^https?:\/\//i.test(cell.hyperlink || '') ? cell.hyperlink : undefined,
    note:
      typeof cell.note === 'string'
        ? cell.note
        : cell.note?.texts?.map((t) => t.text || '').join(''),
    validation: cell.dataValidation?.type
      ? JSON.parse(JSON.stringify(cell.dataValidation))
      : undefined
  };
  return { value, format: JSON.parse(JSON.stringify(format)) };
}
export function writeCell(cell: Cell, value: string, f: CellFormat) {
  const conditional = conditionalRules(f.conditional);
  if (conditional.length)
    cell.worksheet.addConditionalFormatting({
      ref: cell.address,
      rules: conditional.map((rule, i) => ({
        type: 'cellIs',
        operator: rule.operator,
        priority: i + 1,
        formulae: rule.operator === 'between' ? [rule.value, rule.upper!] : [rule.value],
        style: {
          fill: {
            type: 'pattern',
            pattern: 'solid',
            fgColor: { argb: 'FF' + rule.background.slice(1) }
          },
          font: { color: { argb: 'FF' + rule.color.slice(1) } }
        }
      }))
    });
  let v: CellValue = value;
  if (value.startsWith('=') && f.type !== 'text') v = { formula: value.slice(1) };
  else if (f.type === 'date' && Number.isFinite(Date.parse(value))) v = new Date(value);
  else if (f.type === 'boolean') v = value.toLowerCase() === 'true';
  else if (f.type !== 'text' && value.trim() && Number.isFinite(Number(value))) v = Number(value);
  if (!value.startsWith('=') && f.hyperlink && /^(https?:\/\/|mailto:)/i.test(f.hyperlink))
    v = { text: value, hyperlink: f.hyperlink };
  cell.value = v;
  cell.font = {
    bold: !!f.bold,
    italic: !!f.italic,
    underline: !!f.underline,
    strike: !!f.strike,
    name: safeFont(f.fontFamily),
    size: bounded(f.fontSize, 6, 96, 11),
    color: safeColor(f.color) ? { argb: 'FF' + f.color!.slice(1) } : undefined
  };
  if (safeColor(f.background))
    cell.fill = {
      type: 'pattern',
      pattern: 'solid',
      fgColor: { argb: 'FF' + f.background!.slice(1) }
    };
  cell.alignment = {
    horizontal: ['left', 'center', 'right'].includes(f.align || '') ? f.align : undefined,
    vertical: ['top', 'middle', 'bottom'].includes(f.vertical || '') ? f.vertical : undefined,
    wrapText: !!f.wrap
  };
  const decimal = Math.round(bounded(f.decimals, 0, 10, f.number === 'percent' ? 1 : 2));
  const places = decimal ? '.' + '0'.repeat(decimal) : '';
  const numbers: Record<string, string> = {
    decimal: '0' + places,
    percent: '0' + places + '%',
    currency: '$#,##0' + places,
    date: 'yyyy-mm-dd',
    integer: '0',
    thousands: '#,##0' + places,
    scientific: '0' + places + 'E+00',
    text: '@'
  };
  cell.numFmt =
    numbers[f.number || ''] || f.numFmt || (f.type === 'date' ? 'yyyy-mm-dd' : 'General');
  if (f.border) {
    const line = { style: 'thin' as const, color: { argb: 'FF8DA89B' } };
    cell.border = { top: line, bottom: line, left: line, right: line };
  }
  if (f.note) cell.note = String(f.note).slice(0, 10000);
  if (f.validation?.type) cell.dataValidation = f.validation;
}
export function readSheetLayout(sheet: Worksheet, rowIds: string[]): SheetLayout {
  const layout = sheetLayout(undefined);
  sheet.columns.forEach((c, i) => {
    if (i >= 100) return;
    layout.widths[String(i)] = bounded(c.width ? c.width * 7 + 5 : 118, 40, 500, 118);
    if (c.hidden) layout.hiddenColumns.push(i);
  });
  const frozen = sheet.views?.find((v) => v.state === 'frozen');
  if (frozen?.state === 'frozen') {
    layout.frozenRows = Math.min(10, frozen.ySplit || 0);
    layout.frozenColumns = Math.min(5, frozen.xSplit || 0);
  }
  for (const range of sheet.model.merges || []) {
    const parsed = /^([A-Z]+)(\d+):([A-Z]+)(\d+)$/.exec(range);
    if (!parsed) continue;
    const index = (s: string) => [...s].reduce((v, c) => v * 26 + c.charCodeAt(0) - 64, 0) - 1;
    const fromRow = rowIds[Number(parsed[2]) - 1],
      toRow = rowIds[Number(parsed[4]) - 1];
    if (fromRow && toRow)
      layout.merges.push({ fromRow, toRow, fromCol: index(parsed[1]), toCol: index(parsed[3]) });
  }
  return layout;
}
export function validationError(value: string, f: CellFormat): string | null {
  const v = f.validation;
  if (!v || (!value && v.allowBlank)) return null;
  if (v.type === 'list') {
    const source = String(v.formulae?.[0] || '');
    if (
      source.startsWith('"') &&
      source.endsWith('"') &&
      !source.slice(1, -1).split(',').includes(value)
    )
      return 'Choose a value from the cell’s allowed list.';
  }
  if (['whole', 'decimal', 'textLength'].includes(v.type)) {
    const n = v.type === 'textLength' ? value.length : Number(value),
      a = Number(v.formulae?.[0]),
      b = Number(v.formulae?.[1]);
    if (!value && v.type !== 'textLength') return 'This cell requires a value.';
    if (!Number.isFinite(n) || (v.type === 'whole' && !Number.isInteger(n)))
      return 'This cell requires a valid number.';
    const checks: Record<string, boolean> = {
      between: n >= a && n <= b,
      notBetween: n < a || n > b,
      equal: n === a,
      notEqual: n !== a,
      greaterThan: n > a,
      lessThan: n < a,
      greaterThanOrEqual: n >= a,
      lessThanOrEqual: n <= a
    };
    if (checks[v.operator || 'between'] === false)
      return v.error || 'This value does not meet the cell’s validation rule.';
  }
  return null;
}
