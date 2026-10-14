import { E, FormulaError, isError } from './errors';
import { evaluateCriteriaArgs, type CriteriaTest } from './criteria';
import type { EvalContext } from './evaluator';
import {
  ArrayValue,
  MultiRef,
  RefValue,
  clean,
  compareNumbers,
  toNumber,
  type Scalar,
  type Value
} from './values';
import {
  def,
  defScalar,
  num,
  numOr,
  numbersOf,
  numbersOfA,
  numbersFrom,
  matrixOf,
  walkArgs,
  isMissing,
  vector
} from './fn-util';
import { operand } from './evaluator';

function fin(r: number): Scalar {
  return Number.isFinite(r) ? clean(r) : E.NUM;
}
const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);
const mean = (xs: number[]) => sum(xs) / xs.length;

// ------------------------------------------------------------- aggregates

def('SUM', 1, 255, (a, ctx) => fin(sum(numbersOf(a, ctx))));
def('PRODUCT', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  return xs.length === 0 ? 0 : fin(xs.reduce((p, n) => p * n, 1));
});
def('SUMSQ', 1, 255, (a, ctx) => fin(numbersOf(a, ctx).reduce((s, n) => s + n * n, 0)));
def('AVERAGE', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  return xs.length === 0 ? E.DIV0 : fin(mean(xs));
});
def('AVERAGEA', 1, 255, (a, ctx) => {
  const xs = numbersOfA(a, ctx);
  return xs.length === 0 ? E.DIV0 : fin(mean(xs));
});
def('MAX', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  return xs.length === 0 ? 0 : xs.reduce((m, n) => (n > m ? n : m), -Infinity);
});
def('MIN', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  return xs.length === 0 ? 0 : xs.reduce((m, n) => (n < m ? n : m), Infinity);
});
def('MAXA', 1, 255, (a, ctx) => {
  const xs = numbersOfA(a, ctx);
  return xs.length === 0 ? 0 : xs.reduce((m, n) => (n > m ? n : m), -Infinity);
});
def('MINA', 1, 255, (a, ctx) => {
  const xs = numbersOfA(a, ctx);
  return xs.length === 0 ? 0 : xs.reduce((m, n) => (n < m ? n : m), Infinity);
});
def('COUNT', 1, 255, (a, ctx) => {
  let n = 0;
  walkArgs(a, ctx, (v, kind) => {
    if (typeof v === 'number') n++;
    else if (kind === 'direct') {
      if (typeof v === 'boolean') n++;
      else if (typeof v === 'string' && !isError(toNumber(v, ctx.coerce))) n++;
    }
  });
  return n;
});
def('COUNTA', 1, 255, (a, ctx) => {
  let n = 0;
  walkArgs(a, ctx, (v, kind) => {
    if (kind === 'ref' ? v !== null : true) n++;
  });
  return n;
});
def('COUNTBLANK', 1, 1, (a, ctx) => {
  const r = a[0];
  if (!(r instanceof RefValue)) return E.VALUE;
  let filled = 0;
  r.each((v) => {
    ctx.charge(1);
    if (v !== null && v !== '') filled++;
  });
  return r.rows * r.cols - filled;
});
def('COUNTUNIQUE', 1, 255, (a, ctx) => {
  const seen = new Set<string>();
  walkArgs(a, ctx, (v) => {
    if (isError(v)) seen.add('e:' + v.value);
    else if (v === null) return;
    else seen.add(typeof v === 'string' ? 's:' + v.toLowerCase() : typeof v + ':' + String(v));
  });
  return seen.size;
});

// -------------------------------------------------- order statistics

function sorted(xs: number[]) {
  return [...xs].sort((a, b) => a - b);
}
function percentileInc(xs: number[], k: number): number {
  const s = sorted(xs);
  const pos = (s.length - 1) * k;
  const lo = Math.floor(pos);
  const hi = Math.min(lo + 1, s.length - 1);
  return s[lo] + (pos - lo) * (s[hi] - s[lo]);
}
function percentileExc(xs: number[], k: number): number | FormulaError {
  const s = sorted(xs);
  const n = s.length;
  if (k < 1 / (n + 1) || k > n / (n + 1)) return E.NUM;
  const pos = (n + 1) * k - 1;
  const lo = Math.floor(pos);
  const hi = Math.min(lo + 1, n - 1);
  return s[lo] + (pos - lo) * (s[hi] - s[lo]);
}
def('MEDIAN', 1, 255, (a, ctx) => {
  const xs = sorted(numbersOf(a, ctx));
  if (!xs.length) return E.NUM;
  const m = Math.floor(xs.length / 2);
  return xs.length % 2 ? xs[m] : (xs[m - 1] + xs[m]) / 2;
});
const mode = (a: Value[], ctx: EvalContext): Scalar => {
  const xs = numbersOf(a, ctx);
  const counts = new Map<number, number>();
  for (const x of xs) counts.set(x, (counts.get(x) ?? 0) + 1);
  let best = 0,
    out: number | undefined;
  for (const x of xs) {
    const c = counts.get(x)!;
    if (c > best) {
      best = c;
      out = x;
    }
  }
  return best < 2 || out === undefined ? E.NA : out;
};
def('MODE', 1, 255, mode);
def('MODE.SNGL', 1, 255, mode);

function kth(name: string, largest: boolean) {
  def(name, 2, 2, (a, ctx) => {
    const xs = sorted(numbersFrom(a[0], ctx));
    const pick = (kv: Scalar): Scalar => {
      if (isError(kv)) return kv;
      const k = Math.ceil(num(kv, ctx));
      if (k < 1 || k > xs.length) return E.NUM;
      return largest ? xs[xs.length - k] : xs[k - 1];
    };
    const k = operand(a[1]);
    if (k instanceof ArrayValue || k instanceof RefValue) {
      const m = matrixOf(k, ctx);
      return new ArrayValue(m.rows, m.cols, m.data.map(pick));
    }
    return pick(k as Scalar);
  });
}
kth('LARGE', true);
kth('SMALL', false);

function rank(name: string, average: boolean) {
  def(name, 2, 3, (a, ctx) => {
    const x = num(a[0], ctx);
    const order = numOr(a[2], ctx, 0);
    const xs = numbersFrom(a[1], ctx);
    let found = false,
      better = 0,
      same = 0;
    for (const v of xs) {
      const c = compareNumbers(v, x);
      if (c === 0) {
        found = true;
        same++;
      } else if (order === 0 ? c > 0 : c < 0) better++;
    }
    if (!found) return E.NA;
    return average ? better + (same + 1) / 2 : better + 1;
  });
}
rank('RANK', false);
rank('RANK.EQ', false);
rank('RANK.AVG', true);

def('PERCENTILE', 2, 2, (a, ctx) => {
  const xs = numbersFrom(a[0], ctx);
  const k = num(a[1], ctx);
  if (!xs.length || k < 0 || k > 1) return E.NUM;
  return fin(percentileInc(xs, k));
});
FUNCTIONS_ALIAS('PERCENTILE.INC', 'PERCENTILE');
def('PERCENTILE.EXC', 2, 2, (a, ctx) => {
  const xs = numbersFrom(a[0], ctx);
  const k = num(a[1], ctx);
  if (!xs.length) return E.NUM;
  const r = percentileExc(xs, k);
  return isError(r) ? r : fin(r);
});
def('QUARTILE', 2, 2, (a, ctx) => {
  const xs = numbersFrom(a[0], ctx);
  const q = Math.trunc(num(a[1], ctx));
  if (!xs.length || q < 0 || q > 4) return E.NUM;
  return fin(percentileInc(xs, q / 4));
});
FUNCTIONS_ALIAS('QUARTILE.INC', 'QUARTILE');
def('QUARTILE.EXC', 2, 2, (a, ctx) => {
  const xs = numbersFrom(a[0], ctx);
  const q = Math.trunc(num(a[1], ctx));
  if (!xs.length || q < 1 || q > 3) return E.NUM;
  const r = percentileExc(xs, q / 4);
  return isError(r) ? r : fin(r);
});
function percentRank(exclusive: boolean) {
  return (a: Value[], ctx: EvalContext): Scalar => {
    const xs = sorted(numbersFrom(a[0], ctx));
    const x = num(a[1], ctx);
    const sig = isMissing(a[2]) ? 3 : Math.trunc(num(a[2], ctx));
    if (sig < 1) return E.NUM;
    const n = xs.length;
    if (!n) return E.NA;
    if (x < xs[0] || x > xs[n - 1]) return E.NA;
    const i = xs.findIndex((v) => compareNumbers(v, x) >= 0);
    let r: number;
    if (compareNumbers(xs[i], x) === 0) r = exclusive ? (i + 1) / (n + 1) : i / (n - 1);
    else {
      const lo = xs[i - 1],
        hi = xs[i];
      const frac = (x - lo) / (hi - lo);
      r = exclusive ? (i + frac) / (n + 1) : (i - 1 + frac) / (n - 1);
    }
    const k = 10 ** sig;
    return Math.floor(r * k + 1e-9) / k;
  };
}
def('PERCENTRANK', 2, 3, percentRank(false));
def('PERCENTRANK.INC', 2, 3, percentRank(false));
def('PERCENTRANK.EXC', 2, 3, percentRank(true));
def('TRIMMEAN', 2, 2, (a, ctx) => {
  const xs = sorted(numbersFrom(a[0], ctx));
  const p = num(a[1], ctx);
  if (p < 0 || p >= 1 || !xs.length) return E.NUM;
  const drop = Math.floor((xs.length * p) / 2);
  const kept = xs.slice(drop, xs.length - drop);
  return kept.length ? fin(mean(kept)) : E.NUM;
});

// ------------------------------------------------------- dispersion

function variance(xs: number[], sample: boolean): number | FormulaError {
  const n = xs.length;
  if (sample ? n < 2 : n < 1) return E.DIV0;
  const m = mean(xs);
  return xs.reduce((s, x) => s + (x - m) ** 2, 0) / (sample ? n - 1 : n);
}
function varFn(name: string, sample: boolean, all: boolean, root: boolean) {
  def(name, 1, 255, (a, ctx) => {
    const xs = all ? numbersOfA(a, ctx) : numbersOf(a, ctx);
    const v = variance(xs, sample);
    if (isError(v)) return v;
    return fin(root ? Math.sqrt(v) : v);
  });
}
varFn('VAR', true, false, false);
varFn('VAR.S', true, false, false);
varFn('VARS', true, false, false);
varFn('VARP', false, false, false);
varFn('VAR.P', false, false, false);
varFn('VARA', true, true, false);
varFn('VARPA', false, true, false);
varFn('STDEV', true, false, true);
varFn('STDEV.S', true, false, true);
varFn('STDEVS', true, false, true);
varFn('STDEVP', false, false, true);
varFn('STDEV.P', false, false, true);
varFn('STDEVA', true, true, true);
varFn('STDEVPA', false, true, true);
def('AVEDEV', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  if (!xs.length) return E.NUM;
  const m = mean(xs);
  return fin(mean(xs.map((x) => Math.abs(x - m))));
});
def('DEVSQ', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  if (!xs.length) return E.NUM;
  const m = mean(xs);
  return fin(xs.reduce((s, x) => s + (x - m) ** 2, 0));
});
def('GEOMEAN', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  if (!xs.length || xs.some((x) => x <= 0)) return E.NUM;
  return fin(Math.exp(mean(xs.map(Math.log))));
});
def('HARMEAN', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  if (!xs.length || xs.some((x) => x <= 0)) return E.NUM;
  return fin(xs.length / sum(xs.map((x) => 1 / x)));
});
def('SKEW', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  const n = xs.length;
  if (n < 3) return E.DIV0;
  const m = mean(xs);
  const s = Math.sqrt(variance(xs, true) as number);
  if (s === 0) return E.DIV0;
  return fin((n / ((n - 1) * (n - 2))) * sum(xs.map((x) => ((x - m) / s) ** 3)));
});
def('SKEW.P', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  const n = xs.length;
  if (n < 1) return E.DIV0;
  const m = mean(xs);
  const s = Math.sqrt(variance(xs, false) as number);
  if (s === 0) return E.DIV0;
  return fin(sum(xs.map((x) => ((x - m) / s) ** 3)) / n);
});
def('KURT', 1, 255, (a, ctx) => {
  const xs = numbersOf(a, ctx);
  const n = xs.length;
  if (n < 4) return E.DIV0;
  const m = mean(xs);
  const s = Math.sqrt(variance(xs, true) as number);
  if (s === 0) return E.DIV0;
  const k = sum(xs.map((x) => ((x - m) / s) ** 4));
  return fin(
    (n * (n + 1) * k) / ((n - 1) * (n - 2) * (n - 3)) - (3 * (n - 1) ** 2) / ((n - 2) * (n - 3))
  );
});
defScalar('STANDARDIZE', 3, 3, (a, ctx) => {
  const sd = num(a[2], ctx);
  if (sd <= 0) return E.NUM;
  return fin((num(a[0], ctx) - num(a[1], ctx)) / sd);
});

// -------------------------------------------------- paired statistics

/** Numeric pairs from two equally shaped arguments; non-numeric pairs are skipped. */
function pairs(a: Value, b: Value, ctx: EvalContext): [number[], number[]] {
  const x = matrixOf(a, ctx),
    y = matrixOf(b, ctx);
  if (x.rows * x.cols !== y.rows * y.cols) throw E.NA;
  const xs: number[] = [],
    ys: number[] = [];
  for (let i = 0; i < x.data.length; i++) {
    const p = x.data[i],
      q = y.data[i];
    if (isError(p)) throw p;
    if (isError(q)) throw q;
    if (typeof p === 'number' && typeof q === 'number') {
      xs.push(p);
      ys.push(q);
    }
  }
  return [xs, ys];
}
function stats2(xs: number[], ys: number[]) {
  const n = xs.length;
  const mx = mean(xs),
    my = mean(ys);
  let sxx = 0,
    syy = 0,
    sxy = 0;
  for (let i = 0; i < n; i++) {
    sxx += (xs[i] - mx) ** 2;
    syy += (ys[i] - my) ** 2;
    sxy += (xs[i] - mx) * (ys[i] - my);
  }
  return { n, mx, my, sxx, syy, sxy };
}
function pairFn(name: string, f: (s: ReturnType<typeof stats2>) => number | FormulaError, min = 2) {
  def(name, 2, 2, (a, ctx) => {
    const [xs, ys] = pairs(a[0], a[1], ctx);
    if (xs.length < min) return E.DIV0;
    const r = f(stats2(xs, ys));
    return isError(r) ? r : fin(r);
  });
}
const div = (a: number, b: number) => (b === 0 ? E.DIV0 : a / b);
pairFn('CORREL', (s) => div(s.sxy, Math.sqrt(s.sxx * s.syy)));
pairFn('PEARSON', (s) => div(s.sxy, Math.sqrt(s.sxx * s.syy)));
pairFn('RSQ', (s) => div(s.sxy * s.sxy, s.sxx * s.syy));
pairFn('COVAR', (s) => s.sxy / s.n, 1);
pairFn('COVARIANCE.P', (s) => s.sxy / s.n, 1);
pairFn('COVARIANCE.S', (s) => s.sxy / (s.n - 1));
// SLOPE/INTERCEPT/STEYX take (known_y's, known_x's).
function yxFn(name: string, f: (s: ReturnType<typeof stats2>) => number | FormulaError, min = 2) {
  def(name, 2, 2, (a, ctx) => {
    const [ys, xs] = pairs(a[0], a[1], ctx);
    if (xs.length < min) return E.DIV0;
    const r = f(stats2(xs, ys));
    return isError(r) ? r : fin(r);
  });
}
yxFn('SLOPE', (s) => div(s.sxy, s.sxx));
yxFn('INTERCEPT', (s) => {
  const b = div(s.sxy, s.sxx);
  return isError(b) ? b : s.my - b * s.mx;
});
yxFn(
  'STEYX',
  (s) => {
    if (s.n < 3) return E.DIV0;
    const d = div(s.sxy * s.sxy, s.sxx);
    return isError(d) ? d : Math.sqrt((s.syy - d) / (s.n - 2));
  },
  3
);
for (const name of ['FORECAST', 'FORECAST.LINEAR'])
  def(name, 3, 3, (a, ctx) => {
    const x0 = num(a[0], ctx);
    const [ys, xs] = pairs(a[1], a[2], ctx);
    if (xs.length < 2) return E.DIV0;
    const s = stats2(xs, ys);
    if (s.sxx === 0) return E.DIV0;
    const b = s.sxy / s.sxx;
    return fin(s.my - b * s.mx + b * x0);
  });

// -------------------------------------------------------- sum-products

function productArrays(args: Value[], ctx: EvalContext): number[][] {
  const mats = args.map((a) => matrixOf(a, ctx));
  const { rows, cols } = mats[0];
  for (const m of mats) if (m.rows !== rows || m.cols !== cols) throw E.VALUE;
  ctx.charge(rows * cols * mats.length);
  return mats.map((m) =>
    m.data.map((v) => {
      if (isError(v)) throw v;
      return typeof v === 'number' ? v : 0;
    })
  );
}
def('SUMPRODUCT', 1, 255, (a, ctx) => {
  const cols = productArrays(a, ctx);
  const n = cols[0].length;
  let total = 0;
  for (let i = 0; i < n; i++) {
    let p = 1;
    for (const c of cols) p *= c[i];
    total += p;
  }
  return fin(total);
});
function xyFn(name: string, f: (x: number, y: number) => number) {
  def(name, 2, 2, (a, ctx) => {
    const x = matrixOf(a[0], ctx),
      y = matrixOf(a[1], ctx);
    if (x.data.length !== y.data.length) return E.NA;
    let t = 0;
    for (let i = 0; i < x.data.length; i++) {
      const p = x.data[i],
        q = y.data[i];
      if (isError(p)) return p;
      if (isError(q)) return q;
      if (typeof p === 'number' && typeof q === 'number') t += f(p, q);
    }
    return fin(t);
  });
}
xyFn('SUMX2MY2', (x, y) => x * x - y * y);
xyFn('SUMX2PY2', (x, y) => x * x + y * y);
xyFn('SUMXMY2', (x, y) => (x - y) ** 2);
def('PROB', 3, 4, (a, ctx) => {
  const xs = numbersFrom(a[0], ctx),
    ps = numbersFrom(a[1], ctx);
  if (xs.length !== ps.length || !xs.length) return E.NA;
  if (ps.some((p) => p <= 0 || p > 1) || Math.abs(sum(ps) - 1) > 1e-9) return E.NUM;
  const lo = num(a[2], ctx);
  const hi = isMissing(a[3]) ? lo : num(a[3], ctx);
  let t = 0;
  xs.forEach((x, i) => {
    if (x >= lo && x <= hi) t += ps[i];
  });
  return fin(t);
});
def('FREQUENCY', 2, 2, (a, ctx) => {
  const data = numbersFrom(a[0], ctx);
  const bins = numbersFrom(a[1], ctx);
  const order = bins.map((b, i) => [b, i] as const).sort((p, q) => p[0] - q[0]);
  const counts: number[] = new Array(bins.length + 1).fill(0);
  for (const d of data) {
    let slot = bins.length;
    for (let k = 0; k < order.length; k++)
      if (d <= order[k][0]) {
        slot = k;
        break;
      }
    counts[slot]++;
  }
  // Excel returns counts aligned with the sorted bin boundaries.
  return vector(counts);
});

// ------------------------------------------------- criteria aggregates

function singleRef(v: Value): RefValue {
  if (v instanceof RefValue) return v;
  if (v instanceof MultiRef) throw E.VALUE;
  throw E.VALUE;
}

interface CriteriaSet {
  tests: { range: RefValue; test: CriteriaTest }[];
}
function buildCriteria(args: Value[], ctx: EvalContext, scalarCriteria: Scalar[]): CriteriaSet {
  const tests: CriteriaSet['tests'] = [];
  const rows = singleRef(args[0]).rows,
    cols = singleRef(args[0]).cols;
  for (let i = 0; i < args.length; i += 2) {
    const range = singleRef(args[i]);
    if (range.rows !== rows || range.cols !== cols) throw E.VALUE;
    tests.push({ range, test: evaluateCriteriaArgs(scalarCriteria[i / 2], ctx) });
  }
  return { tests };
}

function matchingCells(set: CriteriaSet, ctx: EvalContext, visit: (r: number, c: number) => void) {
  const first = set.tests[0].range;
  ctx.charge(first.rows * first.cols * set.tests.length);
  const area = first.clipped();
  for (let r = 0; r < area.rows; r++)
    for (let c = 0; c < area.cols; c++) {
      let ok = true;
      for (const t of set.tests) {
        if (!t.test(t.range.get(r, c))) {
          ok = false;
          break;
        }
      }
      if (ok) visit(r, c);
    }
}

/** Criteria arguments may be arrays, which lifts the aggregate over them. */
function withCriteria(
  args: Value[],
  ctx: EvalContext,
  criteriaIdx: number[],
  run: (criteria: Scalar[]) => Scalar
): Value {
  const raw = criteriaIdx.map((i) => operand(args[i]));
  const matrix = raw.find(
    (c): c is ArrayValue | RefValue => c instanceof ArrayValue || c instanceof RefValue
  );
  if (!matrix) return run(raw as Scalar[]);
  const m = matrixOf(matrix, ctx);
  return ArrayValue.of(m.rows, m.cols, (r, c) =>
    run(
      raw.map((x) => {
        if (x instanceof ArrayValue || x instanceof RefValue) {
          const mm = matrixOf(x, ctx);
          return mm.get(mm.rows === 1 ? 0 : r, mm.cols === 1 ? 0 : c);
        }
        return x as Scalar;
      })
    )
  );
}

function sumRangeFor(sumArg: Value, first: RefValue): RefValue {
  const s = singleRef(sumArg);
  return new RefValue(s.src, s.sheet, s.r1, s.c1, s.r1 + first.rows - 1, s.c1 + first.cols - 1);
}

def('COUNTIF', 2, 2, (a, ctx) =>
  withCriteria(a, ctx, [1], ([crit]) => {
    const range = singleRef(a[0]);
    const set = buildCriteria([range, range], ctx, [crit]);
    let n = 0;
    matchingCells(set, ctx, () => n++);
    return n;
  })
);
def('COUNTIFS', 2, 254, (a, ctx) => {
  if (a.length % 2) return E.VALUE;
  const idx = a.map((_, i) => i).filter((i) => i % 2 === 1);
  return withCriteria(a, ctx, idx, (crit) => {
    const set = buildCriteria(a, ctx, crit);
    let n = 0;
    matchingCells(set, ctx, () => n++);
    return n;
  });
});

function sumLike(name: string, multi: boolean, finish: (values: number[]) => Scalar) {
  const rangeIdx = multi ? 0 : 0;
  def(name, multi ? 3 : 2, multi ? 255 : 3, (a, ctx) => {
    // SUMIF(range, criteria, [sum_range]) vs SUMIFS(sum_range, range1, crit1, ...)
    const pairsArgs: Value[] = multi ? a.slice(1) : [a[0], a[1]];
    if (pairsArgs.length % 2) return E.VALUE;
    const idx = pairsArgs.map((_, i) => i).filter((i) => i % 2 === 1);
    const target = multi ? singleRef(a[rangeIdx]) : isMissing(a[2]) ? singleRef(a[0]) : null;
    return withCriteria(pairsArgs, ctx, idx, (crit) => {
      const set = buildCriteria(pairsArgs, ctx, crit);
      const first = set.tests[0].range;
      const sumRange = multi ? singleRef(a[0]) : (target ?? sumRangeFor(a[2], first));
      if (multi && (sumRange.rows !== first.rows || sumRange.cols !== first.cols)) return E.VALUE;
      const values: number[] = [];
      let failure: FormulaError | undefined;
      matchingCells(set, ctx, (r, c) => {
        const v = sumRange.get(r, c);
        if (isError(v)) failure ??= v;
        else if (typeof v === 'number') values.push(v);
      });
      return failure ?? finish(values);
    });
  });
}
sumLike('SUMIF', false, (v) => fin(sum(v)));
sumLike('SUMIFS', true, (v) => fin(sum(v)));
sumLike('AVERAGEIF', false, (v) => (v.length ? fin(mean(v)) : E.DIV0));
sumLike('AVERAGEIFS', true, (v) => (v.length ? fin(mean(v)) : E.DIV0));
sumLike('MAXIFS', true, (v) => (v.length ? Math.max(...v) : 0));
sumLike('MINIFS', true, (v) => (v.length ? Math.min(...v) : 0));

// ---------------------------------------------------- SUBTOTAL / AGGREGATE

const NESTED = /\b(?:SUBTOTAL|AGGREGATE)\s*\(/i;
function subtotalValues(args: Value[], ctx: EvalContext, skipErrors: boolean): number[] {
  const out: number[] = [];
  const visit = (ref: RefValue) => {
    const r = ref.clipped();
    ref.src.eachCell(r.sheet, r.r1, r.c1, r.r2, r.c2, (v, row, col) => {
      ctx.charge(1);
      if (isError(v)) {
        if (!skipErrors) throw v;
        return;
      }
      if (typeof v === 'number') {
        const text = r.src.formulaText(r.sheet, row, col);
        if (text !== undefined && NESTED.test(text)) return;
        out.push(v);
      }
    });
  };
  for (const a of args) {
    if (a instanceof RefValue) visit(a);
    else if (a instanceof MultiRef) a.areas.forEach(visit);
    else if (a instanceof ArrayValue)
      a.data.forEach((v) => {
        if (isError(v)) {
          if (!skipErrors) throw v;
        } else if (typeof v === 'number') out.push(v);
      });
    else if (typeof a === 'number') out.push(a);
  }
  return out;
}
function applyAggregate(
  code: number,
  xs: number[],
  count: number,
  counta: number,
  k?: number
): Scalar {
  switch (code) {
    case 1:
      return xs.length ? fin(mean(xs)) : E.DIV0;
    case 2:
      return count;
    case 3:
      return counta;
    case 4:
      return xs.length ? Math.max(...xs) : 0;
    case 5:
      return xs.length ? Math.min(...xs) : 0;
    case 6:
      return xs.length ? fin(xs.reduce((p, n) => p * n, 1)) : 0;
    case 7: {
      const v = variance(xs, true);
      return isError(v) ? v : fin(Math.sqrt(v));
    }
    case 8: {
      const v = variance(xs, false);
      return isError(v) ? v : fin(Math.sqrt(v));
    }
    case 9:
      return fin(sum(xs));
    case 10: {
      const v = variance(xs, true);
      return isError(v) ? v : fin(v);
    }
    case 11: {
      const v = variance(xs, false);
      return isError(v) ? v : fin(v);
    }
    case 12: {
      const s = sorted(xs);
      if (!s.length) return E.NUM;
      const m = Math.floor(s.length / 2);
      return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
    }
    case 13: {
      const counts = new Map<number, number>();
      let best = 0,
        out: number | undefined;
      for (const x of xs) counts.set(x, (counts.get(x) ?? 0) + 1);
      for (const x of xs)
        if (counts.get(x)! > best) {
          best = counts.get(x)!;
          out = x;
        }
      return best < 2 || out === undefined ? E.NA : out;
    }
    case 14:
    case 15: {
      const s = sorted(xs);
      const kk = Math.ceil(k ?? 0);
      if (kk < 1 || kk > s.length) return E.NUM;
      return code === 14 ? s[s.length - kk] : s[kk - 1];
    }
    case 16:
    case 18: {
      if (!xs.length || (k ?? -1) < 0 || (k ?? 2) > 1) return E.NUM;
      return fin(percentileInc(xs, k!));
    }
    case 17: {
      const q = Math.trunc(k ?? -1);
      if (!xs.length || q < 0 || q > 4) return E.NUM;
      return fin(percentileInc(xs, q / 4));
    }
    case 19: {
      const q = Math.trunc(k ?? -1);
      if (!xs.length || q < 1 || q > 3) return E.NUM;
      const r = percentileExc(xs, q / 4);
      return isError(r) ? r : fin(r);
    }
    default:
      return E.VALUE;
  }
}
def('SUBTOTAL', 2, 255, (a, ctx) => {
  let code = Math.trunc(num(a[0], ctx));
  if (code > 100) code -= 100;
  if (code < 1 || code > 11) return E.VALUE;
  const xs = subtotalValues(a.slice(1), ctx, false);
  let counta = 0;
  for (const arg of a.slice(1)) {
    const refs = arg instanceof RefValue ? [arg] : arg instanceof MultiRef ? arg.areas : [];
    for (const ref of refs)
      ref.each((v, r, c) => {
        const t = ref.src.formulaText(ref.sheet, ref.r1 + r, ref.c1 + c);
        if (v !== null && !(t !== undefined && NESTED.test(t))) counta++;
      });
  }
  return applyAggregate(code, xs, xs.length, counta);
});
def('AGGREGATE', 3, 255, (a, ctx) => {
  const code = Math.trunc(num(a[0], ctx));
  const option = Math.trunc(num(a[1], ctx));
  if (code < 1 || code > 19 || option < 0 || option > 7) return E.VALUE;
  const skip = [2, 3, 6, 7].includes(option);
  const needsK = code >= 14;
  const dataArgs: Value[] = needsK ? [a[2]] : a.slice(2);
  const xs = subtotalValues(dataArgs, ctx, skip);
  const k = needsK ? num(a[3], ctx) : undefined;
  return applyAggregate(code, xs, xs.length, xs.length, k);
});

import { FUNCTIONS } from './fn-util';
function FUNCTIONS_ALIAS(alias: string, target: string) {
  FUNCTIONS.set(alias, FUNCTIONS.get(target)!);
}
