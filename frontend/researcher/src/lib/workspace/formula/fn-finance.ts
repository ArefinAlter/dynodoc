import { E, isError } from './errors';
import { clean, type Scalar, type Value } from './values';
import type { EvalContext } from './evaluator';
import {
  def,
  defScalar,
  int,
  num,
  numOr,
  numbersFrom,
  numbersOf,
  isMissing,
  matrixOf,
  type Arg
} from './fn-util';

function fin(r: number): Scalar {
  return Number.isFinite(r) ? clean(r) : E.NUM;
}

function fv(rate: number, nper: number, pmt: number, pv: number, type: number) {
  if (rate === 0) return -(pv + pmt * nper);
  const f = Math.pow(1 + rate, nper);
  return -(pv * f + (pmt * (1 + rate * type) * (f - 1)) / rate);
}
function pmtOf(rate: number, nper: number, pv: number, fvv: number, type: number) {
  if (rate === 0) return -(pv + fvv) / nper;
  const f = Math.pow(1 + rate, nper);
  return -((fvv + pv * f) * rate) / ((1 + rate * type) * (f - 1));
}
function ipmtOf(rate: number, per: number, nper: number, pv: number, f: number, type: number) {
  const pmt = pmtOf(rate, nper, pv, f, type);
  if (type === 1) {
    if (per === 1) return 0;
    return (fv(rate, per - 1, pmt, pv, 1) * rate) / (1 + rate);
  }
  return fv(rate, per - 1, pmt, pv, 0) * rate;
}

const a5 = (a: Arg[], ctx: EvalContext, i: number, d = 0) => numOr(a[i], ctx, d);

defScalar('FV', 3, 5, (a, ctx) =>
  fin(fv(num(a[0], ctx), num(a[1], ctx), num(a[2], ctx), a5(a, ctx, 3), a5(a, ctx, 4) ? 1 : 0))
);
defScalar('PV', 3, 5, (a, ctx) => {
  const rate = num(a[0], ctx),
    n = num(a[1], ctx),
    pmt = num(a[2], ctx),
    f = a5(a, ctx, 3),
    type = a5(a, ctx, 4) ? 1 : 0;
  if (rate === 0) return fin(-(f + pmt * n));
  const g = Math.pow(1 + rate, n);
  return fin(-(f + (pmt * (1 + rate * type) * (g - 1)) / rate) / g);
});
defScalar('PMT', 3, 5, (a, ctx) => {
  const rate = num(a[0], ctx),
    n = num(a[1], ctx);
  if (n === 0) return E.NUM;
  return fin(pmtOf(rate, n, num(a[2], ctx), a5(a, ctx, 3), a5(a, ctx, 4) ? 1 : 0));
});
defScalar('IPMT', 4, 6, (a, ctx) => {
  const rate = num(a[0], ctx),
    per = num(a[1], ctx),
    n = num(a[2], ctx);
  const pv = num(a[3], ctx),
    f = a5(a, ctx, 4),
    type = a5(a, ctx, 5) ? 1 : 0;
  if (per < 1 || per > n) return E.NUM;
  return fin(ipmtOf(rate, per, n, pv, f, type));
});
defScalar('PPMT', 4, 6, (a, ctx) => {
  const rate = num(a[0], ctx),
    per = num(a[1], ctx),
    n = num(a[2], ctx);
  const pv = num(a[3], ctx),
    f = a5(a, ctx, 4),
    type = a5(a, ctx, 5) ? 1 : 0;
  if (per < 1 || per > n) return E.NUM;
  return fin(pmtOf(rate, n, pv, f, type) - ipmtOf(rate, per, n, pv, f, type));
});
defScalar('NPER', 3, 5, (a, ctx) => {
  const rate = num(a[0], ctx),
    pmt = num(a[1], ctx),
    pv = num(a[2], ctx),
    f = a5(a, ctx, 3),
    type = a5(a, ctx, 4) ? 1 : 0;
  if (rate === 0) return pmt === 0 ? E.NUM : fin(-(pv + f) / pmt);
  const k = pmt * (1 + rate * type);
  const ratio = (k - f * rate) / (k + pv * rate);
  if (ratio <= 0) return E.NUM;
  return fin(Math.log(ratio) / Math.log(1 + rate));
});
defScalar('RATE', 3, 6, (a, ctx) => {
  const n = num(a[0], ctx),
    pmt = num(a[1], ctx),
    pv = num(a[2], ctx);
  const f = a5(a, ctx, 3),
    type = a5(a, ctx, 4) ? 1 : 0;
  let r = numOr(a[5], ctx, 0.1);
  for (let i = 0; i < 100; i++) {
    const g = Math.pow(1 + r, n);
    const y = r === 0 ? pv + pmt * n + f : pv * g + pmt * (1 + r * type) * ((g - 1) / r) + f;
    const h = 1e-7;
    const g2 = Math.pow(1 + r + h, n);
    const y2 = pv * g2 + pmt * (1 + (r + h) * type) * ((g2 - 1) / (r + h)) + f;
    const d = (y2 - y) / h;
    if (!Number.isFinite(d) || d === 0) return E.NUM;
    const next = r - y / d;
    if (Math.abs(next - r) < 1e-12) return fin(next);
    r = next;
    if (r <= -1) r = -0.999999;
  }
  return E.NUM;
});

function npv(rate: number, values: number[]) {
  return values.reduce((s, v, i) => s + v / Math.pow(1 + rate, i + 1), 0);
}
def('NPV', 2, 255, (a, ctx) => {
  const rate = num(a[0], ctx);
  if (rate === -1) return E.DIV0;
  return fin(npv(rate, numbersOf(a.slice(1), ctx)));
});
function solve(f: (r: number) => number, guess: number): number | undefined {
  let r = guess;
  for (let i = 0; i < 100; i++) {
    const y = f(r);
    const h = Math.max(1e-7, Math.abs(r) * 1e-7);
    const d = (f(r + h) - y) / h;
    if (!Number.isFinite(d) || d === 0) break;
    const next = r - y / d;
    if (!Number.isFinite(next)) break;
    if (Math.abs(next - r) < 1e-12) return next;
    r = next <= -1 ? (r - 1) / 2 : next;
  }
  // bisection fallback over a wide bracket
  let lo = -0.999999,
    hi = 1e6;
  let flo = f(lo);
  if (!Number.isFinite(flo) || flo * f(hi) > 0) return undefined;
  for (let i = 0; i < 300; i++) {
    const mid = (lo + hi) / 2;
    const fm = f(mid);
    if (fm === 0 || (hi - lo) / 2 < 1e-13) return mid;
    if (fm * flo < 0) hi = mid;
    else {
      lo = mid;
      flo = fm;
    }
  }
  return (lo + hi) / 2;
}
def('IRR', 1, 2, (a, ctx) => {
  const values = numbersFrom(a[0], ctx);
  if (!values.some((v) => v > 0) || !values.some((v) => v < 0)) return E.NUM;
  const guess = isMissing(a[1]) ? 0.1 : num(a[1], ctx);
  const r = solve((x) => values.reduce((s, v, i) => s + v / Math.pow(1 + x, i), 0), guess);
  return r === undefined ? E.NUM : fin(r);
});
def('MIRR', 3, 3, (a, ctx) => {
  const values = numbersFrom(a[0], ctx);
  const fr = num(a[1], ctx),
    rr = num(a[2], ctx);
  const n = values.length;
  const pos = values.map((v) => (v > 0 ? v : 0)),
    neg = values.map((v) => (v < 0 ? v : 0));
  if (!pos.some(Boolean) || !neg.some(Boolean)) return E.DIV0;
  const num1 = -npv(rr, pos) * Math.pow(1 + rr, n);
  const den = npv(fr, neg) * (1 + fr);
  return fin(Math.pow(num1 / den, 1 / (n - 1)) - 1);
});
function cashflows(a: Value[], ctx: EvalContext) {
  const values = matrixOf(a[0], ctx).data.map((v) => {
    if (isError(v)) throw v;
    return Number(v);
  });
  const dates = matrixOf(a[1], ctx).data.map((v) => {
    if (isError(v)) throw v;
    return Math.floor(Number(v));
  });
  if (values.length !== dates.length) throw E.NUM;
  if (dates.some((d) => d < dates[0])) throw E.NUM;
  return { values, dates };
}
def('XNPV', 3, 3, (a, ctx) => {
  const rate = num(a[0], ctx);
  const { values, dates } = cashflows(a.slice(1), ctx);
  if (rate <= -1) return E.NUM;
  return fin(
    values.reduce((s, v, i) => s + v / Math.pow(1 + rate, (dates[i] - dates[0]) / 365), 0)
  );
});
def('XIRR', 2, 3, (a, ctx) => {
  const { values, dates } = cashflows(a, ctx);
  if (!values.some((v) => v > 0) || !values.some((v) => v < 0)) return E.NUM;
  const guess = isMissing(a[2]) ? 0.1 : num(a[2], ctx);
  const r = solve(
    (x) => values.reduce((s, v, i) => s + v / Math.pow(1 + x, (dates[i] - dates[0]) / 365), 0),
    guess
  );
  return r === undefined ? E.NUM : fin(r);
});

// ------------------------------------------------------------ depreciation

defScalar('SLN', 3, 3, (a, ctx) => {
  const life = num(a[2], ctx);
  if (life === 0) return E.DIV0;
  return fin((num(a[0], ctx) - num(a[1], ctx)) / life);
});
defScalar('SYD', 4, 4, (a, ctx) => {
  const cost = num(a[0], ctx),
    salvage = num(a[1], ctx),
    life = num(a[2], ctx),
    per = num(a[3], ctx);
  if (life <= 0 || per <= 0 || per > life) return E.NUM;
  return fin(((cost - salvage) * (life - per + 1) * 2) / (life * (life + 1)));
});
defScalar('DB', 4, 5, (a, ctx) => {
  const cost = num(a[0], ctx),
    salvage = num(a[1], ctx),
    life = num(a[2], ctx),
    period = num(a[3], ctx);
  const month = numOr(a[4], ctx, 12);
  if (cost < 0 || salvage < 0 || life <= 0 || period <= 0 || month < 1 || month > 12) return E.NUM;
  if (period > life + (month < 12 ? 1 : 0)) return E.NUM;
  const rate = Math.round((1 - Math.pow(salvage / cost, 1 / life)) * 1000) / 1000;
  let total = (cost * rate * month) / 12;
  let value = total;
  if (period === 1) return fin(value);
  const last = Math.min(period, life);
  for (let p = 2; p <= last; p++) {
    value = (cost - total) * rate;
    total += value;
  }
  if (period > life) value = ((cost - total) * rate * (12 - month)) / 12;
  return fin(value);
});
defScalar('DDB', 4, 5, (a, ctx) => {
  const cost = num(a[0], ctx),
    salvage = num(a[1], ctx),
    life = num(a[2], ctx),
    period = num(a[3], ctx);
  const factor = numOr(a[4], ctx, 2);
  if (cost < 0 || salvage < 0 || life <= 0 || period <= 0 || factor <= 0 || period > life)
    return E.NUM;
  let total = 0;
  let value = 0;
  for (let p = 1; p <= Math.ceil(period); p++) {
    value = Math.min((cost - total) * (factor / life), Math.max(0, cost - salvage - total));
    if (p > period) value *= period - Math.floor(period);
    total += value;
  }
  return fin(value);
});

// ------------------------------------------------------------ rates and misc

defScalar('EFFECT', 2, 2, (a, ctx) => {
  const r = num(a[0], ctx),
    n = Math.trunc(num(a[1], ctx));
  if (r <= 0 || n < 1) return E.NUM;
  return fin(Math.pow(1 + r / n, n) - 1);
});
defScalar('NOMINAL', 2, 2, (a, ctx) => {
  const r = num(a[0], ctx),
    n = Math.trunc(num(a[1], ctx));
  if (r <= 0 || n < 1) return E.NUM;
  return fin(n * (Math.pow(1 + r, 1 / n) - 1));
});
function cumulative(principal: boolean) {
  return (a: Arg[], ctx: EvalContext): Scalar => {
    const rate = num(a[0], ctx),
      n = num(a[1], ctx),
      pv = num(a[2], ctx);
    const start = int(a[3], ctx),
      end = int(a[4], ctx),
      type = int(a[5], ctx);
    if (
      rate <= 0 ||
      n <= 0 ||
      pv <= 0 ||
      start < 1 ||
      end < start ||
      end > n ||
      (type !== 0 && type !== 1)
    )
      return E.NUM;
    let total = 0;
    for (let p = start; p <= end; p++) {
      const i = ipmtOf(rate, p, n, pv, 0, type);
      total += principal ? pmtOf(rate, n, pv, 0, type) - i : i;
    }
    return fin(total);
  };
}
defScalar('CUMIPMT', 6, 6, cumulative(false));
defScalar('CUMPRINC', 6, 6, cumulative(true));
defScalar('ISPMT', 4, 4, (a, ctx) => {
  const n = num(a[2], ctx);
  if (n === 0) return E.DIV0;
  return fin(num(a[3], ctx) * num(a[0], ctx) * (num(a[1], ctx) / n - 1));
});
defScalar('RRI', 3, 3, (a, ctx) => {
  const n = num(a[0], ctx),
    pv = num(a[1], ctx),
    f = num(a[2], ctx);
  if (n <= 0 || pv === 0) return E.NUM;
  return fin(Math.pow(f / pv, 1 / n) - 1);
});
defScalar('PDURATION', 3, 3, (a, ctx) => {
  const r = num(a[0], ctx),
    pv = num(a[1], ctx),
    f = num(a[2], ctx);
  if (r <= 0 || pv <= 0 || f <= 0) return E.NUM;
  return fin((Math.log(f) - Math.log(pv)) / Math.log(1 + r));
});
def('FVSCHEDULE', 2, 2, (a, ctx) => {
  const principal = num(a[0], ctx);
  const rates = numbersFrom(a[1], ctx);
  return fin(rates.reduce((p, r) => p * (1 + r), principal));
});
defScalar('DOLLARDE', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx);
  const f = Math.trunc(num(a[1], ctx));
  if (f < 0) return E.NUM;
  if (f === 0) return E.DIV0;
  const sign = x < 0 ? -1 : 1;
  const abs = Math.abs(x);
  const whole = Math.floor(abs);
  const digits = Math.ceil(Math.log10(f));
  return fin(sign * (whole + ((abs - whole) * 10 ** digits) / f));
});
defScalar('DOLLARFR', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx);
  const f = Math.trunc(num(a[1], ctx));
  if (f < 0) return E.NUM;
  if (f === 0) return E.DIV0;
  const sign = x < 0 ? -1 : 1;
  const abs = Math.abs(x);
  const whole = Math.floor(abs);
  const digits = Math.ceil(Math.log10(f));
  return fin(sign * (whole + ((abs - whole) * f) / 10 ** digits));
});

function billDays(a: Arg[], ctx: EvalContext) {
  const s = Math.floor(num(a[0], ctx)),
    m = Math.floor(num(a[1], ctx));
  const days = m - s;
  if (days <= 0 || days > 365) throw E.NUM;
  return days;
}
defScalar('TBILLEQ', 3, 3, (a, ctx) => {
  const days = billDays(a, ctx);
  const d = num(a[2], ctx);
  if (d <= 0) return E.NUM;
  if (days <= 182) return fin((365 * d) / (360 - d * days));
  const price = 1 - (d * days) / 360;
  const t = days / 365;
  const r = (-2 * t + 2 * Math.sqrt(t * t - (2 * t - 1) * (1 - 1 / price))) / (2 * t - 1);
  return fin(r);
});
defScalar('TBILLPRICE', 3, 3, (a, ctx) => {
  const days = billDays(a, ctx);
  const d = num(a[2], ctx);
  if (d <= 0) return E.NUM;
  const p = 100 * (1 - (d * days) / 360);
  return p <= 0 ? E.NUM : fin(p);
});
defScalar('TBILLYIELD', 3, 3, (a, ctx) => {
  const days = billDays(a, ctx);
  const pr = num(a[2], ctx);
  if (pr <= 0) return E.NUM;
  return fin(((100 - pr) / pr) * (360 / days));
});
