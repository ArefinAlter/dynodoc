import { E, FormulaError, isError } from './errors';
import { parseDateTimeText, type DateOrder } from './dates';

export type Scalar = number | string | boolean | null | FormulaError;

/** Anything with a rectangular shape. */
export interface Matrix {
  readonly rows: number;
  readonly cols: number;
  get(row: number, col: number): Scalar;
}

/** Concrete array value (array literals and array-producing functions). Row-major. */
export class ArrayValue implements Matrix {
  constructor(
    readonly rows: number,
    readonly cols: number,
    readonly data: Scalar[]
  ) {}
  get(r: number, c: number): Scalar {
    return this.data[r * this.cols + c];
  }
  static from2D(rows: Scalar[][]): ArrayValue {
    const cols = rows.reduce((m, r) => Math.max(m, r.length), 0);
    const data: Scalar[] = [];
    for (const r of rows) for (let c = 0; c < cols; c++) data.push(c < r.length ? r[c] : E.NA);
    return new ArrayValue(rows.length, cols, data);
  }
  static of(rows: number, cols: number, fn: (r: number, c: number) => Scalar): ArrayValue {
    const data: Scalar[] = new Array(rows * cols);
    for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) data[r * cols + c] = fn(r, c);
    return new ArrayValue(rows, cols, data);
  }
  toRows(): Scalar[][] {
    const out: Scalar[][] = [];
    for (let r = 0; r < this.rows; r++)
      out.push(this.data.slice(r * this.cols, (r + 1) * this.cols));
    return out;
  }
}

/** Read access to workbook cells for reference values. */
export interface CellSource {
  cellValue(sheet: number, row: number, col: number): Scalar;
  /** Visit populated cells in row-major order; blank cells are skipped. */
  eachCell(
    sheet: number,
    r1: number,
    c1: number,
    r2: number,
    c2: number,
    fn: (value: Scalar, row: number, col: number) => void
  ): void;
  /** True when the cell was never given content (distinguishes blank from ""). */
  isBlank(sheet: number, row: number, col: number): boolean;
  isFormulaCell(sheet: number, row: number, col: number): boolean;
  formulaText(sheet: number, row: number, col: number): string | undefined;
  sheetName(sheet: number): string | undefined;
  sheetCount(): number;
  /** Rows/columns that hold any content, to keep whole-column references cheap. */
  extent(sheet: number): { rows: number; cols: number };
}

/** A reference to a rectangular block of cells on one sheet (0-based, inclusive). */
export class RefValue implements Matrix {
  constructor(
    readonly src: CellSource,
    readonly sheet: number,
    readonly r1: number,
    readonly c1: number,
    readonly r2: number,
    readonly c2: number
  ) {}
  get rows() {
    return this.r2 - this.r1 + 1;
  }
  get cols() {
    return this.c2 - this.c1 + 1;
  }
  get isCell() {
    return this.r1 === this.r2 && this.c1 === this.c2;
  }
  get(r: number, c: number): Scalar {
    return this.src.cellValue(this.sheet, this.r1 + r, this.c1 + c);
  }
  /** Populated cells only; coordinates are relative to the reference. */
  each(fn: (value: Scalar, row: number, col: number) => void) {
    this.src.eachCell(this.sheet, this.r1, this.c1, this.r2, this.c2, (v, r, c) =>
      fn(v, r - this.r1, c - this.c1)
    );
  }
  /** Rows/columns clipped to populated content, for whole-column/row references. */
  clipped(): RefValue {
    const ext = this.src.extent(this.sheet);
    const r2 = Math.max(this.r1, Math.min(this.r2, ext.rows - 1));
    const c2 = Math.max(this.c1, Math.min(this.c2, ext.cols - 1));
    return r2 === this.r2 && c2 === this.c2
      ? this
      : new RefValue(this.src, this.sheet, this.r1, this.c1, r2, c2);
  }
}

/** A union of references, `(A1:A3,C1:C3)`. Only reference-aware functions accept it. */
export class MultiRef {
  constructor(readonly areas: RefValue[]) {}
}

export const MISSING = Symbol('missing-argument');
export type Value = Scalar | ArrayValue | RefValue | MultiRef | typeof MISSING;

export function isMatrix(v: unknown): v is ArrayValue | RefValue {
  return v instanceof ArrayValue || v instanceof RefValue;
}

// ---------------------------------------------------------------- numbers and text

/** Round to 15 significant digits, as Excel does before comparing or truncating. */
export function round15(n: number): number {
  if (n === 0 || !Number.isFinite(n)) return n;
  return Number(n.toPrecision(15));
}

/** Equality/ordering of numbers at 15 significant digits. */
export function compareNumbers(a: number, b: number): number {
  if (a === b) return 0;
  const diff = Math.abs(a - b);
  if (diff > Math.max(Math.abs(a), Math.abs(b)) * 1e-14) return a < b ? -1 : 1;
  const x = round15(a),
    y = round15(b);
  return x === y ? 0 : x < y ? -1 : 1;
}

/** Number → text the way concatenation and TEXT(...,"General") write it (15 digits). */
export function numberToText(n: number): string {
  if (n === 0) return '0';
  if (!Number.isFinite(n)) return '#NUM!';
  const neg = n < 0;
  const [mantissa, exponent] = Math.abs(n).toExponential(14).split('e');
  const exp = Number(exponent);
  const digits = mantissa.replace('.', '').replace(/0+$/, '') || '0';
  const sig = digits.length;
  const decimals = exp < 0 ? -exp - 1 + sig : 0;
  let out: string;
  if (exp >= 20 || (exp < 0 && decimals > 16)) {
    out =
      digits[0] +
      (sig > 1 ? '.' + digits.slice(1) : '') +
      'E' +
      (exp < 0 ? '-' : '+') +
      String(Math.abs(exp)).padStart(2, '0');
  } else if (exp >= 0) {
    out =
      sig <= exp + 1
        ? digits + '0'.repeat(exp + 1 - sig)
        : digits.slice(0, exp + 1) + '.' + digits.slice(exp + 1);
  } else {
    out = '0.' + '0'.repeat(-exp - 1) + digits;
  }
  return neg ? '-' + out : out;
}

export interface CoerceOptions {
  order: DateOrder;
  year: number;
}

/**
 * Text → number: plain and grouped numbers, percent, currency, parenthesised negatives,
 * and dates/times. Returns null when the text is not numeric.
 */
export function parseNumberText(input: string, o: CoerceOptions): number | null {
  const s = input.trim();
  if (!s) return null;
  let m = /^\((.+)\)$/.exec(s);
  if (m) {
    const inner = parseNumberText(m[1], o);
    return inner === null ? null : -inner;
  }
  m =
    /^([+-])?\s*(\$)?\s*([+-])?\s*(\d+(?:,\d{3,})*(?:\.\d*)?|\.\d+)(?:[eE]([+-]?\d+))?\s*(%)?$/.exec(
      s
    );
  if (m && !(m[1] && m[3])) {
    const core = m[4].replace(/,/g, '');
    let n = Number(core + (m[5] ? 'e' + m[5] : ''));
    if (!Number.isFinite(n)) return null;
    if (m[1] === '-' || m[3] === '-') n = -n;
    if (m[6]) n /= 100;
    return n;
  }
  if (!/[\d]/.test(s)) return null;
  return parseDateTimeText(s, o.order, o.year);
}

export function toNumber(v: Scalar, o: CoerceOptions): number | FormulaError {
  switch (typeof v) {
    case 'number':
      return v;
    case 'boolean':
      return v ? 1 : 0;
    case 'string': {
      const n = parseNumberText(v, o);
      return n === null ? E.VALUE : n;
    }
    default:
      return v === null ? 0 : v;
  }
}

export function toText(v: Scalar): string | FormulaError {
  switch (typeof v) {
    case 'string':
      return v;
    case 'number':
      return numberToText(v);
    case 'boolean':
      return v ? 'TRUE' : 'FALSE';
    default:
      return v === null ? '' : v;
  }
}

export function toBoolean(v: Scalar): boolean | FormulaError {
  switch (typeof v) {
    case 'boolean':
      return v;
    case 'number':
      return v !== 0;
    case 'string': {
      const t = v.trim().toUpperCase();
      return t === 'TRUE' ? true : t === 'FALSE' ? false : E.VALUE;
    }
    default:
      return v === null ? false : v;
  }
}

const collator = new Intl.Collator('en', { sensitivity: 'accent' });
export function compareText(a: string, b: string): number {
  if (a === b) return 0;
  return collator.compare(a, b);
}

/** Excel ordering: numbers < text < logicals; blank matches the other operand's type. */
export function compareScalars(a: Scalar, b: Scalar): number {
  if (a === null) {
    if (b === null) return 0;
    a = typeof b === 'string' ? '' : typeof b === 'boolean' ? false : 0;
  } else if (b === null) {
    b = typeof a === 'string' ? '' : typeof a === 'boolean' ? false : 0;
  }
  const ta = typeof a,
    tb = typeof b;
  if (ta === tb) {
    if (ta === 'number') return compareNumbers(a as number, b as number);
    if (ta === 'string') return compareText(a as string, b as string);
    return a === b ? 0 : a ? 1 : -1;
  }
  const rank = (t: string) => (t === 'number' ? 0 : t === 'string' ? 1 : 2);
  return rank(ta) < rank(tb) ? -1 : 1;
}

export function firstError(...values: unknown[]): FormulaError | undefined {
  for (const v of values) if (isError(v)) return v;
  return undefined;
}

/** Excel's "-0" never displays; normalise it. */
export function clean(n: number): number {
  return n === 0 ? 0 : n;
}
