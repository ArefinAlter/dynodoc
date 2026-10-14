import { E, FormulaError } from './errors';
import { clean, type Scalar } from './values';
import { def, defScalar, int, num, numOr, boolOr, isMissing, numbersFrom } from './fn-util';

// ----------------------------------------------------------- special functions

const SQRT2 = Math.SQRT2;
const SQRT_2PI = Math.sqrt(2 * Math.PI);

/** erfc for x >= 0 via continued fraction; accurate to ~1e-15 relative. */
function erfcPositive(x: number): number {
  if (x < 2.5) return 1 - erfSeries(x);
  if (x > 27) return 0;
  // Lentz evaluation of the continued fraction K = x + (1/2)/(x + 1/(x + (3/2)/(x + ...)))
  let f = x;
  let c = x;
  let d = 0;
  const tiny = 1e-300;
  for (let n = 1; n < 400; n++) {
    const an = n / 2;
    d = x + an * d;
    d = d === 0 ? tiny : 1 / d;
    c = x + an / c;
    if (c === 0) c = tiny;
    const delta = c * d;
    f *= delta;
    if (Math.abs(delta - 1) < 1e-16) break;
  }
  return Math.exp(-x * x) / (f * Math.sqrt(Math.PI));
}
function erfSeries(x: number): number {
  // erf(x) = 2/sqrt(pi) * exp(-x^2) * sum_{n>=0} 2^n x^(2n+1) / (2n+1)!!
  let term = x;
  let sum = x;
  for (let n = 1; n < 200; n++) {
    term *= (2 * x * x) / (2 * n + 1);
    sum += term;
    if (Math.abs(term) < Math.abs(sum) * 1e-17) break;
  }
  return (2 / Math.sqrt(Math.PI)) * Math.exp(-x * x) * sum;
}
export function erf(x: number): number {
  if (x < 0) return -erf(-x);
  if (x < 2.5) return erfSeries(x);
  return 1 - erfcPositive(x);
}
export function erfc(x: number): number {
  if (x < 0) return 2 - erfc(-x);
  return erfcPositive(x);
}
export const normCdf = (z: number) => 0.5 * erfc(-z / SQRT2);
const normPdf = (z: number) => Math.exp(-0.5 * z * z) / SQRT_2PI;

const LANCZOS = [
  0.99999999999980993, 676.5203681218851, -1259.1392167224028, 771.32342877765313,
  -176.61502916214059, 12.507343278686905, -0.13857109526572012, 9.9843695780195716e-6,
  1.5056327351493116e-7
];
export function lgamma(x: number): number {
  if (x < 0.5) return Math.log(Math.PI / Math.abs(Math.sin(Math.PI * x))) - lgamma(1 - x);
  x -= 1;
  let a = LANCZOS[0];
  const t = x + 7.5;
  for (let i = 1; i < 9; i++) a += LANCZOS[i] / (x + i);
  return 0.5 * Math.log(2 * Math.PI) + (x + 0.5) * Math.log(t) - t + Math.log(a);
}
export function gamma(x: number): number {
  if (Number.isInteger(x) && x > 0 && x < 171) {
    let r = 1;
    for (let i = 2; i < x; i++) r *= i;
    return r;
  }
  if (x < 0.5) return Math.PI / (Math.sin(Math.PI * x) * gamma(1 - x));
  return Math.exp(lgamma(x));
}
/** Regularised lower incomplete gamma P(a, x). */
export function gammaP(a: number, x: number): number {
  if (x <= 0) return 0;
  if (x < a + 1) {
    let sum = 1 / a;
    let term = sum;
    for (let n = 1; n < 1000; n++) {
      term *= x / (a + n);
      sum += term;
      if (Math.abs(term) < Math.abs(sum) * 1e-16) break;
    }
    return sum * Math.exp(-x + a * Math.log(x) - lgamma(a));
  }
  // continued fraction for Q
  let b = x + 1 - a;
  let c = 1 / 1e-300;
  let d = 1 / b;
  let h = d;
  for (let i = 1; i < 1000; i++) {
    const an = -i * (i - a);
    b += 2;
    d = an * d + b;
    if (Math.abs(d) < 1e-300) d = 1e-300;
    c = b + an / c;
    if (Math.abs(c) < 1e-300) c = 1e-300;
    d = 1 / d;
    const delta = d * c;
    h *= delta;
    if (Math.abs(delta - 1) < 1e-16) break;
  }
  return 1 - Math.exp(-x + a * Math.log(x) - lgamma(a)) * h;
}
function betaCf(a: number, b: number, x: number): number {
  const tiny = 1e-300;
  let c = 1;
  let d = 1 - ((a + b) * x) / (a + 1);
  if (Math.abs(d) < tiny) d = tiny;
  d = 1 / d;
  let h = d;
  for (let m = 1; m < 1000; m++) {
    const m2 = 2 * m;
    let aa = (m * (b - m) * x) / ((a + m2 - 1) * (a + m2));
    d = 1 + aa * d;
    if (Math.abs(d) < tiny) d = tiny;
    c = 1 + aa / c;
    if (Math.abs(c) < tiny) c = tiny;
    d = 1 / d;
    h *= d * c;
    aa = (-(a + m) * (a + b + m) * x) / ((a + m2) * (a + m2 + 1));
    d = 1 + aa * d;
    if (Math.abs(d) < tiny) d = tiny;
    c = 1 + aa / c;
    if (Math.abs(c) < tiny) c = tiny;
    d = 1 / d;
    const delta = d * c;
    h *= delta;
    if (Math.abs(delta - 1) < 1e-16) break;
  }
  return h;
}
/** Regularised incomplete beta I_x(a, b). */
export function betaI(x: number, a: number, b: number): number {
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  const bt = Math.exp(
    lgamma(a + b) - lgamma(a) - lgamma(b) + a * Math.log(x) + b * Math.log(1 - x)
  );
  if (x < (a + 1) / (a + b + 2)) return (bt * betaCf(a, b, x)) / a;
  return 1 - (bt * betaCf(b, a, 1 - x)) / b;
}

/** Solve cdf(x) = p on a monotonic increasing cdf over [lo, hi] by bisection. */
function invert(cdf: (x: number) => number, p: number, lo: number, hi: number): number {
  for (let i = 0; i < 200; i++) {
    const mid = (lo + hi) / 2;
    if (cdf(mid) < p) lo = mid;
    else hi = mid;
    if (hi - lo <= Math.abs(mid) * 1e-15 + 1e-300) break;
  }
  return (lo + hi) / 2;
}
function normInv(p: number): number {
  return invert(normCdf, p, -40, 40);
}
function expandedInvert(cdf: (x: number) => number, p: number, start = 1): number {
  let hi = start;
  while (cdf(hi) < p && hi < 1e300) hi *= 2;
  return invert(cdf, p, 0, hi);
}

// ------------------------------------------------------------- registration

function fin(r: number): Scalar {
  return Number.isFinite(r) ? clean(r) : E.NUM;
}
const prob = (p: number) => {
  if (!(p > 0 && p < 1)) throw E.NUM;
  return p;
};
const flag = (v: Scalar | undefined) => boolOr(v, false);

// Normal family
defScalar('NORM.S.DIST', 2, 2, (a, ctx) => {
  const z = num(a[0], ctx);
  return fin(flag(a[1]) ? normCdf(z) : normPdf(z));
});
defScalar('NORMSDIST', 1, 2, (a, ctx) => {
  const z = num(a[0], ctx);
  return fin(isMissing(a[1]) || flag(a[1]) ? normCdf(z) : normPdf(z));
});
for (const name of ['NORM.DIST', 'NORMDIST'])
  defScalar(name, 4, 4, (a, ctx) => {
    const x = num(a[0], ctx),
      mu = num(a[1], ctx),
      sd = num(a[2], ctx);
    if (sd <= 0) return E.NUM;
    const z = (x - mu) / sd;
    return fin(flag(a[3]) ? normCdf(z) : normPdf(z) / sd);
  });
defScalar('NORM.S.INV', 1, 1, (a, ctx) => fin(normInv(prob(num(a[0], ctx)))));
defScalar('NORMSINV', 1, 1, (a, ctx) => fin(normInv(prob(num(a[0], ctx)))));
for (const name of ['NORM.INV', 'NORMINV'])
  defScalar(name, 3, 3, (a, ctx) => {
    const p = prob(num(a[0], ctx)),
      mu = num(a[1], ctx),
      sd = num(a[2], ctx);
    if (sd <= 0) return E.NUM;
    return fin(mu + sd * normInv(p));
  });
defScalar('GAUSS', 1, 1, (a, ctx) => fin(normCdf(num(a[0], ctx)) - 0.5));
defScalar('PHI', 1, 1, (a, ctx) => fin(normPdf(num(a[0], ctx))));
defScalar('ERF', 1, 2, (a, ctx) => {
  const lo = num(a[0], ctx);
  return fin(isMissing(a[1]) ? erf(lo) : erf(num(a[1], ctx)) - erf(lo));
});
defScalar('ERF.PRECISE', 1, 1, (a, ctx) => fin(erf(num(a[0], ctx))));
defScalar('ERFC', 1, 1, (a, ctx) => fin(erfc(num(a[0], ctx))));
defScalar('ERFC.PRECISE', 1, 1, (a, ctx) => fin(erfc(num(a[0], ctx))));
defScalar('FISHER', 1, 1, (a, ctx) => {
  const x = num(a[0], ctx);
  if (Math.abs(x) >= 1) return E.NUM;
  return fin(0.5 * Math.log((1 + x) / (1 - x)));
});
defScalar('FISHERINV', 1, 1, (a, ctx) => {
  const y = num(a[0], ctx);
  const e = Math.exp(2 * y);
  return fin((e - 1) / (e + 1));
});
for (const name of ['LOGNORM.DIST', 'LOGNORMDIST'])
  defScalar(name, name === 'LOGNORMDIST' ? 3 : 4, 4, (a, ctx) => {
    const x = num(a[0], ctx),
      mu = num(a[1], ctx),
      sd = num(a[2], ctx);
    if (x <= 0 || sd <= 0) return E.NUM;
    const z = (Math.log(x) - mu) / sd;
    const cum = name === 'LOGNORMDIST' ? true : flag(a[3]);
    return fin(cum ? normCdf(z) : normPdf(z) / (x * sd));
  });
for (const name of ['LOGNORM.INV', 'LOGINV'])
  defScalar(name, 3, 3, (a, ctx) => {
    const p = prob(num(a[0], ctx)),
      mu = num(a[1], ctx),
      sd = num(a[2], ctx);
    if (sd <= 0) return E.NUM;
    return fin(Math.exp(mu + sd * normInv(p)));
  });
for (const name of ['EXPON.DIST', 'EXPONDIST'])
  defScalar(name, 3, 3, (a, ctx) => {
    const x = num(a[0], ctx),
      l = num(a[1], ctx);
    if (x < 0 || l <= 0) return E.NUM;
    return fin(flag(a[2]) ? 1 - Math.exp(-l * x) : l * Math.exp(-l * x));
  });
for (const name of ['WEIBULL.DIST', 'WEIBULL'])
  defScalar(name, 4, 4, (a, ctx) => {
    const x = num(a[0], ctx),
      al = num(a[1], ctx),
      be = num(a[2], ctx);
    if (x < 0 || al <= 0 || be <= 0) return E.NUM;
    return fin(
      flag(a[3])
        ? 1 - Math.exp(-Math.pow(x / be, al))
        : (al / Math.pow(be, al)) * Math.pow(x, al - 1) * Math.exp(-Math.pow(x / be, al))
    );
  });

// Gamma family
defScalar('GAMMA', 1, 1, (a, ctx) => {
  const x = num(a[0], ctx);
  if (x <= 0 && Number.isInteger(x)) return E.NUM;
  return fin(gamma(x));
});
for (const name of ['GAMMALN', 'GAMMALN.PRECISE'])
  defScalar(name, 1, 1, (a, ctx) => {
    const x = num(a[0], ctx);
    if (x <= 0) return E.NUM;
    return fin(lgamma(x));
  });
for (const name of ['GAMMA.DIST', 'GAMMADIST'])
  defScalar(name, 4, 4, (a, ctx) => {
    const x = num(a[0], ctx),
      al = num(a[1], ctx),
      be = num(a[2], ctx);
    if (x < 0 || al <= 0 || be <= 0) return E.NUM;
    if (flag(a[3])) return fin(gammaP(al, x / be));
    return fin(Math.exp((al - 1) * Math.log(x) - x / be - lgamma(al) - al * Math.log(be)));
  });
for (const name of ['GAMMA.INV', 'GAMMAINV'])
  defScalar(name, 3, 3, (a, ctx) => {
    const p = num(a[0], ctx),
      al = num(a[1], ctx),
      be = num(a[2], ctx);
    if (p < 0 || p >= 1 || al <= 0 || be <= 0) return E.NUM;
    if (p === 0) return 0;
    return fin(be * expandedInvert((x) => gammaP(al, x), p, al));
  });
const chiCdf = (x: number, df: number) => gammaP(df / 2, x / 2);
for (const name of [
  'CHISQ.DIST',
  'CHISQ.DIST.RT',
  'CHIDIST',
  'CHISQ.INV',
  'CHISQ.INV.RT',
  'CHIINV'
]) {
  const right = /RT$|^CHIDIST$|^CHIINV$/.test(name);
  const inverse = /INV/.test(name);
  const cumArg = name === 'CHISQ.DIST';
  defScalar(name, cumArg ? 3 : 2, cumArg ? 3 : 2, (a, ctx) => {
    const x = num(a[0], ctx),
      df = Math.trunc(num(a[1], ctx));
    if (df < 1 || df > 1e10) return E.NUM;
    if (inverse) {
      if (x < 0 || x > 1) return E.NUM;
      const p = right ? 1 - x : x;
      if (p <= 0) return 0;
      if (p >= 1) return E.NUM;
      return fin(expandedInvert((v) => chiCdf(v, df), p, df));
    }
    if (x < 0) return E.NUM;
    if (right) return fin(1 - chiCdf(x, df));
    if (flag(a[2])) return fin(chiCdf(x, df));
    return fin(
      Math.exp((df / 2 - 1) * Math.log(x) - x / 2 - (df / 2) * Math.log(2) - lgamma(df / 2))
    );
  });
}

// Beta, t, F
const betaCdf = (x: number, a: number, b: number) => betaI(x, a, b);
for (const name of ['BETA.DIST', 'BETADIST'])
  defScalar(name, name === 'BETADIST' ? 3 : 4, 6, (a, ctx) => {
    const legacy = name === 'BETADIST';
    const x0 = num(a[0], ctx),
      al = num(a[1], ctx),
      be = num(a[2], ctx);
    const cum = legacy ? true : flag(a[3]);
    const lo = numOr(a[legacy ? 3 : 4], ctx, 0),
      hi = numOr(a[legacy ? 4 : 5], ctx, 1);
    if (al <= 0 || be <= 0 || x0 < lo || x0 > hi || lo === hi) return E.NUM;
    const x = (x0 - lo) / (hi - lo);
    if (cum) return fin(betaCdf(x, al, be));
    return fin(
      Math.exp(
        lgamma(al + be) -
          lgamma(al) -
          lgamma(be) +
          (al - 1) * Math.log(x) +
          (be - 1) * Math.log(1 - x)
      ) /
        (hi - lo)
    );
  });
for (const name of ['BETA.INV', 'BETAINV'])
  defScalar(name, 3, 5, (a, ctx) => {
    const p = num(a[0], ctx),
      al = num(a[1], ctx),
      be = num(a[2], ctx);
    const lo = numOr(a[3], ctx, 0),
      hi = numOr(a[4], ctx, 1);
    if (p <= 0 || p > 1 || al <= 0 || be <= 0 || lo >= hi) return E.NUM;
    return fin(lo + (hi - lo) * invert((x) => betaCdf(x, al, be), p, 0, 1));
  });
function tCdf(t: number, df: number) {
  const x = df / (df + t * t);
  const half = 0.5 * betaI(x, df / 2, 0.5);
  return t > 0 ? 1 - half : half;
}
defScalar('T.DIST', 3, 3, (a, ctx) => {
  const t = num(a[0], ctx),
    df = Math.trunc(num(a[1], ctx));
  if (df < 1) return E.NUM;
  if (flag(a[2])) return fin(tCdf(t, df));
  return fin(
    Math.exp(lgamma((df + 1) / 2) - lgamma(df / 2)) /
      Math.sqrt(df * Math.PI) /
      Math.pow(1 + (t * t) / df, (df + 1) / 2)
  );
});
defScalar('T.DIST.2T', 2, 2, (a, ctx) => {
  const t = num(a[0], ctx),
    df = Math.trunc(num(a[1], ctx));
  if (df < 1 || t < 0) return E.NUM;
  return fin(2 * (1 - tCdf(t, df)));
});
defScalar('T.DIST.RT', 2, 2, (a, ctx) => {
  const t = num(a[0], ctx),
    df = Math.trunc(num(a[1], ctx));
  if (df < 1) return E.NUM;
  return fin(1 - tCdf(t, df));
});
defScalar('TDIST', 3, 3, (a, ctx) => {
  const t = num(a[0], ctx),
    df = Math.trunc(num(a[1], ctx)),
    tails = Math.trunc(num(a[2], ctx));
  if (df < 1 || t < 0 || (tails !== 1 && tails !== 2)) return E.NUM;
  return fin(tails * (1 - tCdf(t, df)));
});
function tInv(p: number, df: number) {
  if (p === 0.5) return 0;
  const upper = invert((x) => tCdf(x, df), p > 0.5 ? p : 1 - p, 0, 1e8);
  return p > 0.5 ? upper : -upper;
}
defScalar('T.INV', 2, 2, (a, ctx) => {
  const p = prob(num(a[0], ctx)),
    df = Math.trunc(num(a[1], ctx));
  if (df < 1) return E.NUM;
  return fin(tInv(p, df));
});
for (const name of ['T.INV.2T', 'TINV'])
  defScalar(name, 2, 2, (a, ctx) => {
    const p = num(a[0], ctx),
      df = Math.trunc(num(a[1], ctx));
    if (!(p > 0 && p <= 1) || df < 1) return E.NUM;
    return fin(tInv(1 - p / 2, df));
  });
function fCdf(x: number, d1: number, d2: number) {
  if (x <= 0) return 0;
  return betaI((d1 * x) / (d1 * x + d2), d1 / 2, d2 / 2);
}
defScalar('F.DIST', 4, 4, (a, ctx) => {
  const x = num(a[0], ctx),
    d1 = Math.trunc(num(a[1], ctx)),
    d2 = Math.trunc(num(a[2], ctx));
  if (x < 0 || d1 < 1 || d2 < 1) return E.NUM;
  if (flag(a[3])) return fin(fCdf(x, d1, d2));
  const lg = lgamma((d1 + d2) / 2) - lgamma(d1 / 2) - lgamma(d2 / 2);
  return fin(
    Math.exp(
      lg +
        (d1 / 2) * Math.log(d1 / d2) +
        (d1 / 2 - 1) * Math.log(x) -
        ((d1 + d2) / 2) * Math.log(1 + (d1 * x) / d2)
    )
  );
});
for (const name of ['F.DIST.RT', 'FDIST'])
  defScalar(name, 3, 3, (a, ctx) => {
    const x = num(a[0], ctx),
      d1 = Math.trunc(num(a[1], ctx)),
      d2 = Math.trunc(num(a[2], ctx));
    if (x < 0 || d1 < 1 || d2 < 1) return E.NUM;
    return fin(1 - fCdf(x, d1, d2));
  });
defScalar('F.INV', 3, 3, (a, ctx) => {
  const p = num(a[0], ctx),
    d1 = Math.trunc(num(a[1], ctx)),
    d2 = Math.trunc(num(a[2], ctx));
  if (p < 0 || p >= 1 || d1 < 1 || d2 < 1) return E.NUM;
  if (p === 0) return 0;
  return fin(expandedInvert((x) => fCdf(x, d1, d2), p));
});
for (const name of ['F.INV.RT', 'FINV'])
  defScalar(name, 3, 3, (a, ctx) => {
    const p = num(a[0], ctx),
      d1 = Math.trunc(num(a[1], ctx)),
      d2 = Math.trunc(num(a[2], ctx));
    if (p <= 0 || p > 1 || d1 < 1 || d2 < 1) return E.NUM;
    if (p === 1) return 0;
    return fin(expandedInvert((x) => fCdf(x, d1, d2), 1 - p));
  });

// Discrete families
function binomPmf(k: number, n: number, p: number) {
  if (p === 0) return k === 0 ? 1 : 0;
  if (p === 1) return k === n ? 1 : 0;
  return Math.exp(
    lgamma(n + 1) - lgamma(k + 1) - lgamma(n - k + 1) + k * Math.log(p) + (n - k) * Math.log(1 - p)
  );
}
function binomCdf(k: number, n: number, p: number) {
  let s = 0;
  for (let i = 0; i <= k; i++) s += binomPmf(i, n, p);
  return Math.min(1, s);
}
for (const name of ['BINOM.DIST', 'BINOMDIST'])
  defScalar(name, 4, 4, (a, ctx) => {
    const k = Math.trunc(num(a[0], ctx)),
      n = Math.trunc(num(a[1], ctx)),
      p = num(a[2], ctx);
    if (k < 0 || k > n || n < 0 || p < 0 || p > 1) return E.NUM;
    return fin(flag(a[3]) ? binomCdf(k, n, p) : binomPmf(k, n, p));
  });
for (const name of ['BINOM.INV', 'CRITBINOM'])
  defScalar(name, 3, 3, (a, ctx) => {
    const n = Math.trunc(num(a[0], ctx)),
      p = num(a[1], ctx),
      alpha = num(a[2], ctx);
    if (n < 0 || p < 0 || p > 1 || alpha < 0 || alpha > 1) return E.NUM;
    let cum = 0;
    for (let k = 0; k <= n; k++) {
      cum += binomPmf(k, n, p);
      if (cum >= alpha - 1e-15) return k;
    }
    return n;
  });
defScalar('BINOM.DIST.RANGE', 3, 4, (a, ctx) => {
  const n = Math.trunc(num(a[0], ctx)),
    p = num(a[1], ctx),
    lo = Math.trunc(num(a[2], ctx));
  const hi = isMissing(a[3]) ? lo : Math.trunc(num(a[3], ctx));
  if (n < 0 || p < 0 || p > 1 || lo < 0 || hi > n || lo > hi) return E.NUM;
  let s = 0;
  for (let k = lo; k <= hi; k++) s += binomPmf(k, n, p);
  return fin(s);
});
for (const name of ['POISSON.DIST', 'POISSON'])
  defScalar(name, 3, 3, (a, ctx) => {
    const k = Math.trunc(num(a[0], ctx)),
      l = num(a[1], ctx);
    if (k < 0 || l < 0) return E.NUM;
    const pmf = (i: number) =>
      l === 0 ? (i === 0 ? 1 : 0) : Math.exp(-l + i * Math.log(l) - lgamma(i + 1));
    if (!flag(a[2])) return fin(pmf(k));
    let s = 0;
    for (let i = 0; i <= k; i++) s += pmf(i);
    return fin(Math.min(1, s));
  });
for (const name of ['NEGBINOM.DIST', 'NEGBINOMDIST'])
  defScalar(name, name === 'NEGBINOMDIST' ? 3 : 4, 4, (a, ctx) => {
    const f = Math.trunc(num(a[0], ctx)),
      s = Math.trunc(num(a[1], ctx)),
      p = num(a[2], ctx);
    if (f < 0 || s < 1 || p < 0 || p > 1) return E.NUM;
    const pmf = (i: number) =>
      Math.exp(lgamma(i + s) - lgamma(s) - lgamma(i + 1) + s * Math.log(p) + i * Math.log(1 - p));
    if (name === 'NEGBINOMDIST' || !flag(a[3])) return fin(pmf(f));
    let t = 0;
    for (let i = 0; i <= f; i++) t += pmf(i);
    return fin(Math.min(1, t));
  });
for (const name of ['HYPGEOM.DIST', 'HYPGEOMDIST'])
  defScalar(name, name === 'HYPGEOMDIST' ? 4 : 5, 5, (a, ctx) => {
    const k = Math.trunc(num(a[0], ctx)),
      n = Math.trunc(num(a[1], ctx)),
      K = Math.trunc(num(a[2], ctx)),
      N = Math.trunc(num(a[3], ctx));
    if (n < 0 || K < 0 || N < 1 || n > N || K > N || k < 0 || k > Math.min(n, K) || k < n - (N - K))
      return E.NUM;
    const c = (x: number, y: number) => lgamma(x + 1) - lgamma(y + 1) - lgamma(x - y + 1);
    const pmf = (i: number) => Math.exp(c(K, i) + c(N - K, n - i) - c(N, n));
    if (name === 'HYPGEOMDIST' || !flag(a[4])) return fin(pmf(k));
    let t = 0;
    for (let i = Math.max(0, n - (N - K)); i <= k; i++) t += pmf(i);
    return fin(Math.min(1, t));
  });

for (const name of ['CONFIDENCE.NORM', 'CONFIDENCE'])
  defScalar(name, 3, 3, (a, ctx) => {
    const alpha = num(a[0], ctx),
      sd = num(a[1], ctx),
      n = Math.trunc(num(a[2], ctx));
    if (alpha <= 0 || alpha >= 1 || sd <= 0 || n < 1) return E.NUM;
    return fin((-normInv(alpha / 2) * sd) / Math.sqrt(n));
  });
defScalar('CONFIDENCE.T', 3, 3, (a, ctx) => {
  const alpha = num(a[0], ctx),
    sd = num(a[1], ctx),
    n = Math.trunc(num(a[2], ctx));
  if (alpha <= 0 || alpha >= 1 || sd <= 0 || n < 1) return E.NUM;
  if (n === 1) return E.DIV0;
  return fin((tInv(1 - alpha / 2, n - 1) * sd) / Math.sqrt(n));
});
for (const name of ['Z.TEST', 'ZTEST'])
  def(name, 2, 3, (a, ctx) => {
    const xs = numbersFrom(a[0], ctx);
    const x = num(a[1], ctx);
    if (!xs.length) return E.NA;
    const mean = xs.reduce((s, v) => s + v, 0) / xs.length;
    let sd: number;
    if (isMissing(a[2])) {
      if (xs.length < 2) return E.DIV0;
      sd = Math.sqrt(xs.reduce((s, v) => s + (v - mean) ** 2, 0) / (xs.length - 1));
    } else sd = num(a[2], ctx);
    if (sd === 0) return E.DIV0;
    return fin(1 - normCdf((mean - x) / (sd / Math.sqrt(xs.length))));
  });

// ----------------------------------------------------- engineering / bits

const BIT_MAX = 2 ** 48;
function bitArg(v: Scalar | undefined, ctx: Parameters<typeof num>[1]): number {
  const n = num(v, ctx);
  if (n < 0 || n !== Math.floor(n) || n >= BIT_MAX) throw E.NUM;
  return n;
}
function bitOp(name: string, f: (x: bigint, y: bigint) => bigint) {
  defScalar(name, 2, 2, (a, ctx) =>
    Number(f(BigInt(bitArg(a[0], ctx)), BigInt(bitArg(a[1], ctx))))
  );
}
bitOp('BITAND', (x, y) => x & y);
bitOp('BITOR', (x, y) => x | y);
bitOp('BITXOR', (x, y) => x ^ y);
defScalar('BITLSHIFT', 2, 2, (a, ctx) => {
  const x = bitArg(a[0], ctx),
    s = int(a[1], ctx);
  if (Math.abs(s) > 53) return E.NUM;
  const r = s >= 0 ? x * 2 ** s : Math.floor(x / 2 ** -s);
  return r >= BIT_MAX ? E.NUM : r;
});
defScalar('BITRSHIFT', 2, 2, (a, ctx) => {
  const x = bitArg(a[0], ctx),
    s = int(a[1], ctx);
  if (Math.abs(s) > 53) return E.NUM;
  const r = s >= 0 ? Math.floor(x / 2 ** s) : x * 2 ** -s;
  return r >= BIT_MAX ? E.NUM : r;
});

const RADIX: Record<string, number> = { BIN: 2, OCT: 8, DEC: 10, HEX: 16 };
function fromRadix(text: string, radix: number): number | FormulaError {
  const t = text.trim().toUpperCase();
  if (t === '') return 0;
  const maxLen = radix === 10 ? 20 : 10;
  if (t.length > maxLen) return E.NUM;
  const digits = '0123456789ABCDEF'.slice(0, radix);
  for (const ch of t) if (!digits.includes(ch)) return E.NUM;
  let n = BigInt(0);
  for (const ch of t) n = n * BigInt(radix) + BigInt(digits.indexOf(ch));
  const bits = radix === 2 ? 10 : radix === 8 ? 30 : radix === 16 ? 40 : 0;
  if (bits && t.length === maxLen && n >= BigInt(2) ** BigInt(bits - 1))
    n -= BigInt(2) ** BigInt(bits);
  return Number(n);
}
function toRadix(n: number, radix: number, places: number | undefined): string | FormulaError {
  const bits = radix === 2 ? 10 : radix === 8 ? 30 : 40;
  const limit = 2 ** (bits - 1);
  if (n < -limit || n >= limit) return E.NUM;
  let s: string;
  if (n < 0) s = (BigInt(2) ** BigInt(bits) + BigInt(n)).toString(radix).toUpperCase();
  else {
    s = n.toString(radix).toUpperCase();
    if (places !== undefined) {
      if (places < s.length || places > 10 || places < 1) return E.NUM;
      s = s.padStart(places, '0');
    }
  }
  return s;
}
for (const from of ['BIN', 'OCT', 'DEC', 'HEX'])
  for (const to of ['BIN', 'OCT', 'DEC', 'HEX']) {
    if (from === to) continue;
    const name = `${from}2${to}`;
    defScalar(name, 1, to === 'DEC' ? 1 : 2, (a, ctx) => {
      const raw = from === 'DEC' ? num(a[0], ctx) : String(a[0] ?? '');
      let n: number | FormulaError;
      if (from === 'DEC') n = Math.trunc(raw as number);
      else n = fromRadix(raw as string, RADIX[from]);
      if (n instanceof FormulaError) return n;
      if (to === 'DEC') return n;
      const places = isMissing(a[1]) ? undefined : int(a[1], ctx);
      return toRadix(n, RADIX[to], places);
    });
  }
defScalar('DELTA', 1, 2, (a, ctx) => (num(a[0], ctx) === numOr(a[1], ctx, 0) ? 1 : 0));
defScalar('GESTEP', 1, 2, (a, ctx) => (num(a[0], ctx) >= numOr(a[1], ctx, 0) ? 1 : 0));
