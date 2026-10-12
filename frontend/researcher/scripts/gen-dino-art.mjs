#!/usr/bin/env node
// Generates the pencil + halftone dinosaur illustrations used by the marketing
// site as themable inline-SVG Svelte components (src/lib/art/*.svelte).
//
//   pnpm gen:art                 # regenerate all eight plates
//   node scripts/gen-dino-art.mjs rex egg   # only some
//
// Pipeline: hand-authored silhouettes (centripetal Catmull-Rom control points)
// -> flattened polygons -> raster masks + distance fields -> simple light model
// -> 45 degree halftone dots, diagonal pencil hatching and wobbly two-pass
// pencil contours. Everything is deterministic (seeded PRNG), uses only
// currentColor + var(--art-accent), no filters, no <image>, no external refs.

import { mkdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..');
const OUT = join(ROOT, 'src', 'lib', 'art');

// ---------------------------------------------------------------- utilities

const clamp = (v, a, b) => (v < a ? a : v > b ? b : v);
const smooth = (a, b, v) => {
  const t = clamp((v - a) / (b - a), 0, 1);
  return t * t * (3 - 2 * t);
};

function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function noise1(seed) {
  const r = rng(seed);
  const tbl = Array.from({ length: 512 }, () => r() * 2 - 1);
  return (t) => {
    const i = Math.floor(t);
    const f = t - i;
    const u = f * f * (3 - 2 * f);
    const a = tbl[((i % 512) + 512) % 512];
    const b = tbl[(((i + 1) % 512) + 512) % 512];
    return a + (b - a) * u;
  };
}

function noise2(seed) {
  const r = rng(seed);
  const S = 64;
  const tbl = Float32Array.from({ length: S * S }, () => r() * 2 - 1);
  const at = (x, y) => tbl[(((y % S) + S) % S) * S + (((x % S) + S) % S)];
  return (x, y) => {
    const xi = Math.floor(x);
    const yi = Math.floor(y);
    const fx = x - xi;
    const fy = y - yi;
    const ux = fx * fx * (3 - 2 * fx);
    const uy = fy * fy * (3 - 2 * fy);
    const a = at(xi, yi) + (at(xi + 1, yi) - at(xi, yi)) * ux;
    const b = at(xi, yi + 1) + (at(xi + 1, yi + 1) - at(xi, yi + 1)) * ux;
    return a + (b - a) * uy;
  };
}

// Light comes from the upper left, towards the viewer.
const LIGHT = (() => {
  const v = [-0.55, -0.62, 0.56];
  const m = Math.hypot(...v);
  return v.map((a) => a / m);
})();
const LXY = (() => {
  const m = Math.hypot(LIGHT[0], LIGHT[1]);
  return [LIGHT[0] / m, LIGHT[1] / m];
})();

// ------------------------------------------------------------ path helpers

function numStr(n10) {
  let s = (n10 / 10).toFixed(1);
  s = s.replace(/\.0$/, '').replace(/^(-?)0\./, '$1.');
  if (s === '-0') s = '0';
  return s;
}

/** Items are command letters or integers in tenths of a unit. */
function pathStr(items) {
  let out = '';
  let prev = null;
  for (const it of items) {
    if (typeof it === 'string') {
      out += it;
      prev = null;
    } else {
      const t = numStr(it);
      if (prev !== null && t[0] !== '-' && !(t[0] === '.' && prev.includes('.'))) out += ' ';
      out += t;
      prev = t;
    }
  }
  return out;
}

// ---------------------------------------------------------- spline / raster

function crPoint(p0, p1, p2, p3, u) {
  const k = (a, b) => Math.max(Math.hypot(b[0] - a[0], b[1] - a[1]), 1e-3) ** 0.5;
  const t0 = 0;
  const t1 = t0 + k(p0, p1);
  const t2 = t1 + k(p1, p2);
  const t3 = t2 + k(p2, p3);
  const t = t1 + (t2 - t1) * u;
  const mix = (a, b, wa, wb) => [a[0] * wa + b[0] * wb, a[1] * wa + b[1] * wb];
  const A1 = mix(p0, p1, (t1 - t) / (t1 - t0), (t - t0) / (t1 - t0));
  const A2 = mix(p1, p2, (t2 - t) / (t2 - t1), (t - t1) / (t2 - t1));
  const A3 = mix(p2, p3, (t3 - t) / (t3 - t2), (t - t2) / (t3 - t2));
  const B1 = mix(A1, A2, (t2 - t) / (t2 - t0), (t - t0) / (t2 - t0));
  const B2 = mix(A2, A3, (t3 - t) / (t3 - t1), (t - t1) / (t3 - t1));
  return mix(B1, B2, (t2 - t) / (t2 - t1), (t - t1) / (t2 - t1));
}

/** Flatten a control polygon ([x,y] or [x,y,'c'] for a hard corner). */
function flatten(pts, closed, step = 1.2) {
  const n = pts.length;
  const out = [];
  const idx = [];
  const segs = closed ? n : n - 1;
  const P = (i) => pts[closed ? ((i % n) + n) % n : clamp(i, 0, n - 1)];
  for (let i = 0; i < segs; i++) {
    const p1 = P(i);
    const p2 = P(i + 1);
    let p0 = P(i - 1);
    let p3 = P(i + 2);
    if (p1[2] === 'c') p0 = p1;
    if (p2[2] === 'c') p3 = p2;
    const chord = Math.hypot(p2[0] - p1[0], p2[1] - p1[1]);
    const m = Math.max(2, Math.ceil(chord / step));
    idx.push(out.length);
    for (let k = 0; k < m; k++) out.push(crPoint(p0, p1, p2, p3, k / m));
  }
  if (!closed) {
    idx.push(out.length);
    out.push([pts[n - 1][0], pts[n - 1][1]]);
  }
  return { pts: out, idx };
}

function rasterPoly(poly, W, H) {
  const mask = new Uint8Array(W * H);
  const n = poly.length;
  let ymin = Infinity;
  let ymax = -Infinity;
  for (const p of poly) {
    if (p[1] < ymin) ymin = p[1];
    if (p[1] > ymax) ymax = p[1];
  }
  const y0 = Math.max(0, Math.floor(ymin - 1));
  const y1 = Math.min(H - 1, Math.ceil(ymax + 1));
  const xs = [];
  for (let y = y0; y <= y1; y++) {
    const yc = y + 0.5;
    xs.length = 0;
    for (let i = 0; i < n; i++) {
      const a = poly[i];
      const b = poly[(i + 1) % n];
      if ((a[1] <= yc && b[1] > yc) || (b[1] <= yc && a[1] > yc)) {
        xs.push(a[0] + ((yc - a[1]) / (b[1] - a[1])) * (b[0] - a[0]));
      }
    }
    xs.sort((p, q) => p - q);
    for (let k = 0; k + 1 < xs.length; k += 2) {
      const xa = Math.max(0, Math.ceil(xs[k] - 0.5));
      const xb = Math.min(W - 1, Math.floor(xs[k + 1] - 0.5));
      for (let x = xa; x <= xb; x++) mask[y * W + x] = 1;
    }
  }
  return mask;
}

/** Euclidean distance (cells) from each inside cell to the nearest outside cell. */
function edtInside(mask, W, H) {
  const INF = 1e12;
  const g = new Float32Array(W * H);
  for (let i = 0; i < g.length; i++) g[i] = mask[i] ? INF : 0;
  const m = Math.max(W, H);
  const f = new Float64Array(m);
  const d = new Float64Array(m);
  const v = new Int32Array(m);
  const z = new Float64Array(m + 1);
  const pass = (n, get, set) => {
    for (let i = 0; i < n; i++) f[i] = get(i);
    let k = 0;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;
    for (let q = 1; q < n; q++) {
      let s;
      for (;;) {
        const r = v[k];
        s = (f[q] + q * q - (f[r] + r * r)) / (2 * q - 2 * r);
        if (s <= z[k] && k > 0) k--;
        else break;
      }
      k++;
      v[k] = q;
      z[k] = s;
      z[k + 1] = INF;
    }
    k = 0;
    for (let q = 0; q < n; q++) {
      while (z[k + 1] < q) k++;
      const r = v[k];
      d[q] = (q - r) * (q - r) + f[r];
    }
    for (let i = 0; i < n; i++) set(i, d[i]);
  };
  for (let x = 0; x < W; x++)
    pass(
      H,
      (y) => g[y * W + x],
      (y, val) => {
        g[y * W + x] = val;
      }
    );
  for (let y = 0; y < H; y++)
    pass(
      W,
      (x) => g[y * W + x],
      (x, val) => {
        g[y * W + x] = val;
      }
    );
  for (let i = 0; i < g.length; i++) g[i] = mask[i] ? Math.max(0.5, Math.sqrt(g[i]) - 0.5) : 0;
  return g;
}

function boxBlur(src, W, H, r) {
  const tmp = new Float32Array(src.length);
  const out = new Float32Array(src.length);
  const k = 2 * r + 1;
  for (let y = 0; y < H; y++) {
    let acc = 0;
    for (let x = -r; x <= r; x++) acc += src[y * W + clamp(x, 0, W - 1)];
    for (let x = 0; x < W; x++) {
      tmp[y * W + x] = acc / k;
      acc += src[y * W + clamp(x + r + 1, 0, W - 1)] - src[y * W + clamp(x - r, 0, W - 1)];
    }
  }
  for (let x = 0; x < W; x++) {
    let acc = 0;
    for (let y = -r; y <= r; y++) acc += tmp[clamp(y, 0, H - 1) * W + x];
    for (let y = 0; y < H; y++) {
      out[y * W + x] = acc / k;
      acc += tmp[clamp(y + r + 1, 0, H - 1) * W + x] - tmp[clamp(y - r, 0, H - 1) * W + x];
    }
  }
  return out;
}

function maxFilter(src, W, H, r) {
  const tmp = new Float32Array(src.length);
  const out = new Float32Array(src.length);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      let m = 0;
      const a = Math.max(0, x - r);
      const b = Math.min(W - 1, x + r);
      for (let i = a; i <= b; i++) if (src[y * W + i] > m) m = src[y * W + i];
      tmp[y * W + x] = m;
    }
  }
  for (let x = 0; x < W; x++) {
    for (let y = 0; y < H; y++) {
      let m = 0;
      const a = Math.max(0, y - r);
      const b = Math.min(H - 1, y + r);
      for (let i = a; i <= b; i++) if (tmp[i * W + x] > m) m = tmp[i * W + x];
      out[y * W + x] = m;
    }
  }
  return out;
}

// -------------------------------------------------------------- geometry

const rot = (pts, deg, cx = 0, cy = 0) => {
  const a = (deg * Math.PI) / 180;
  const c = Math.cos(a);
  const s = Math.sin(a);
  return pts.map(([x, y, f]) => {
    const dx = x - cx;
    const dy = y - cy;
    const p = [cx + dx * c - dy * s, cy + dx * s + dy * c];
    if (f) p.push(f);
    return p;
  });
};
const mv = (pts, dx, dy) => pts.map(([x, y, f]) => (f ? [x + dx, y + dy, f] : [x + dx, y + dy]));
const scl = (pts, k, cx = 0, cy = 0, ky = k) =>
  pts.map(([x, y, f]) => {
    const p = [cx + (x - cx) * k, cy + (y - cy) * ky];
    if (f) p.push(f);
    return p;
  });
const flipX = (pts, cx) => pts.map(([x, y, f]) => (f ? [2 * cx - x, y, f] : [2 * cx - x, y]));
const ellipse = (cx, cy, rx, ry, n = 10, deg = 0) =>
  rot(
    Array.from({ length: n }, (_, i) => {
      const a = (i / n) * Math.PI * 2;
      return [cx + Math.cos(a) * rx, cy + Math.sin(a) * ry];
    }),
    deg,
    cx,
    cy
  );

/** A tilted document page with a folded corner. Returns shape + strokes. */
function pagePieces(cx, cy, w, h, deg, fold = 0.2, lines = 4) {
  const hw = w / 2;
  const hh = h / 2;
  const f = Math.min(w, h) * fold;
  const body = [
    [-hw, -hh, 'c'],
    [hw - f, -hh, 'c'],
    [hw, -hh + f, 'c'],
    [hw, hh, 'c'],
    [-hw, hh, 'c']
  ];
  const tx = (pts) => mv(rot(pts, deg), cx, cy);
  const foldLine = tx([
    [hw - f, -hh, 'c'],
    [hw - f, -hh + f, 'c'],
    [hw, -hh + f, 'c']
  ]);
  const text = [];
  for (let i = 0; i < lines; i++) {
    const y = -hh + h * 0.3 + (i * (h * 0.58)) / Math.max(1, lines - 1);
    const len = i === lines - 1 ? 0.45 : i === 0 ? 0.7 : 0.8;
    text.push(
      tx([
        [-hw * 0.72, y],
        [-hw * 0.72 + w * 0.72 * len * 0.5, y + 0.4],
        [-hw * 0.72 + w * 0.72 * len * 1.0, y]
      ])
    );
  }
  return { body: tx(body), foldLine, text };
}

/** Place local coordinates: rotate by deg about origin, scale, translate. */
const place = (pts, x, y, deg = 0, k = 1) => mv(rot(k === 1 ? pts : scl(pts, k), deg), x, y);

/**
 * A variable-width tube around a centre-line spline. Returns control points of
 * a closed outline (use as a primitive). widths[i] belongs to center[i].
 */
function tube(center, widths, { step = 7, tip0 = false, tip1 = false } = {}) {
  const fl = flatten(center, false, 1.2);
  const pl = fl.pts;
  const s = [0];
  for (let i = 1; i < pl.length; i++)
    s.push(s[i - 1] + Math.hypot(pl[i][0] - pl[i - 1][0], pl[i][1] - pl[i - 1][1]));
  const total = s[s.length - 1];
  const ks = fl.idx.map((i) => s[Math.min(i, s.length - 1)]);
  const wAt = (u) => {
    if (u <= ks[0]) return widths[0];
    for (let i = 1; i < ks.length; i++) {
      if (u <= ks[i]) {
        const t = (u - ks[i - 1]) / Math.max(1e-6, ks[i] - ks[i - 1]);
        const e = t * t * (3 - 2 * t);
        return widths[i - 1] + (widths[i] - widths[i - 1]) * (0.5 * t + 0.5 * e);
      }
    }
    return widths[widths.length - 1];
  };
  const at = (u) => {
    let j = 1;
    while (j < s.length - 1 && s[j] < u) j++;
    const t = (u - s[j - 1]) / Math.max(1e-6, s[j] - s[j - 1]);
    const a = pl[j - 1];
    const b = pl[j];
    const L = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
    return [
      a[0] + (b[0] - a[0]) * t,
      a[1] + (b[1] - a[1]) * t,
      (b[0] - a[0]) / L,
      (b[1] - a[1]) / L
    ];
  };
  const n = Math.max(3, Math.round(total / step));
  const left = [];
  const right = [];
  for (let i = 0; i <= n; i++) {
    const u = (i / n) * total;
    const [x, y, tx, ty] = at(u);
    const w = wAt(u) / 2;
    left.push([x - ty * w, y + tx * w]);
    right.push([x + ty * w, y - tx * w]);
  }
  const cap = (i, dir, w, sharp) => {
    const [x, y, tx, ty] = at(i === 0 ? 0 : total);
    const r = w / 2;
    if (sharp || r < 2)
      return [[x + dir * tx * Math.max(r, 1), y + dir * ty * Math.max(r, 1), 'c']];
    const mk = (a) => {
      const ca = Math.cos(a);
      const sa = Math.sin(a);
      // rotate tangent by a, scaled
      return [x + dir * (tx * ca - ty * sa) * r, y + dir * (ty * ca + tx * sa) * r];
    };
    return [mk(0.9), mk(0), mk(-0.9)];
  };
  const pts = [];
  pts.push(...left);
  pts.push(...cap(1, 1, widths[widths.length - 1], tip1));
  pts.push(...right.reverse());
  pts.push(...cap(0, -1, widths[0], tip0));
  // cap(…) for start produces points in order tip-left→tip→tip-right as seen from
  // the end; they are traversed right→left here, which is the correct winding.
  return pts.map((p) => (p.length > 2 ? p : [p[0], p[1]]));
}

/** Marching squares on a smoothed signed-distance field. Returns closed loops. */
function contours(mask, W, H) {
  const PW = W + 2;
  const PH = H + 2;
  const inM = new Uint8Array(PW * PH);
  const outM = new Uint8Array(PW * PH).fill(1);
  for (let y = 0; y < H; y++)
    for (let x = 0; x < W; x++)
      if (mask[y * W + x]) {
        inM[(y + 1) * PW + x + 1] = 1;
        outM[(y + 1) * PW + x + 1] = 0;
      }
  const din = edtInside(inM, PW, PH);
  const dout = edtInside(outM, PW, PH);
  const f = new Float32Array(PW * PH);
  for (let i = 0; i < f.length; i++) f[i] = inM[i] ? din[i] : -dout[i];
  const pos = new Map();
  const link = new Map();
  const add = (ka, kb) => {
    if (!link.has(ka)) link.set(ka, []);
    if (!link.has(kb)) link.set(kb, []);
    link.get(ka).push(kb);
    link.get(kb).push(ka);
  };
  for (let y = 0; y < PH - 1; y++) {
    for (let x = 0; x < PW - 1; x++) {
      const fa = f[y * PW + x];
      const fb = f[y * PW + x + 1];
      const fc = f[(y + 1) * PW + x + 1];
      const fd = f[(y + 1) * PW + x];
      const c = (fa > 0 ? 1 : 0) | (fb > 0 ? 2 : 0) | (fc > 0 ? 4 : 0) | (fd > 0 ? 8 : 0);
      if (c === 0 || c === 15) continue;
      // coordinates in unpadded space; cell centres sit at +0.5
      const px = x - 1 + 0.5;
      const py = y - 1 + 0.5;
      const T = () => {
        const k = `h${x},${y}`;
        if (!pos.has(k)) pos.set(k, [px + fa / (fa - fb), py]);
        return k;
      };
      const R = () => {
        const k = `v${x + 1},${y}`;
        if (!pos.has(k)) pos.set(k, [px + 1, py + fb / (fb - fc)]);
        return k;
      };
      const B = () => {
        const k = `h${x},${y + 1}`;
        if (!pos.has(k)) pos.set(k, [px + fd / (fd - fc), py + 1]);
        return k;
      };
      const L = () => {
        const k = `v${x},${y}`;
        if (!pos.has(k)) pos.set(k, [px, py + fa / (fa - fd)]);
        return k;
      };
      const mid = (fa + fb + fc + fd) / 4;
      switch (c) {
        case 1:
        case 14:
          add(L(), T());
          break;
        case 2:
        case 13:
          add(T(), R());
          break;
        case 3:
        case 12:
          add(L(), R());
          break;
        case 4:
        case 11:
          add(R(), B());
          break;
        case 6:
        case 9:
          add(T(), B());
          break;
        case 7:
        case 8:
          add(L(), B());
          break;
        case 5:
          if (mid > 0) {
            add(T(), R());
            add(L(), B());
          } else {
            add(L(), T());
            add(R(), B());
          }
          break;
        case 10:
          if (mid > 0) {
            add(L(), T());
            add(R(), B());
          } else {
            add(T(), R());
            add(L(), B());
          }
          break;
        default:
          break;
      }
    }
  }
  const seen = new Set();
  const loops = [];
  for (const start of link.keys()) {
    if (seen.has(start)) continue;
    const loop = [];
    let prev = null;
    let cur = start;
    while (cur && !seen.has(cur)) {
      seen.add(cur);
      loop.push(pos.get(cur));
      const nb = link.get(cur);
      const next = nb.find((k) => k !== prev && !seen.has(k)) ?? null;
      prev = cur;
      cur = next;
    }
    if (loop.length > 8) loops.push(loop);
  }
  // light smoothing (closed 1-2-1 filter, a few passes)
  return loops.map((lp) => {
    let p = lp;
    for (let it = 0; it < 3; it++) {
      const n = p.length;
      p = p.map((q, i) => {
        const a = p[(i + n - 1) % n];
        const b = p[(i + 1) % n];
        return [(a[0] + 2 * q[0] + b[0]) / 4, (a[1] + 2 * q[1] + b[1]) / 4];
      });
    }
    return p;
  });
}

// ------------------------------------------------------------ figure model

class Figure {
  constructor(o) {
    this.name = o.name;
    this.w = o.w;
    this.h = o.h;
    this.s = o.s ?? 3; // 45 degree lattice step
    this.seed = o.seed ?? 1;
    this.vy = o.vy ?? 0; // crop: viewBox starts at y = vy
    this.shapes = [];
    this.strokes = [];
    this.marks = [];
    this.parts = [];
  }
  _part(id) {
    if (!this.parts.includes(id)) this.parts.push(id);
  }
  /** Filled region: occludes lower shapes, receives light, may draw its contour. */
  shape(o) {
    this._part(o.part);
    this.shapes.push({ tone: true, closed: true, z: 1, outline: true, ...o });
    return this;
  }
  /** Free pencil line. */
  stroke(o) {
    this._part(o.part);
    this.strokes.push({ z: 1, w: 1, closed: false, ...o });
    return this;
  }
  /** Random accent/ink speckles placed on a named shape where `where` agrees. */
  spots(o) {
    this._part(o.part);
    (this.spotSets ??= []).push(o);
    return this;
  }
  /** Solid ink or accent dot of radius r (eyes, nostrils, spots). */
  dot(part, x, y, r, accent = false, z = 9) {
    this._part(part);
    this.marks.push({ part, x, y, r, accent, z });
    return this;
  }
}

// --------------------------------------------------------------- compiler

const T_CUT = [0.12, 0.33, 0.55];
const DIA = [0, 0.27, 0.5, 0.76]; // dot diameter / lattice pitch

function compile(fig) {
  const W = fig.w;
  const H = fig.h;
  const N = W * H;
  const S = fig.s;
  const rand = rng(fig.seed * 7919 + 13);
  const tex1 = noise2(fig.seed + 11);
  const tex2 = noise2(fig.seed + 29);

  // ---- shapes: flatten, raster, distance
  const shapes = fig.shapes
    .map((sh, i) => ({ ...sh, order: i, zz: sh.z + i * 1e-4 }))
    .sort((a, b) => a.zz - b.zz);
  for (const sh of shapes) {
    let mask;
    if (sh.prims) {
      // union / subtraction of primitives, optional goo smoothing, contour by marching squares
      mask = new Uint8Array(N);
      for (const pr of sh.prims) {
        const fl = flatten(pr.pts, true, 1.1);
        const m = rasterPoly(fl.pts, W, H);
        for (let i = 0; i < N; i++) {
          if (!m[i]) continue;
          mask[i] = pr.sub ? 0 : 1;
        }
      }
      if (sh.goo) {
        let fm = Float32Array.from(mask);
        fm = boxBlur(boxBlur(fm, W, H, sh.goo), W, H, sh.goo);
        for (let i = 0; i < N; i++) mask[i] = fm[i] > 0.5 ? 1 : 0;
      }
      sh.loops = contours(mask, W, H);
      sh.idx = null;
    } else {
      const fl = flatten(sh.pts, sh.closed, 1.1);
      sh.loops = [fl.pts];
      sh.idx = fl.idx;
      mask = rasterPoly(fl.pts, W, H);
    }
    sh.mask = mask;
    sh.d = edtInside(mask, W, H);
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const lp of sh.loops)
      for (const p of lp) {
        x0 = Math.min(x0, p[0]);
        y0 = Math.min(y0, p[1]);
        x1 = Math.max(x1, p[0]);
        y1 = Math.max(y1, p[1]);
      }
    sh.bb = [
      Math.max(0, Math.floor(x0) - 1),
      Math.max(0, Math.floor(y0) - 1),
      Math.min(W - 1, Math.ceil(x1) + 1),
      Math.min(H - 1, Math.ceil(y1) + 1)
    ];
    // signed area per loop (y down): > 0 means visually clockwise
    sh.cws = sh.loops.map((lp) => {
      let a = 0;
      for (let i = 0; i < lp.length; i++) {
        const p = lp[i];
        const q = lp[(i + 1) % lp.length];
        a += p[0] * q[1] - q[0] * p[1];
      }
      return a > 0;
    });
  }
  if (process.env.DBG)
    shapes.forEach((sh) =>
      console.log(sh.name ?? '-', sh.loops.length, sh.loops.map((l) => l.length).join('/'))
    );
  const owner = new Int16Array(N).fill(-1);
  shapes.forEach((sh, si) => {
    const [x0, y0, x1, y1] = sh.bb;
    for (let y = y0; y <= y1; y++)
      for (let x = x0; x <= x1; x++) if (sh.mask[y * W + x]) owner[y * W + x] = si;
  });

  // ---- darkness map
  const D = new Float32Array(N);
  shapes.forEach((sh, si) => {
    if (!sh.tone) return;
    const [x0, y0, x1, y1] = sh.bb;
    const bw = x1 - x0 + 1;
    const bh = y1 - y0 + 1;
    const rc = sh.radius ?? 17;
    // work in a cropped window for the filters
    const d = new Float32Array(bw * bh);
    for (let y = 0; y < bh; y++)
      for (let x = 0; x < bw; x++) d[y * bw + x] = sh.d[(y + y0) * W + (x + x0)];
    const dB = boxBlur(boxBlur(d, bw, bh, 1), bw, bh, 1);
    const lm = boxBlur(maxFilter(d, bw, bh, rc), bw, bh, 3);
    const xr = Math.max(1, x1 - x0);
    const yr = Math.max(1, y1 - y0);
    for (let y = 0; y < bh; y++) {
      for (let x = 0; x < bw; x++) {
        const gi = (y + y0) * W + (x + x0);
        if (owner[gi] !== si) continue;
        const i = y * bw + x;
        const px = x + x0;
        const py = y + y0;
        let val;
        if (sh.flat !== undefined) {
          // flat card: constant tone, soft fall-off to the edge, optional gradient
          const g = sh.grad
            ? sh.grad[0] * ((px - x0) / xr - 0.5) + sh.grad[1] * ((py - y0) / yr - 0.5)
            : 0;
          val = sh.flat + g - (sh.edge ?? 0) * (1 - smooth(0, sh.edgeW ?? 7, d[i]));
          if (sh.fade) val *= smooth(0, sh.fade, d[i]);
        } else {
          const t = clamp(d[i] / Math.max(4, Math.min(rc, lm[i])), 0, 1);
          const gx = (dB[y * bw + Math.min(bw - 1, x + 1)] - dB[y * bw + Math.max(0, x - 1)]) / 2;
          const gy = (dB[Math.min(bh - 1, y + 1) * bw + x] - dB[Math.max(0, y - 1) * bw + x]) / 2;
          const gm = Math.hypot(gx, gy);
          const ox = gm > 1e-3 ? -gx / gm : 0;
          const oy = gm > 1e-3 ? -gy / gm : 0;
          const m = 1 - t;
          const nz = Math.sqrt(Math.max(0, 1 - m * m));
          const lam = Math.max(0.1, ox * m * LIGHT[0] + oy * m * LIGHT[1] + nz * LIGHT[2]);
          val = (1 - lam) * (sh.gain ?? 1.05) + (sh.dark ?? 0);
          const vv = (py - y0) / yr;
          val += (sh.vgrad ?? 0.32) * (vv * vv) - 0.12 * (sh.vgrad ?? 0.32);
          val += (sh.hgrad ?? 0.16) * ((px - x0) / xr - 0.5);
        }
        val += 0.06 * tex1(px / 24, py / 24) + 0.04 * tex2(px / 8, py / 8);
        D[gi] = val;
      }
    }
  });
  // cast shadows from nearer shapes toward the lower right
  const shadow = new Float32Array(N);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const i = y * W + x;
      const o = owner[i];
      if (o < 0 || !shapes[o].tone || shapes[o].flat !== undefined) continue;
      let sh = 0;
      for (let k = 3; k <= 15; k += 3) {
        const qx = Math.round(x + LXY[0] * k);
        const qy = Math.round(y + LXY[1] * k);
        if (qx < 0 || qy < 0 || qx >= W || qy >= H) continue;
        const oq = owner[qy * W + qx];
        if (oq >= 0 && oq !== o && shapes[oq].zz > shapes[o].zz && shapes[oq].casts !== false) {
          sh += (1 - k / 18) * 0.6;
        }
      }
      shadow[i] = Math.min(sh, 0.6);
    }
  }
  for (let i = 0; i < N; i++) if (owner[i] >= 0) D[i] = clamp(D[i] + shadow[i] * 0.85, 0, 1);

  // ---- halftone dots on a 45 degree lattice
  const dotLists = new Map(); // key part|accent|cls -> [x,y]
  const addDot = (part, accent, cls, x, y) => {
    const key = `${part}|${accent ? 1 : 0}|${cls}`;
    let l = dotLists.get(key);
    if (!l) dotLists.set(key, (l = []));
    l.push([x, y]);
  };
  const pitch = S * Math.SQRT2;
  const cell = new Uint8Array(N); // hatch eligibility
  for (let j = 0; j * S < H; j++) {
    for (let i = (j & 1) === 0 ? 0 : 1; i * S < W; i += 2) {
      const x = i * S;
      const y = j * S;
      const gi = Math.min(H - 1, y) * W + Math.min(W - 1, x);
      const o = owner[gi];
      if (o < 0 || !shapes[o].tone) continue;
      const sh = shapes[o];
      const v = D[gi];
      const cls = v >= T_CUT[2] ? 3 : v >= T_CUT[1] ? 2 : v >= T_CUT[0] ? 1 : 0;
      if (cls === 0) continue;
      addDot(sh.part, !!sh.accent, cls, x, y);
    }
  }
  void cell;

  // ---- pencil hatching in the deepest shadows
  const hatches = new Map(); // part -> items (tenths)
  const addHatch = (part, x0, y0, x1, y1) => {
    let l = hatches.get(part);
    if (!l) hatches.set(part, (l = []));
    l.push([
      Math.round(x0 * 10),
      Math.round(y0 * 10),
      Math.round((x1 - x0) * 10),
      Math.round((y1 - y0) * 10)
    ]);
  };
  const hatchFamily = (offset, thr, spacing) => {
    for (let c = -H + offset; c < W + H; c += spacing) {
      let runStart = null;
      let runOwner = -1;
      const flush = (xEnd) => {
        if (runStart === null) return;
        const sh = shapes[runOwner];
        let a = runStart;
        const b = xEnd;
        if (sh && sh.hatch !== 0 && !sh.flat && b - a >= 4) {
          // break long runs into pencil-length strokes
          while (a < b) {
            let len = 9 + rand() * 15;
            if (b - a < len * 1.4) len = b - a;
            const e = Math.min(b, a + len);
            if (e - a >= 4 && rand() > 0.16) {
              const j0 = rand() * 1.4;
              const j1 = rand() * 1.4;
              const xa = a + j0;
              const xb = e - j1;
              if (xb - xa > 3) {
                const wob = (rand() - 0.5) * 0.9;
                addHatch(sh.part, xa, c - xa, xb, c - xb + wob);
              }
            }
            a = e + 1 + rand() * 3;
          }
        }
        runStart = null;
      };
      for (let x = 0; x < W; x++) {
        const y = c - x;
        if (y < 0 || y >= H) {
          flush(x);
          continue;
        }
        const gi = Math.floor(y) * W + x;
        const o = owner[gi];
        const ok = o >= 0 && shapes[o].tone && D[gi] > thr + (shapes[o].hatchBias ?? 0);
        if (ok && runStart !== null && o !== runOwner) flush(x);
        if (ok) {
          if (runStart === null) {
            runStart = x;
            runOwner = o;
          }
        } else flush(x);
      }
      flush(W);
    }
  };
  hatchFamily(0, 0.64, 5);
  hatchFamily(2.5, 0.86, 5);

  // ---- outlines
  const hiddenAt = (x, y, zz, self) => {
    const cx = clamp(Math.floor(x), 0, W - 1);
    const cy = clamp(Math.floor(y), 0, H - 1);
    const o = owner[cy * W + cx];
    if (o < 0 || o === self) return false;
    const osh = shapes[o];
    if (osh.zz <= zz || osh.occlude === false) return false;
    return osh.d[cy * W + cx] >= 1.0;
  };
  const resample = (pl, ds) => {
    const out = [];
    let acc = 0;
    let next = 0;
    out.push([pl[0][0], pl[0][1], 0]);
    next = ds;
    for (let i = 1; i < pl.length; i++) {
      const a = pl[i - 1];
      const b = pl[i];
      const L = Math.hypot(b[0] - a[0], b[1] - a[1]);
      while (acc + L >= next && L > 0) {
        const t = (next - acc) / L;
        out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, next]);
        next += ds;
      }
      acc += L;
    }
    const last = pl[pl.length - 1];
    const lp = out[out.length - 1];
    if (Math.hypot(last[0] - lp[0], last[1] - lp[1]) > 0.4) out.push([last[0], last[1], acc]);
    return out;
  };
  const dp = (pts, tol) => {
    const keep = new Uint8Array(pts.length);
    keep[0] = keep[pts.length - 1] = 1;
    const stack = [[0, pts.length - 1]];
    while (stack.length) {
      const [a, b] = stack.pop();
      let md = 0;
      let mi = -1;
      const A = pts[a];
      const B = pts[b];
      const dx = B[0] - A[0];
      const dy = B[1] - A[1];
      const L = Math.hypot(dx, dy) || 1;
      for (let i = a + 1; i < b; i++) {
        const dd = Math.abs((pts[i][0] - A[0]) * dy - (pts[i][1] - A[1]) * dx) / L;
        if (dd > md) {
          md = dd;
          mi = i;
        }
      }
      if (mi >= 0 && (md > tol || Math.hypot(dx, dy) > 15)) {
        keep[mi] = 1;
        stack.push([a, mi], [mi, b]);
      }
    }
    return pts.filter((_, i) => keep[i]);
  };
  const emitCurve = (pts) => {
    // pts: [x,y] floats -> midpoint quadratic smoothing in tenths
    const P = pts.map(([x, y]) => [Math.round(x * 10), Math.round(y * 10)]);
    if (P.length < 2) return '';
    const items = ['M', P[0][0], P[0][1]];
    if (P.length === 2) {
      items.push('l', P[1][0] - P[0][0], P[1][1] - P[0][1]);
      return pathStr(items);
    }
    let cx = P[0][0];
    let cy = P[0][1];
    items.push('q');
    for (let i = 1; i < P.length - 1; i++) {
      const mx = Math.round((P[i][0] + P[i + 1][0]) / 2);
      const my = Math.round((P[i][1] + P[i + 1][1]) / 2);
      items.push(P[i][0] - cx, P[i][1] - cy, mx - cx, my - cy);
      cx = mx;
      cy = my;
    }
    const L = P[P.length - 1];
    items.push('l', L[0] - cx, L[1] - cy);
    return pathStr(items);
  };

  let nSeed = fig.seed * 101;
  /**
   * Turn a contour polyline into pencil strokes. kind: 'shape' (closed outward
   * normals known) or 'line'. Returns { main, sec, weight } path strings.
   */
  const pencil = (
    poly,
    {
      zz,
      self,
      wScale = 1,
      cw = true,
      pressure = true,
      closedLoop = false,
      skip = null,
      single = false
    }
  ) => {
    const rs = resample(poly, 2);
    if (rs.length < 2) return null;
    const nA = noise1(nSeed++);
    const nB = noise1(nSeed++);
    const nC = noise1(nSeed++);
    const nD = noise1(nSeed++);
    const nW = noise1(nSeed++);
    const amp = 0.8 * wScale;
    const make = (nL, nH, a, hf) =>
      rs.map((p, i) => {
        const a0 = rs[Math.max(0, i - 1)];
        const a1 = rs[Math.min(rs.length - 1, i + 1)];
        let tx = a1[0] - a0[0];
        let ty = a1[1] - a0[1];
        const m = Math.hypot(tx, ty) || 1;
        tx /= m;
        ty /= m;
        const off = a * nL(p[2] / 46) + hf * nH(p[2] / 9);
        return [p[0] - ty * off, p[1] + tx * off, p[2], tx, ty];
      });
    const A = make(nA, nB, amp, 0.22);
    const B = make(nC, nD, amp * 1.35, 0.25);
    const small = single || rs[rs.length - 1][2] < 120;
    const visible = rs.map(
      (p) =>
        !hiddenAt(p[0], p[1], zz, self) &&
        !(skip && skip.some(([a, b, c, d]) => p[0] >= a && p[0] <= c && p[1] >= b && p[1] <= d))
    );
    // fill tiny gaps
    for (let i = 1; i < visible.length - 1; i++) {
      if (!visible[i] && visible[i - 1]) {
        let j = i;
        while (j < visible.length && !visible[j]) j++;
        if (j < visible.length && j - i <= 2) for (let k = i; k < j; k++) visible[k] = true;
      }
    }
    const runs = [];
    let cur = null;
    for (let i = 0; i < rs.length; i++) {
      if (visible[i]) {
        if (!cur) cur = [];
        cur.push(i);
      } else if (cur) {
        runs.push(cur);
        cur = null;
      }
    }
    if (cur) runs.push(cur);
    const mainPaths = [];
    const secPaths = [];
    const weightPaths = [];
    for (const run of runs) {
      if (run.length < 2) continue;
      const pa = dp(
        run.map((i) => A[i]),
        0.65
      );
      mainPaths.push(emitCurve(pa));
      // second, offset pass covering about two thirds
      let i = 0;
      while (!small && i < run.length) {
        const len = Math.round(12 + rand() * 26);
        const gap = Math.round(7 + rand() * 18);
        const seg = run.slice(i, i + len);
        if (seg.length >= 4 && rand() > 0.18) {
          secPaths.push(
            emitCurve(
              dp(
                seg.map((k) => B[k]),
                0.85
              )
            )
          );
        }
        i += len + gap;
      }
      // pressure: heavier overlay on the shadow side
      if (pressure && !small) {
        let k = 0;
        while (k < run.length) {
          const len = Math.round(12 + rand() * 26);
          const seg = run.slice(k, k + len);
          const mid = A[seg[Math.floor(seg.length / 2)]];
          const nx = cw ? mid[4] : -mid[4];
          const ny = cw ? -mid[3] : mid[3];
          const facing = nx * LXY[0] + ny * LXY[1];
          const p = -facing * 0.9 + 0.55 * nW(mid[2] / 60);
          if (seg.length >= 5 && p > 0.35)
            weightPaths.push(
              emitCurve(
                dp(
                  seg.map((q) => A[q]),
                  0.65
                )
              )
            );
          k += len;
        }
      }
    }
    return {
      main: mainPaths,
      sec: secPaths,
      weight: weightPaths,
      hasRuns: runs.length > 0,
      rs,
      loop: closedLoop
    };
  };

  const strokeItems = []; // for part-level assembly: { part, kind:'ol'|'ol2'|'ow', accent, d }
  const pushOl = (part, accent, res, wMain) => {
    if (!res) return;
    if (res.main.length)
      strokeItems.push({ part, accent, kind: 'ol', d: res.main.join(''), w: wMain });
    if (res.sec.length)
      strokeItems.push({ part, accent, kind: 'ol2', d: res.sec.join(''), w: wMain * 0.62 });
    if (res.weight.length)
      strokeItems.push({ part, accent, kind: 'ow', d: res.weight.join(''), w: wMain * 1.5 });
  };

  shapes.forEach((sh, si) => {
    if (!sh.outline || sh.nostroke) return;
    sh.loops.forEach((poly, li) => {
      let pl;
      if (sh.outline === true || !sh.idx) {
        pl = [...poly, poly[0]];
      } else {
        const [a, b] = sh.outline; // control indices, cyclic
        const n = sh.pts.length;
        let ia = sh.idx[((a % n) + n) % n];
        const ib = sh.idx[((b % n) + n) % n];
        pl = [];
        let guard = 0;
        while (guard++ < poly.length + 2) {
          pl.push(poly[ia]);
          if (ia === ib) break;
          ia = (ia + 1) % poly.length;
        }
      }
      const res = pencil(pl, {
        zz: sh.zz,
        self: si,
        wScale: sh.wobble ?? 1,
        cw: sh.cws[li],
        skip: sh.skipOutline
      });
      pushOl(sh.part, !!sh.accentLine, res, sh.lw ?? 1.5);
    });
  });
  for (const st of fig.strokes) {
    const fl = flatten(st.pts, st.closed, 1.1);
    const pl = st.closed ? [...fl.pts, fl.pts[0]] : fl.pts;
    const res = pencil(pl, {
      zz: st.z + 0.05,
      self: -2,
      wScale: st.wobble ?? 0.7,
      pressure: st.pressure ?? false,
      single: st.single ?? (st.w ?? 1) <= 0.7
    });
    pushOl(st.part, !!st.accent, res, (st.lw ?? 1.5) * (st.w ?? 1));
  }

  // ---- speckles
  (fig.spotSets ?? []).forEach((sp, k) => {
    const rr = rng(fig.seed * 31 + k * 17 + 5);
    const si = shapes.findIndex((x) => x.name === sp.on);
    if (si < 0) return;
    const [bx0, by0, bx1, by1] = shapes[si].bb;
    const placed = [];
    for (let tries = 0; placed.length < sp.n && tries < 6000; tries++) {
      const x = bx0 + rr() * (bx1 - bx0);
      const y = by0 + rr() * (by1 - by0);
      const gi = Math.floor(y) * W + Math.floor(x);
      if (owner[gi] !== si) continue;
      if (!sp.where(x, y, D[gi], shapes[si].d[gi])) continue;
      if (placed.some((p) => Math.hypot(p[0] - x, p[1] - y) < sp.minDist)) continue;
      placed.push([x, y]);
      const r = Math.round((sp.r[0] + rr() * (sp.r[1] - sp.r[0])) * 5) / 5;
      fig.marks.push({
        part: sp.part,
        x: Math.round(x),
        y: Math.round(y),
        r,
        accent: sp.accent !== false,
        z: 9
      });
    }
  });

  // ---- marks (solid dots)
  const markGroups = new Map();
  for (const m of fig.marks) {
    if (hiddenAt(m.x, m.y, m.z, -3)) continue;
    const key = `${m.part}|${m.accent ? 1 : 0}|${m.r}`;
    let g = markGroups.get(key);
    if (!g) markGroups.set(key, (g = { part: m.part, accent: m.accent, r: m.r, pts: [] }));
    g.pts.push([m.x, m.y]);
  }

  // ---- assemble svg
  const dotPath = (list) => {
    list.sort((p, q) => p[1] - q[1] || p[0] - q[0]);
    const items = [];
    let cx = 0;
    let cy = 0;
    list.forEach(([x, y], k) => {
      if (k === 0) items.push('M', x * 10, y * 10, 'h', 0);
      else items.push('m', (x - cx) * 10, (y - cy) * 10, 'h', 0);
      cx = x;
      cy = y;
    });
    return pathStr(items);
  };
  // Halftone dots: horizontal runs on the lattice drawn as one dashed line each
  // (dash pattern "0 2S" + round caps = one dot every 2S units).
  const runPath = (list) => {
    list.sort((p, q) => p[1] - q[1] || p[0] - q[0]);
    const items = [];
    let cx = 0;
    let cy = 0;
    let first = true;
    const flush = (x, y, n) => {
      const len = n === 1 ? 1 : (n - 1) * 2 * S * 10 + 1;
      if (first) items.push('M', x * 10, y * 10, 'h', len);
      else items.push('m', x * 10 - cx, y * 10 - cy, 'h', len);
      first = false;
      cx = x * 10 + len;
      cy = y * 10;
    };
    let rx = 0;
    let ry = 0;
    let rn = 0;
    for (const [x, y] of list) {
      if (rn > 0 && y === ry && x === rx + rn * 2 * S) rn++;
      else {
        if (rn > 0) flush(rx, ry, rn);
        rx = x;
        ry = y;
        rn = 1;
      }
    }
    if (rn > 0) flush(rx, ry, rn);
    return pathStr(items);
  };
  const hatchPath = (list) => {
    const items = [];
    let cx = 0;
    let cy = 0;
    list.forEach(([x, y, dx, dy], k) => {
      if (k === 0) items.push('M', x, y, 'l', dx, dy);
      else items.push('m', x - cx, y - cy, 'l', dx, dy);
      cx = x + dx;
      cy = y + dy;
    });
    return pathStr(items);
  };
  let dotCount = 0;
  const out = [];
  for (const part of fig.parts) {
    const inner = [];
    // tone layers, ink then accent
    for (const accent of [0, 1]) {
      for (const cls of [1, 2, 3]) {
        const l = dotLists.get(`${part}|${accent}|${cls}`);
        if (!l || !l.length) continue;
        dotCount += l.length;
        const dia = +(DIA[cls] * pitch).toFixed(2);
        inner.push(
          `<path class="t t${cls}${accent ? ' a' : ''}" stroke-width="${dia}" stroke-dasharray="0 ${2 * S}" d="${runPath(l)}" />`
        );
      }
    }
    const hat = hatches.get(part);
    if (hat && hat.length) {
      inner.push(
        `<path class="ht" stroke-opacity=".85" stroke-width="0.8" d="${hatchPath(hat)}" />`
      );
    }
    const sItems = strokeItems.filter((s) => s.part === part);
    const olParts = [];
    // Weight overlays are merged per colour; tiny pencil strokes are merged per
    // width so a plate with dozens of small bones does not pay per-path overhead.
    const merged = new Map();
    const wStr = (w) => (Math.round((w ?? 1.5) * 10) / 10).toString().replace(/^0\./, '.');
    const olTag = (kind, accent, w, d) =>
      `<path class="ol${kind === 'ol2' ? ' o2' : ''}${accent ? ' a' : ''}"${
        kind === 'ol2' ? ' stroke-opacity=".55"' : ''
      } pathLength="1" stroke-width="${wStr(w)}" d="${d}" />`;
    for (const s of sItems) {
      if (s.kind === 'ow' || s.d.length < 240) {
        const key = `${s.kind}|${s.accent ? 1 : 0}|${s.kind === 'ow' ? 0 : wStr(s.w)}`;
        const g = merged.get(key) ?? { kind: s.kind, accent: s.accent, w: s.w, d: '' };
        g.d += s.d;
        merged.set(key, g);
      } else {
        olParts.push(olTag(s.kind, s.accent, s.w, s.d));
      }
    }
    for (const g of merged.values()) {
      if (g.kind === 'ow') {
        olParts.push(`<path class="ow${g.accent ? ' a' : ''}" stroke-width="1.9" d="${g.d}" />`);
      } else {
        olParts.push(olTag(g.kind, g.accent, g.w, g.d));
      }
    }
    // marks last
    const marks = [];
    for (const g of markGroups.values()) {
      if (g.part !== part) continue;
      marks.push(
        `<path class="m${g.accent ? ' a' : ''}" stroke-width="${(g.r * 2).toFixed(1)}" d="${dotPath(
          g.pts.map(([x, y]) => [x, y]).map(([x, y]) => [x, y])
        )}" />`
      );
    }
    out.push(
      `<g class="part part-${part}">${inner.join('')}${olParts.join('')}${marks.join('')}</g>`
    );
  }
  const all = out.join('\n');
  const bytes = (re) => (all.match(re) ?? []).join('').length;
  const stats = {
    dots: bytes(/<path class="t [^>]*>/g),
    hatch: bytes(/<path class="ht"[^>]*>/g),
    ol: bytes(/<path class="ol(?: a)?"[^>]*>/g),
    ol2: bytes(/<path class="ol o2[^>]*>/g),
    ow: bytes(/<path class="ow[^>]*>/g),
    marks: bytes(/<path class="m[ "][^>]*>/g)
  };
  return { markup: all, dotCount, stats };
}

// ----------------------------------------------------------------- figures

const FIGURES = [];
const figs = (n) => FIGURES.find((f) => f.name === n);
void figs;

// REX -------------------------------------------------------------------
/** Paper-white triangular teeth hanging from (or rising out of) a mouth line. */
function teeth(f, part, z, line, { from, to, step, size, dir = 1, xf }) {
  const fl = flatten(line, false, 1);
  const pl = fl.pts;
  const s = [0];
  for (let i = 1; i < pl.length; i++)
    s.push(s[i - 1] + Math.hypot(pl[i][0] - pl[i - 1][0], pl[i][1] - pl[i - 1][1]));
  const total = s[s.length - 1];
  const at = (u) => {
    let j = 1;
    while (j < s.length - 1 && s[j] < u) j++;
    const t = (u - s[j - 1]) / Math.max(1e-6, s[j] - s[j - 1]);
    return [
      pl[j - 1][0] + (pl[j][0] - pl[j - 1][0]) * t,
      pl[j - 1][1] + (pl[j][1] - pl[j - 1][1]) * t
    ];
  };
  let k = 0;
  for (let u = from * total; u < to * total; u += step, k++) {
    const a = at(u);
    const b = at(Math.min(total, u + step * 0.8));
    const sz = size * (1 - 0.3 * (u / total)) * (k % 2 ? 0.85 : 1);
    const tip = [(a[0] + b[0]) / 2 + 0.6, (a[1] + b[1]) / 2 + dir * sz];
    f.shape({
      part,
      z,
      tone: false,
      casts: false,
      lw: 1.1,
      wobble: 0.2,
      pts: xf([
        [a[0], a[1], 'c'],
        [tip[0], tip[1], 'c'],
        [b[0], b[1], 'c']
      ])
    });
  }
}

// prettier-ignore
function buildRex() {
  const f = new Figure({ name: 'rex', w: 600, h: 400, vy: 40, s: 3, seed: 7 });
  const HEAD = (pts) => place(pts, 40, 160, -8, 1.0);
  // ground shadow
  f.shape({
    name: 'ground',
    part: 'ground',
    z: 0,
    outline: false,
    casts: false,
    pts: ellipse(322, 377, 270, 14, 12),
    flat: 0.5,
    fade: 12
  });
  const legBody = (dx, dy) => [
    {
      pts: mv(
        tube(
          [
            [404, 172], [384, 220], [362, 262], [402, 312], [392, 352]
          ],
          [84, 72, 44, 26, 22]
        ),
        dx,
        dy
      )
    },
    {
      pts: mv(
        tube(
          [
            [396, 344], [372, 360], [344, 372]
          ],
          [30, 20, 9],
          { tip1: true }
        ),
        dx,
        dy
      )
    },
    {
      pts: mv(
        tube(
          [
            [400, 338], [380, 350], [358, 360]
          ],
          [24, 15, 6],
          { tip1: true }
        ),
        dx,
        dy
      )
    },
    {
      pts: mv(
        tube(
          [
            [394, 352], [380, 366], [366, 376]
          ],
          [22, 14, 6],
          { tip1: true }
        ),
        dx,
        dy
      )
    }
  ];
  // far leg
  f.shape({
    name: 'farleg',
    part: 'body',
    z: 1,
    dark: 0.22,
    hatchBias: -0.1,
    prims: legBody(44, -6),
    goo: 1
  });
  // torso + neck + tail
  f.shape({
    name: 'torso',
    part: 'body',
    z: 2,
    lw: 1.2,
    prims: [
      {
        pts: [
          [236, 154], [268, 126], [330, 108], [392, 98], [434, 106], [454, 134], [440, 172],
          [402, 206], [350, 230], [300, 241], [258, 238], [232, 218], [224, 184]
        ]
      },
      {
        pts: tube(
          [
            [420, 128], [480, 142], [532, 164], [583, 196]
          ],
          [76, 54, 28, 5],
          { tip1: true }
        )
      },
      {
        pts: tube(
          [
            [262, 166], [238, 128], [210, 122], [186, 142], [172, 168]
          ],
          [86, 66, 54, 54, 52]
        )
      }
    ],
    goo: 2
  });
  // head (own shape so it gets its own rim light)
  f.shape({
    name: 'head',
    part: 'body',
    z: 2.2,
    lw: 1.2,
    vgrad: 0.12,
    skipOutline: [[150, 120, 190, 214]],
    pts: HEAD([
      [0, 36], [1, 22], [10, 10], [24, 4], [46, 5], [74, 3], [96, -6], [112, -15], [128, -10],
      [142, 2], [152, 20], [155, 42], [148, 62], [132, 78], [108, 84], [80, 78], [52, 68], [26, 56],
      [8, 46]
    ])
  });
  // near leg
  f.shape({
    name: 'leg',
    part: 'body',
    z: 3,
    lw: 1.2,
    skipOutline: [[330, 60, 480, 168]],
    prims: legBody(0, 0),
    goo: 1
  });
  // arm
  f.shape({
    name: 'arm',
    part: 'body',
    z: 4,
    lw: 1.4,
    prims: [
      {
        pts: tube(
          [
            [252, 190], [244, 212], [224, 226], [205, 230]
          ],
          [24, 17, 12, 9]
        )
      }
    ]
  });
  // head details
  const mouth = [
    [3, 40], [30, 49], [58, 57], [88, 61], [112, 58], [130, 50], [140, 38]
  ];
  f.stroke({ part: 'body', z: 2.3, w: 1, pts: HEAD(mouth) });
  teeth(f, 'body', 2.4, HEAD(mouth), { from: 0.04, to: 0.8, step: 11, size: 12, xf: (p) => p });
  // eye
  f.shape({
    part: 'body',
    z: 2.5,
    tone: false,
    lw: 1.3,
    casts: false,
    pts: HEAD(ellipse(111, 6, 7, 5.6, 8))
  });
  f.dot('body', ...HEAD([[111, 6.5]])[0], 3.0);
  f.dot('body', ...HEAD([[109.5, 5]])[0], 1.2, true);
  f.dot('body', ...HEAD([[22, 14]])[0], 1.8);
  // brow + cheek lines
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.8,
    pts: HEAD([
      [96, -4], [110, -9], [126, -3]
    ])
  });
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.7,
    pts: HEAD([
      [100, 22], [118, 34], [128, 50]
    ])
  });
  // toe creases on the near foot
  f.stroke({
    part: 'body',
    z: 3.1,
    w: 0.6,
    pts: [
      [384, 352], [366, 362], [350, 368]
    ]
  });
  f.stroke({
    part: 'body',
    z: 3.1,
    w: 0.6,
    pts: [
      [388, 346], [372, 352], [356, 358]
    ]
  });
  // belly scutes
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.6,
    pts: [
      [262, 226], [284, 234], [306, 236]
    ]
  });
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.6,
    pts: [
      [300, 214], [322, 224], [346, 222]
    ]
  });
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.6,
    pts: [
      [332, 200], [352, 208], [372, 204]
    ]
  });
  // claws on the hand
  f.shape({
    name: 'claw1',
    part: 'body',
    z: 4.1,
    lw: 1.1,
    prims: [
      {
        pts: tube(
          [
            [210, 224], [198, 230], [188, 240]
          ],
          [8, 6, 2],
          { tip1: true }
        )
      }
    ]
  });
  f.shape({
    name: 'claw2',
    part: 'body',
    z: 4.1,
    lw: 1.1,
    prims: [
      {
        pts: tube(
          [
            [212, 218], [200, 220], [189, 224]
          ],
          [7, 5, 2],
          { tip1: true }
        )
      }
    ]
  });
  // page
  const pg = pagePieces(174, 266, 74, 96, -6, 0.2, 5);
  f.shape({
    name: 'page',
    part: 'page',
    z: 3.5,
    pts: pg.body,
    flat: 0.34,
    edge: 0.18,
    edgeW: 5,
    grad: [0.16, 0.22],
    accent: true,
    lw: 1.5,
    casts: true
  });
  f.stroke({ part: 'page', z: 3.5, w: 0.8, pts: pg.foldLine });
  pg.text.forEach((t) => f.stroke({ part: 'page', z: 3.5, w: 0.7, pts: t }));
  // accent speckles on the back
  f.spots({
    part: 'body',
    on: 'torso',
    n: 16,
    r: [1.6, 2.6],
    minDist: 20,
    where: (x, y, dv, d) => d > 9 && y < 205 && x > 290 && dv < 0.55
  });
  // ground pencil strokes
  [
    [110, 382, 62],
    [196, 388, 40],
    [270, 380, 70],
    [352, 386, 36],
    [430, 380, 66],
    [500, 387, 34],
    [330, 395, 52]
  ].forEach(([x, y, l]) =>
    f.stroke({
      part: 'ground',
      z: 0,
      w: 0.55,
      wobble: 0.4,
      pts: [
        [x, y],
        [x + l / 2, y + 0.6],
        [x + l, y]
      ]
    })
  );
  return f;
}
FIGURES.push(buildRex());

// STEGO ------------------------------------------------------------------
// prettier-ignore
function buildStego() {
  const f = new Figure({ name: 'stego', w: 600, h: 400, vy: 36, s: 3, seed: 21 });
  const HEAD = (pts) => place(pts, 494, 272, 15, 0.92);
  groundFor(f, 310, 378, 262, 15);
  // plates: near row (ink) and far row (accent) - pages standing on the spine
  const plate = (z, accent, spec, lines, dx = 0, dy = 0, name) => {
    const [bx, by, th, w, h] = spec;
    const n = [Math.sin((th * Math.PI) / 180), -Math.cos((th * Math.PI) / 180)];
    const c = [bx + dx + n[0] * (h / 2 - 8), by + dy + n[1] * (h / 2 - 8)];
    const pg = pagePieces(c[0], c[1], w, h, th, 0.2, lines);
    f.shape({
      name,
      part: 'plates',
      z,
      pts: pg.body,
      accent,
      lw: 1.3,
      flat: accent ? 0.3 : 0.38,
      edge: 0.1,
      edgeW: 5,
      grad: [0.18, 0.34],
      casts: true
    });
    f.stroke({ part: 'plates', z, w: 0.7, pts: pg.foldLine });
    pg.text.forEach((t) => f.stroke({ part: 'plates', z, w: 0.6, pts: t }));
  };
  const PL = [
    [418, 198, 38, 34, 54],
    [378, 176, 30, 42, 68],
    [338, 157, 16, 50, 82],
    [298, 148, 0, 54, 94],
    [258, 153, -14, 52, 88],
    [222, 172, -28, 46, 74],
    [190, 206, -42, 38, 58],
    [164, 228, -50, 30, 44]
  ];
  PL.forEach((p, i) => plate(0.9 + i * 0.001, true, p, 0, 20, -14, 'far' + i));
  PL.forEach((p, i) => plate(1.5 + i * 0.001, false, p, i > 1 && i < 6 ? 2 : 0, 0, 0, 'near' + i));
  // far legs
  f.shape({
    name: 'farlegs',
    part: 'body',
    z: 1.6,
    dark: 0.22,
    hatchBias: -0.1,
    goo: 1,
    prims: [
      {
        pts: tube(
          [
            [326, 222], [304, 280], [298, 330], [302, 366]
          ],
          [84, 58, 40, 34]
        )
      },
      {
        pts: tube(
          [
            [300, 360], [280, 370], [262, 374]
          ],
          [36, 22, 10],
          { tip1: true }
        )
      },
      {
        pts: tube(
          [
            [388, 270], [394, 322], [390, 368]
          ],
          [48, 36, 34]
        )
      },
      {
        pts: tube(
          [
            [390, 362], [376, 370], [364, 374]
          ],
          [32, 20, 10],
          { tip1: true }
        )
      }
    ]
  });
  // torso + tail + neck
  f.shape({
    name: 'torso',
    part: 'body',
    z: 2,
    lw: 1.2,
    goo: 2,
    vgrad: 0.16,
    prims: [
      {
        pts: [
          [180, 218], [215, 178], [265, 154], [305, 148], [350, 162], [400, 188], [440, 216],
          [448, 252], [420, 286], [360, 302], [290, 304], [230, 292], [192, 264]
        ]
      },
      {
        pts: tube(
          [
            [214, 208], [166, 230], [120, 248], [88, 256], [70, 257]
          ],
          [72, 48, 28, 15, 9],
          { tip1: true }
        )
      },
      {
        pts: tube(
          [
            [430, 214], [466, 238], [492, 262], [504, 276]
          ],
          [86, 56, 40, 34]
        )
      }
    ]
  });
  f.shape({
    name: 'head',
    part: 'body',
    z: 2.2,
    lw: 1.2,
    vgrad: 0.1,
    pts: HEAD([
      [-6, -8], [22, -16], [56, -14], [88, -4], [108, 8], [106, 20], [88, 27], [54, 30], [22, 28],
      [0, 16]
    ])
  });
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.9,
    pts: HEAD([
      [104, 18], [80, 22], [50, 24], [20, 22]
    ])
  });
  f.dot('body', ...HEAD([[58, 2]])[0], 3.0);
  f.dot('body', ...HEAD([[56.5, 0.5]])[0], 1.1, true);
  f.dot('body', ...HEAD([[100, 6]])[0], 1.4);
  // near legs
  f.shape({
    name: 'hindleg',
    part: 'body',
    z: 3,
    lw: 1.2,
    goo: 1,
    skipOutline: [[200, 120, 340, 226]],
    prims: [
      {
        pts: tube(
          [
            [296, 218], [272, 280], [262, 330], [266, 366]
          ],
          [90, 62, 42, 36]
        )
      },
      {
        pts: tube(
          [
            [264, 358], [244, 368], [226, 374]
          ],
          [38, 24, 10],
          { tip1: true }
        )
      }
    ]
  });
  f.shape({
    name: 'frontleg',
    part: 'body',
    z: 3,
    lw: 1.2,
    goo: 1,
    skipOutline: [[380, 220, 470, 262]],
    prims: [
      {
        pts: tube(
          [
            [420, 262], [428, 318], [422, 368]
          ],
          [54, 40, 36]
        )
      },
      {
        pts: tube(
          [
            [422, 362], [406, 370], [392, 374]
          ],
          [34, 22, 10],
          { tip1: true }
        )
      }
    ]
  });
  // tail spikes (thagomizer): two pairs
  const spike = (name, z, base, tip, w) =>
    f.shape({
      name,
      part: 'body',
      z,
      lw: 1.1,
      hatch: 0,
      prims: [
        {
          pts: tube(
            [base, [(base[0] + tip[0]) / 2 + 2, (base[1] + tip[1]) / 2], tip],
            [w, w * 0.6, 1.5],
            { tip1: true }
          )
        }
      ]
    });
  spike('sp1', 1.7, [136, 238], [140, 182], 18);
  spike('sp2', 1.7, [108, 248], [84, 196], 16);
  spike('sp3', 2.5, [122, 242], [100, 184], 18);
  spike('sp4', 2.5, [92, 251], [52, 208], 15);
  // pencil details
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.6,
    pts: [
      [300, 262], [330, 276], [360, 274]
    ]
  });
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.6,
    pts: [
      [330, 240], [360, 252], [392, 248]
    ]
  });
  f.stroke({
    part: 'body',
    z: 3.1,
    w: 0.6,
    pts: [
      [250, 296], [262, 304], [276, 300]
    ]
  });
  [
    [236, 372], [246, 372], [400, 372], [410, 372]
  ].forEach((p) => f.stroke({ part: 'body', z: 3.2, w: 0.6, pts: [p, [p[0] - 3, p[1] - 9]] }));
  f.spots({
    part: 'body',
    on: 'torso',
    n: 14,
    r: [1.6, 2.6],
    minDist: 20,
    where: (x, y, dv, d) => d > 9 && x > 200 && x < 440 && y < 270 && dv < 0.55
  });
  groundStrokes(f, [
    [90, 386, 60],
    [190, 392, 44],
    [280, 384, 70],
    [380, 390, 36],
    [450, 384, 62],
    [520, 391, 34]
  ]);
  return f;
}
FIGURES.push(buildStego());

// shared helpers ---------------------------------------------------------
function groundFor(f, cx, y, rx, ry = 15) {
  f.shape({
    name: 'ground',
    part: 'ground',
    z: 0,
    outline: false,
    casts: false,
    pts: ellipse(cx, y, rx, ry, 12),
    flat: 0.46,
    fade: 12
  });
}
function groundStrokes(f, list) {
  list.forEach(([x, y, l]) =>
    f.stroke({
      part: 'ground',
      z: 0,
      w: 0.55,
      wobble: 0.4,
      pts: [
        [x, y],
        [x + l / 2, y + 0.6],
        [x + l, y]
      ]
    })
  );
}

// TRIKE ------------------------------------------------------------------
// prettier-ignore
function buildTrike() {
  const f = new Figure({ name: 'trike', w: 600, h: 450, vy: 60, s: 3, seed: 33 });
  const HEAD = (pts) => place(pts, 44, 282, 4, 1.12);
  groundFor(f, 320, 421, 272, 15);
  // far legs
  f.shape({
    name: 'farlegs',
    part: 'body',
    z: 1,
    dark: 0.22,
    hatchBias: -0.1,
    goo: 1,
    prims: [
      {
        pts: tube(
          [
            [322, 332], [304, 380], [302, 414]
          ],
          [66, 52, 48]
        )
      },
      {
        pts: tube(
          [
            [304, 410], [290, 417], [274, 420]
          ],
          [46, 28, 12],
          { tip1: true }
        )
      },
      {
        pts: tube(
          [
            [462, 324], [454, 374], [458, 414]
          ],
          [92, 64, 54]
        )
      },
      {
        pts: tube(
          [
            [458, 410], [442, 417], [426, 420]
          ],
          [50, 30, 12],
          { tip1: true }
        )
      }
    ]
  });
  // torso + tail
  f.shape({
    name: 'torso',
    part: 'body',
    z: 2,
    lw: 1.2,
    goo: 2,
    vgrad: 0.2,
    prims: [
      {
        pts: [
          [292, 264], [322, 214], [388, 192], [458, 196], [516, 220], [550, 262], [554, 312],
          [520, 352], [446, 368], [370, 366], [318, 348], [292, 310]
        ]
      },
      {
        pts: tube(
          [
            [530, 258], [568, 294], [596, 340]
          ],
          [54, 30, 6],
          { tip1: true }
        )
      }
    ]
  });
  // frill: scalloped shield behind the skull
  const frill = [];
  const FC = [232, -26];
  const K = 11;
  for (let k = 0; k <= K; k++) {
    const a = ((-200 + k * (275 / K)) * Math.PI) / 180;
    const a2 = ((-200 + (k + 0.5) * (275 / K)) * Math.PI) / 180;
    frill.push([FC[0] + Math.cos(a) * 82, FC[1] + Math.sin(a) * 92]);
    if (k < K) frill.push([FC[0] + Math.cos(a2) * 90, FC[1] + Math.sin(a2) * 100]);
  }
  frill.push([210, 70], [172, 64]);
  f.shape({
    name: 'frill',
    part: 'head',
    z: 2.4,
    lw: 1.4,
    flat: 0.2,
    edge: -0.4,
    edgeW: 9,
    grad: [0.18, 0.28],
    hatch: 0,
    pts: HEAD(frill)
  });
  // frill ridges
  [
    [-40, -8], [-10, 12], [15, 22], [38, 12], [58, -6], [72, -30]
  ].forEach(([dx], i) => {
    const a = ((-120 + i * 36) * Math.PI) / 180;
    f.stroke({
      part: 'head',
      z: 2.5,
      w: 0.6,
      pts: HEAD([
        [176, 24],
        [FC[0] + Math.cos(a) * 44, FC[1] + Math.sin(a) * 50 + 10],
        [FC[0] + Math.cos(a) * 70 + dx * 0.1, FC[1] + Math.sin(a) * 78]
      ])
    });
  });
  f.spots({
    part: 'head',
    on: 'frill',
    n: 8,
    r: [2, 3.2],
    minDist: 24,
    where: (x, y, dv, d) => d > 12
  });
  // skull
  f.shape({
    name: 'skull',
    part: 'head',
    z: 2.6,
    lw: 1.4,
    vgrad: 0.12,
    pts: HEAD([
      [4, 70, 'c'], [0, 54], [10, 40], [30, 34], [56, 30], [90, 20], [124, 12], [152, 8], [178, 20],
      [188, 52], [176, 88], [146, 106], [108, 108], [74, 100], [44, 88], [22, 80]
    ])
  });
  const horn = (name, z, c, w) =>
    f.shape({
      name,
      part: 'head',
      z,
      lw: 1.3,
      accent: true,
      hatch: 0,
      flat: 0.34,
      grad: [0.1, 0.3],
      edge: 0.2,
      edgeW: 5,
      prims: [{ pts: HEAD(tube(c, w, { tip1: true })) }]
    });
  horn(
    'hornfar',
    2.5,
    [
      [150, 20], [126, -12], [98, -48]
    ],
    [34, 18, 3]
  );
  horn(
    'hornnear',
    2.7,
    [
      [130, 22], [100, -8], [68, -42]
    ],
    [38, 21, 3]
  );
  horn(
    'hornnose',
    2.7,
    [
      [62, 30], [52, 10], [42, -8]
    ],
    [30, 15, 3]
  );
  f.stroke({
    part: 'head',
    z: 2.8,
    w: 0.9,
    pts: HEAD([
      [2, 66], [30, 90], [74, 100], [118, 96], [152, 84]
    ])
  });
  f.stroke({
    part: 'head',
    z: 2.8,
    w: 0.7,
    pts: HEAD([
      [14, 50], [30, 62], [36, 84]
    ])
  });
  f.shape({
    part: 'head',
    z: 2.9,
    tone: false,
    lw: 1.3,
    casts: false,
    pts: HEAD(ellipse(124, 50, 8.5, 7, 8))
  });
  f.dot('head', ...HEAD([[124, 50.5]])[0], 3.4);
  f.dot('head', ...HEAD([[122, 48.5]])[0], 1.3, true);
  f.dot('head', ...HEAD([[46, 40]])[0], 1.9);
  f.stroke({
    part: 'head',
    z: 2.8,
    w: 0.7,
    pts: HEAD([
      [100, 36], [124, 33], [150, 38]
    ])
  });
  // near legs
  f.shape({
    name: 'frontleg',
    part: 'body',
    z: 3,
    lw: 1.2,
    goo: 1,
    skipOutline: [[290, 300, 420, 345]],
    prims: [
      {
        pts: tube(
          [
            [354, 328], [346, 380], [340, 414]
          ],
          [76, 58, 54]
        )
      },
      {
        pts: tube(
          [
            [342, 410], [326, 417], [310, 420]
          ],
          [52, 30, 12],
          { tip1: true }
        )
      }
    ]
  });
  f.shape({
    name: 'hindleg',
    part: 'body',
    z: 3,
    lw: 1.2,
    goo: 1,
    skipOutline: [[440, 270, 560, 340]],
    prims: [
      {
        pts: tube(
          [
            [498, 320], [494, 374], [500, 414]
          ],
          [102, 70, 60]
        )
      },
      {
        pts: tube(
          [
            [500, 410], [482, 417], [464, 420]
          ],
          [56, 32, 12],
          { tip1: true }
        )
      }
    ]
  });
  f.stroke({
    part: 'body',
    z: 3.1,
    w: 0.6,
    pts: [
      [352, 262], [374, 276], [382, 300]
    ]
  });
  f.stroke({
    part: 'body',
    z: 3.1,
    w: 0.6,
    pts: [
      [364, 246], [388, 258], [404, 282]
    ]
  });
  f.stroke({
    part: 'body',
    z: 3.1,
    w: 0.6,
    pts: [
      [452, 226], [474, 240], [482, 266]
    ]
  });
  f.stroke({
    part: 'body',
    z: 3.1,
    w: 0.6,
    pts: [
      [420, 332], [444, 342], [456, 362]
    ]
  });
  [
    [330, 416], [318, 418], [490, 416], [478, 418]
  ].forEach((p) => f.stroke({ part: 'body', z: 3.2, w: 0.6, pts: [p, [p[0] - 4, p[1] - 10]] }));
  f.spots({
    part: 'body',
    on: 'torso',
    n: 14,
    r: [1.6, 2.6],
    minDist: 20,
    where: (x, y, dv, d) => d > 10 && x > 410 && y < 320 && dv < 0.55
  });
  groundStrokes(f, [
    [70, 430, 50],
    [180, 435, 40],
    [270, 429, 66],
    [390, 434, 40],
    [470, 429, 60],
    [540, 435, 30]
  ]);
  return f;
}
FIGURES.push(buildTrike());

// BRACHIO ----------------------------------------------------------------
// prettier-ignore
function buildBrachio() {
  const f = new Figure({ name: 'brachio', w: 600, h: 400, vy: 28, s: 3, seed: 47 });
  const HEAD = (pts) => place(flipX(pts, 0), 116, 66, -14, 1.35);
  groundFor(f, 330, 379, 258, 13);
  // far hind leg
  f.shape({
    name: 'farleg',
    part: 'body',
    z: 1,
    dark: 0.22,
    hatchBias: -0.1,
    goo: 1,
    prims: [
      {
        pts: tube(
          [
            [462, 292], [456, 338], [460, 372]
          ],
          [80, 56, 50]
        )
      },
      {
        pts: tube(
          [
            [460, 368], [446, 375], [432, 378]
          ],
          [50, 30, 12],
          { tip1: true }
        )
      }
    ]
  });
  // body, neck, tail
  f.shape({
    name: 'torso',
    part: 'body',
    z: 2,
    lw: 1.2,
    goo: 2,
    prims: [
      {
        pts: [
          [306, 238], [344, 200], [416, 182], [488, 194], [530, 232], [532, 284], [490, 316],
          [420, 326], [350, 318], [314, 284]
        ]
      },
      {
        pts: tube(
          [
            [518, 232], [560, 258], [584, 296], [590, 336]
          ],
          [66, 40, 20, 5],
          { tip1: true }
        )
      },
      {
        pts: tube(
          [
            [334, 232], [292, 178], [244, 122], [190, 88], [146, 76], [124, 78]
          ],
          [82, 58, 42, 30, 24, 22]
        )
      }
    ]
  });
  f.shape({
    name: 'head',
    part: 'body',
    z: 2.2,
    lw: 1.2,
    vgrad: 0.1,
    skipOutline: [[100, 40, 190, 120]],
    pts: HEAD([
      [-6, 4], [4, -9], [22, -15], [44, -11], [62, -1], [68, 11], [58, 20], [34, 23], [10, 21],
      [-2, 14]
    ])
  });
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.8,
    pts: HEAD([
      [64, 13], [44, 18], [20, 18]
    ])
  });
  f.shape({
    part: 'body',
    z: 2.5,
    tone: false,
    lw: 1.1,
    casts: false,
    pts: HEAD(ellipse(32, -2, 5.6, 5, 8))
  });
  f.dot('body', ...HEAD([[32.5, -1.5]])[0], 2.3);
  f.dot('body', ...HEAD([[31, -3]])[0], 0.9, true);
  f.dot('body', ...HEAD([[60, 1]])[0], 1.3);
  // hind leg (near)
  f.shape({
    name: 'hindleg',
    part: 'body',
    z: 3,
    lw: 1.2,
    goo: 1,
    skipOutline: [[430, 190, 560, 270]],
    prims: [
      {
        pts: tube(
          [
            [494, 288], [488, 336], [494, 372]
          ],
          [92, 62, 54]
        )
      },
      {
        pts: tube(
          [
            [492, 368], [476, 375], [460, 378]
          ],
          [54, 32, 12],
          { tip1: true }
        )
      }
    ]
  });
  f.stroke({
    part: 'body',
    z: 3.1,
    w: 0.6,
    pts: [
      [452, 262], [472, 278], [486, 276]
    ]
  });
  // the chart: a spreadsheet panel standing in front
  const panel = [
    [34, 268, 'c'], [420, 262, 'c'], [424, 380, 'c'], [38, 386, 'c']
  ];
  f.shape({
    name: 'panel',
    part: 'chart',
    z: 5,
    pts: panel,
    flat: 0.06,
    edge: 0,
    lw: 1.5,
    casts: true,
    hatch: 0
  });
  for (let i = 1; i < 5; i++) {
    const y = 262 + i * 22.4;
    f.stroke({
      part: 'chart',
      z: 5,
      w: 0.45,
      wobble: 0.4,
      pts: [
        [36, y + 1.2],
        [230, y],
        [422, y - 0.8]
      ]
    });
  }
  for (let i = 1; i < 8; i++) {
    const x = 34 + i * 48.5;
    f.stroke({
      part: 'chart',
      z: 5,
      w: 0.4,
      wobble: 0.4,
      pts: [
        [x, 266 - i * 0.2],
        [x + 1, 330],
        [x + 2, 384]
      ]
    });
  }
  const baseY = 372;
  const hs = [28, 46, 38, 66, 52, 84, 70];
  hs.forEach((h, i) => {
    const x0 = 56 + i * 49;
    f.shape({
      name: 'bar' + i,
      part: 'chart',
      z: 5.2 + i * 0.001,
      accent: i % 2 === 0,
      lw: 1.3,
      pts: [
        [x0, baseY, 'c'],
        [x0, baseY - h, 'c'],
        [x0 + 32, baseY - h - 0.6, 'c'],
        [x0 + 32, baseY, 'c']
      ],
      flat: i % 2 === 0 ? 0.42 : 0.52,
      edge: 0.15,
      edgeW: 5,
      grad: [0.0, 0.3],
      hatch: 0
    });
  });
  f.stroke({
    part: 'chart',
    z: 5.3,
    w: 1.1,
    pts: [
      [48, 276],
      [48, baseY + 1, 'c'],
      [418, baseY + 1, 'c']
    ]
  });
  f.spots({
    part: 'body',
    on: 'torso',
    n: 12,
    r: [1.6, 2.6],
    minDist: 20,
    where: (x, y, dv, d) => d > 10 && y < 262 && x > 330 && dv < 0.55
  });
  groundStrokes(f, [
    [440, 389, 50],
    [510, 385, 40],
    [270, 392, 40],
    [130, 394, 60]
  ]);
  return f;
}
FIGURES.push(buildBrachio());

// PTERO ------------------------------------------------------------------
// prettier-ignore
function buildPtero() {
  const f = new Figure({ name: 'ptero', w: 600, h: 400, vy: 44, s: 3, seed: 59 });
  const HEAD = (pts) => place(flipX(pts, 0), 232, 206, -11, 1.12);
  const wingR0 = [
    [314, 188, 'c'], [352, 154], [404, 120, 'c'], [470, 106], [530, 116], [586, 146, 'c'],
    [556, 172], [518, 192], [470, 212], [420, 230], [372, 242], [336, 240, 'c']
  ];
  const WS = (pts) => scl(pts, 0.94, 300, 205);
  const wingR = WS(wingR0);
  const mir = (pts) => rot(flipX(WS(pts), 300), 5, 300, 205);
  const wing = (name, pts, part) =>
    f.shape({
      name,
      part,
      z: 1,
      pts,
      lw: 1.3,
      flat: 0.2,
      grad: [0.0, 0.26],
      edge: 0.1,
      edgeW: 6,
      hatch: 0,
      casts: false
    });
  wing('wingR', wingR, 'wingR');
  wing('wingL', mir(wingR0), 'wingL');
  const bone = (part, c, w, name) =>
    f.shape({
      name,
      part,
      z: 1.5,
      lw: 1.1,
      hatch: 0,
      hgrad: 0,
      vgrad: 0.1,
      prims: [{ pts: tube(c, w, { tip1: true }) }]
    });
  const arm = [
    [316, 188], [360, 150], [404, 120]
  ];
  const fin = [
    [404, 120], [470, 106], [530, 116], [586, 146]
  ];
  bone('wingR', WS(arm), [14, 11, 9], 'armR');
  bone('wingR', WS(fin), [8, 6, 5, 2], 'fingR');
  bone('wingL', mir(arm), [14, 11, 9], 'armL');
  bone('wingL', mir(fin), [8, 6, 5, 2], 'fingL');
  const ribs = [
    [
      [404, 120], [470, 150], [542, 176]
    ],
    [
      [404, 120], [446, 170], [486, 206]
    ],
    [
      [404, 120], [420, 180], [428, 226]
    ]
  ];
  ribs.forEach((c) => {
    f.stroke({ part: 'wingR', z: 1.6, w: 0.6, pts: WS(c) });
    f.stroke({ part: 'wingL', z: 1.6, w: 0.6, pts: mir(c) });
  });
  // body, neck, head
  f.shape({
    name: 'body',
    part: 'body',
    z: 2,
    lw: 1.3,
    goo: 2,
    prims: [
      { pts: ellipse(300, 212, 50, 28, 10, -8) },
      {
        pts: tube(
          [
            [274, 204], [256, 196], [236, 196]
          ],
          [34, 22, 16]
        )
      }
    ]
  });
  f.shape({
    name: 'head',
    part: 'body',
    z: 2.2,
    lw: 1.2,
    vgrad: 0.1,
    pts: HEAD([
      [-12, 2], [-4, -11], [14, -15], [40, -11], [80, -5], [122, 1, 'c'], [80, 6], [46, 12],
      [20, 18], [2, 18], [-10, 11]
    ])
  });
  f.shape({
    name: 'crest',
    part: 'body',
    z: 2.1,
    lw: 1.2,
    hatch: 0,
    prims: [
      {
        pts: HEAD(
          tube(
            [
              [4, -10], [-26, -24], [-62, -38]
            ],
            [18, 10, 2],
            { tip1: true }
          )
        )
      }
    ]
  });
  f.stroke({
    part: 'body',
    z: 2.3,
    w: 0.8,
    pts: HEAD([
      [112, 1], [76, 7], [36, 13], [6, 15]
    ])
  });
  f.dot('body', ...HEAD([[22, -2]])[0], 2.8);
  f.dot('body', ...HEAD([[20.8, -3.2]])[0], 1.0, true);
  // feet holding a page
  const pg = pagePieces(312, 308, 68, 86, 7, 0.2, 4);
  f.shape({
    name: 'page',
    part: 'page',
    z: 3,
    pts: pg.body,
    flat: 0.34,
    edge: 0.18,
    edgeW: 5,
    grad: [0.16, 0.22],
    accent: true,
    lw: 1.4,
    casts: true
  });
  f.stroke({ part: 'page', z: 3, w: 0.8, pts: pg.foldLine });
  pg.text.forEach((t) => f.stroke({ part: 'page', z: 3, w: 0.7, pts: t }));
  const leg = (name, c, w) =>
    f.shape({ name, part: 'body', z: 3.5, lw: 1.1, hatch: 0, prims: [{ pts: tube(c, w) }] });
  leg(
    'legL',
    [
      [292, 230], [288, 254], [292, 272]
    ],
    [18, 11, 9]
  );
  leg(
    'legR',
    [
      [320, 232], [326, 256], [328, 270]
    ],
    [18, 11, 9]
  );
  [
    [
      [284, 272], [292, 264], [302, 272]
    ],
    [
      [320, 270], [330, 262], [340, 270]
    ]
  ].forEach((c) => f.stroke({ part: 'body', z: 3.6, w: 0.9, pts: c }));
  return f;
}
FIGURES.push(buildPtero());

// EGG --------------------------------------------------------------------
// prettier-ignore
function buildEgg() {
  const f = new Figure({ name: 'egg', w: 400, h: 400, vy: 0, s: 3, seed: 71 });
  const cx = 196;
  const cy = 256;
  const rx = 86;
  const ry = 120;
  const eggPt = (t) => [cx + rx * Math.cos(t) * (1 + 0.14 * Math.sin(t)), cy + ry * Math.sin(t)];
  const crackY = 214;
  const sn = (crackY - cy) / ry;
  const tr = Math.asin(sn);
  const tl = Math.PI - tr;
  const arc = (a, b, n) => Array.from({ length: n + 1 }, (_, i) => eggPt(a + ((b - a) * i) / n));
  const L = eggPt(tl);
  const R = eggPt(tr);
  const zig = [[L[0], L[1], 'c']];
  const N = 9;
  for (let i = 1; i < N; i++) {
    zig.push([L[0] + ((R[0] - L[0]) * i) / N, crackY + (i % 2 ? -10 : 6), 'c']);
  }
  zig.push([R[0], R[1], 'c']);
  const ends = (a) => a.map((p, i) => (i === 0 || i === a.length - 1 ? [p[0], p[1], 'c'] : p));
  const lowerShell = [...ends(arc(tr, tl, 9)), ...zig.slice(1, -1).reverse()];
  const cap = [...ends(arc(tl, 2 * Math.PI + tr, 9)), ...zig.slice(1, -1).reverse()];
  const capMoved = mv(rot(cap, 24, R[0], R[1]), 52, -58);
  groundFor(f, 200, 376, 138, 13);
  // dark inside of the broken shell
  f.shape({
    name: 'inside',
    part: 'egg',
    z: 1,
    flat: 0.75,
    edge: 0,
    hatch: 0,
    outline: false,
    pts: ellipse(cx, crackY - 6, 76, 17, 10, -2)
  });
  // baby: neck + head
  f.shape({
    name: 'neck',
    part: 'baby',
    z: 1.5,
    lw: 1.3,
    vgrad: 0.1,
    prims: [
      {
        pts: tube(
          [
            [182, 250], [178, 224], [184, 196]
          ],
          [44, 38, 36]
        )
      }
    ]
  });
  const BH = (pts) => place(pts, 192, 160, -6);
  f.shape({
    name: 'bhead',
    part: 'baby',
    z: 1.7,
    lw: 1.4,
    vgrad: 0.12,
    pts: BH([
      [-40, 10], [-34, -14], [-12, -30], [18, -32], [44, -20], [62, -4], [66, 8, 'c'], [52, 18],
      [26, 26], [-8, 28]
    ])
  });
  [
    [-22, -28], [-4, -33], [14, -34]
  ].forEach(([x, y], i) =>
    f.shape({
      name: 'bump' + i,
      part: 'baby',
      z: 1.6,
      lw: 1.1,
      hatch: 0,
      prims: [
        {
          pts: BH(
            tube(
              [
                [x, y + 8],
                [x + 1, y - 7]
              ],
              [12, 2],
              { tip1: true }
            )
          )
        }
      ]
    })
  );
  f.stroke({
    part: 'baby',
    z: 1.9,
    w: 0.9,
    pts: BH([
      [64, 7], [46, 13], [22, 15]
    ])
  });
  f.shape({
    part: 'baby',
    z: 1.8,
    tone: false,
    lw: 1.3,
    casts: false,
    pts: BH(ellipse(14, -6, 10.5, 9.5, 8))
  });
  f.dot('baby', ...BH([[15, -5]])[0], 4.4);
  f.dot('baby', ...BH([[12.5, -8.5]])[0], 1.8, true);
  f.dot('baby', ...BH([[55, -3]])[0], 1.5);
  // lower shell with the zig-zag rim
  f.shape({ name: 'shell', part: 'egg', z: 2, lw: 1.4, pts: lowerShell, vgrad: 0.18 });
  // lifted cap
  f.shape({ name: 'cap', part: 'cap', z: 2.2, lw: 1.4, pts: capMoved, vgrad: 0.12 });
  // flying shards
  [
    [
      [54, 140], [72, 122], [82, 146]
    ],
    [
      [330, 176], [354, 166], [346, 192]
    ],
    [
      [300, 96], [316, 84], [314, 108]
    ]
  ].forEach((tri, i) =>
    f.shape({
      name: 'shard' + i,
      part: 'cap',
      z: 2.2,
      lw: 1.1,
      flat: 0.28,
      edge: -0.15,
      edgeW: 4,
      hatch: 0,
      pts: tri.map(([x, y]) => [x, y, 'c'])
    })
  );
  f.spots({
    part: 'egg',
    on: 'shell',
    n: 12,
    r: [2, 3.6],
    minDist: 22,
    where: (x, y, dv, d) => d > 8 && y > 236
  });
  f.spots({
    part: 'cap',
    on: 'cap',
    n: 5,
    r: [2, 3.2],
    minDist: 22,
    where: (x, y, dv, d) => d > 8
  });
  f.stroke({
    part: 'egg',
    z: 2.3,
    w: 0.8,
    pts: [
      [154, 228], [149, 246], [157, 262], [151, 282]
    ]
  });
  f.stroke({
    part: 'egg',
    z: 2.3,
    w: 0.7,
    pts: [
      [234, 226], [242, 244], [238, 258]
    ]
  });
  groundStrokes(f, [
    [70, 386, 60],
    [170, 391, 50],
    [250, 385, 70],
    [120, 397, 40]
  ]);
  return f;
}
FIGURES.push(buildEgg());

// TRACKS -----------------------------------------------------------------
// prettier-ignore
function buildTracks() {
  const f = new Figure({ name: 'tracks', w: 800, h: 100, vy: 0, s: 3, seed: 83 });
  const print = (name, x, y, size, deg, accent, flat) => {
    const a = (deg * Math.PI) / 180;
    const fx = Math.cos(a);
    const fy = Math.sin(a);
    const toe = (ang, len, wid) => {
      const b = a + (ang * Math.PI) / 180;
      const x0 = x + fx * size * 0.1;
      const y0 = y + fy * size * 0.1;
      const tx = x0 + Math.cos(b) * len * size;
      const ty = y0 + Math.sin(b) * len * size;
      // toe pad ...
      const pad = tube(
        [
          [x0, y0],
          [(x0 + tx) / 2, (y0 + ty) / 2],
          [tx, ty]
        ],
        [wid * size, wid * size * 0.92, wid * size * 0.62]
      );
      // ... and a small claw at the tip
      const cx = tx + Math.cos(b) * size * 0.2;
      const cy = ty + Math.sin(b) * size * 0.2;
      const claw = tube(
        [
          [tx - Math.cos(b) * 2, ty - Math.sin(b) * 2],
          [cx, cy]
        ],
        [wid * size * 0.5, 1.6],
        { tip1: true }
      );
      return [{ pts: pad }, { pts: claw }];
    };
    f.shape({
      name,
      part: 'prints',
      z: 1,
      accent,
      lw: 1.3,
      hatch: 0,
      flat,
      edge: -0.46,
      edgeW: 5,
      grad: [0.12, 0.12],
      goo: 1,
      prims: [
        ...toe(-38, 0.74, 0.24),
        ...toe(0, 0.92, 0.27),
        ...toe(38, 0.74, 0.24),
        {
          pts: ellipse(x - fx * size * 0.04, y - fy * size * 0.04, size * 0.2, size * 0.18, 8, deg)
        }
      ]
    });
  };
  const N = 7;
  for (let i = 0; i < N; i++) {
    const x = 62 + i * 108;
    const y = 50 + (i % 2 ? 12 : -12) + Math.sin(i * 0.9) * 4;
    const size = 58 + i * 1.5;
    print('p' + i, x, y, size, (i % 2 ? -8 : 8) + Math.sin(i) * 5, i === N - 1, 0.24 + i * 0.03);
  }
  const r = rng(91);
  for (let i = 0; i < 40; i++) {
    const x = 16 + r() * 768;
    const y = 8 + r() * 80;
    f.dot('prints', Math.round(x), Math.round(y), r() > 0.75 ? 1.7 : 1.0, false, 0.5);
  }
  [
    [24, 91, 140],
    [250, 93, 110],
    [460, 91, 150],
    [660, 93, 120]
  ].forEach(([x, y, l]) =>
    f.stroke({
      part: 'ground',
      z: 0,
      w: 0.55,
      wobble: 0.4,
      pts: [
        [x, y],
        [x + l / 2, y + 0.8],
        [x + l, y]
      ]
    })
  );
  return f;
}
FIGURES.push(buildTracks());

// FOSSIL -----------------------------------------------------------------
// prettier-ignore
function buildFossil() {
  const f = new Figure({ name: 'fossil', w: 600, h: 450, vy: 40, s: 3, seed: 97 });
  const X0 = 44;
  const X1 = 520;
  const bnd = (base, amp, ph, slope) => {
    const out = [];
    for (let k = 0; k <= 24; k++) {
      const x = X0 + ((X1 - X0) * k) / 24;
      out.push([
        x,
        base +
          amp * Math.sin(x / 41 + ph) +
          amp * 0.45 * Math.sin(x / 15 + ph * 2) +
          slope * (x - 280)
      ]);
    }
    return out;
  };
  const B = [
    bnd(92, 6, 0.4, 0.02),
    bnd(140, 7, 1.7, -0.03),
    bnd(190, 7, 3.1, 0.035),
    bnd(240, 6, 4.2, -0.02),
    bnd(326, 8, 0.9, 0.03),
    bnd(372, 7, 2.2, -0.025),
    bnd(414, 4, 5.0, 0.0)
  ];
  const tones = [0.52, 0.2, 0.36, 0.5, 0.26, 0.64];
  for (let i = 0; i < 6; i++) {
    const top = B[i];
    const bot = B[i + 1];
    const pts = [
      [top[0][0], top[0][1], 'c'],
      ...top.slice(1, -1),
      [top[top.length - 1][0], top[top.length - 1][1], 'c'],
      [bot[bot.length - 1][0], bot[bot.length - 1][1], 'c'],
      ...bot.slice(1, -1).reverse(),
      [bot[0][0], bot[0][1], 'c']
    ];
    f.shape({
      name: 'layer' + i,
      part: 'rock',
      z: 1 + i * 0.01,
      pts,
      flat: tones[i],
      edge: 0,
      hatch: 0,
      outline: false,
      accent: i === 3,
      grad: [0.04, 0.08],
      casts: false
    });
  }
  // boundaries and block sides, drawn once as pencil lines
  for (let i = 0; i < 7; i++)
    f.stroke({ part: 'rock', z: 1.5, w: i === 0 || i === 6 ? 1.2 : 0.8, single: true, pts: B[i] });
  f.stroke({
    part: 'rock',
    z: 1.5,
    w: 1.2,
    single: true,
    pts: [
      [X0, B[0][0][1]],
      [X0 - 1, 250],
      [X0, B[6][0][1]]
    ]
  });
  f.stroke({
    part: 'rock',
    z: 1.5,
    w: 1.2,
    single: true,
    pts: [
      [X1, B[0][B[0].length - 1][1]],
      [X1 + 1, 250],
      [X1, B[6][B[6].length - 1][1]]
    ]
  });
  const bandDots = (pts, off) => {
    for (let k = 0; k < pts.length - 1; k++) {
      const [a, b] = [pts[k], pts[k + 1]];
      for (let t = 0; t < 1; t += 0.34) {
        f.dot(
          'rock',
          Math.round(a[0] + (b[0] - a[0]) * t),
          Math.round(a[1] + (b[1] - a[1]) * t + off),
          0.9,
          false,
          1.5
        );
      }
    }
  };
  for (let i = 1; i < 6; i++) bandDots(B[i], 5);
  // pebbles and bedding dashes
  const rr = rng(1234);
  for (let i = 0; i < 22; i++) {
    const layer = Math.floor(rr() * 6);
    const x = X0 + 14 + rr() * (X1 - X0 - 56);
    const k = Math.max(0, Math.min(B[0].length - 1, Math.round(((x - X0) / (X1 - X0)) * 24)));
    const yTop = B[layer][k][1];
    const yBot = B[layer + 1][k][1];
    const y = yTop + 9 + rr() * Math.max(2, yBot - yTop - 18);
    if (rr() < 0.5) {
      f.stroke({
        part: 'rock',
        z: 1.4,
        w: 0.5,
        wobble: 0.3,
        pts: [
          [x, y],
          [x + 8, y + 0.5],
          [x + 18 + rr() * 8, y]
        ]
      });
    } else {
      f.stroke({
        part: 'rock',
        z: 1.4,
        w: 0.55,
        closed: true,
        wobble: 0.3,
        pts: ellipse(x, y, 3.5 + rr() * 3, 2.5 + rr() * 2, 6)
      });
    }
  }
  // grass tufts
  for (let x = X0 + 10; x < X1 - 4; x += 24 + (x % 7)) {
    const k = Math.round(((x - X0) / (X1 - X0)) * 24);
    const y = B[0][Math.min(k, B[0].length - 1)][1];
    f.stroke({
      part: 'rock',
      z: 1.4,
      w: 0.55,
      wobble: 0.2,
      pts: [
        [x, y],
        [x - 3, y - 9],
        [x - 6, y - 12]
      ]
    });
    f.stroke({
      part: 'rock',
      z: 1.4,
      w: 0.55,
      wobble: 0.2,
      pts: [
        [x + 2, y],
        [x + 3, y - 11],
        [x + 8, y - 14]
      ]
    });
  }
  // --- skeleton (facing left)
  const bone = (name, pts, o = {}) =>
    f.shape({
      name,
      part: 'bones',
      z: 3,
      lw: 1.2,
      flat: 0.1,
      edge: -0.2,
      edgeW: 5,
      grad: [0.14, 0.22],
      hatch: 0,
      casts: true,
      ...o,
      pts
    });
  const tubeBone = (name, c, w, o = {}) =>
    f.shape({
      name,
      part: 'bones',
      z: 3.05,
      lw: 1.1,
      hatch: 0,
      flat: 0.12,
      edge: -0.2,
      edgeW: 4,
      grad: [0.14, 0.2],
      ...o,
      prims: [{ pts: tube(c, w, { tip1: !!o.tip }) }]
    });
  const SK = (pts) => place(pts, 52, 250, 6, 0.8);
  const skull = [
    [-2, 36], [1, 22], [10, 10], [24, 4], [46, 5], [74, 3], [96, -6], [112, -15], [128, -10],
    [142, 2], [152, 20], [155, 42], [148, 62], [132, 78], [108, 84], [80, 78], [52, 68], [26, 56],
    [8, 46]
  ];
  bone('skull', SK(skull), { lw: 1.3 });
  const dark = (name, pts) =>
    f.shape({
      name,
      part: 'bones',
      z: 3.1,
      lw: 1.1,
      flat: 0.8,
      edge: 0,
      hatch: 0,
      casts: false,
      pts
    });
  dark('orbit', SK(ellipse(114, 12, 9.5, 8.5, 8)));
  dark(
    'antorb',
    SK([
      [70, 12, 'c'], [94, 8, 'c'], [84, 32, 'c']
    ])
  );
  dark('nostril', SK(ellipse(22, 14, 6, 4, 6)));
  f.stroke({
    part: 'bones',
    z: 3.2,
    w: 0.9,
    pts: SK([
      [4, 40], [34, 52], [70, 58], [104, 58], [134, 48]
    ])
  });
  for (let i = 0; i < 8; i++) {
    const tx = 12 + i * 12;
    const ty = 46 + i * 1.5 + (i > 5 ? 3 : 0);
    f.shape({
      name: 'tooth' + i,
      part: 'bones',
      z: 3.3,
      tone: false,
      lw: 0.9,
      wobble: 0.2,
      casts: false,
      pts: SK([
        [tx, ty, 'c'],
        [tx + 3.5, ty + 12, 'c'],
        [tx + 8, ty, 'c']
      ])
    });
  }
  // vertebral column
  const spine = [];
  for (let i = 0; i < 14; i++) {
    const t = i / 13;
    const x = 184 + t * 240;
    const y = 262 - Math.sin(Math.min(1, t * 1.15) * Math.PI * 0.62) * 34 + t * t * 26;
    spine.push([x, y]);
  }
  spine.forEach(([x, y], i) => {
    f.shape({
      name: 'vert' + i,
      part: 'bones',
      z: 3 + i * 0.001,
      tone: false,
      lw: 1.1,
      casts: true,
      pts: ellipse(x, y, 10, 12, 7, (i - 7) * 3)
    });
    if (i % 2 === 0)
      f.stroke({
        part: 'bones',
        z: 3.2,
        w: 0.7,
        pts: [
          [x - 2, y - 11],
          [x - 4, y - 24]
        ]
      });
  });
  // ribs
  for (let i = 0; i < 7; i++) {
    const [sx, sy] = spine[3 + i];
    const len = 54 + Math.sin(i * 0.7) * 12;
    tubeBone(
      'rib' + i,
      [
        [sx, sy + 8],
        [sx - 10, sy + 8 + len * 0.5],
        [sx - 2, sy + 8 + len]
      ],
      [9, 7, 3],
      {
        z: 2.9 + i * 0.001,
        tip: true,
        flat: 0.12,
        edgeW: 3
      }
    );
  }
  // pelvis, hind limb
  bone('ilium', [
    [404, 230], [440, 218], [470, 234], [466, 264], [440, 278], [412, 268]
  ]);
  tubeBone(
    'ischium',
    [
      [440, 268], [452, 290], [470, 312]
    ],
    [16, 10, 5],
    { tip: true }
  );
  tubeBone(
    'femur',
    [
      [434, 258], [418, 288], [406, 314]
    ],
    [24, 15, 13],
    { lw: 1.2 }
  );
  tubeBone(
    'tibia',
    [
      [406, 314], [418, 340], [414, 356]
    ],
    [15, 10, 12],
    { lw: 1.2 }
  );
  [
    [414, 354], [400, 358], [386, 360]
  ].forEach((p, i) =>
    tubeBone('toe' + i, [p, [p[0] - 10, p[1] + 3]], [8, 3], { tip: true, tone: false })
  );
  // tail vertebrae
  for (let i = 0; i < 11; i++) {
    const t = i / 10;
    const x = 448 + t * 56;
    const y = 252 + Math.pow(t, 1.4) * 46;
    const sz = 1 - t * 0.7;
    f.shape({
      name: 'tv' + i,
      part: 'bones',
      z: 3 + i * 0.001,
      tone: false,
      lw: 1.0,
      casts: false,
      pts: ellipse(x, y, 8.5 * sz + 1.5, 10 * sz + 1.5, 6, 20)
    });
  }
  tubeBone(
    'arm',
    [
      [238, 262], [230, 290], [210, 302]
    ],
    [14, 10, 6],
    { tip: true }
  );
  // leader lines and label chips (dotted bands)
  const labelY = [B[1], B[2], B[3], B[4], B[5]].map((b) => b[b.length - 1][1]);
  labelY.forEach((y, i) => {
    for (let x = 530; x < 566; x += 6) f.dot('rock', x, Math.round(y), 1.0, false, 1.5);
    const acc = i === 2;
    f.shape({
      name: 'chip' + i,
      part: 'labels',
      z: 6,
      accent: acc,
      lw: 1.1,
      hatch: 0,
      flat: acc ? 0.4 : 0.08,
      edge: 0,
      pts: [
        [568, y - 7, 'c'],
        [596, y - 7, 'c'],
        [596, y + 7, 'c'],
        [568, y + 7, 'c']
      ]
    });
    f.stroke({
      part: 'labels',
      z: 6,
      w: 0.6,
      pts: [
        [573, y - 1.5],
        [589, y - 1.5]
      ]
    });
    f.stroke({
      part: 'labels',
      z: 6,
      w: 0.6,
      pts: [
        [573, y + 2.5],
        [583, y + 2.5]
      ]
    });
  });
  return f;
}
FIGURES.push(buildFossil());

// -------------------------------------------------------------- svelte out

function svelteFor(fig, markup) {
  const name = fig.name;
  return `<!-- Generated by scripts/gen-dino-art.mjs (${name}) - do not edit by hand. -->
<script lang="ts">
  type Props = {
    class?: string;
    role?: 'img' | 'presentation' | 'none';
    'aria-hidden'?: 'true' | 'false' | boolean;
    'aria-label'?: string;
  };
  let { class: klass, role, 'aria-hidden': ariaHidden, 'aria-label': ariaLabel }: Props = $props();
</script>

<svg
  class={klass}
  {role}
  aria-hidden={ariaHidden}
  aria-label={ariaLabel}
  viewBox="0 ${fig.vy} ${fig.w} ${fig.h - fig.vy}"
  fill="none"
  stroke="currentColor"
  stroke-linecap="round"
  stroke-linejoin="round"
  focusable="false"
>
${markup}
</svg>

<style>
  .a {
    stroke: var(--art-accent, #1f7a4d);
  }
</style>
`;
}

const only = process.argv.slice(2).filter((a) => !a.startsWith('--'));
const noFormat = process.argv.includes('--no-format');
mkdirSync(OUT, { recursive: true });
const written = [];
for (const fig of FIGURES) {
  if (only.length && !only.includes(fig.name)) continue;
  const t0 = Date.now();
  const { markup, dotCount, stats } = compile(fig);
  const file = join(OUT, fig.name[0].toUpperCase() + fig.name.slice(1) + '.svelte');
  writeFileSync(file, svelteFor(fig, markup));
  written.push(file);
  console.log(
    `${fig.name}: ${(markup.length / 1024).toFixed(1)} KB, ${dotCount} dots, ${Date.now() - t0} ms  [` +
      Object.entries(stats)
        .map(([k, v]) => `${k} ${(v / 1024).toFixed(1)}`)
        .join(', ') +
      ']'
  );
}

if (!noFormat && written.length) {
  const require = createRequire(import.meta.url);
  const bin = require.resolve('prettier/bin/prettier.cjs');
  const r = spawnSync(process.execPath, [bin, '--write', ...written], {
    cwd: ROOT,
    encoding: 'utf8'
  });
  if (r.status !== 0) {
    console.error(r.stdout, r.stderr);
    process.exit(r.status ?? 1);
  }
}
