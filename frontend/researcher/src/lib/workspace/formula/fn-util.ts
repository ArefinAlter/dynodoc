import { E, FormulaError, isError } from './errors';
import {
  broadcast,
  operand,
  materialize,
  type EvalContext,
  type FunctionDef,
  type LazyArg
} from './evaluator';
import {
  ArrayValue,
  MISSING,
  MultiRef,
  RefValue,
  toBoolean,
  toNumber,
  toText,
  type Scalar,
  type Value
} from './values';

export const FUNCTIONS = new Map<string, FunctionDef>();

export type Arg = Scalar | undefined;

/** Register an eager function; arguments arrive evaluated (references stay references). */
export function def(
  name: string,
  min: number,
  max: number,
  impl: (args: Value[], ctx: EvalContext) => Value,
  opts: { volatile?: boolean } = {}
) {
  FUNCTIONS.set(name, { min, max, impl, volatile: opts.volatile });
}

/** Register a function whose arguments are evaluated on demand (IF, IFERROR, ...). */
export function defLazy(
  name: string,
  min: number,
  max: number,
  lazy: (args: LazyArg[], ctx: EvalContext) => Value
) {
  FUNCTIONS.set(name, { min, max, lazy });
}

/**
 * Register a function of scalar arguments. Array/range arguments are applied element by
 * element (Excel's array lifting) and error arguments propagate unless `passErrors`.
 */
export function defScalar(
  name: string,
  min: number,
  max: number,
  impl: (a: Arg[], ctx: EvalContext) => Scalar,
  opts: { passErrors?: boolean; volatile?: boolean } = {}
) {
  def(
    name,
    min,
    max,
    (args, ctx) => {
      const ops: (Scalar | undefined | ArrayValue | RefValue)[] = args.map((a) =>
        a === MISSING ? undefined : (operand(a) as Scalar | ArrayValue | RefValue)
      );
      const run = (vals: Arg[]): Scalar => {
        if (!opts.passErrors) for (const v of vals) if (isError(v)) return v;
        try {
          return impl(vals, ctx);
        } catch (e) {
          if (e instanceof FormulaError) return e;
          throw e;
        }
      };
      const hasMatrix = ops.some((o) => o instanceof ArrayValue || o instanceof RefValue);
      if (!hasMatrix) return run(ops as Arg[]);
      return broadcast(
        ops.map((o) => (o === undefined ? null : o)) as (Scalar | ArrayValue | RefValue)[],
        ctx,
        (vals) => run(vals)
      );
    },
    { volatile: opts.volatile }
  );
}

// ------------------------------------------------------------ argument coercion

export function num(v: Arg | Value, ctx: EvalContext): number {
  if (v === undefined || v === MISSING) return 0;
  const s = operand(v as Value) as Scalar;
  if (s instanceof ArrayValue || s instanceof RefValue) throw E.VALUE;
  const n = typeof s === 'number' ? s : toNumber(s as Scalar, ctx.coerce);
  if (isError(n)) throw n;
  return n;
}
export function numOr(v: Arg | Value, ctx: EvalContext, fallback: number): number {
  return v === undefined || v === MISSING ? fallback : num(v, ctx);
}
/** Truncated integer argument. */
export function int(v: Arg | Value, ctx: EvalContext): number {
  return Math.trunc(num(v, ctx));
}
export function intOr(v: Arg | Value, ctx: EvalContext, fallback: number): number {
  return v === undefined || v === MISSING ? fallback : int(v, ctx);
}
export function str(v: Arg | Value): string {
  if (v === undefined || v === MISSING) return '';
  const s = operand(v as Value) as Scalar;
  if (s instanceof ArrayValue || s instanceof RefValue) throw E.VALUE;
  const t = toText(s);
  if (isError(t)) throw t;
  return t;
}
export function strOr(v: Arg | Value, fallback: string): string {
  return v === undefined || v === MISSING ? fallback : str(v);
}
export function bool(v: Arg | Value): boolean {
  if (v === undefined || v === MISSING) return false;
  const o = operand(v as Value);
  if (o instanceof ArrayValue || o instanceof RefValue) throw E.VALUE;
  const b = toBoolean(o as Scalar);
  if (isError(b)) throw b;
  return b;
}
export function boolOr(v: Arg | Value, fallback: boolean): boolean {
  return v === undefined || v === MISSING ? fallback : bool(v);
}
export function isMissing(v: Value | Arg): boolean {
  return v === undefined || v === MISSING;
}

/** Throw the error if the value is one (to short-circuit through try/catch). */
export function check<T>(v: T | FormulaError): T {
  if (isError(v)) throw v;
  return v;
}

// -------------------------------------------------------------------- aggregates

export type Kind = 'direct' | 'ref';

/** Visit every scalar in the arguments; reference/array elements are 'ref', literals 'direct'. */
export function walkArgs(
  args: Value[],
  ctx: EvalContext,
  cb: (v: Scalar, kind: Kind) => void,
  opts: { includeBlank?: boolean } = {}
): void {
  const visitRef = (ref: RefValue) => {
    if (opts.includeBlank) {
      const r = ref.rows > 100_000 || ref.cols > 5_000 ? ref.clipped() : ref;
      ctx.charge(r.rows * r.cols);
      for (let i = 0; i < r.rows; i++) for (let j = 0; j < r.cols; j++) cb(r.get(i, j), 'ref');
    } else {
      ref.each((v) => {
        ctx.charge(1);
        cb(v, 'ref');
      });
    }
  };
  for (const a of args) {
    if (a === MISSING) cb(0, 'direct');
    else if (a instanceof RefValue) visitRef(a);
    else if (a instanceof MultiRef) a.areas.forEach(visitRef);
    else if (a instanceof ArrayValue) {
      ctx.charge(a.data.length);
      for (const v of a.data) cb(v, 'ref');
    } else cb(a, 'direct');
  }
}

/**
 * Numbers for SUM-like functions: direct text/booleans coerce, referenced text/booleans
 * and blanks are skipped, and the first error wins.
 */
export function numbersOf(args: Value[], ctx: EvalContext): number[] {
  const out: number[] = [];
  walkArgs(args, ctx, (v, kind) => {
    if (isError(v)) throw v;
    if (typeof v === 'number') out.push(v);
    else if (kind === 'direct') {
      if (typeof v === 'boolean') out.push(v ? 1 : 0);
      else if (typeof v === 'string') {
        const n = toNumber(v, ctx.coerce);
        if (isError(n)) throw n;
        out.push(n);
      } else if (v === null) out.push(0);
    }
  });
  return out;
}

/** Numbers for the *A functions: referenced text counts as 0 and TRUE/FALSE as 1/0. */
export function numbersOfA(args: Value[], ctx: EvalContext): number[] {
  const out: number[] = [];
  walkArgs(args, ctx, (v, kind) => {
    if (isError(v)) throw v;
    if (typeof v === 'number') out.push(v);
    else if (typeof v === 'boolean') out.push(v ? 1 : 0);
    else if (typeof v === 'string') {
      if (kind === 'direct') {
        const n = toNumber(v, ctx.coerce);
        if (isError(n)) throw n;
        out.push(n);
      } else out.push(0);
    } else if (v === null && kind === 'direct') out.push(0);
  });
  return out;
}

/** Numbers from a single reference/array argument only (statistical pairs, percentiles). */
export function numbersFrom(arg: Value, ctx: EvalContext): number[] {
  const out: number[] = [];
  walkArgs([arg], ctx, (v, kind) => {
    if (isError(v)) throw v;
    if (typeof v === 'number') out.push(v);
    else if (kind === 'direct') {
      if (typeof v === 'boolean') out.push(v ? 1 : 0);
      else if (typeof v === 'string') {
        const n = toNumber(v, ctx.coerce);
        if (isError(n)) throw n;
        out.push(n);
      }
    }
  });
  return out;
}

/** Convert an argument to a concrete matrix (error thrown on failure). */
export function matrixOf(arg: Value, ctx: EvalContext): ArrayValue {
  const m = materialize(arg, ctx);
  if (isError(m)) throw m;
  return m;
}

/** Full-sized matrix keeping blanks as null (references are read cell by cell). */
export function scalarsOf(
  arg: Value,
  ctx: EvalContext
): { rows: number; cols: number; data: Scalar[] } {
  const m = matrixOf(arg, ctx);
  return { rows: m.rows, cols: m.cols, data: m.data };
}

export function resultArray(rows: Scalar[][]): ArrayValue {
  return ArrayValue.from2D(rows);
}

export function vector(values: Scalar[], vertical = true): ArrayValue {
  return vertical
    ? new ArrayValue(values.length, 1, values)
    : new ArrayValue(1, values.length, values);
}

export { E };
