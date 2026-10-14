import { DAYS, MONTHS, serialToYmd, weekdayOf } from './dates';
import { numberToText } from './values';

/** Excel number-format engine for TEXT(): numbers, percentages, scientific, fractions, dates. */

type Tok =
  | { k: 'lit'; v: string }
  | { k: 'digit'; ch: '0' | '#' | '?' }
  | { k: 'dec' }
  | { k: 'comma' }
  | { k: 'pct' }
  | { k: 'exp'; sign: '+' | '-' }
  | { k: 'slash' }
  | { k: 'text' }
  | { k: 'date'; v: string };

interface Section {
  toks: Tok[];
  cond?: { op: string; value: number };
}

function splitSections(fmt: string): string[] {
  const out: string[] = [];
  let cur = '';
  let quoted = false;
  for (let i = 0; i < fmt.length; i++) {
    const ch = fmt[i];
    if (ch === '"') quoted = !quoted;
    if (ch === '\\' && !quoted && i + 1 < fmt.length) {
      cur += ch + fmt[++i];
      continue;
    }
    if (ch === ';' && !quoted) {
      out.push(cur);
      cur = '';
      continue;
    }
    cur += ch;
  }
  out.push(cur);
  return out;
}

const DATE_TOKEN =
  /^(?:yyyy|yy|mmmmm|mmmm|mmm|mm|m|dddd|ddd|dd|d|hh|h|ss|s|AM\/PM|am\/pm|A\/P|a\/p)/;

function parseSection(src: string): Section {
  const toks: Tok[] = [];
  let cond: Section['cond'];
  let i = 0;
  const lit = (v: string) => toks.push({ k: 'lit', v });
  while (i < src.length) {
    const ch = src[i];
    if (ch === '"') {
      const end = src.indexOf('"', i + 1);
      const text = end < 0 ? src.slice(i + 1) : src.slice(i + 1, end);
      lit(text);
      i = end < 0 ? src.length : end + 1;
      continue;
    }
    if (ch === '\\') {
      lit(src[i + 1] ?? '');
      i += 2;
      continue;
    }
    if (ch === '_') {
      lit(' ');
      i += 2;
      continue;
    }
    if (ch === '*') {
      i += 2;
      continue;
    }
    if (ch === '[') {
      const end = src.indexOf(']', i);
      const body = end < 0 ? src.slice(i + 1) : src.slice(i + 1, end);
      i = end < 0 ? src.length : end + 1;
      const elapsed = /^(h+|m+|s+)$/i.exec(body);
      if (elapsed) {
        toks.push({ k: 'date', v: '[' + elapsed[1][0].toLowerCase() + ']' });
        continue;
      }
      const c = /^(<=|>=|<>|<|>|=)\s*(-?[\d.]+)$/.exec(body);
      if (c) {
        cond = { op: c[1], value: Number(c[2]) };
        continue;
      }
      const cur = /^\$([^-]*)(?:-.*)?$/.exec(body);
      if (cur) {
        if (cur[1]) lit(cur[1]);
        continue;
      }
      continue; // colors, locale ids
    }
    const rest = src.slice(i);
    if (/^general/i.test(rest)) {
      toks.push({ k: 'date', v: 'General' });
      i += 7;
      continue;
    }
    const dm = DATE_TOKEN.exec(rest);
    if (dm) {
      toks.push({ k: 'date', v: dm[0] });
      i += dm[0].length;
      continue;
    }
    if (ch === '0' || ch === '#' || ch === '?') {
      toks.push({ k: 'digit', ch });
    } else if (ch === '.') toks.push({ k: 'dec' });
    else if (ch === ',') toks.push({ k: 'comma' });
    else if (ch === '%') toks.push({ k: 'pct' });
    else if (ch === '/') toks.push({ k: 'slash' });
    else if (ch === '@') toks.push({ k: 'text' });
    else if ((ch === 'E' || ch === 'e') && (src[i + 1] === '+' || src[i + 1] === '-')) {
      toks.push({ k: 'exp', sign: src[i + 1] as '+' | '-' });
      i += 2;
      continue;
    } else lit(ch);
    i++;
  }
  return { toks, cond };
}

function holds(cond: NonNullable<Section['cond']>, n: number) {
  switch (cond.op) {
    case '<':
      return n < cond.value;
    case '>':
      return n > cond.value;
    case '<=':
      return n <= cond.value;
    case '>=':
      return n >= cond.value;
    case '<>':
      return n !== cond.value;
    default:
      return n === cond.value;
  }
}

/** Plain decimal text of |n| rounded half away from zero to `digits` places. */
export function fixed(abs: number, digits: number): string {
  if (abs === 0) return digits > 0 ? '0.' + '0'.repeat(digits) : '0';
  const [mant, exps] = abs.toExponential(14).split('e');
  const exp = Number(exps);
  const d = mant.replace('.', ''); // 15 significant digits
  // value = 0.d × 10^(exp+1)
  let int: string, frac: string;
  if (exp >= 0) {
    int = d.length <= exp + 1 ? d + '0'.repeat(exp + 1 - d.length) : d.slice(0, exp + 1);
    frac = d.length <= exp + 1 ? '' : d.slice(exp + 1);
  } else {
    int = '0';
    frac = '0'.repeat(-exp - 1) + d;
  }
  if (frac.length <= digits) frac = frac.padEnd(digits, '0');
  else {
    const up = frac.charCodeAt(digits) >= 53; // '5'
    frac = frac.slice(0, digits);
    if (up) {
      // propagate carry through int+frac
      const all = (int + frac).split('');
      let i = all.length - 1;
      for (; i >= 0; i--) {
        if (all[i] === '9') all[i] = '0';
        else {
          all[i] = String(Number(all[i]) + 1);
          break;
        }
      }
      let joined = all.join('');
      if (i < 0) joined = '1' + joined;
      int = joined.slice(0, joined.length - digits) || '0';
      frac = joined.slice(joined.length - digits);
    }
  }
  int = int.replace(/^0+(?=\d)/, '');
  return digits > 0 ? `${int}.${frac}` : int;
}

function group(digits: string): string {
  return digits.replace(/\B(?=(\d{3})+$)/g, ',');
}

function numberSection(abs: number, toks: Tok[]): string {
  let pct = 0;
  for (const t of toks) if (t.k === 'pct') pct++;
  if (pct) abs *= 100 ** pct;

  const hasExp = toks.some((t) => t.k === 'exp');
  const slashAt = toks.findIndex((t) => t.k === 'slash');
  const decAt = toks.findIndex((t) => t.k === 'dec');
  // Thousands handling: commas between digit placeholders group; trailing commas scale.
  const digitIdx = toks.map((t, i) => (t.k === 'digit' ? i : -1)).filter((i) => i >= 0);
  const firstDigit = digitIdx[0] ?? -1;
  const lastDigit = digitIdx.length ? digitIdx[digitIdx.length - 1] : -1;
  let grouping = false;
  let scale = 0;
  toks.forEach((t, i) => {
    if (t.k !== 'comma') return;
    if (firstDigit >= 0 && i > firstDigit && i < lastDigit) grouping = true;
    else if (firstDigit >= 0 && i > lastDigit && (decAt < 0 || i < decAt || decAt < lastDigit))
      scale++;
  });
  abs /= 1000 ** scale;

  // Fractions "# ?/?" and "?/8".
  if (slashAt >= 0 && !hasExp) return fractionSection(abs, toks, slashAt);

  const intSlots: ('0' | '#' | '?')[] = [];
  const fracSlots: ('0' | '#' | '?')[] = [];
  let seenDec = false;
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i];
    if (t.k === 'dec' && i === decAt) seenDec = true;
    else if (t.k === 'digit') (seenDec ? fracSlots : intSlots).push(t.ch);
    if (t.k === 'exp') break;
  }

  let expText = '';
  if (hasExp) {
    const expTok = toks.findIndex((t) => t.k === 'exp');
    const expDigits = toks.slice(expTok + 1).filter((t) => t.k === 'digit').length || 1;
    const sign = (toks[expTok] as { sign: '+' | '-' }).sign;
    let e = 0;
    if (abs !== 0) {
      e = Math.floor(Math.log10(abs));
      if (intSlots.length > 1) e -= intSlots.length - 1;
      let m = abs / 10 ** e;
      // rounding may carry into the next power of ten
      if (Number(fixed(m, fracSlots.length)) >= 10 ** Math.max(1, intSlots.length)) {
        e += 1;
        m = abs / 10 ** e;
      }
      abs = m;
    }
    expText =
      'E' + (e < 0 ? '-' : sign === '+' ? '+' : '') + String(Math.abs(e)).padStart(expDigits, '0');
  }

  const text = fixed(abs, fracSlots.length);
  const [intDigitsRaw, fracDigitsRaw = ''] = text.split('.');
  const zeroInt = /^0+$/.test(intDigitsRaw);
  const intDigits = zeroInt ? '' : intDigitsRaw;
  // Fraction digits: drop trailing zeros that only '#'/'?' placeholders cover.
  const fracOut: string[] = fracDigitsRaw.split('');
  for (let j = fracSlots.length - 1; j >= 0; j--) {
    if (fracSlots[j] !== '0' && fracOut[j] === '0') fracOut[j] = fracSlots[j] === '?' ? ' ' : '';
    else break;
  }
  // Integer digits fill placeholders right-to-left; overflow joins the leftmost.
  const slots = intSlots.length;
  const intFilled: string[] = new Array(slots).fill('');
  let overflow = '';
  if (slots > 0) {
    const d = intDigits;
    for (let j = 0; j < slots; j++) {
      const idx = d.length - 1 - j;
      const slot = intSlots[slots - 1 - j];
      let ch = '';
      if (idx >= 0) ch = d[idx];
      else if (slot === '0') ch = '0';
      else if (slot === '?') ch = ' ';
      intFilled[slots - 1 - j] = ch;
    }
    if (d.length > slots) overflow = d.slice(0, d.length - slots);
  }
  if (grouping) {
    const joined = overflow + intFilled.join('');
    const lead = /^ */.exec(joined)![0];
    const body = joined.slice(lead.length);
    // group only the digit run
    const m = /^(\d+)(.*)$/.exec(body);
    const grouped = m ? group(m[1]) + m[2] : body;
    overflow = lead + grouped;
    intFilled.fill('');
  }

  // Assemble following token order.
  let out = '';
  let slotIdx = 0;
  let fracIdx = 0;
  let afterDec = false;
  let leadingEmitted = false;
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i];
    switch (t.k) {
      case 'lit':
        out += t.v;
        break;
      case 'pct':
        out += '%';
        break;
      case 'dec':
        if (i === decAt) {
          afterDec = true;
          out += '.';
        }
        break;
      case 'digit':
        if (!afterDec) {
          if (!leadingEmitted) {
            out += overflow;
            leadingEmitted = true;
          }
          out += intFilled[slotIdx++] ?? '';
        } else out += fracOut[fracIdx++] ?? '';
        break;
      case 'exp':
        out += expText;
        i = toks.length; // exponent digits are consumed
        break;
      default:
        break;
    }
  }
  if (slots === 0 && intDigits && decAt < 0 && fracSlots.length === 0 && !hasExp && !out.length)
    out = intDigits;
  return out;
}

function fractionSection(abs: number, toks: Tok[], slashAt: number): string {
  // numerator placeholders before the slash (after the integer part), denominator after.
  let denomDigits = 0;
  let fixedDenom = '';
  for (let i = slashAt + 1; i < toks.length; i++) {
    const t = toks[i];
    if (t.k === 'digit' && t.ch === '?') denomDigits++;
    else if (t.k === 'digit') {
      denomDigits++;
      fixedDenom += t.ch;
    } else if (t.k === 'lit' && /^\d+$/.test(t.v)) {
      fixedDenom += t.v;
      denomDigits += t.v.length;
    } else break;
  }
  const hasInt = toks.slice(0, slashAt).some((t) => t.k === 'lit' && /\s/.test(t.v));
  const whole = Math.floor(abs);
  let frac = abs - whole;
  let num = 0,
    den = 1;
  const limit = 10 ** Math.max(1, denomDigits) - 1;
  if (/^\d+$/.test(fixedDenom) && Number(fixedDenom) > 0) {
    den = Number(fixedDenom);
    num = Math.round((hasInt ? frac : abs) * den);
  } else {
    const target = hasInt ? frac : abs;
    let best = Infinity;
    for (let d = 1; d <= limit; d++) {
      const nn = Math.round(target * d);
      const err = Math.abs(target - nn / d);
      if (err < best - 1e-12) {
        best = err;
        num = nn;
        den = d;
        if (err < 1e-12) break;
      }
    }
  }
  let w = hasInt ? whole : 0;
  if (hasInt && num >= den) {
    w += Math.floor(num / den);
    num %= den;
  }
  frac = 0;
  // Assemble: integer part (if any), space, numerator/denominator with padding.
  const numSlots = toks.slice(0, slashAt).filter((t) => t.k === 'digit').length - (hasInt ? 1 : 0);
  const pad = (s: string, width: number, ch: string) => s.padStart(Math.max(width, 0), ch);
  const numTok = toks
    .slice(0, slashAt)
    .filter((t) => t.k === 'digit')
    .slice(hasInt ? 1 : 0);
  const numPad = numTok.some((t) => t.k === 'digit' && t.ch === '0') ? '0' : ' ';
  const denPadded = den.toString().padEnd(denomDigits, ' ');
  const prefixLits = toks
    .slice(0, slashAt)
    .filter((t) => t.k === 'lit')
    .map((t) => (t as { v: string }).v)
    .join('');
  if (num === 0 && hasInt)
    return (
      (w ? String(w) : '0') +
      ' '.repeat(Math.max(1, prefixLits.length) + 0) +
      ' '.repeat(numSlots + 1 + denomDigits)
    );
  const left = hasInt ? (w || num === 0 ? String(w) : '') + (prefixLits || ' ') : '';
  return left + pad(String(num), numSlots, numPad) + '/' + denPadded;
}

/** Excel's General format as shown in a default-width cell: at most 11 characters. */
export function formatGeneral(n: number): string {
  if (n === 0) return '0';
  const abs = Math.abs(n);
  const text = numberToText(n);
  if (text.length <= 11 && !/E/.test(text)) return text;
  // Shrink to fit 11 characters.
  if (abs >= 1e-9 && abs < 1e11) {
    const intLen = Math.floor(Math.log10(abs)) + 1;
    const decimals = Math.max(0, 10 - Math.max(intLen, 1) - (n < 0 ? 1 : 0));
    const fixedText = Number(abs.toFixed(Math.min(decimals, 15)))
      .toString()
      .replace(/e.*/, '');
    if (!/e/.test(fixedText)) return (n < 0 ? '-' : '') + fixedText;
  }
  let digits = 5;
  for (; digits >= 0; digits--) {
    const t = abs
      .toExponential(digits)
      .replace(/(\.\d*?)0+e/, '$1e')
      .replace(/\.e/, 'e');
    const [m, e] = t.split('e');
    const out = `${n < 0 ? '-' : ''}${m}E${e[0]}${e.slice(1).padStart(2, '0')}`;
    if (out.length <= 11) return out;
  }
  return numberToText(n);
}

function dateSection(serial: number, toks: Tok[]): string | null {
  if (!(serial >= 0)) return null;
  const dateToks = toks.filter((t): t is Extract<Tok, { k: 'date' }> => t.k === 'date');
  const fracDigits = (() => {
    const i = toks.findIndex(
      (t) => t.k === 'date' && /^(s|ss|\[s\])$/.test((t as { v: string }).v)
    );
    if (i < 0) return 0;
    let n = 0;
    let j = i + 1;
    if (toks[j]?.k === 'dec') {
      j++;
      while (toks[j]?.k === 'digit') {
        n++;
        j++;
      }
    }
    return n;
  })();
  const scale = 10 ** fracDigits;
  const totalSeconds = Math.round(serial * 86400 * scale) / scale;
  const day = Math.floor(totalSeconds / 86400 + 1e-9);
  const secOfDay = totalSeconds - day * 86400;
  const ymd = serialToYmd(day);
  if (!ymd && toks.some((t) => t.k === 'date' && /^[ymd]/.test(t.v))) return null;
  const hasAmPm = dateToks.some((t) => /^(am\/pm|a\/p)$/i.test(t.v));
  const hours = Math.floor(secOfDay / 3600);
  const minutes = Math.floor((secOfDay % 3600) / 60);
  const seconds = Math.floor(secOfDay % 60);
  const subsecond = secOfDay - Math.floor(secOfDay);
  const pad2 = (n: number) => String(n).padStart(2, '0');
  // Month-versus-minute: "m" after hours or before seconds means minutes.
  const isMinute = (idx: number) => {
    for (let j = idx - 1; j >= 0; j--) {
      const t = toks[j];
      if (t.k === 'date') {
        if (/^(h|hh|\[h\])$/.test(t.v)) return true;
        break;
      }
    }
    for (let j = idx + 1; j < toks.length; j++) {
      const t = toks[j];
      if (t.k === 'date') {
        if (/^(s|ss|\[s\])$/.test(t.v)) return true;
        break;
      }
    }
    return false;
  };
  let out = '';
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i];
    if (t.k === 'lit') {
      out += t.v;
      continue;
    }
    if (t.k === 'dec' || t.k === 'digit') {
      // fractional seconds
      if (t.k === 'dec' && toks[i + 1]?.k === 'digit') {
        const digs = [];
        let j = i + 1;
        while (toks[j]?.k === 'digit') {
          digs.push(1);
          j++;
        }
        if (fracDigits > 0)
          out +=
            '.' +
            String(Math.round(subsecond * scale))
              .padStart(digs.length, '0')
              .slice(0, digs.length);
        i = j - 1;
        continue;
      }
      if (t.k === 'dec') out += '.';
      continue;
    }
    if (t.k === 'comma') {
      out += ',';
      continue;
    }
    if (t.k === 'pct') {
      out += '%';
      continue;
    }
    if (t.k === 'slash') {
      out += '/';
      continue;
    }
    if (t.k === 'text' || t.k === 'exp') continue;
    const v = t.v;
    if (v === 'General') {
      out += formatGeneral(serial);
      continue;
    }
    switch (v.toLowerCase()) {
      case 'yyyy':
        out += String(ymd!.y).padStart(4, '0');
        break;
      case 'yy':
        out += pad2(ymd!.y % 100);
        break;
      case 'mmmmm':
        out += MONTHS[ymd!.m - 1][0];
        break;
      case 'mmmm':
        if (isMinute(i)) out += pad2(minutes);
        else out += MONTHS[ymd!.m - 1];
        break;
      case 'mmm':
        out += MONTHS[ymd!.m - 1].slice(0, 3);
        break;
      case 'mm':
        out += isMinute(i) ? pad2(minutes) : pad2(ymd!.m);
        break;
      case 'm':
        out += isMinute(i) ? String(minutes) : String(ymd!.m);
        break;
      case 'dddd':
        out += DAYS[weekdayOf(day) - 1];
        break;
      case 'ddd':
        out += DAYS[weekdayOf(day) - 1].slice(0, 3);
        break;
      case 'dd':
        out += pad2(ymd!.d);
        break;
      case 'd':
        out += String(ymd!.d);
        break;
      case 'hh':
        out += pad2(hasAmPm ? hours % 12 || 12 : hours);
        break;
      case 'h':
        out += String(hasAmPm ? hours % 12 || 12 : hours);
        break;
      case 'ss':
        out += pad2(seconds);
        break;
      case 's':
        out += String(seconds);
        break;
      case '[h]':
        out += String(Math.floor(totalSeconds / 3600));
        break;
      case '[m]':
        out += String(Math.floor(totalSeconds / 60));
        break;
      case '[s]':
        out += String(Math.floor(totalSeconds));
        break;
      case 'am/pm':
        out += hours >= 12 ? 'PM' : 'AM';
        break;
      case 'a/p':
        out += hours >= 12 ? 'P' : 'A';
        break;
      default:
        break;
    }
  }
  return out;
}

const isDateSection = (toks: Tok[]) =>
  toks.some((t) => t.k === 'date' && t.v !== 'General' && !/^\[/.test(t.v)) ||
  toks.some((t) => t.k === 'date' && /^\[[hms]\]$/.test(t.v));

/** Apply a number format to a number, text or logical value. */
export function formatWithPattern(value: number | string | boolean, pattern: string): string {
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
  const raw = splitSections(pattern);
  const sections = raw.map(parseSection);
  if (typeof value === 'string') {
    const textSection = sections.find((s) => s.toks.some((t) => t.k === 'text'));
    if (!textSection) return value;
    return textSection.toks
      .map((t) => (t.k === 'text' ? value : t.k === 'lit' ? t.v : ''))
      .join('');
  }
  const n = value;
  if (pattern === '') return '';
  let chosen: Section | undefined;
  let negativeSlot = false;
  if (sections.some((s) => s.cond)) {
    chosen = sections.find((s) => s.cond && holds(s.cond, n));
    if (!chosen) chosen = sections.find((s) => !s.cond) ?? sections[sections.length - 1];
    negativeSlot =
      !!chosen.cond &&
      (chosen.cond.op === '<' || chosen.cond.op === '<=') &&
      chosen.cond.value <= 0;
  } else if (sections.length === 1) chosen = sections[0];
  else if (n > 0) chosen = sections[0];
  else if (n < 0) {
    chosen = sections[1];
    negativeSlot = true;
  } else chosen = sections[2] ?? sections[0];
  const toks = chosen.toks;
  if (toks.length === 0) return '';
  if (toks.some((t) => t.k === 'text') && !toks.some((t) => t.k === 'digit' || t.k === 'date')) {
    return toks.map((t) => (t.k === 'text' ? formatGeneral(n) : t.k === 'lit' ? t.v : '')).join('');
  }
  if (isDateSection(toks)) {
    const out = dateSection(n, toks);
    return out === null ? '#VALUE!' : out;
  }
  if (toks.length === 1 && toks[0].k === 'date' && toks[0].v === 'General') return formatGeneral(n);
  const abs = Math.abs(n);
  const body = numberSection(
    abs,
    toks.map((t) => (t.k === 'date' ? { k: 'lit', v: '' } : t)) as Tok[]
  );
  const hasDigitOutput = /[1-9]/.test(body);
  const sign = n < 0 && !negativeSlot && hasDigitOutput ? '-' : '';
  return sign + body;
}
