import { E } from './errors';
import { numberToText } from './values';
import { defScalar, num, strOr, type Arg } from './fn-util';
import type { EvalContext } from './evaluator';

/** Complex numbers are carried as text (`3+4i`), as in Excel's engineering functions. */
interface Complex {
  re: number;
  im: number;
  suffix: 'i' | 'j';
}

const NUMBER = String.raw`(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?`;
const BOTH = new RegExp(String.raw`^([+-]?${NUMBER})([+-])(${NUMBER})?([ij])$`);
const IMAG = new RegExp(String.raw`^([+-]?)(${NUMBER})?([ij])$`);
const REAL = new RegExp(String.raw`^([+-]?${NUMBER})$`);

function parse(arg: Arg): Complex {
  if (typeof arg === 'number') return { re: arg, im: 0, suffix: 'i' };
  if (typeof arg === 'boolean') throw E.VALUE;
  const text = String(arg ?? '').trim();
  if (text === '') return { re: 0, im: 0, suffix: 'i' };
  let m = BOTH.exec(text);
  if (m) {
    const im = (m[3] === undefined ? 1 : Number(m[3])) * (m[2] === '-' ? -1 : 1);
    return { re: Number(m[1]), im, suffix: m[4] as 'i' | 'j' };
  }
  m = IMAG.exec(text);
  if (m) {
    const im = (m[2] === undefined ? 1 : Number(m[2])) * (m[1] === '-' ? -1 : 1);
    return { re: 0, im, suffix: m[3] as 'i' | 'j' };
  }
  m = REAL.exec(text);
  if (m) return { re: Number(m[1]), im: 0, suffix: 'i' };
  throw E.NUM;
}
function format(c: Complex): string {
  const { re, im, suffix } = c;
  const clean = (n: number) => (n === 0 ? 0 : n);
  const r = numberToText(clean(re)),
    i = clean(im);
  if (i === 0) return r;
  const imag = i === 1 ? '' : i === -1 ? '-' : numberToText(i);
  if (re === 0) return `${imag}${suffix}`;
  return `${r}${i > 0 ? '+' : ''}${imag}${suffix}`;
}
function suffixOf(...cs: Complex[]): 'i' | 'j' {
  let found: 'i' | 'j' | undefined;
  for (const c of cs) {
    if (c.im === 0 && c.re !== 0 && !found) continue;
    if (found && c.suffix !== found && c.im !== 0) throw E.VALUE;
    if (c.im !== 0) found ??= c.suffix;
  }
  return found ?? cs.find((c) => c.im !== 0)?.suffix ?? 'i';
}
const polar = (c: Complex) => ({ r: Math.hypot(c.re, c.im), t: Math.atan2(c.im, c.re) });
const rect = (r: number, t: number, suffix: 'i' | 'j'): Complex => ({
  re: r * Math.cos(t),
  im: r * Math.sin(t),
  suffix
});
const mul = (a: Complex, b: Complex): Complex => ({
  re: a.re * b.re - a.im * b.im,
  im: a.re * b.im + a.im * b.re,
  suffix: a.suffix
});
function div(a: Complex, b: Complex): Complex {
  const d = b.re * b.re + b.im * b.im;
  if (d === 0) throw E.NUM;
  return {
    re: (a.re * b.re + a.im * b.im) / d,
    im: (a.im * b.re - a.re * b.im) / d,
    suffix: a.suffix
  };
}
const out = (c: Complex) => {
  if (!Number.isFinite(c.re) || !Number.isFinite(c.im)) throw E.NUM;
  return format(c);
};

defScalar('COMPLEX', 2, 3, (a, ctx) => {
  const suffix = strOr(a[2], 'i');
  if (suffix !== 'i' && suffix !== 'j') return E.VALUE;
  return format({ re: num(a[0], ctx), im: num(a[1], ctx), suffix });
});
defScalar('IMREAL', 1, 1, (a) => parse(a[0]).re);
defScalar('IMAGINARY', 1, 1, (a) => parse(a[0]).im);
defScalar('IMABS', 1, 1, (a) => {
  const c = parse(a[0]);
  return Math.hypot(c.re, c.im);
});
defScalar('IMARGUMENT', 1, 1, (a) => {
  const c = parse(a[0]);
  if (c.re === 0 && c.im === 0) return E.DIV0;
  return Math.atan2(c.im, c.re);
});
defScalar('IMCONJUGATE', 1, 1, (a) => {
  const c = parse(a[0]);
  return out({ ...c, im: -c.im });
});

function variadic(name: string, combine: (acc: Complex, next: Complex) => Complex) {
  defScalar(name, 1, 255, (a) => {
    const cs = a.map((v) => parse(v));
    const suffix = suffixOf(...cs);
    return out({ ...cs.slice(1).reduce(combine, cs[0]), suffix });
  });
}
variadic('IMSUM', (x, y) => ({ re: x.re + y.re, im: x.im + y.im, suffix: x.suffix }));
variadic('IMPRODUCT', mul);
defScalar('IMSUB', 2, 2, (a) => {
  const x = parse(a[0]),
    y = parse(a[1]);
  const suffix = suffixOf(x, y);
  return out({ re: x.re - y.re, im: x.im - y.im, suffix });
});
defScalar('IMDIV', 2, 2, (a) => {
  const x = parse(a[0]),
    y = parse(a[1]);
  const suffix = suffixOf(x, y);
  return out({ ...div(x, y), suffix });
});

function unaryComplex(name: string, f: (c: Complex) => Complex) {
  defScalar(name, 1, 1, (a) => {
    const c = parse(a[0]);
    return out({ ...f(c), suffix: suffixOf(c) });
  });
}
const ONE: Complex = { re: 1, im: 0, suffix: 'i' };
const sinC = (c: Complex): Complex => ({
  re: Math.sin(c.re) * Math.cosh(c.im),
  im: Math.cos(c.re) * Math.sinh(c.im),
  suffix: c.suffix
});
const cosC = (c: Complex): Complex => ({
  re: Math.cos(c.re) * Math.cosh(c.im),
  im: -Math.sin(c.re) * Math.sinh(c.im),
  suffix: c.suffix
});
const sinhC = (c: Complex): Complex => ({
  re: Math.sinh(c.re) * Math.cos(c.im),
  im: Math.cosh(c.re) * Math.sin(c.im),
  suffix: c.suffix
});
const coshC = (c: Complex): Complex => ({
  re: Math.cosh(c.re) * Math.cos(c.im),
  im: Math.sinh(c.re) * Math.sin(c.im),
  suffix: c.suffix
});
unaryComplex('IMSIN', sinC);
unaryComplex('IMCOS', cosC);
unaryComplex('IMSINH', sinhC);
unaryComplex('IMCOSH', coshC);
unaryComplex('IMTAN', (c) => div(sinC(c), cosC(c)));
unaryComplex('IMCOT', (c) => div(cosC(c), sinC(c)));
unaryComplex('IMSEC', (c) => div(ONE, cosC(c)));
unaryComplex('IMCSC', (c) => div(ONE, sinC(c)));
unaryComplex('IMSECH', (c) => div(ONE, coshC(c)));
unaryComplex('IMCSCH', (c) => div(ONE, sinhC(c)));
unaryComplex('IMEXP', (c) => ({
  re: Math.exp(c.re) * Math.cos(c.im),
  im: Math.exp(c.re) * Math.sin(c.im),
  suffix: c.suffix
}));
const lnC = (c: Complex): Complex => {
  if (c.re === 0 && c.im === 0) throw E.NUM;
  const p = polar(c);
  return { re: Math.log(p.r), im: p.t, suffix: c.suffix };
};
unaryComplex('IMLN', lnC);
unaryComplex('IMLOG10', (c) => {
  const l = lnC(c);
  return { re: l.re / Math.LN10, im: l.im / Math.LN10, suffix: c.suffix };
});
unaryComplex('IMLOG2', (c) => {
  const l = lnC(c);
  return { re: l.re / Math.LN2, im: l.im / Math.LN2, suffix: c.suffix };
});
unaryComplex('IMSQRT', (c) => {
  const p = polar(c);
  return rect(Math.sqrt(p.r), p.t / 2, c.suffix);
});
defScalar('IMPOWER', 2, 2, (a, ctx: EvalContext) => {
  const c = parse(a[0]);
  const n = num(a[1], ctx);
  if (c.re === 0 && c.im === 0) return n > 0 ? '0' : E.NUM;
  const p = polar(c);
  return out(rect(Math.pow(p.r, n), p.t * n, suffixOf(c)));
});
