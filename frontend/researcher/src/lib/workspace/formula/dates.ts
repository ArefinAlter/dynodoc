/**
 * The 1900 date system, including its deliberate Excel quirks: serial 60 is the
 * non-existent 1900-02-29 and serial 0 is "January 0, 1900".
 */
export type DateOrder = 'DMY' | 'MDY';

export const MAX_SERIAL = 2958465; // 9999-12-31

/** Days since 1970-01-01 for a proleptic Gregorian date. */
function daysFromCivil(y: number, m: number, d: number) {
  y -= m <= 2 ? 1 : 0;
  const era = Math.floor(y / 400);
  const yoe = y - era * 400;
  const doy = Math.floor((153 * (m + (m > 2 ? -3 : 9)) + 2) / 5) + d - 1;
  const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy;
  return era * 146097 + doe - 719468;
}
function civilFromDays(z: number): [number, number, number] {
  z += 719468;
  const era = Math.floor(z / 146097);
  const doe = z - era * 146097;
  const yoe = Math.floor(
    (doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365
  );
  const y = yoe + era * 400;
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100));
  const mp = Math.floor((5 * doy + 2) / 153);
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1;
  const m = mp + (mp < 10 ? 3 : -9);
  return [y + (m <= 2 ? 1 : 0), m, d];
}
const MARCH_1_1900 = daysFromCivil(1900, 3, 1);

export function isLeapYear(y: number) {
  return (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0;
}
export function daysInMonth(y: number, m: number) {
  if (m === 2) return isLeapYear(y) ? 29 : 28;
  return [4, 6, 9, 11].includes(m) ? 30 : 31;
}

/** Serial for a year/month/day; months and days may overflow (DATE semantics). */
export function ymdToSerial(y: number, m: number, d: number): number {
  y += Math.floor((m - 1) / 12);
  m = ((((m - 1) % 12) + 12) % 12) + 1;
  let first: number;
  if (y === 1900 && m <= 2) first = m === 1 ? 1 : 32;
  else first = daysFromCivil(y, m, 1) - MARCH_1_1900 + 61;
  return first + d - 1;
}

/** Calendar parts for a serial, or null when it is outside 0..9999-12-31. */
export function serialToYmd(serial: number): { y: number; m: number; d: number } | null {
  const n = Math.floor(serial);
  if (!(n >= 0) || n > MAX_SERIAL) return null;
  if (n === 0) return { y: 1900, m: 1, d: 0 };
  if (n === 60) return { y: 1900, m: 2, d: 29 };
  if (n < 60) {
    const [y, m, d] = civilFromDays(MARCH_1_1900 - 60 + n);
    return { y, m, d };
  }
  const [y, m, d] = civilFromDays(MARCH_1_1900 + n - 61);
  return { y, m, d };
}

/** 1 = Sunday … 7 = Saturday, from serial arithmetic (so 0 is a Saturday, as in Excel). */
export function weekdayOf(serial: number) {
  return ((((Math.floor(serial) - 1) % 7) + 7) % 7) + 1;
}

export function timeParts(serial: number) {
  const frac = serial - Math.floor(serial);
  const total = Math.round(frac * 86400);
  const wrapped = total % 86400;
  return { h: Math.floor(wrapped / 3600), m: Math.floor((wrapped % 3600) / 60), s: wrapped % 60 };
}

export const MONTHS = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December'
];
export const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];

function monthFromName(text: string) {
  const t = text.toLowerCase();
  if (t.length < 3) return 0;
  for (let i = 0; i < 12; i++) {
    const full = MONTHS[i].toLowerCase();
    if (
      full === t ||
      (t.length >= 3 && full.startsWith(t.slice(0, 3)) && (full.startsWith(t) || t === 'sept'))
    )
      return i + 1;
  }
  return 0;
}
function fullYear(text: string) {
  const n = Number(text);
  if (text.length <= 2) return n < 30 ? 2000 + n : 1900 + n;
  return n;
}

/** Fraction of a day for `h:mm[:ss[.fff]] [AM|PM]`, or null. Hours past 24 are kept. */
export function parseTimeText(text: string): number | null {
  const m = /^(\d{1,2})(?::(\d{1,2})(?::(\d{1,2})(\.\d+)?)?)?\s*([AaPp][Mm]?)?$/.exec(text.trim());
  if (!m) return null;
  if (m[2] === undefined && m[5] === undefined) return null;
  let h = Number(m[1]);
  const min = m[2] === undefined ? 0 : Number(m[2]);
  const sec = (m[3] === undefined ? 0 : Number(m[3])) + (m[4] ? Number(m[4]) : 0);
  if (m[5]) {
    if (h < 1 || h > 12) return null;
    h = (h % 12) + (m[5][0].toLowerCase() === 'p' ? 12 : 0);
  }
  return (h * 3600 + min * 60 + sec) / 86400;
}

/**
 * Parse a date and/or time written as text, in the forms Excel accepts: 2026-01-15,
 * 1/15/2026, 15-Jan-2026, 15 Jan 2026, Jan 15, 2026, Jan 2026, Jan 15, 15 Jan, 1/15, with
 * an optional time. Returns a serial (with a fraction for times) or null. The order of
 * all-numeric day and month follows `order`, as Excel follows the system locale.
 */
export function parseDateTimeText(
  input: string,
  order: DateOrder,
  currentYear: number
): number | null {
  const text = input.trim().replace(/\s+/g, ' ');
  if (!text || text.length > 60) return null;
  let time: number | null = null;
  let datePart = text;
  const timeAtEnd =
    /(?:^| )(\d{1,2}:\d{1,2}(?::\d{1,2}(?:\.\d+)?)?(?: ?[AaPp][Mm])?|\d{1,2} ?[AaPp][Mm])$/.exec(
      text
    );
  if (timeAtEnd) {
    const candidate = parseTimeText(timeAtEnd[1].replace(/(\d) ?([AaPp][Mm])$/, '$1 $2'));
    if (candidate !== null) {
      time = candidate;
      datePart = text.slice(0, timeAtEnd.index).trim();
    }
  }
  if (!datePart) return time;
  const date = parseDateOnly(datePart, order, currentYear);
  if (date === null) return null;
  return date + (time ?? 0);
}

function validYmd(y: number, m: number, d: number) {
  return (
    y >= 1900 &&
    y <= 9999 &&
    m >= 1 &&
    m <= 12 &&
    d >= 1 &&
    d <= (y === 1900 && m === 2 ? 29 : daysInMonth(y, m))
  );
}

function parseDateOnly(text: string, order: DateOrder, currentYear: number): number | null {
  const make = (y: number, mo: number, d: number) =>
    validYmd(y, mo, d) ? ymdToSerial(y, mo, d) : null;
  let m: RegExpExecArray | null;
  // Year first: 2026-01-15, 2026/1/5
  if ((m = /^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/.exec(text)))
    return make(Number(m[1]), Number(m[2]), Number(m[3]));
  // Numeric: 1/15/2026, 15-1-26
  if ((m = /^(\d{1,2})([/-])(\d{1,2})\2(\d{1,2}|\d{4})$/.exec(text))) {
    const a = Number(m[1]),
      b = Number(m[3]);
    const [mo, d] = order === 'MDY' ? [a, b] : [b, a];
    return make(fullYear(m[4]), mo, d);
  }
  // Month and year: 1/2026
  if ((m = /^(\d{1,2})[/-](\d{4})$/.exec(text))) return make(Number(m[2]), Number(m[1]), 1);
  // Day and month in the current year: 1/15
  if ((m = /^(\d{1,2})\/(\d{1,2})$/.exec(text))) {
    const a = Number(m[1]),
      b = Number(m[2]);
    const [mo, d] = order === 'MDY' ? [a, b] : [b, a];
    return make(currentYear, mo, d);
  }
  // 15 Jan 2026, 15-Jan-2026
  if ((m = /^(\d{1,2})([ -])([A-Za-z]{3,9})\2(\d{1,2}|\d{4})$/.exec(text))) {
    const mo = monthFromName(m[3]);
    return mo ? make(fullYear(m[4]), mo, Number(m[1])) : null;
  }
  // Jan 15, 2026
  if ((m = /^([A-Za-z]{3,9}) (\d{1,2}), ?(\d{1,2}|\d{4})$/.exec(text))) {
    const mo = monthFromName(m[1]);
    return mo ? make(fullYear(m[3]), mo, Number(m[2])) : null;
  }
  // Jan 2026
  if ((m = /^([A-Za-z]{3,9}) (\d{4})$/.exec(text))) {
    const mo = monthFromName(m[1]);
    return mo ? make(Number(m[2]), mo, 1) : null;
  }
  // Jan 15, Jan-15
  if ((m = /^([A-Za-z]{3,9})[ -](\d{1,2})$/.exec(text))) {
    const mo = monthFromName(m[1]);
    return mo ? make(currentYear, mo, Number(m[2])) : null;
  }
  // 15 Jan, 15-Jan
  if ((m = /^(\d{1,2})[ -]([A-Za-z]{3,9})$/.exec(text))) {
    const mo = monthFromName(m[2]);
    return mo ? make(currentYear, mo, Number(m[1])) : null;
  }
  return null;
}
