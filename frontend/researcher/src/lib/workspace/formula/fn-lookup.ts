import { E, FormulaError, isError } from './errors';
import { operand, type EvalContext } from './evaluator';
import { wildcardRegExp } from './criteria';
import { colToName, sheetPrefix } from './parser';
import {
  ArrayValue,
  MISSING,
  MultiRef,
  RefValue,
  compareScalars,
  toNumber,
  type Scalar,
  type Value
} from './values';
import { def, isMissing, int, intOr, num, boolOr, matrixOf, str, vector } from './fn-util';

// ------------------------------------------------------------------ helpers

type Grid = { rows: number; cols: number; get(r: number, c: number): Scalar };

function grid(v: Value, ctx: EvalContext): Grid {
  if (v instanceof RefValue) {
    ctx.charge(Math.min(v.rows * v.cols, 1));
    return v;
  }
  if (v instanceof MultiRef) throw E.VALUE;
  if (v instanceof ArrayValue) return v;
  if (v === MISSING) throw E.VALUE;
  if (isError(v)) throw v;
  return new ArrayValue(1, 1, [v]);
}
const blankToZero = (v: Scalar): Scalar => (v === null ? 0 : v);

/** Excel equality for lookups: same type, text case-insensitive, optional wildcards. */
function exactTest(lookup: Scalar, wildcard: boolean): (cell: Scalar) => boolean {
  if (typeof lookup === 'string' && wildcard) {
    const re = wildcardRegExp(lookup);
    if (re) return (c) => typeof c === 'string' && re.test(c);
  }
  return (c) => {
    if (typeof c !== typeof lookup) return false;
    if (isError(c) || isError(lookup))
      return isError(c) && isError(lookup) && c.value === lookup.value;
    return compareScalars(c, lookup) === 0;
  };
}

/** Binary search for the last element <= lookup (ascending) as Excel does; -1 if none. */
function searchAscending(n: number, at: (i: number) => Scalar, lookup: Scalar): number {
  let lo = 0,
    hi = n - 1,
    best = -1;
  const sameKind = (v: Scalar) => typeof v === typeof lookup;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    let v = at(mid);
    if (v === null) v = typeof lookup === 'string' ? '' : typeof lookup === 'boolean' ? false : 0;
    if (!sameKind(v) || isError(v)) {
      // Skip mismatched types by scanning towards the last same-type element.
      let j = mid;
      while (j >= lo && (!sameKind(at(j)) || isError(at(j)))) j--;
      if (j < lo) {
        lo = mid + 1;
        continue;
      }
      v = at(j);
    }
    const c = compareScalars(v, lookup);
    if (c === 0) {
      // Excel returns the last of a run of equal values.
      let j = mid;
      while (j + 1 < n && compareScalars(at(j + 1), lookup) === 0) j++;
      return j;
    }
    if (c < 0) {
      best = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return best;
}
function searchDescending(n: number, at: (i: number) => Scalar, lookup: Scalar): number {
  let lo = 0,
    hi = n - 1,
    best = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const v = at(mid);
    if (typeof v !== typeof lookup || isError(v)) {
      lo = mid + 1;
      continue;
    }
    const c = compareScalars(v, lookup);
    if (c === 0) return mid;
    if (c > 0) {
      best = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return best;
}

function vectorAt(g: Grid): { n: number; at: (i: number) => Scalar } | null {
  if (g.rows === 1) return { n: g.cols, at: (i) => g.get(0, i) };
  if (g.cols === 1) return { n: g.rows, at: (i) => g.get(i, 0) };
  return null;
}

/** Apply `fn` to each element when `v` is an array (lifting a scalar lookup). */
function liftLookup(v: Value, ctx: EvalContext, fn: (s: Scalar) => Value): Value {
  const o = operand(v);
  if (o instanceof ArrayValue || o instanceof RefValue) {
    const m = matrixOf(o, ctx);
    return ArrayValue.of(m.rows, m.cols, (r, c) => {
      const x = operand(fn(m.get(r, c)));
      return x instanceof ArrayValue || x instanceof RefValue ? E.VALUE : (x as Scalar);
    });
  }
  return fn(o as Scalar);
}

// ------------------------------------------------------- row/column/address

def('ROW', 0, 1, (a, ctx) => {
  if (isMissing(a[0])) return ctx.row + 1;
  const v = a[0];
  const ref = v instanceof MultiRef ? v.areas[0] : v;
  if (!(ref instanceof RefValue)) return E.VALUE;
  if (ref.rows === 1) return ref.r1 + 1;
  return vector(Array.from({ length: ref.rows }, (_, i) => ref.r1 + i + 1));
});
def('COLUMN', 0, 1, (a, ctx) => {
  if (isMissing(a[0])) return ctx.col + 1;
  const v = a[0];
  const ref = v instanceof MultiRef ? v.areas[0] : v;
  if (!(ref instanceof RefValue)) return E.VALUE;
  if (ref.cols === 1) return ref.c1 + 1;
  return vector(
    Array.from({ length: ref.cols }, (_, i) => ref.c1 + i + 1),
    false
  );
});
def('ROWS', 1, 1, (a) => {
  const v = a[0];
  if (v instanceof RefValue || v instanceof ArrayValue) return v.rows;
  if (v instanceof MultiRef) return E.REF;
  return isError(v) ? v : 1;
});
def('COLUMNS', 1, 1, (a) => {
  const v = a[0];
  if (v instanceof RefValue || v instanceof ArrayValue) return v.cols;
  if (v instanceof MultiRef) return E.REF;
  return isError(v) ? v : 1;
});
def('AREAS', 1, 1, (a) => {
  const v = a[0];
  if (v instanceof MultiRef) return v.areas.length;
  return v instanceof RefValue ? 1 : E.VALUE;
});
def('ADDRESS', 2, 5, (a, ctx) => {
  const row = int(a[0], ctx),
    col = int(a[1], ctx);
  const abs = intOr(a[2], ctx, 1);
  const a1 = boolOr(a[3], true);
  if (row < 1 || col < 1 || row > 1048576 || col > 16384 || abs < 1 || abs > 4) return E.VALUE;
  const rowAbs = abs === 1 || abs === 2,
    colAbs = abs === 1 || abs === 3;
  let text: string;
  if (a1) text = (colAbs ? '$' : '') + colToName(col - 1) + (rowAbs ? '$' : '') + row;
  else text = 'R' + (rowAbs ? row : `[${row}]`) + 'C' + (colAbs ? col : `[${col}]`);
  if (!isMissing(a[4])) text = sheetPrefix(str(a[4])) + text;
  return text;
});

// ----------------------------------------------------------- INDEX / MATCH

function indexOne(v: Value, rowN: number, colN: number | undefined, ctx: EvalContext): Value {
  const g = grid(v, ctx);
  let row = rowN;
  let col = colN;
  if (row < 0 || (col !== undefined && col < 0)) return E.VALUE;
  if (col === undefined) {
    if (g.rows === 1) {
      // a single index into a row vector addresses its columns
      col = row;
      row = 1;
    } else if (g.cols === 1) col = 1;
    else {
      if (row !== 0) return E.REF;
      col = 0;
    }
  }
  if (row > g.rows || col > g.cols) return E.REF;
  return pick(v, row, col, g, ctx);
}
function pick(v: Value, row: number, col: number, g: Grid, ctx: EvalContext): Value {
  const r1 = row === 0 ? 0 : row - 1,
    r2 = row === 0 ? g.rows - 1 : row - 1;
  const c1 = col === 0 ? 0 : col - 1,
    c2 = col === 0 ? g.cols - 1 : col - 1;
  if (v instanceof RefValue)
    return new RefValue(v.src, v.sheet, v.r1 + r1, v.c1 + c1, v.r1 + r2, v.c1 + c2);
  const rows = r2 - r1 + 1,
    cols = c2 - c1 + 1;
  if (rows === 1 && cols === 1) return g.get(r1, c1);
  ctx.charge(rows * cols);
  return ArrayValue.of(rows, cols, (r, c) => g.get(r1 + r, c1 + c));
}
def('INDEX', 2, 4, (a, ctx) => {
  const rowArg = operand(a[1]);
  const colArg = isMissing(a[2]) ? undefined : operand(a[2]);
  if (isError(rowArg)) return rowArg;
  if (colArg !== undefined && isError(colArg)) return colArg;
  const asNum = (x: unknown): number => {
    if (x === undefined) return 0;
    const n = toNumber(x as Scalar, ctx.coerce);
    if (isError(n)) throw n;
    return Math.trunc(n);
  };
  const arrays = (x: unknown) => x instanceof ArrayValue || x instanceof RefValue;
  if (!arrays(rowArg) && !arrays(colArg)) {
    return indexOne(a[0], asNum(rowArg), colArg === undefined ? undefined : asNum(colArg), ctx);
  }
  const rm = arrays(rowArg) ? matrixOf(rowArg as Value, ctx) : null;
  const cm = arrays(colArg) ? matrixOf(colArg as Value, ctx) : null;
  const rows = Math.max(rm?.rows ?? 1, cm?.rows ?? 1),
    cols = Math.max(rm?.cols ?? 1, cm?.cols ?? 1);
  return ArrayValue.of(rows, cols, (r, c) => {
    try {
      const rv = rm ? rm.get(rm.rows === 1 ? 0 : r, rm.cols === 1 ? 0 : c) : rowArg;
      const cv = cm ? cm.get(cm.rows === 1 ? 0 : r, cm.cols === 1 ? 0 : c) : colArg;
      const out = operand(indexOne(a[0], asNum(rv), cv === undefined ? undefined : asNum(cv), ctx));
      return out instanceof ArrayValue || out instanceof RefValue ? E.VALUE : (out as Scalar);
    } catch (e) {
      if (e instanceof FormulaError) return e;
      throw e;
    }
  });
});

def('MATCH', 2, 3, (a, ctx) => {
  const type = isMissing(a[2]) ? 1 : Math.trunc(num(a[2], ctx));
  const g = grid(a[1], ctx);
  const vec = vectorAt(g);
  return liftLookup(a[0], ctx, (lookup) => {
    if (isError(lookup)) return lookup;
    if (!vec) return E.NA;
    const target = lookup === null ? 0 : lookup;
    if (type === 0) {
      const test = exactTest(target, true);
      for (let i = 0; i < vec.n; i++) if (test(vec.at(i))) return i + 1;
      return E.NA;
    }
    const i =
      type > 0 ? searchAscending(vec.n, vec.at, target) : searchDescending(vec.n, vec.at, target);
    return i < 0 ? E.NA : i + 1;
  });
});

def('XMATCH', 2, 4, (a, ctx) => {
  const mode = isMissing(a[2]) ? 0 : Math.trunc(num(a[2], ctx));
  const search = isMissing(a[3]) ? 1 : Math.trunc(num(a[3], ctx));
  const g = grid(a[1], ctx);
  const vec = vectorAt(g);
  return liftLookup(a[0], ctx, (lookup) => {
    if (isError(lookup)) return lookup;
    if (!vec) return E.VALUE;
    const i = xFind(vec, lookup === null ? 0 : lookup, mode, search);
    return i < 0 ? E.NA : i + 1;
  });
});

function xFind(
  vec: { n: number; at: (i: number) => Scalar },
  lookup: Scalar,
  mode: number,
  search: number
): number {
  if (![0, 1, -1, 2].includes(mode) || ![1, -1, 2, -2].includes(search)) throw E.VALUE;
  if (search === 2 || search === -2) {
    const asc = search === 2;
    if (mode === 0 || mode === 2) {
      // binary search for exact match
      let lo = 0,
        hi = vec.n - 1;
      while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        const v = vec.at(mid);
        if (typeof v !== typeof lookup || isError(v)) return linearExact();
        const c = compareScalars(v, lookup);
        if (c === 0) return mid;
        if (c < 0 === asc) lo = mid + 1;
        else hi = mid - 1;
      }
      return -1;
    }
    return linearNearest();
  }
  return linearExact();

  function linearExact(): number {
    const test = exactTest(lookup, mode === 2);
    const order = search < 0 ? -1 : 1;
    if (mode === 0 || mode === 2) {
      for (let k = 0; k < vec.n; k++) {
        const i = order > 0 ? k : vec.n - 1 - k;
        if (test(vec.at(i))) return i;
      }
      return -1;
    }
    return linearNearest();
  }
  function linearNearest(): number {
    // exact match wins; otherwise next smaller (-1) or next larger (1)
    let best = -1;
    let bestValue: Scalar = null;
    for (let k = 0; k < vec.n; k++) {
      const i = search < 0 ? vec.n - 1 - k : k;
      const v = vec.at(i);
      if (typeof v !== typeof lookup || isError(v)) continue;
      const c = compareScalars(v, lookup);
      if (c === 0) return i;
      if (mode === -1 && c < 0 && (best < 0 || compareScalars(v, bestValue) > 0)) {
        best = i;
        bestValue = v;
      } else if (mode === 1 && c > 0 && (best < 0 || compareScalars(v, bestValue) < 0)) {
        best = i;
        bestValue = v;
      }
    }
    return best;
  }
}

def('XLOOKUP', 3, 6, (a, ctx) => {
  const lookupGrid = grid(a[1], ctx);
  const ret = grid(a[2], ctx);
  const vec = vectorAt(lookupGrid);
  const mode = isMissing(a[4]) ? 0 : Math.trunc(num(a[4], ctx));
  const search = isMissing(a[5]) ? 1 : Math.trunc(num(a[5], ctx));
  return liftLookup(a[0], ctx, (lookup) => {
    if (isError(lookup)) return lookup;
    if (!vec) return E.VALUE;
    const vertical = lookupGrid.cols === 1;
    if ((vertical ? ret.rows : ret.cols) !== vec.n) return E.VALUE;
    const i = xFind(vec, lookup === null ? 0 : lookup, mode, search);
    if (i < 0) return isMissing(a[3]) ? E.NA : (operand(a[3]) as Scalar);
    if (vertical) {
      if (ret.cols === 1) return blankToZero(ret.get(i, 0));
      return ArrayValue.of(1, ret.cols, (_r, c) => blankToZero(ret.get(i, c)));
    }
    if (ret.rows === 1) return blankToZero(ret.get(0, i));
    return ArrayValue.of(ret.rows, 1, (r) => blankToZero(ret.get(r, i)));
  });
});

function tableLookup(horizontal: boolean) {
  return (a: Value[], ctx: EvalContext): Value => {
    const table = grid(a[1], ctx);
    const idx = Math.trunc(num(a[2], ctx));
    const approx = isMissing(a[3]) ? true : boolOr(operand(a[3]) as Scalar, true);
    const size = horizontal ? table.rows : table.cols;
    if (idx < 1) return E.VALUE;
    if (idx > size) return E.REF;
    const n = horizontal ? table.cols : table.rows;
    const at = (i: number) => (horizontal ? table.get(0, i) : table.get(i, 0));
    return liftLookup(a[0], ctx, (lookup) => {
      if (isError(lookup)) return lookup;
      const target = lookup === null ? 0 : lookup;
      let i = -1;
      if (approx) i = searchAscending(n, at, target);
      else {
        const test = exactTest(target, true);
        for (let k = 0; k < n; k++)
          if (test(at(k))) {
            i = k;
            break;
          }
      }
      if (i < 0) return E.NA;
      return blankToZero(horizontal ? table.get(idx - 1, i) : table.get(i, idx - 1));
    });
  };
}
def('VLOOKUP', 3, 4, tableLookup(false));
def('HLOOKUP', 3, 4, tableLookup(true));
def('LOOKUP', 2, 3, (a, ctx) => {
  const g = grid(a[1], ctx);
  return liftLookup(a[0], ctx, (lookup) => {
    if (isError(lookup)) return lookup;
    const target = lookup === null ? 0 : lookup;
    let vec = vectorAt(g);
    let result: Grid;
    if (!isMissing(a[2])) {
      result = grid(a[2], ctx);
      if (!vec) return E.NA;
    } else if (vec) {
      result = g;
    } else {
      // array form: search first row (wide) or first column (tall), return the last
      if (g.cols > g.rows) {
        vec = { n: g.cols, at: (i) => g.get(0, i) };
        result = { rows: 1, cols: g.cols, get: (_r, c) => g.get(g.rows - 1, c) };
      } else {
        vec = { n: g.rows, at: (i) => g.get(i, 0) };
        result = { rows: g.rows, cols: 1, get: (r) => g.get(r, g.cols - 1) };
      }
    }
    const i = searchAscending(vec.n, vec.at, target);
    if (i < 0) return E.NA;
    const out = result.rows === 1 ? result.get(0, i) : result.get(i, 0);
    return blankToZero(out);
  });
});

// --------------------------------------------------- reference construction

def(
  'OFFSET',
  3,
  5,
  (a, ctx) => {
    const base = a[0];
    if (!(base instanceof RefValue)) return E.VALUE;
    const dr = int(a[1], ctx),
      dc = int(a[2], ctx);
    const h = isMissing(a[3]) ? base.rows : int(a[3], ctx);
    const w = isMissing(a[4]) ? base.cols : int(a[4], ctx);
    if (h < 1 || w < 1) return E.REF;
    const r1 = base.r1 + dr,
      c1 = base.c1 + dc;
    if (r1 < 0 || c1 < 0 || r1 + h > 1048576 || c1 + w > 16384) return E.REF;
    return new RefValue(base.src, base.sheet, r1, c1, r1 + h - 1, c1 + w - 1);
  },
  { volatile: true }
);
def(
  'INDIRECT',
  1,
  2,
  (a, ctx) => {
    const text = str(a[0]).trim();
    if (!text) return E.REF;
    if (!isMissing(a[1]) && !boolOr(a[1], true)) return E.REF;
    return ctx.evalReferenceText(text);
  },
  { volatile: true }
);

// ------------------------------------------------------------- array tools

def('TRANSPOSE', 1, 1, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  return ArrayValue.of(m.cols, m.rows, (r, c) => m.get(c, r));
});
def('ARRAYFORMULA', 1, 1, (a) => a[0]);
def('ARRAY_CONSTRAIN', 3, 3, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const r = int(a[1], ctx),
    c = int(a[2], ctx);
  if (r < 1 || c < 1) return E.VALUE;
  return ArrayValue.of(Math.min(r, m.rows), Math.min(c, m.cols), (i, j) => m.get(i, j));
});

function sortKey(v: Scalar): number {
  if (isError(v)) return 3;
  if (v === null) return 4;
  return typeof v === 'number' ? 0 : typeof v === 'string' ? 1 : 2;
}
function compareForSort(x: Scalar, y: Scalar): number {
  const kx = sortKey(x),
    ky = sortKey(y);
  if (kx !== ky) return kx < ky ? -1 : 1;
  if (kx >= 3) return 0;
  return compareScalars(x, y);
}
function sortByKeys(m: ArrayValue, keys: { v: ArrayValue; desc: boolean }[], byCol: boolean) {
  const n = byCol ? m.cols : m.rows;
  const order = Array.from({ length: n }, (_, i) => i);
  order.sort((p, q) => {
    for (const k of keys) {
      const x = k.v.data[byCol ? p : p];
      const y = k.v.data[byCol ? q : q];
      const c = compareForSort(x, y);
      if (c) return k.desc ? -c : c;
    }
    return p - q;
  });
  return byCol
    ? ArrayValue.of(m.rows, m.cols, (r, c) => m.get(r, order[c]))
    : ArrayValue.of(m.rows, m.cols, (r, c) => m.get(order[r], c));
}
def('SORT', 1, 4, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const byCol = boolOr(operand(a[3]) as Scalar, false);
  const idx = isMissing(a[1]) ? [1] : matrixOf(a[1], ctx).data.map((v) => Math.trunc(num(v, ctx)));
  const ord = isMissing(a[2]) ? [1] : matrixOf(a[2], ctx).data.map((v) => Math.trunc(num(v, ctx)));
  const lines = byCol ? m.rows : m.cols;
  const keys = idx.map((k, i) => {
    if (k < 1 || k > lines) throw E.VALUE;
    const o = ord[Math.min(i, ord.length - 1)];
    if (o !== 1 && o !== -1) throw E.VALUE;
    const v = byCol
      ? m.data.slice((k - 1) * m.cols, k * m.cols)
      : m.data.filter((_, j) => j % m.cols === k - 1);
    return { v: new ArrayValue(v.length, 1, v), desc: o === -1 };
  });
  return sortByKeys(m, keys, byCol);
});
def('SORTBY', 2, 255, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const keys: { v: ArrayValue; desc: boolean }[] = [];
  let byCol: boolean | undefined;
  for (let i = 1; i < a.length; i += 2) {
    const k = matrixOf(a[i], ctx);
    const horizontal = k.rows === 1 && k.cols > 1;
    const vertical = k.cols === 1;
    if (!horizontal && !vertical) return E.VALUE;
    const col = horizontal && !(k.cols === 1);
    if (byCol === undefined) byCol = col && m.cols === k.cols && !(m.rows === k.rows && !col);
    if ((byCol ? m.cols : m.rows) !== k.data.length) return E.VALUE;
    const o = isMissing(a[i + 1]) ? 1 : Math.trunc(num(a[i + 1], ctx));
    if (o !== 1 && o !== -1) return E.VALUE;
    keys.push({ v: k, desc: o === -1 });
  }
  return sortByKeys(m, keys, !!byCol);
});
def('UNIQUE', 1, 3, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const byCol = boolOr(operand(a[1]) as Scalar, false);
  const once = boolOr(operand(a[2]) as Scalar, false);
  const n = byCol ? m.cols : m.rows;
  const lineOf = (i: number): Scalar[] =>
    byCol
      ? Array.from({ length: m.rows }, (_, r) => m.get(r, i))
      : Array.from({ length: m.cols }, (_, c) => m.get(i, c));
  const keyOf = (line: Scalar[]) =>
    line
      .map((v) =>
        isError(v)
          ? 'e' + v.value
          : v === null
            ? 'n'
            : typeof v === 'string'
              ? 's' + v.toLowerCase()
              : typeof v + String(v)
      )
      .join('\u0001');
  const counts = new Map<string, number>();
  const firsts: number[] = [];
  const keys: string[] = [];
  for (let i = 0; i < n; i++) {
    const k = keyOf(lineOf(i));
    keys.push(k);
    if (!counts.has(k)) {
      counts.set(k, 0);
      firsts.push(i);
    }
    counts.set(k, counts.get(k)! + 1);
  }
  const chosen = once ? firsts.filter((i) => counts.get(keys[i]) === 1) : firsts;
  if (!chosen.length) return E.CALC;
  return byCol
    ? ArrayValue.of(m.rows, chosen.length, (r, c) => m.get(r, chosen[c]))
    : ArrayValue.of(chosen.length, m.cols, (r, c) => m.get(chosen[r], c));
});
def('FILTER', 2, 3, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const inc = matrixOf(a[1], ctx);
  const byRow =
    inc.cols === 1 && inc.rows === m.rows
      ? true
      : inc.rows === 1 && inc.cols === m.cols
        ? false
        : null;
  if (byRow === null) return E.VALUE;
  const keep: number[] = [];
  inc.data.forEach((v, i) => {
    if (isError(v)) throw v;
    if (typeof v === 'string' || v === null) {
      if (typeof v === 'string') throw E.VALUE;
      return;
    }
    if (typeof v === 'boolean' ? v : v !== 0) keep.push(i);
  });
  if (!keep.length) return isMissing(a[2]) ? E.CALC : (operand(a[2]) as Scalar);
  return byRow
    ? ArrayValue.of(keep.length, m.cols, (r, c) => m.get(keep[r], c))
    : ArrayValue.of(m.rows, keep.length, (r, c) => m.get(r, keep[c]));
});
function stack(horizontal: boolean) {
  return (a: Value[], ctx: EvalContext): Value => {
    const mats = a.map((x) => matrixOf(x, ctx));
    if (horizontal) {
      const rows = Math.max(...mats.map((m) => m.rows));
      const cols = mats.reduce((s, m) => s + m.cols, 0);
      return ArrayValue.of(rows, cols, (r, c) => {
        let off = c;
        for (const m of mats) {
          if (off < m.cols) return r < m.rows ? m.get(r, off) : E.NA;
          off -= m.cols;
        }
        return E.NA;
      });
    }
    const cols = Math.max(...mats.map((m) => m.cols));
    const rows = mats.reduce((s, m) => s + m.rows, 0);
    return ArrayValue.of(rows, cols, (r, c) => {
      let off = r;
      for (const m of mats) {
        if (off < m.rows) return c < m.cols ? m.get(off, c) : E.NA;
        off -= m.rows;
      }
      return E.NA;
    });
  };
}
def('VSTACK', 1, 255, stack(false));
def('HSTACK', 1, 255, stack(true));
def('TAKE', 2, 3, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const r = int(a[1], ctx);
  const c = isMissing(a[2]) ? m.cols : int(a[2], ctx);
  if (r === 0 || c === 0) return E.CALC;
  const rows = Math.min(Math.abs(r), m.rows),
    cols = Math.min(Math.abs(c), m.cols);
  const r0 = r > 0 ? 0 : m.rows - rows,
    c0 = c > 0 || isMissing(a[2]) ? 0 : m.cols - cols;
  return ArrayValue.of(rows, cols, (i, j) => m.get(r0 + i, c0 + j));
});
def('DROP', 2, 3, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const r = int(a[1], ctx);
  const c = isMissing(a[2]) ? 0 : int(a[2], ctx);
  const rows = m.rows - Math.min(Math.abs(r), m.rows),
    cols = m.cols - Math.min(Math.abs(c), m.cols);
  if (rows <= 0 || cols <= 0) return E.CALC;
  const r0 = r >= 0 ? m.rows - rows : 0,
    c0 = c >= 0 ? m.cols - cols : 0;
  return ArrayValue.of(rows, cols, (i, j) => m.get(r0 + i, c0 + j));
});
def('CHOOSECOLS', 2, 255, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const idx = a.slice(1).flatMap((x) => matrixOf(x, ctx).data.map((v) => Math.trunc(num(v, ctx))));
  if (idx.some((i) => i === 0 || Math.abs(i) > m.cols)) return E.VALUE;
  return ArrayValue.of(m.rows, idx.length, (r, c) =>
    m.get(r, idx[c] > 0 ? idx[c] - 1 : m.cols + idx[c])
  );
});
def('CHOOSEROWS', 2, 255, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const idx = a.slice(1).flatMap((x) => matrixOf(x, ctx).data.map((v) => Math.trunc(num(v, ctx))));
  if (idx.some((i) => i === 0 || Math.abs(i) > m.rows)) return E.VALUE;
  return ArrayValue.of(idx.length, m.cols, (r, c) =>
    m.get(idx[r] > 0 ? idx[r] - 1 : m.rows + idx[r], c)
  );
});
function flatten(vertical: boolean) {
  return (a: Value[], ctx: EvalContext): Value => {
    const m = matrixOf(a[0], ctx);
    const ignore = intOr(a[1], ctx, 0);
    const byCol = boolOr(operand(a[2]) as Scalar, false);
    const out: Scalar[] = [];
    const visit = (r: number, c: number) => {
      const v = m.get(r, c);
      if ((ignore === 1 || ignore === 3) && v === null) return;
      if ((ignore === 2 || ignore === 3) && isError(v)) return;
      out.push(v);
    };
    if (byCol) for (let c = 0; c < m.cols; c++) for (let r = 0; r < m.rows; r++) visit(r, c);
    else for (let r = 0; r < m.rows; r++) for (let c = 0; c < m.cols; c++) visit(r, c);
    if (!out.length) return E.CALC;
    return vector(out, vertical);
  };
}
def('TOCOL', 1, 3, flatten(true));
def('TOROW', 1, 3, flatten(false));
def('EXPAND', 2, 4, (a, ctx) => {
  const m = matrixOf(a[0], ctx);
  const rows = int(a[1], ctx);
  const cols = isMissing(a[2]) ? m.cols : int(a[2], ctx);
  const pad = isMissing(a[3]) ? E.NA : (operand(a[3]) as Scalar);
  if (rows < m.rows || cols < m.cols) return E.VALUE;
  return ArrayValue.of(rows, cols, (r, c) => (r < m.rows && c < m.cols ? m.get(r, c) : pad));
});
function wrap(byRow: boolean) {
  return (a: Value[], ctx: EvalContext): Value => {
    const m = matrixOf(a[0], ctx);
    const count = int(a[1], ctx);
    const pad = isMissing(a[2]) ? E.NA : (operand(a[2]) as Scalar);
    if (count < 1 || (m.rows > 1 && m.cols > 1)) return E.VALUE;
    const items = m.data;
    const lines = Math.ceil(items.length / count);
    return byRow
      ? ArrayValue.of(lines, count, (r, c) =>
          r * count + c < items.length ? items[r * count + c] : pad
        )
      : ArrayValue.of(count, lines, (r, c) =>
          c * count + r < items.length ? items[c * count + r] : pad
        );
  };
}
def('WRAPROWS', 2, 3, wrap(true));
def('WRAPCOLS', 2, 3, wrap(false));

// ------------------------------------------------------------- matrices

function numericMatrix(v: Value, ctx: EvalContext): number[][] {
  const m = matrixOf(v, ctx);
  const out: number[][] = [];
  for (let r = 0; r < m.rows; r++) {
    const row: number[] = [];
    for (let c = 0; c < m.cols; c++) {
      const x = m.get(r, c);
      if (isError(x)) throw x;
      if (typeof x !== 'number') throw E.VALUE;
      row.push(x);
    }
    out.push(row);
  }
  return out;
}
def('MMULT', 2, 2, (a, ctx) => {
  const x = numericMatrix(a[0], ctx),
    y = numericMatrix(a[1], ctx);
  if (x[0].length !== y.length) return E.VALUE;
  ctx.charge(x.length * y[0].length * y.length);
  return ArrayValue.of(x.length, y[0].length, (r, c) => {
    let s = 0;
    for (let k = 0; k < y.length; k++) s += x[r][k] * y[k][c];
    return s;
  });
});
function determinant(m: number[][]): number {
  const n = m.length;
  const a = m.map((r) => r.slice());
  let det = 1;
  for (let i = 0; i < n; i++) {
    let p = i;
    for (let r = i + 1; r < n; r++) if (Math.abs(a[r][i]) > Math.abs(a[p][i])) p = r;
    if (a[p][i] === 0) return 0;
    if (p !== i) {
      [a[p], a[i]] = [a[i], a[p]];
      det = -det;
    }
    det *= a[i][i];
    for (let r = i + 1; r < n; r++) {
      const f = a[r][i] / a[i][i];
      for (let c = i; c < n; c++) a[r][c] -= f * a[i][c];
    }
  }
  return det;
}
def('MDETERM', 1, 1, (a, ctx) => {
  const m = numericMatrix(a[0], ctx);
  if (m.length !== m[0].length) return E.VALUE;
  return determinant(m);
});
def('MINVERSE', 1, 1, (a, ctx) => {
  const m = numericMatrix(a[0], ctx);
  const n = m.length;
  if (n !== m[0].length) return E.VALUE;
  const aug = m.map((r, i) => [...r, ...Array.from({ length: n }, (_, j) => (i === j ? 1 : 0))]);
  for (let i = 0; i < n; i++) {
    let p = i;
    for (let r = i + 1; r < n; r++) if (Math.abs(aug[r][i]) > Math.abs(aug[p][i])) p = r;
    if (Math.abs(aug[p][i]) < 1e-300) return E.NUM;
    [aug[p], aug[i]] = [aug[i], aug[p]];
    const d = aug[i][i];
    for (let c = 0; c < 2 * n; c++) aug[i][c] /= d;
    for (let r = 0; r < n; r++) {
      if (r === i) continue;
      const f = aug[r][i];
      for (let c = 0; c < 2 * n; c++) aug[r][c] -= f * aug[i][c];
    }
  }
  return ArrayValue.of(n, n, (r, c) => aug[r][n + c]);
});
def('MUNIT', 1, 1, (a, ctx) => {
  const n = int(a[0], ctx);
  if (n < 1 || n > 1000) return E.VALUE;
  return ArrayValue.of(n, n, (r, c) => (r === c ? 1 : 0));
});
