import { FormulaError, errorFromText, isError } from './errors';
import {
  compareNumbers,
  compareText,
  parseNumberText,
  type CoerceOptions,
  type Scalar
} from './values';

export type CriteriaTest = (cell: Scalar) => boolean;

/** Wildcard text → RegExp (`*`, `?`, `~` escapes), anchored and case-insensitive. */
export function wildcardRegExp(pattern: string): RegExp | null {
  if (!/[*?~]/.test(pattern)) return null;
  let out = '';
  for (let i = 0; i < pattern.length; i++) {
    const ch = pattern[i];
    if (ch === '~' && i + 1 < pattern.length) {
      out += pattern[++i].replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    } else if (ch === '*') out += '[\\s\\S]*';
    else if (ch === '?') out += '[\\s\\S]';
    else out += ch.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }
  return new RegExp('^' + out + '$', 'i');
}

export function textMatcher(pattern: string): (s: string) => boolean {
  const re = wildcardRegExp(pattern);
  if (re) return (s) => re.test(s);
  if (/^[\x20-\x7e]*$/.test(pattern)) {
    // Plain ASCII criteria are by far the most common; skip the collator.
    const lower = pattern.toLowerCase();
    return (s) =>
      s.length === pattern.length
        ? s.toLowerCase() === lower || (/[^\x20-\x7e]/.test(s) && compareText(s, pattern) === 0)
        : /[^\x20-\x7e]/.test(s) && compareText(s, pattern) === 0;
  }
  return (s) => compareText(s, pattern) === 0;
}

/** Build a predicate for COUNTIF-style criteria. */
export function evaluateCriteriaArgs(
  criteria: Scalar | undefined,
  ctx: { coerce: CoerceOptions }
): CriteriaTest {
  if (criteria === undefined || criteria === null) return (c) => c === null || c === '';
  if (isError(criteria)) {
    const code = criteria.value;
    return (c) => isError(c) && c.value === code;
  }
  if (typeof criteria === 'number')
    return (c) =>
      typeof c === 'number'
        ? compareNumbers(c, criteria) === 0
        : typeof c === 'string' && numericText(c, ctx) === criteria;
  if (typeof criteria === 'boolean') return (c) => c === criteria;
  const m = /^(<=|>=|<>|<|>|=)?([\s\S]*)$/.exec(criteria)!;
  const op = m[1] ?? '=';
  const rest = m[2];
  const negate = op === '<>';
  const apply = (positive: CriteriaTest): CriteriaTest => (negate ? (c) => !positive(c) : positive);

  if (rest === '') {
    if (m[1] === undefined) return (c) => c === null || c === '';
    return negate ? (c) => c !== null && c !== '' : (c) => c === null;
  }
  const upper = rest.toUpperCase();
  if (upper === 'TRUE' || upper === 'FALSE') {
    const want = upper === 'TRUE';
    return apply((c) => c === want);
  }
  const err = errorFromText(rest);
  if (err) {
    const code = err.value;
    return apply((c) => isError(c) && (c as FormulaError).value === code);
  }
  const n = parseNumberText(rest, ctx.coerce);
  if (n !== null) {
    if (op === '=' || op === '<>')
      return apply((c) =>
        typeof c === 'number'
          ? compareNumbers(c, n) === 0
          : typeof c === 'string' && numericText(c, ctx) === n
      );
    return (c) => {
      if (typeof c !== 'number') return false;
      const cmp = compareNumbers(c, n);
      return op === '<' ? cmp < 0 : op === '>' ? cmp > 0 : op === '<=' ? cmp <= 0 : cmp >= 0;
    };
  }
  // Text criteria.
  if (op === '=' || op === '<>') {
    const test = textMatcher(rest);
    return apply((c) => typeof c === 'string' && test(c));
  }
  return (c) => {
    if (typeof c !== 'string') return false;
    const cmp = compareText(c, rest);
    return op === '<' ? cmp < 0 : op === '>' ? cmp > 0 : op === '<=' ? cmp <= 0 : cmp >= 0;
  };
}

function numericText(s: string, ctx: { coerce: CoerceOptions }): number | null {
  if (!/\d/.test(s)) return null;
  return parseNumberText(s, ctx.coerce);
}
