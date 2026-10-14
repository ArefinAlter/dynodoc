import { E, isError } from './errors';
import type { EvalContext } from './evaluator';
import { evaluateCriteriaArgs, textMatcher } from './criteria';
import { betaI, gammaP } from './fn-dist';
import { ArrayValue, RefValue, clean, compareText, type Scalar, type Value } from './values';
import { def, defScalar, matrixOf, num, numbersFrom, isMissing, intOr } from './fn-util';

// ------------------------------------------------------------ database

interface Table {
  headers: string[];
  rows: Scalar[][];
}
function table(v: Value, ctx: EvalContext): Table {
  const m = matrixOf(v, ctx);
  if (m.rows < 1) throw E.VALUE;
  const headers = Array.from({ length: m.cols }, (_, c) => {
    const h = m.get(0, c);
    return isError(h) ? '' : String(h ?? '');
  });
  const rows: Scalar[][] = [];
  for (let r = 1; r < m.rows; r++) rows.push(Array.from({ length: m.cols }, (_, c) => m.get(r, c)));
  return { headers, rows };
}

/** Database criteria: a plain text criterion matches by prefix, others follow COUNTIF rules. */
function databaseCriterion(raw: Scalar, ctx: EvalContext): (cell: Scalar) => boolean {
  if (typeof raw === 'string' && raw !== '' && !/^(<=|>=|<>|<|>|=)/.test(raw)) {
    const pattern = raw;
    if (/[*?~]/.test(pattern)) return evaluateCriteriaArgs(raw, ctx);
    const test = textMatcher(pattern + '*');
    return (c) => typeof c === 'string' && test(c);
  }
  return evaluateCriteriaArgs(raw, ctx);
}

function matchingRecords(database: Table, criteria: Value, ctx: EvalContext): Scalar[][] {
  const crit = matrixOf(criteria, ctx);
  if (crit.rows < 2) throw E.VALUE;
  const columns: { index: number }[] = [];
  for (let c = 0; c < crit.cols; c++) {
    const header = crit.get(0, c);
    const name = isError(header) ? '' : String(header ?? '');
    const index = database.headers.findIndex((h) => compareText(h, name) === 0);
    columns.push({ index });
  }
  return database.rows.filter((record) => {
    // Rows of the criteria range are alternatives; the filled cells of one row must all hold.
    for (let r = 1; r < crit.rows; r++) {
      let ok = true;
      for (let c = 0; c < crit.cols && ok; c++) {
        const cell = crit.get(r, c);
        if (cell === null || cell === '') continue;
        const col = columns[c].index;
        ok = col >= 0 && databaseCriterion(cell, ctx)(record[col]);
      }
      if (ok) return true;
    }
    return false;
  });
}

function fieldIndex(database: Table, field: Value): number {
  const v = field instanceof RefValue ? field.get(0, 0) : (field as Scalar);
  if (isError(v)) throw v;
  if (typeof v === 'number') {
    const i = Math.trunc(v) - 1;
    if (i < 0 || i >= database.headers.length) throw E.VALUE;
    return i;
  }
  if (typeof v === 'string') {
    const i = database.headers.findIndex((h) => compareText(h, v) === 0);
    if (i < 0) throw E.VALUE;
    return i;
  }
  throw E.VALUE;
}

function databaseFn(name: string, reduce: (values: Scalar[], numbers: number[]) => Scalar) {
  def(name, 3, 3, (a, ctx) => {
    const db = table(a[0], ctx);
    const col = fieldIndex(db, a[1]);
    const records = matchingRecords(db, a[2], ctx);
    const values = records.map((r) => r[col]);
    for (const v of values) if (isError(v)) throw v;
    const numbers = values.filter((v): v is number => typeof v === 'number');
    return reduce(values, numbers);
  });
}
const variance = (xs: number[], sample: boolean) => {
  const n = xs.length;
  if (sample ? n < 2 : n < 1) return E.DIV0;
  const mean = xs.reduce((s, x) => s + x, 0) / n;
  return xs.reduce((s, x) => s + (x - mean) ** 2, 0) / (sample ? n - 1 : n);
};
databaseFn('DSUM', (_v, n) => n.reduce((s, x) => s + x, 0));
databaseFn('DCOUNT', (_v, n) => n.length);
databaseFn('DCOUNTA', (v) => v.filter((x) => x !== null && x !== '').length);
databaseFn('DAVERAGE', (_v, n) => (n.length ? n.reduce((s, x) => s + x, 0) / n.length : E.DIV0));
databaseFn('DMAX', (_v, n) => (n.length ? Math.max(...n) : 0));
databaseFn('DMIN', (_v, n) => (n.length ? Math.min(...n) : 0));
databaseFn('DPRODUCT', (_v, n) => (n.length ? n.reduce((p, x) => p * x, 1) : 0));
databaseFn('DGET', (v) => (v.length === 1 ? (v[0] ?? 0) : v.length === 0 ? E.VALUE : E.NUM));
databaseFn('DVAR', (_v, n) => variance(n, true));
databaseFn('DVARP', (_v, n) => variance(n, false));
databaseFn('DSTDEV', (_v, n) => {
  const v = variance(n, true);
  return typeof v === 'number' ? Math.sqrt(v) : v;
});
databaseFn('DSTDEVP', (_v, n) => {
  const v = variance(n, false);
  return typeof v === 'number' ? Math.sqrt(v) : v;
});

// -------------------------------------------------------- significance tests

function tCdfTwoTail(t: number, df: number) {
  return betaI(df / (df + t * t), df / 2, 0.5);
}
function fCdf(x: number, d1: number, d2: number) {
  return x <= 0 ? 0 : betaI((d1 * x) / (d1 * x + d2), d1 / 2, d2 / 2);
}
const mean = (xs: number[]) => xs.reduce((s, x) => s + x, 0) / xs.length;
const sampleVar = (xs: number[]) => {
  const m = mean(xs);
  return xs.reduce((s, x) => s + (x - m) ** 2, 0) / (xs.length - 1);
};
function pairedValues(a: Value, b: Value, ctx: EvalContext): [number[], number[]] {
  const x = matrixOf(a, ctx),
    y = matrixOf(b, ctx);
  if (x.data.length !== y.data.length) throw E.NA;
  const xs: number[] = [],
    ys: number[] = [];
  x.data.forEach((p, i) => {
    const q = y.data[i];
    if (typeof p === 'number' && typeof q === 'number') {
      xs.push(p);
      ys.push(q);
    }
  });
  return [xs, ys];
}
for (const name of ['T.TEST', 'TTEST'])
  def(name, 4, 4, (a, ctx) => {
    const tails = Math.trunc(num(a[2], ctx));
    const type = Math.trunc(num(a[3], ctx));
    if ((tails !== 1 && tails !== 2) || type < 1 || type > 3) return E.NUM;
    let t: number, df: number;
    if (type === 1) {
      const [x, y] = pairedValues(a[0], a[1], ctx);
      const n = x.length;
      if (n < 2) return E.DIV0;
      const d = x.map((v, i) => v - y[i]);
      const sd = Math.sqrt(sampleVar(d));
      if (sd === 0) return E.DIV0;
      t = mean(d) / (sd / Math.sqrt(n));
      df = n - 1;
    } else {
      const x = numbersFrom(a[0], ctx),
        y = numbersFrom(a[1], ctx);
      if (x.length < 2 || y.length < 2) return E.DIV0;
      const vx = sampleVar(x),
        vy = sampleVar(y);
      if (type === 2) {
        df = x.length + y.length - 2;
        const pooled = ((x.length - 1) * vx + (y.length - 1) * vy) / df;
        t = (mean(x) - mean(y)) / Math.sqrt(pooled * (1 / x.length + 1 / y.length));
      } else {
        const sx = vx / x.length,
          sy = vy / y.length;
        t = (mean(x) - mean(y)) / Math.sqrt(sx + sy);
        df = (sx + sy) ** 2 / (sx ** 2 / (x.length - 1) + sy ** 2 / (y.length - 1));
      }
    }
    if (!Number.isFinite(t)) return E.DIV0;
    const two = tCdfTwoTail(Math.abs(t), df);
    return clean(tails === 2 ? two : two / 2);
  });
for (const name of ['F.TEST', 'FTEST'])
  def(name, 2, 2, (a, ctx) => {
    const x = numbersFrom(a[0], ctx),
      y = numbersFrom(a[1], ctx);
    if (x.length < 2 || y.length < 2) return E.DIV0;
    const vx = sampleVar(x),
      vy = sampleVar(y);
    if (vx === 0 || vy === 0) return E.DIV0;
    const f = vx / vy;
    const p = fCdf(f, x.length - 1, y.length - 1);
    return clean(Math.min(1, 2 * Math.min(p, 1 - p)));
  });
for (const name of ['CHISQ.TEST', 'CHITEST'])
  def(name, 2, 2, (a, ctx) => {
    const actual = matrixOf(a[0], ctx),
      expected = matrixOf(a[1], ctx);
    if (actual.rows !== expected.rows || actual.cols !== expected.cols) return E.NA;
    let stat = 0;
    for (let i = 0; i < actual.data.length; i++) {
      const o = actual.data[i],
        e = expected.data[i];
      if (typeof o !== 'number' || typeof e !== 'number') continue;
      if (e === 0) return E.DIV0;
      stat += (o - e) ** 2 / e;
    }
    const df =
      actual.rows > 1 && actual.cols > 1
        ? (actual.rows - 1) * (actual.cols - 1)
        : Math.max(actual.rows, actual.cols) - 1;
    if (df < 1) return E.DIV0;
    return clean(chiRightTail(stat, df));
  });
function chiRightTail(x: number, df: number): number {
  return 1 - gammaP(df / 2, x / 2);
}

// ----------------------------------------------------------------- misc

defScalar('ISBINARY', 1, 1, (a) => /^[01]{1,10}$/.test(String(a[0] ?? '')));
function pooling(name: string, reduce: (values: number[]) => number) {
  def(name, 2, 3, (a, ctx) => {
    const m = matrixOf(a[0], ctx);
    const size = Math.trunc(num(a[1], ctx));
    const stride = isMissing(a[2]) ? size : intOr(a[2], ctx, size);
    if (size < 1 || stride < 1) return E.VALUE;
    const rows = Math.max(1, Math.ceil((m.rows - size + 1) / stride));
    const cols = Math.max(1, Math.ceil((m.cols - size + 1) / stride));
    return ArrayValue.of(rows, cols, (r, c) => {
      const window: number[] = [];
      for (let i = 0; i < size; i++)
        for (let j = 0; j < size; j++) {
          const v =
            r * stride + i < m.rows && c * stride + j < m.cols
              ? m.get(r * stride + i, c * stride + j)
              : null;
          if (typeof v === 'number') window.push(v);
        }
      return window.length ? reduce(window) : 0;
    });
  });
}
pooling('MAXPOOL', (w) => Math.max(...w));
pooling('MEDIANPOOL', (w) => {
  const s = [...w].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
});
