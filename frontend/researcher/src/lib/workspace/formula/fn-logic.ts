import { E, FormulaError, isError } from './errors';
import { operand, broadcast } from './evaluator';
import {
  ArrayValue,
  MISSING,
  MultiRef,
  RefValue,
  compareScalars,
  toBoolean,
  toNumber,
  type Scalar,
  type Value
} from './values';
import { def, defLazy, defScalar, bool, walkArgs, isMissing } from './fn-util';

// ------------------------------------------------------------------ branching

/** Condition truth for lazily evaluated IF-style functions; arrays lift over the branches. */
function truth(v: Scalar): boolean | FormulaError {
  if (isError(v)) return v;
  return toBoolean(v);
}

defLazy('IF', 2, 3, (args, ctx) => {
  const cond = operand(args[0].eval());
  const pick = (c: boolean) => {
    const idx = c ? 1 : 2;
    if (idx >= args.length) return false;
    if (args[idx].omitted) return 0;
    return args[idx].eval();
  };
  if (cond instanceof ArrayValue || cond instanceof RefValue) {
    const thenV = args[1].omitted ? 0 : operand(args[1].eval());
    const elseV = args.length < 3 ? false : args[2].omitted ? 0 : operand(args[2].eval());
    return broadcast([cond, thenV, elseV], ctx, ([c, t, f]) => {
      const b = truth(c);
      if (isError(b)) return b;
      return b ? t : f;
    });
  }
  const b = truth(cond as Scalar);
  if (isError(b)) return b;
  return pick(b);
});
defLazy('IFS', 2, 254, (args) => {
  if (args.length % 2) return E.NA;
  for (let i = 0; i < args.length; i += 2) {
    const c = truth(operand(args[i].eval()) as Scalar);
    if (isError(c)) return c;
    if (c) return args[i + 1].eval();
  }
  return E.NA;
});
defLazy('IFERROR', 2, 2, (args, ctx) => {
  const v = args[0].eval();
  const o = operand(v);
  if (o instanceof ArrayValue || o instanceof RefValue) {
    const fallback = operand(args[1].eval());
    return broadcast([o, fallback], ctx, ([x, y]) => (isError(x) ? y : x));
  }
  return isError(o) ? args[1].eval() : v;
});
defLazy('IFNA', 2, 2, (args, ctx) => {
  const v = args[0].eval();
  const o = operand(v);
  if (o instanceof ArrayValue || o instanceof RefValue) {
    const fallback = operand(args[1].eval());
    return broadcast([o, fallback], ctx, ([x, y]) => (isError(x) && x.type === 'NA' ? y : x));
  }
  return isError(o) && o.type === 'NA' ? args[1].eval() : v;
});
defLazy('SWITCH', 3, 255, (args) => {
  const target = operand(args[0].eval());
  if (isError(target)) return target;
  const rest = args.length - 1;
  const hasDefault = rest % 2 === 1;
  const pairs = hasDefault ? rest - 1 : rest;
  for (let i = 0; i < pairs; i += 2) {
    const candidate = operand(args[1 + i].eval());
    if (isError(candidate)) return candidate;
    if (
      compareScalars(target as Scalar, candidate as Scalar) === 0 &&
      typeof target === typeof candidate
    )
      return args[2 + i].eval();
  }
  return hasDefault ? args[args.length - 1].eval() : E.NA;
});
defLazy('CHOOSE', 2, 255, (args, ctx) => {
  const idx = operand(args[0].eval());
  const pick = (v: Scalar): Value => {
    if (isError(v)) return v;
    const n = toNumber(v, ctx.coerce);
    if (isError(n)) return n;
    const k = Math.trunc(n);
    if (k < 1 || k >= args.length) return E.VALUE;
    return args[k].eval();
  };
  if (idx instanceof ArrayValue || idx instanceof RefValue) {
    const m =
      idx instanceof ArrayValue ? idx : ArrayValue.of(idx.rows, idx.cols, (r, c) => idx.get(r, c));
    return ArrayValue.of(m.rows, m.cols, (r, c) => {
      const v = operand(pick(m.get(r, c)));
      return v instanceof ArrayValue || v instanceof RefValue ? E.VALUE : (v as Scalar);
    });
  }
  return pick(idx as Scalar);
});

// ------------------------------------------------------------------ booleans

/** AND/OR/XOR: logical values from references/arrays (text ignored) and direct arguments. */
function logicalValues(args: Value[], ctx: Parameters<typeof walkArgs>[1]): boolean[] {
  const out: boolean[] = [];
  let sawError: FormulaError | undefined;
  walkArgs(args, ctx, (v, kind) => {
    if (isError(v)) {
      sawError ??= v;
      return;
    }
    if (typeof v === 'boolean') out.push(v);
    else if (typeof v === 'number') out.push(v !== 0);
    else if (typeof v === 'string' && kind === 'direct') {
      const t = v.trim().toUpperCase();
      if (t === 'TRUE') out.push(true);
      else if (t === 'FALSE') out.push(false);
    }
  });
  if (sawError) throw sawError;
  if (!out.length) throw E.VALUE;
  return out;
}
def('AND', 1, 255, (a, ctx) => logicalValues(a, ctx).every(Boolean));
def('OR', 1, 255, (a, ctx) => logicalValues(a, ctx).some(Boolean));
def('XOR', 1, 255, (a, ctx) => logicalValues(a, ctx).filter(Boolean).length % 2 === 1);
defScalar('NOT', 1, 1, (a) => !bool(a[0]));
def('TRUE', 0, 0, () => true);
def('FALSE', 0, 0, () => false);

// -------------------------------------------------------------- information

defScalar('ISBLANK', 1, 1, () => false, { passErrors: true });
// ISBLANK needs the reference itself to tell an empty cell from "": replace with a ref-aware version.
def('ISBLANK', 1, 1, (a) => {
  const v = a[0];
  if (v instanceof RefValue) {
    if (v.isCell) return v.src.isBlank(v.sheet, v.r1, v.c1);
    return ArrayValue.of(v.rows, v.cols, (r, c) => v.src.isBlank(v.sheet, v.r1 + r, v.c1 + c));
  }
  return v === MISSING ? true : false;
});
defScalar('ISNUMBER', 1, 1, (a) => typeof a[0] === 'number', { passErrors: true });
defScalar('ISTEXT', 1, 1, (a) => typeof a[0] === 'string', { passErrors: true });
defScalar('ISNONTEXT', 1, 1, (a) => typeof a[0] !== 'string', { passErrors: true });
defScalar('ISLOGICAL', 1, 1, (a) => typeof a[0] === 'boolean', { passErrors: true });
defScalar('ISERROR', 1, 1, (a) => isError(a[0]), { passErrors: true });
defScalar('ISERR', 1, 1, (a) => isError(a[0]) && (a[0] as FormulaError).type !== 'NA', {
  passErrors: true
});
defScalar('ISNA', 1, 1, (a) => isError(a[0]) && (a[0] as FormulaError).type === 'NA', {
  passErrors: true
});
defScalar('ISEVEN', 1, 1, (a, ctx) => Math.trunc(Math.abs(toNum(a[0], ctx))) % 2 === 0);
defScalar('ISODD', 1, 1, (a, ctx) => Math.trunc(Math.abs(toNum(a[0], ctx))) % 2 === 1);
function toNum(v: Scalar | undefined, ctx: { coerce: Parameters<typeof toNumber>[1] }): number {
  const n = toNumber(v ?? null, ctx.coerce);
  if (isError(n)) throw n;
  return n;
}
def('ISREF', 1, 1, (a) => a[0] instanceof RefValue || a[0] instanceof MultiRef);
def('ISFORMULA', 1, 1, (a) => {
  const v = a[0];
  if (!(v instanceof RefValue)) return E.VALUE;
  return v.src.isFormulaCell(v.sheet, v.r1, v.c1);
});
def('FORMULATEXT', 1, 1, (a) => {
  const v = a[0];
  if (!(v instanceof RefValue)) return E.NA;
  return v.src.formulaText(v.sheet, v.r1, v.c1) ?? E.NA;
});
def('NA', 0, 0, () => E.NA);
defScalar('N', 1, 1, (a) => {
  const v = a[0];
  if (typeof v === 'number') return v;
  if (typeof v === 'boolean') return v ? 1 : 0;
  return 0;
});
def('TYPE', 1, 1, (a) => {
  const v = a[0];
  if (v instanceof ArrayValue) return 64;
  if (v instanceof RefValue && !v.isCell) return 64;
  const s = operand(v);
  if (isError(s)) return 16;
  if (typeof s === 'number') return 1;
  if (typeof s === 'string') return 2;
  if (typeof s === 'boolean') return 4;
  return 1;
});
defScalar(
  'ERROR.TYPE',
  1,
  1,
  (a) => {
    const v = a[0];
    if (isError(v)) return v.errorTypeNumber ?? E.NA;
    return E.NA;
  },
  { passErrors: true }
);
def('SHEET', 0, 1, (a, ctx) => {
  if (isMissing(a[0])) return ctx.sheet + 1;
  const v = a[0];
  if (v instanceof RefValue) return v.sheet + 1;
  if (typeof v === 'string') {
    const id = ctx.sheetIdByName(v);
    return id === undefined ? E.NA : id + 1;
  }
  return E.VALUE;
});
def('SHEETS', 0, 1, (a, ctx) => {
  if (isMissing(a[0])) return ctx.src.sheetCount();
  return a[0] instanceof RefValue || a[0] instanceof MultiRef ? 1 : E.VALUE;
});
def('VERSION', 0, 0, () => 'Dynodoc formula engine 1');
def('HYPERLINK', 1, 2, (a) => {
  const target = operand(a[0]);
  const label = isMissing(a[1]) ? target : operand(a[1]);
  return label as Scalar;
});
