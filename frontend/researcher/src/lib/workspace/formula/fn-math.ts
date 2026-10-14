import { E, FormulaError, isError } from './errors';
import { fixed } from './numfmt';
import { power } from './evaluator';
import { clean, round15, type Scalar } from './values';
import { def, defScalar, int, intOr, num, numOr, numbersOf, isMissing } from './fn-util';
import { ArrayValue } from './values';

function fin(r: number | FormulaError): Scalar {
  if (isError(r)) return r;
  return Number.isFinite(r) ? clean(r) : E.NUM;
}

const unary = (name: string, f: (x: number) => number | FormulaError) =>
  defScalar(name, 1, 1, (a, ctx) => fin(f(num(a[0], ctx))));

// ---------------------------------------------------------------- rounding

/** Round half away from zero at `digits` decimal places (negative = left of the point). */
export function roundDigits(x: number, digits: number): number {
  if (!Number.isFinite(x) || x === 0) return x;
  digits = Math.trunc(digits);
  const sign = x < 0 ? -1 : 1;
  const abs = Math.abs(x);
  if (digits >= 0) {
    if (digits > 15) return x;
    return sign * Number(fixed(abs, digits));
  }
  const k = 10 ** -digits;
  if (!Number.isFinite(k)) return 0;
  return sign * Number(fixed(abs / k, 0)) * k;
}
function roundAway(x: number, digits: number, up: boolean): number {
  if (!Number.isFinite(x) || x === 0) return x;
  digits = Math.trunc(digits);
  const k = 10 ** Math.abs(digits);
  const scaled = digits >= 0 ? Math.abs(x) * k : Math.abs(x) / k;
  const r = round15(scaled);
  const n = up ? Math.ceil(r) : Math.floor(r);
  const out = digits >= 0 ? n / k : n * k;
  return x < 0 ? -out : out;
}

defScalar('ROUND', 2, 2, (a, ctx) => fin(roundDigits(num(a[0], ctx), int(a[1], ctx))));
defScalar('ROUNDUP', 2, 2, (a, ctx) => fin(roundAway(num(a[0], ctx), int(a[1], ctx), true)));
defScalar('ROUNDDOWN', 2, 2, (a, ctx) => fin(roundAway(num(a[0], ctx), int(a[1], ctx), false)));
defScalar('TRUNC', 1, 2, (a, ctx) => fin(roundAway(num(a[0], ctx), intOr(a[1], ctx, 0), false)));
unary('INT', (x) => Math.floor(round15(x)));
defScalar('MROUND', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx),
    m = num(a[1], ctx);
  if (m === 0) return 0;
  if (x * m < 0) return E.NUM;
  const r = Math.round(Math.abs(round15(x / m))) * Math.abs(m);
  return fin(x < 0 ? -r : r);
});
defScalar('CEILING', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx),
    s = num(a[1], ctx);
  if (s === 0) return 0;
  if (x > 0 && s < 0) return E.NUM;
  const q = round15(x / s);
  return fin(Math.ceil(q) * s);
});
defScalar('FLOOR', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx),
    s = num(a[1], ctx);
  if (s === 0) return x === 0 ? 0 : E.DIV0;
  if (x > 0 && s < 0) return E.NUM;
  const q = round15(x / s);
  return fin(Math.floor(q) * s);
});
defScalar('CEILING.MATH', 1, 3, (a, ctx) => {
  const x = num(a[0], ctx);
  const s = Math.abs(numOr(a[1], ctx, 1));
  const mode = numOr(a[2], ctx, 0);
  if (s === 0) return 0;
  const q = round15(x / s);
  if (x < 0 && mode !== 0) return fin(-Math.ceil(-q) * s);
  return fin(Math.ceil(q) * s);
});
defScalar('FLOOR.MATH', 1, 3, (a, ctx) => {
  const x = num(a[0], ctx);
  const s = Math.abs(numOr(a[1], ctx, 1));
  const mode = numOr(a[2], ctx, 0);
  if (s === 0) return 0;
  const q = round15(x / s);
  if (x < 0 && mode !== 0) return fin(-Math.floor(-q) * s);
  return fin(Math.floor(q) * s);
});
for (const name of ['CEILING.PRECISE', 'ISO.CEILING'])
  defScalar(name, 1, 2, (a, ctx) => {
    const s = Math.abs(numOr(a[1], ctx, 1));
    if (s === 0) return 0;
    return fin(Math.ceil(round15(num(a[0], ctx) / s)) * s);
  });
defScalar('FLOOR.PRECISE', 1, 2, (a, ctx) => {
  const s = Math.abs(numOr(a[1], ctx, 1));
  if (s === 0) return 0;
  return fin(Math.floor(round15(num(a[0], ctx) / s)) * s);
});
unary('EVEN', (x) => {
  const r = Math.ceil(Math.abs(round15(x)) / 2) * 2;
  return x < 0 ? -r : r;
});
unary('ODD', (x) => {
  const a = Math.abs(round15(x));
  let r = Math.ceil(a);
  if (r % 2 === 0) r += 1;
  return x < 0 ? -r : r;
});

// ------------------------------------------------------------- basic math

unary('ABS', Math.abs);
unary('SIGN', (x) => Math.sign(x));
unary('SQRT', (x) => (x < 0 ? E.NUM : Math.sqrt(x)));
unary('SQRTPI', (x) => (x < 0 ? E.NUM : Math.sqrt(x * Math.PI)));
unary('EXP', Math.exp);
unary('LN', (x) => (x <= 0 ? E.NUM : Math.log(x)));
unary('LOG10', (x) => (x <= 0 ? E.NUM : Math.log10(x)));
defScalar('LOG', 1, 2, (a, ctx) => {
  const x = num(a[0], ctx);
  const b = numOr(a[1], ctx, 10);
  if (x <= 0 || b <= 0) return E.NUM;
  if (b === 1) return E.DIV0;
  return fin(Math.log(x) / Math.log(b));
});
defScalar('POWER', 2, 2, (a, ctx) => fin(power(num(a[0], ctx), num(a[1], ctx))));
defScalar('MOD', 2, 2, (a, ctx) => {
  const n = num(a[0], ctx),
    d = num(a[1], ctx);
  if (d === 0) return E.DIV0;
  const r = n - d * Math.floor(n / d);
  return fin(Math.abs(r) < Math.abs(d) * 1e-15 ? 0 : r);
});
defScalar('QUOTIENT', 2, 2, (a, ctx) => {
  const d = num(a[1], ctx);
  if (d === 0) return E.DIV0;
  return fin(Math.trunc(round15(num(a[0], ctx) / d)));
});
def('PI', 0, 0, () => Math.PI);
unary('DEGREES', (x) => (x * 180) / Math.PI);
unary('RADIANS', (x) => (x * Math.PI) / 180);
unary('SIN', Math.sin);
unary('COS', Math.cos);
unary('TAN', Math.tan);
unary('ASIN', (x) => (Math.abs(x) > 1 ? E.NUM : Math.asin(x)));
unary('ACOS', (x) => (Math.abs(x) > 1 ? E.NUM : Math.acos(x)));
unary('ATAN', Math.atan);
defScalar('ATAN2', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx),
    y = num(a[1], ctx);
  if (x === 0 && y === 0) return E.DIV0;
  return fin(Math.atan2(y, x));
});
unary('SINH', Math.sinh);
unary('COSH', Math.cosh);
unary('TANH', Math.tanh);
unary('ASINH', Math.asinh);
unary('ACOSH', (x) => (x < 1 ? E.NUM : Math.acosh(x)));
unary('ATANH', (x) => (Math.abs(x) >= 1 ? E.NUM : Math.atanh(x)));
unary('COT', (x) => (x === 0 ? E.DIV0 : 1 / Math.tan(x)));
unary('SEC', (x) => 1 / Math.cos(x));
unary('CSC', (x) => (x === 0 ? E.DIV0 : 1 / Math.sin(x)));
unary('ACOT', (x) => Math.PI / 2 - Math.atan(x));
unary('COTH', (x) => (x === 0 ? E.DIV0 : 1 / Math.tanh(x)));
unary('SECH', (x) => 1 / Math.cosh(x));
unary('CSCH', (x) => (x === 0 ? E.DIV0 : 1 / Math.sinh(x)));
unary('ACOTH', (x) => (Math.abs(x) <= 1 ? E.NUM : 0.5 * Math.log((x + 1) / (x - 1))));

// ----------------------------------------------------------- number theory

function factorial(n: number): number {
  let r = 1;
  for (let i = 2; i <= n; i++) r *= i;
  return r;
}
unary('FACT', (x) => {
  const n = Math.trunc(x);
  if (n < 0 || n > 170) return E.NUM;
  return factorial(n);
});
unary('FACTDOUBLE', (x) => {
  const n = Math.trunc(x);
  if (n < -1 || n > 300) return E.NUM;
  let r = 1;
  for (let i = n; i > 1; i -= 2) r *= i;
  return r;
});
function combin(n: number, k: number) {
  if (k > n - k) k = n - k;
  let r = 1;
  for (let i = 1; i <= k; i++) r = (r * (n - k + i)) / i;
  return Math.round(r);
}
defScalar('COMBIN', 2, 2, (a, ctx) => {
  const n = int(a[0], ctx),
    k = int(a[1], ctx);
  if (n < 0 || k < 0 || k > n) return E.NUM;
  return fin(combin(n, k));
});
defScalar('COMBINA', 2, 2, (a, ctx) => {
  const n = int(a[0], ctx),
    k = int(a[1], ctx);
  if (n < 0 || k < 0 || (n === 0 && k > 0)) return E.NUM;
  if (k === 0) return 1;
  return fin(combin(n + k - 1, k));
});
defScalar('PERMUT', 2, 2, (a, ctx) => {
  const n = int(a[0], ctx),
    k = int(a[1], ctx);
  if (n < 0 || k < 0 || k > n) return E.NUM;
  let r = 1;
  for (let i = 0; i < k; i++) r *= n - i;
  return fin(r);
});
function gcd2(a: number, b: number) {
  a = Math.abs(a);
  b = Math.abs(b);
  while (b) [a, b] = [b, a % b];
  return a;
}
def('GCD', 1, 255, (args, ctx) => {
  const nums = numbersOf(args, ctx).map(Math.trunc);
  if (nums.some((n) => n < 0)) return E.NUM;
  return nums.reduce(gcd2, 0);
});
def('LCM', 1, 255, (args, ctx) => {
  const nums = numbersOf(args, ctx).map(Math.trunc);
  if (nums.some((n) => n < 0)) return E.NUM;
  if (nums.includes(0)) return 0;
  return fin(nums.reduce((l, n) => (l * n) / gcd2(l, n), 1));
});
def('MULTINOMIAL', 1, 255, (args, ctx) => {
  const nums = numbersOf(args, ctx).map(Math.trunc);
  if (nums.some((n) => n < 0)) return E.NUM;
  const total = nums.reduce((a, b) => a + b, 0);
  if (total > 170) return E.NUM;
  return fin(Math.round(factorial(total) / nums.reduce((p, n) => p * factorial(n), 1)));
});
const ROMAN_TABLE: [number, string][] = [
  [1000, 'M'],
  [900, 'CM'],
  [500, 'D'],
  [400, 'CD'],
  [100, 'C'],
  [90, 'XC'],
  [50, 'L'],
  [40, 'XL'],
  [10, 'X'],
  [9, 'IX'],
  [5, 'V'],
  [4, 'IV'],
  [1, 'I']
];
defScalar('ROMAN', 1, 2, (a, ctx) => {
  let n = int(a[0], ctx);
  if (n < 0 || n > 3999) return E.VALUE;
  let out = '';
  for (const [v, s] of ROMAN_TABLE)
    while (n >= v) {
      out += s;
      n -= v;
    }
  return out;
});
defScalar('ARABIC', 1, 1, (a) => {
  const t = String(a[0] ?? '')
    .trim()
    .toUpperCase();
  if (t === '') return 0;
  const neg = t.startsWith('-');
  const s = neg ? t.slice(1) : t;
  if (!/^[MDCLXVI]+$/.test(s)) return E.VALUE;
  const val: Record<string, number> = { I: 1, V: 5, X: 10, L: 50, C: 100, D: 500, M: 1000 };
  let total = 0;
  for (let i = 0; i < s.length; i++) {
    const v = val[s[i]];
    const next = val[s[i + 1]] ?? 0;
    total += v < next ? -v : v;
  }
  return neg ? -total : total;
});
defScalar('BASE', 2, 3, (a, ctx) => {
  const n = int(a[0], ctx),
    radix = int(a[1], ctx),
    min = intOr(a[2], ctx, 0);
  if (n < 0 || radix < 2 || radix > 36 || min < 0 || min > 255) return E.NUM;
  return n.toString(radix).toUpperCase().padStart(min, '0');
});
defScalar('DECIMAL', 2, 2, (a, ctx) => {
  const text = String(a[0] ?? '').trim();
  const radix = int(a[1], ctx);
  if (radix < 2 || radix > 36) return E.NUM;
  if (text === '') return 0;
  const digits = '0123456789abcdefghijklmnopqrstuvwxyz'.slice(0, radix);
  let n = 0;
  for (const ch of text.toLowerCase()) {
    const d = digits.indexOf(ch);
    if (d < 0) return E.NUM;
    n = n * radix + d;
  }
  return fin(n);
});

// ---------------------------------------------------------------- random

def('RAND', 0, 0, (_a, ctx) => ctx.random(), { volatile: true });
def(
  'RANDBETWEEN',
  2,
  2,
  (a, ctx) => {
    const lo = Math.ceil(num(a[0], ctx)),
      hi = Math.floor(num(a[1], ctx));
    if (lo > hi) return E.NUM;
    return lo + Math.floor(ctx.random() * (hi - lo + 1));
  },
  { volatile: true }
);
def(
  'RANDARRAY',
  0,
  5,
  (a, ctx) => {
    const rows = intOr(a[0], ctx, 1),
      cols = intOr(a[1], ctx, 1);
    const lo = numOr(a[2], ctx, 0),
      hi = numOr(a[3], ctx, 1);
    const whole = !isMissing(a[4]) && num(a[4], ctx) !== 0;
    if (rows < 1 || cols < 1 || rows * cols > 1_000_000 || lo > hi) return E.VALUE;
    return ArrayValue.of(rows, cols, () =>
      whole
        ? Math.ceil(lo) + Math.floor(ctx.random() * (Math.floor(hi) - Math.ceil(lo) + 1))
        : lo + ctx.random() * (hi - lo)
    );
  },
  { volatile: true }
);

// ------------------------------------------------------------ array shaping

def('SEQUENCE', 1, 4, (a, ctx) => {
  const rows = Math.trunc(num(a[0], ctx));
  const cols = Math.trunc(numOr(a[1], ctx, 1));
  const start = numOr(a[2], ctx, 1),
    step = numOr(a[3], ctx, 1);
  if (rows === 0 || cols === 0) return E.CALC;
  if (rows < 0 || cols < 0 || rows * cols > 1_000_000) return E.VALUE;
  return ArrayValue.of(rows, cols, (r, c) => start + (r * cols + c) * step);
});

// ----------------------------------------------------------- series sums

def('SERIESSUM', 4, 4, (a, ctx) => {
  const x = num(a[0], ctx),
    n = num(a[1], ctx),
    m = num(a[2], ctx);
  const coeffs = numbersOf([a[3]], ctx);
  let sum = 0;
  coeffs.forEach((c, i) => {
    sum += c * Math.pow(x, n + i * m);
  });
  return fin(sum);
});
