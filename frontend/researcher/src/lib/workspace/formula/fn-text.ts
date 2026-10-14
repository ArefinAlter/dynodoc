import { E, isError } from './errors';
import { formatWithPattern } from './numfmt';
import { wildcardRegExp } from './criteria';
import { roundDigits } from './fn-math';
import { operand, type EvalContext } from './evaluator';
import { ArrayValue, parseNumberText, toText, type Scalar, type Value } from './values';
import {
  def,
  defScalar,
  int,
  intOr,
  num,
  numOr,
  str,
  strOr,
  boolOr,
  isMissing,
  walkArgs
} from './fn-util';

const MAX_TEXT = 32767;
const fit = (s: string) => (s.length > MAX_TEXT ? E.VALUE : s);

defScalar('LEN', 1, 1, (a) => str(a[0]).length);
defScalar('UPPER', 1, 1, (a) => str(a[0]).toUpperCase());
defScalar('LOWER', 1, 1, (a) => str(a[0]).toLowerCase());
// Excel capitalises a letter that follows a non-letter, including digits.
defScalar('PROPER', 1, 1, (a) => {
  const s = str(a[0]);
  let out = '';
  let prevLetter = false;
  for (const ch of s) {
    const letter = /\p{L}/u.test(ch);
    out += letter ? (prevLetter ? ch.toLowerCase() : ch.toUpperCase()) : ch;
    prevLetter = letter;
  }
  return out;
});
defScalar('TRIM', 1, 1, (a) => str(a[0]).replace(/ +/g, ' ').replace(/^ | $/g, ''));
defScalar('CLEAN', 1, 1, (a) => [...str(a[0])].filter((ch) => ch.charCodeAt(0) >= 32).join(''));

const CP1252 = '€\u0081‚ƒ„…†‡ˆ‰Š‹Œ\u008dŽ\u008f\u0090‘’“”•–—˜™š›œ\u009džŸ';
defScalar('CHAR', 1, 1, (a, ctx) => {
  const n = Math.trunc(num(a[0], ctx));
  if (n < 1 || n > 255) return E.VALUE;
  return n >= 128 && n <= 159 ? CP1252[n - 128] : String.fromCharCode(n);
});
defScalar('CODE', 1, 1, (a) => {
  const s = str(a[0]);
  if (!s) return E.VALUE;
  const i = CP1252.indexOf(s[0]);
  return i >= 0 ? 128 + i : s.charCodeAt(0);
});
defScalar('UNICHAR', 1, 1, (a, ctx) => {
  const n = Math.trunc(num(a[0], ctx));
  if (n < 1 || n > 0x10ffff || (n >= 0xd800 && n <= 0xdfff)) return E.VALUE;
  return String.fromCodePoint(n);
});
defScalar('UNICODE', 1, 1, (a) => {
  const s = str(a[0]);
  return s ? s.codePointAt(0)! : E.VALUE;
});

defScalar('LEFT', 1, 2, (a, ctx) => {
  const n = intOr(a[1], ctx, 1);
  if (n < 0) return E.VALUE;
  return str(a[0]).slice(0, n);
});
defScalar('RIGHT', 1, 2, (a, ctx) => {
  const n = intOr(a[1], ctx, 1);
  if (n < 0) return E.VALUE;
  const s = str(a[0]);
  return n === 0 ? '' : s.slice(-n);
});
defScalar('MID', 3, 3, (a, ctx) => {
  const start = int(a[1], ctx),
    len = int(a[2], ctx);
  if (start < 1 || len < 0) return E.VALUE;
  return str(a[0]).substr(start - 1, len);
});
defScalar('REPT', 2, 2, (a, ctx) => {
  const n = int(a[1], ctx);
  if (n < 0) return E.VALUE;
  const s = str(a[0]);
  if (s.length * n > MAX_TEXT) return E.VALUE;
  return s.repeat(n);
});
defScalar('REPLACE', 4, 4, (a, ctx) => {
  const s = str(a[0]);
  const start = int(a[1], ctx),
    n = int(a[2], ctx);
  if (start < 1 || n < 0) return E.VALUE;
  return fit(s.slice(0, start - 1) + str(a[3]) + s.slice(start - 1 + n));
});
defScalar('SUBSTITUTE', 3, 4, (a, ctx) => {
  const text = str(a[0]),
    from = str(a[1]),
    to = str(a[2]);
  if (from === '') return text;
  if (isMissing(a[3])) return fit(text.split(from).join(to));
  const which = int(a[3], ctx);
  if (which < 1) return E.VALUE;
  let idx = -1;
  for (let i = 0; i < which; i++) {
    idx = text.indexOf(from, idx + 1 + (i ? from.length - 1 : 0));
    if (idx < 0) return text;
  }
  return fit(text.slice(0, idx) + to + text.slice(idx + from.length));
});
defScalar('FIND', 2, 3, (a, ctx) => {
  const needle = str(a[0]),
    hay = str(a[1]);
  const start = intOr(a[2], ctx, 1);
  if (start < 1 || start > hay.length) return E.VALUE;
  const i = hay.indexOf(needle, start - 1);
  return i < 0 ? E.VALUE : i + 1;
});
defScalar('SEARCH', 2, 3, (a, ctx) => {
  const needle = str(a[0]),
    hay = str(a[1]);
  const start = intOr(a[2], ctx, 1);
  if (start < 1 || start > hay.length) return E.VALUE;
  const re = wildcardRegExp(needle);
  if (!re) {
    const i = hay.toLowerCase().indexOf(needle.toLowerCase(), start - 1);
    return i < 0 ? E.VALUE : i + 1;
  }
  const body = new RegExp(re.source.slice(1, -1), 'i');
  const m = body.exec(hay.slice(start - 1));
  return m ? m.index + start : E.VALUE;
});
defScalar('EXACT', 2, 2, (a) => str(a[0]) === str(a[1]));

// ---------------------------------------------------------------- joining

def('CONCAT', 1, 255, (a, ctx) => {
  let out = '';
  let err = null as Scalar;
  walkArgs(
    a,
    ctx,
    (v) => {
      if (isError(v)) err ??= v;
      else out += toText(v) as string;
      if (out.length > MAX_TEXT) err ??= E.VALUE;
    },
    { includeBlank: false }
  );
  return err ?? out;
});
defScalar('CONCATENATE', 1, 255, (a) => {
  let out = '';
  for (const v of a) out += str(v);
  return fit(out);
});
def('TEXTJOIN', 3, 255, (a, ctx) => {
  const delimiters: string[] = [];
  walkArgs([a[0]], ctx, (v) => delimiters.push(String(toText(v))), { includeBlank: true });
  if (!delimiters.length) delimiters.push('');
  const ignoreEmpty = boolOr(operand(a[1]) as Scalar, false);
  const parts: string[] = [];
  let err: Scalar = null;
  walkArgs(
    a.slice(2),
    ctx,
    (v) => {
      if (isError(v)) {
        err ??= v;
        return;
      }
      const t = toText(v) as string;
      if (ignoreEmpty && t === '') return;
      parts.push(t);
    },
    { includeBlank: !ignoreEmpty }
  );
  if (err) return err;
  let out = '';
  parts.forEach((p, i) => {
    if (i) out += delimiters[(i - 1) % delimiters.length];
    out += p;
  });
  return fit(out);
});

// ---------------------------------------------------------- number <-> text

defScalar('VALUE', 1, 1, (a, ctx) => {
  const v = a[0];
  if (typeof v === 'number') return v;
  if (typeof v === 'boolean') return E.VALUE;
  if (v === null || v === undefined) return 0;
  const n = parseNumberText(String(v), ctx.coerce);
  return n === null ? E.VALUE : n;
});
defScalar('NUMBERVALUE', 1, 3, (a) => {
  const dec = strOr(a[1], '.');
  const grp = strOr(a[2], ',');
  let s = str(a[0]).replace(/\s+/g, '');
  if (s === '') return 0;
  if (grp) s = s.split(grp).join('');
  let pct = 0;
  s = s.replace(/%+$/, (m) => {
    pct = m.length;
    return '';
  });
  if (dec !== '.') s = s.split(dec).join('.');
  if (!/^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(s)) return E.VALUE;
  return Number(s) / 100 ** pct;
});
defScalar('TEXT', 2, 2, (a, ctx) => {
  let v = a[0];
  const format = str(a[1]);
  if (v === null || v === undefined) v = 0;
  if (typeof v === 'string') {
    const n = /\d/.test(v) ? parseNumberText(v, ctx.coerce) : null;
    if (n !== null) v = n;
  }
  return fit(formatWithPattern(v as number | string | boolean, format));
});
function currency(n: number, decimals: number, symbol: string, commas: boolean) {
  const d = Math.trunc(decimals);
  const rounded = roundDigits(n, d);
  const abs = Math.abs(rounded);
  const fixed = abs.toFixed(Math.max(0, Math.min(d, 20)));
  const [whole, frac = ''] = fixed.split('.');
  let int = d < 0 ? String(abs) : whole;
  if (commas) int = int.replace(/\B(?=(\d{3})+$)/g, ',');
  const body = symbol + int + (frac ? '.' + frac : '');
  return rounded < 0 ? (symbol ? `(${body})` : '-' + body) : body;
}
defScalar('DOLLAR', 1, 2, (a, ctx) => currency(num(a[0], ctx), numOr(a[1], ctx, 2), '$', true));
defScalar('FIXED', 1, 3, (a, ctx) =>
  currency(num(a[0], ctx), numOr(a[1], ctx, 2), '', !boolOr(a[2], false))
);
defScalar('T', 1, 1, (a) => (typeof a[0] === 'string' ? a[0] : ''), { passErrors: false });

// ------------------------------------------------------- split and slice

function delimiterList(v: Value, ctx: EvalContext): string[] {
  const out: string[] = [];
  walkArgs([v], ctx, (x) => out.push(String(toText(x))), { includeBlank: true });
  return out;
}
function findDelimiter(
  text: string,
  delims: string[],
  from: number,
  ci: boolean
): { index: number; length: number } | null {
  let best: { index: number; length: number } | null = null;
  const hay = ci ? text.toLowerCase() : text;
  for (const d of delims) {
    if (d === '') continue;
    const i = hay.indexOf(ci ? d.toLowerCase() : d, from);
    if (i >= 0 && (!best || i < best.index || (i === best.index && d.length > best.length)))
      best = { index: i, length: d.length };
  }
  return best;
}
function beforeAfter(after: boolean) {
  return (a: Value[], ctx: EvalContext): Scalar => {
    const text = str(a[0]);
    const delims = delimiterList(a[1], ctx);
    const instance = intOr(a[2], ctx, 1);
    const ci = intOr(a[3], ctx, 0) === 1;
    const matchEnd = intOr(a[4], ctx, 0) === 1;
    const notFound = isMissing(a[5]) ? E.NA : (operand(a[5]) as Scalar);
    if (instance === 0 || Math.abs(instance) > text.length + 1) return E.VALUE;
    if (delims.every((d) => d === '')) return after ? text : '';
    const positions: { index: number; length: number }[] = [];
    let from = 0;
    for (;;) {
      const f = findDelimiter(text, delims, from, ci);
      if (!f) break;
      positions.push(f);
      from = f.index + Math.max(1, f.length);
    }
    let chosen: { index: number; length: number } | undefined;
    if (instance > 0) chosen = positions[instance - 1];
    else chosen = positions[positions.length + instance];
    if (!chosen) {
      if (matchEnd && Math.abs(instance) === positions.length + 1)
        chosen = instance > 0 ? { index: text.length, length: 0 } : { index: 0, length: 0 };
      else return notFound;
    }
    return after ? text.slice(chosen.index + chosen.length) : text.slice(0, chosen.index);
  };
}
def('TEXTBEFORE', 2, 6, beforeAfter(false));
def('TEXTAFTER', 2, 6, beforeAfter(true));
def('TEXTSPLIT', 2, 6, (a, ctx) => {
  const text = str(a[0]);
  const colDelims = delimiterList(a[1], ctx).filter((d) => d !== '');
  const rowDelims = isMissing(a[2]) ? [] : delimiterList(a[2], ctx).filter((d) => d !== '');
  const ignoreEmpty = boolOr(operand(a[3]) as Scalar, false);
  const ci = intOr(a[4], ctx, 0) === 1;
  const pad = isMissing(a[5]) ? E.NA : (operand(a[5]) as Scalar);
  const split = (s: string, delims: string[]): string[] => {
    if (!delims.length) return [s];
    const out: string[] = [];
    let from = 0;
    for (;;) {
      const f = findDelimiter(s, delims, from, ci);
      if (!f) break;
      out.push(s.slice(from, f.index));
      from = f.index + f.length;
    }
    out.push(s.slice(from));
    return out;
  };
  let rows = split(text, rowDelims).map((r) => split(r, colDelims));
  if (ignoreEmpty) rows = rows.map((r) => r.filter((c) => c !== '')).filter((r) => r.length);
  const width = rows.reduce((m, r) => Math.max(m, r.length), 0);
  if (!rows.length || !width) return E.CALC;
  return ArrayValue.of(rows.length, width, (r, c) => (c < rows[r].length ? rows[r][c] : pad));
});
// HyperFormula's SPLIT(text, index): zero-based space-separated word.
defScalar('SPLIT', 2, 2, (a, ctx) => {
  const parts = str(a[0]).split(' ');
  const i = int(a[1], ctx);
  if (i < 0 || i >= parts.length) return E.VALUE;
  return parts[i];
});
