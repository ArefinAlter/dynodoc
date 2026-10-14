import { E, FormulaError, isError } from './errors';
import type { DateOrder } from './dates';
import {
  ArrayValue,
  MISSING,
  MultiRef,
  RefValue,
  compareScalars,
  clean,
  toNumber,
  toText,
  type CellSource,
  type CoerceOptions,
  type Matrix,
  type Scalar,
  type Value
} from './values';
import type { Node, RefNode } from './parser';

/** Per-evaluation environment handed to every function. */
export interface EvalContext {
  readonly src: CellSource;
  /** Sheet, row and column of the formula being evaluated. */
  readonly sheet: number;
  readonly row: number;
  readonly col: number;
  readonly order: DateOrder;
  readonly year: number;
  readonly coerce: CoerceOptions;
  now(): number;
  random(): number;
  /** Count work; throws a FormulaError when the evaluation budget is exhausted. */
  charge(units: number): void;
  lookupName(name: string, sheetName: string | null, from: number): { ast: Node } | undefined;
  sheetIdByName(name: string): number | undefined;
  /** Evaluate reference text such as `Sheet1!A1:B2` or a defined name (INDIRECT). */
  evalReferenceText(text: string): Value;
  /** Name nesting depth, bounded to stop self-referencing names. */
  depth: number;
}

export interface LazyArg {
  readonly omitted: boolean;
  eval(): Value;
}

export interface FunctionDef {
  min: number;
  max: number;
  impl?: (args: Value[], ctx: EvalContext) => Value;
  lazy?: (args: LazyArg[], ctx: EvalContext) => Value;
  volatile?: boolean;
}

export const MAX_ARRAY_CELLS = 1_000_000;

export type FunctionTable = ReadonlyMap<string, FunctionDef>;

// ---------------------------------------------------------------- normalisation

/** Treat 1×1 matrices as scalars; keep larger ones; reject unions. */
export function operand(v: Value): Scalar | Matrix {
  if (v instanceof RefValue) return v.isCell ? v.get(0, 0) : v;
  if (v instanceof ArrayValue) return v.rows === 1 && v.cols === 1 ? v.data[0] : v;
  if (v instanceof MultiRef) return E.VALUE;
  if (v === MISSING) return null;
  return v;
}

/** Materialise a reference/array to a concrete array, clipping whole-line references. */
export function materialize(v: Value | Matrix, ctx: EvalContext): ArrayValue | FormulaError {
  const m = operand(v as Value);
  if (isError(m)) return m;
  if (m instanceof ArrayValue) return m;
  if (m instanceof RefValue) {
    const ref = m.rows > 100_000 || m.cols > 5_000 ? m.clipped() : m;
    const cells = ref.rows * ref.cols;
    if (cells > MAX_ARRAY_CELLS) return E.VALUE;
    ctx.charge(cells);
    return ArrayValue.of(ref.rows, ref.cols, (r, c) => {
      const x = ref.get(r, c);
      return x;
    });
  }
  return new ArrayValue(1, 1, [m as Scalar]);
}

function isMatrix(v: Scalar | Matrix): v is Matrix {
  return v instanceof ArrayValue || v instanceof RefValue;
}

/** Elementwise application with Excel broadcasting (single row/column stretches; others #N/A). */
export function broadcast(
  inputs: (Scalar | Matrix)[],
  ctx: EvalContext,
  fn: (values: Scalar[]) => Scalar
): Scalar | ArrayValue {
  let rows = 1,
    cols = 1;
  let any = false;
  const mats: (Scalar | ArrayValue)[] = inputs.map((x) => {
    if (!isMatrix(x)) return x;
    any = true;
    const m = materialize(x, ctx);
    if (isError(m)) return m;
    return m;
  });
  if (!any) return fn(mats as Scalar[]);
  for (const x of mats) {
    if (x instanceof ArrayValue) {
      rows = Math.max(rows, x.rows);
      cols = Math.max(cols, x.cols);
    }
  }
  if (rows * cols > MAX_ARRAY_CELLS) return E.VALUE;
  ctx.charge(rows * cols);
  const buf: Scalar[] = new Array(mats.length);
  return ArrayValue.of(rows, cols, (r, c) => {
    for (let k = 0; k < mats.length; k++) {
      const x = mats[k];
      if (x instanceof ArrayValue) {
        const rr = x.rows === 1 ? 0 : r;
        const cc = x.cols === 1 ? 0 : c;
        if (rr >= x.rows || cc >= x.cols) return E.NA;
        buf[k] = x.data[rr * x.cols + cc];
      } else buf[k] = x;
    }
    return fn(buf);
  });
}

// ------------------------------------------------------------------- operators

export function power(a: number, b: number): number | FormulaError {
  if (a === 0 && b === 0) return E.NUM;
  if (a === 0 && b < 0) return E.DIV0;
  if (a < 0 && !Number.isInteger(b)) {
    // Excel returns the real root for reciprocals of odd integers, e.g. (-8)^(1/3).
    const inv = 1 / b;
    if (
      Number.isInteger(Math.round(inv)) &&
      Math.abs(inv - Math.round(inv)) < 1e-12 &&
      Math.round(inv) % 2 !== 0
    )
      return -Math.pow(-a, b);
    return E.NUM;
  }
  const r = Math.pow(a, b);
  return Number.isFinite(r) ? r : E.NUM;
}

/** Threshold below which an add/subtract result is treated as zero (2^-49 of the operands). */
const SNAP = 2 ** -49;

export type ArithOp = '+' | '-' | '*' | '/' | '^';

function arithmetic(op: ArithOp, x: Scalar, y: Scalar, ctx: EvalContext, snap: boolean): Scalar {
  if (isError(x)) return x;
  if (isError(y)) return y;
  const a = typeof x === 'number' ? x : toNumber(x, ctx.coerce);
  if (isError(a)) return a;
  const b = typeof y === 'number' ? y : toNumber(y, ctx.coerce);
  if (isError(b)) return b;
  let r: number;
  switch (op) {
    case '+':
      r = a + b;
      break;
    case '-':
      r = a - b;
      break;
    case '*':
      r = a * b;
      break;
    case '/':
      if (b === 0) return E.DIV0;
      r = a / b;
      break;
    default: {
      const p = power(a, b);
      if (isError(p)) return p;
      r = p;
    }
  }
  if (!Number.isFinite(r)) return E.NUM;
  if (r !== 0 && Math.abs(r) < 2.2250738585072014e-308) return 0;
  if (snap && r !== 0 && (op === '+' || op === '-')) {
    const scale = Math.max(Math.abs(a), Math.abs(b));
    if (Math.abs(r) < scale * SNAP) return 0;
  }
  return clean(r);
}

export type CompareOp = '=' | '<>' | '<' | '>' | '<=' | '>=';

function comparison(op: CompareOp, x: Scalar, y: Scalar): Scalar {
  if (isError(x)) return x;
  if (isError(y)) return y;
  const c = compareScalars(x, y);
  switch (op) {
    case '=':
      return c === 0;
    case '<>':
      return c !== 0;
    case '<':
      return c < 0;
    case '>':
      return c > 0;
    case '<=':
      return c <= 0;
    default:
      return c >= 0;
  }
}

const MAX_TEXT = 32767;
function concat(x: Scalar, y: Scalar): Scalar {
  if (isError(x)) return x;
  if (isError(y)) return y;
  const a = toText(x),
    b = toText(y);
  if (isError(a)) return a;
  if (isError(b)) return b;
  if (a.length + b.length > MAX_TEXT) return E.VALUE;
  return a + b;
}

export function binaryOp(
  op: ArithOp | CompareOp | '&',
  left: Value,
  right: Value,
  ctx: EvalContext,
  snap = false
): Value {
  const a = operand(left),
    b = operand(right);
  const fn =
    op === '&'
      ? (v: Scalar[]) => concat(v[0], v[1])
      : op === '=' || op === '<>' || op === '<' || op === '>' || op === '<=' || op === '>='
        ? (v: Scalar[]) => comparison(op, v[0], v[1])
        : (v: Scalar[]) => arithmetic(op, v[0], v[1], ctx, snap);
  if (!isMatrix(a) && !isMatrix(b)) return fn([a as Scalar, b as Scalar]);
  return broadcast([a, b], ctx, fn);
}

export function unaryOp(op: '-' | '+' | '%', arg: Value, ctx: EvalContext): Value {
  if (op === '+') return arg;
  const a = operand(arg);
  const fn = (v: Scalar[]): Scalar => {
    const x = v[0];
    if (isError(x)) return x;
    const n = toNumber(x, ctx.coerce);
    if (isError(n)) return n;
    return op === '-' ? clean(-n) : clean(n / 100);
  };
  if (!isMatrix(a)) return fn([a]);
  return broadcast([a], ctx, fn);
}

// ------------------------------------------------------------------ references

function refSheet(node: RefNode | { sheetId?: number }, ctx: EvalContext): number {
  return node.sheetId === undefined ? ctx.sheet : node.sheetId;
}

export function refValue(node: RefNode, ctx: EvalContext): Value {
  const sheet = refSheet(node, ctx);
  if (sheet < 0) return E.REF;
  return new RefValue(ctx.src, sheet, node.r1, node.c1, node.r2, node.c2);
}

function asRef(v: Value): RefValue | FormulaError {
  if (v instanceof RefValue) return v;
  if (isError(v)) return v;
  return E.VALUE;
}

function rangeOp(l: Value, r: Value): Value {
  const a = asRef(l),
    b = asRef(r);
  if (isError(a)) return a;
  if (isError(b)) return b;
  if (a.sheet !== b.sheet) return E.VALUE;
  return new RefValue(
    a.src,
    a.sheet,
    Math.min(a.r1, b.r1),
    Math.min(a.c1, b.c1),
    Math.max(a.r2, b.r2),
    Math.max(a.c2, b.c2)
  );
}

function intersectOp(l: Value, r: Value): Value {
  const a = asRef(l),
    b = asRef(r);
  if (isError(a)) return a;
  if (isError(b)) return b;
  if (a.sheet !== b.sheet) return E.VALUE;
  const r1 = Math.max(a.r1, b.r1),
    r2 = Math.min(a.r2, b.r2),
    c1 = Math.max(a.c1, b.c1),
    c2 = Math.min(a.c2, b.c2);
  if (r1 > r2 || c1 > c2) return E.NULL;
  return new RefValue(a.src, a.sheet, r1, c1, r2, c2);
}

/** Implicit intersection of a multi-cell reference with the formula's own row/column. */
function implicitIntersection(v: Value, ctx: EvalContext): Value {
  if (v instanceof RefValue) {
    if (v.isCell) return v;
    if (v.cols === 1 && ctx.row >= v.r1 && ctx.row <= v.r2 && v.sheet === ctx.sheet)
      return new RefValue(v.src, v.sheet, ctx.row, v.c1, ctx.row, v.c1);
    if (v.rows === 1 && ctx.col >= v.c1 && ctx.col <= v.c2 && v.sheet === ctx.sheet)
      return new RefValue(v.src, v.sheet, v.r1, ctx.col, v.r1, ctx.col);
    if (
      v.sheet === ctx.sheet &&
      ctx.row >= v.r1 &&
      ctx.row <= v.r2 &&
      ctx.col >= v.c1 &&
      ctx.col <= v.c2
    )
      return new RefValue(v.src, v.sheet, ctx.row, ctx.col, ctx.row, ctx.col);
    return E.VALUE;
  }
  if (v instanceof ArrayValue) return v.data[0];
  return v;
}

// -------------------------------------------------------------------- evaluate

const FN_PREFIX = /^_xl(?:fn|ws|pm)\.(?:_xlws\.)?/i;
export function functionKey(name: string): string {
  return name.replace(FN_PREFIX, '').toUpperCase();
}

export function evaluate(
  node: Node,
  ctx: EvalContext,
  fns: FunctionTable,
  snapRoot = false
): Value {
  switch (node.t) {
    case 'num':
      return node.v;
    case 'str':
      return node.v;
    case 'bool':
      return node.v;
    case 'err':
      return node.v;
    case 'ref':
      return refValue(node, ctx);
    case 'paren':
      return evaluate(node.x, ctx, fns, snapRoot);
    case 'name': {
      if (ctx.depth > 32) return E.NAME;
      const def = ctx.lookupName(node.name, node.sheet, ctx.sheet);
      if (!def) return E.NAME;
      const nested: EvalContext = Object.create(ctx, { depth: { value: ctx.depth + 1 } });
      return evaluate(def.ast, nested, fns);
    }
    case 'un': {
      if (node.op === '@') return implicitIntersection(evaluate(node.arg, ctx, fns), ctx);
      return unaryOp(node.op, evaluate(node.arg, ctx, fns), ctx);
    }
    case 'bin': {
      const l = evaluate(node.l, ctx, fns);
      const r = evaluate(node.r, ctx, fns);
      return binaryOp(node.op, l, r, ctx, snapRoot && (node.op === '+' || node.op === '-'));
    }
    case 'arr': {
      const rows = node.rows.map((row) =>
        row.map((c) => {
          const v = operand(evaluate(c, ctx, fns));
          return isMatrix(v) ? E.VALUE : v;
        })
      );
      return ArrayValue.from2D(rows);
    }
    case 'rng':
      return rangeOp(evaluate(node.l, ctx, fns), evaluate(node.r, ctx, fns));
    case 'isect':
      return intersectOp(evaluate(node.l, ctx, fns), evaluate(node.r, ctx, fns));
    case 'union': {
      const areas: RefValue[] = [];
      for (const p of node.parts) {
        const v = evaluate(p, ctx, fns);
        if (isError(v)) return v;
        if (v instanceof MultiRef) areas.push(...v.areas);
        else if (v instanceof RefValue) areas.push(v);
        else return E.VALUE;
      }
      return new MultiRef(areas);
    }
    case 'call':
      return callFunction(node, ctx, fns);
    default:
      return E.ERROR;
  }
}

function callFunction(
  node: Extract<Node, { t: 'call' }>,
  ctx: EvalContext,
  fns: FunctionTable
): Value {
  const def = fns.get(functionKey(node.name));
  if (!def) return E.NAME;
  const n = node.args.length;
  if (n < def.min || n > def.max)
    return new FormulaError('ERROR', `Wrong number of arguments to ${node.name.toUpperCase()}`);
  try {
    if (def.lazy) {
      const lazyArgs: LazyArg[] = node.args.map((a) => ({
        omitted: a === null,
        eval: () => (a === null ? MISSING : evaluate(a, ctx, fns))
      }));
      return def.lazy(lazyArgs, ctx);
    }
    const args: Value[] = new Array(n);
    for (let i = 0; i < n; i++) {
      const a = node.args[i];
      args[i] = a === null ? MISSING : evaluate(a, ctx, fns);
    }
    return def.impl!(args, ctx);
  } catch (e) {
    if (e instanceof FormulaError) return e;
    throw e;
  }
}

/** Collect every function name used in a formula, for volatility detection. */
export function usesFunction(node: Node, test: (key: string) => boolean): boolean {
  let found = false;
  const visit = (n: Node | null) => {
    if (!n || found) return;
    switch (n.t) {
      case 'call':
        if (test(functionKey(n.name))) found = true;
        n.args.forEach(visit);
        break;
      case 'un':
        visit(n.arg);
        break;
      case 'bin':
      case 'rng':
      case 'isect':
        visit(n.l);
        visit(n.r);
        break;
      case 'arr':
        n.rows.forEach((r) => r.forEach(visit));
        break;
      case 'paren':
        visit(n.x);
        break;
      case 'union':
        n.parts.forEach(visit);
        break;
      default:
        break;
    }
  };
  visit(node);
  return found;
}
