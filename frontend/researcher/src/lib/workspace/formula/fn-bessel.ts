import { E } from './errors';
import { clean } from './values';
import { defScalar, num } from './fn-util';

const EULER = 0.5772156649015329;

/** J_n(x) for x > 0 by Miller's backward recurrence, normalised with J0 + 2*sum J2k = 1. */
function besselJSequence(x: number, count: number): number[] {
  const start = 2 * Math.ceil((Math.max(count, x) + 20 + Math.sqrt(40 * Math.max(count, x))) / 2);
  const values = new Array<number>(start + 2).fill(0);
  values[start] = 1e-300;
  for (let k = start; k > 0; k--) {
    values[k - 1] = (2 * k * values[k]) / x - values[k + 1];
    if (Math.abs(values[k - 1]) > 1e250) {
      for (let j = k - 1; j <= start; j++) values[j] *= 1e-250;
    }
  }
  let norm = values[0];
  for (let k = 2; k <= start; k += 2) norm += 2 * values[k];
  return values.slice(0, count + 1).map((v) => v / norm);
}

export function besselJ(x: number, n: number): number {
  if (x === 0) return n === 0 ? 1 : 0;
  const sign = x < 0 && n % 2 === 1 ? -1 : 1;
  return sign * besselJSequence(Math.abs(x), n)[n];
}

export function besselY(x: number, n: number): number {
  // enough J terms for the Neumann series of Y0 and Y1
  const terms = Math.ceil(Math.max(30, x * 1.5 + 30));
  const J = besselJSequence(x, 2 * terms + 2);
  const log = Math.log(x / 2) + EULER;
  let sum0 = 0;
  let sum1 = 0;
  for (let k = 1; k <= terms; k++) {
    const sign = k % 2 === 1 ? 1 : -1;
    sum0 += (sign * J[2 * k]) / k;
    sum1 += (sign * (J[2 * k - 1] - J[2 * k + 1])) / k;
  }
  const y0 = (2 / Math.PI) * (log * J[0] + 2 * sum0);
  const y1 = (2 / Math.PI) * (-J[0] / x + log * J[1] - sum1);
  if (n === 0) return y0;
  let a = y0,
    b = y1;
  for (let k = 1; k < n; k++) {
    const next = (2 * k * b) / x - a;
    a = b;
    b = next;
  }
  return b;
}

/** I_n(x) for x > 0 by backward recurrence, normalised with I0 + 2*sum Ik = e^x. */
export function besselI(x: number, n: number): number {
  if (x === 0) return n === 0 ? 1 : 0;
  const ax = Math.abs(x);
  const start = 2 * Math.ceil((Math.max(n, ax) + 20 + Math.sqrt(40 * Math.max(n, ax))) / 2);
  const values = new Array<number>(start + 2).fill(0);
  values[start] = 1e-300;
  for (let k = start; k > 0; k--) {
    values[k - 1] = (2 * k * values[k]) / ax + values[k + 1];
    if (Math.abs(values[k - 1]) > 1e250) {
      for (let j = k - 1; j <= start; j++) values[j] *= 1e-250;
    }
  }
  let norm = values[0];
  for (let k = 1; k <= start; k++) norm += 2 * values[k];
  const result = (values[n] / norm) * Math.exp(ax);
  return x < 0 && n % 2 === 1 ? -result : result;
}

/** K_n(x) for x > 0 from K_nu(x) = integral of exp(-x cosh t) cosh(nu t) dt (trapezoid rule). */
export function besselK(x: number, n: number): number {
  const h = 0.05;
  let sum = 0.5 * Math.exp(-x);
  for (let k = 1; k < 2000; k++) {
    const t = k * h;
    const e = -x * Math.cosh(t);
    if (e < -745) break;
    sum += Math.exp(e) * Math.cosh(n * t);
  }
  return sum * h;
}

function order(v: number | undefined): number {
  const n = Math.trunc(v ?? 0);
  if (n < 0) throw E.NUM;
  return n;
}
defScalar('BESSELJ', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx);
  const n = order(num(a[1], ctx));
  return clean(besselJ(x, n));
});
defScalar('BESSELY', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx);
  const n = order(num(a[1], ctx));
  if (x <= 0) return E.NUM;
  return clean(besselY(x, n));
});
defScalar('BESSELI', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx);
  const n = order(num(a[1], ctx));
  return clean(besselI(x, n));
});
defScalar('BESSELK', 2, 2, (a, ctx) => {
  const x = num(a[0], ctx);
  const n = order(num(a[1], ctx));
  if (x <= 0) return E.NUM;
  return clean(besselK(x, n));
});
