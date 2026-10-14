import { E, isError } from './errors';
import {
  MAX_SERIAL,
  daysInMonth,
  isLeapYear,
  parseDateTimeText,
  parseTimeText,
  serialToYmd,
  timeParts,
  weekdayOf,
  ymdToSerial
} from './dates';
import { operand, type EvalContext } from './evaluator';
import { toNumber, type Scalar, type Value } from './values';
import {
  def,
  defScalar,
  int,
  intOr,
  num,
  str,
  boolOr,
  isMissing,
  walkArgs,
  type Arg
} from './fn-util';

/** Serial argument: numbers, numeric/date text, booleans and blanks. */
function serial(v: Arg, ctx: EvalContext): number {
  return num(v, ctx);
}
function ymdOf(v: Arg, ctx: EvalContext) {
  const n = serial(v, ctx);
  const ymd = serialToYmd(n);
  if (!ymd) throw E.NUM;
  return { ...ymd, n: Math.floor(n) };
}

defScalar('DATE', 3, 3, (a, ctx) => {
  let y = int(a[0], ctx);
  const m = int(a[1], ctx),
    d = int(a[2], ctx);
  if (y < 0 || y > 9999) return E.NUM;
  if (y < 1900) y += 1900;
  const s = ymdToSerial(y, m, d);
  if (s < 0 || s > MAX_SERIAL) return E.NUM;
  return s;
});
defScalar('YEAR', 1, 1, (a, ctx) => ymdOf(a[0], ctx).y);
defScalar('MONTH', 1, 1, (a, ctx) => ymdOf(a[0], ctx).m);
defScalar('DAY', 1, 1, (a, ctx) => ymdOf(a[0], ctx).d);
defScalar('HOUR', 1, 1, (a, ctx) => {
  const n = serial(a[0], ctx);
  if (n < 0) return E.NUM;
  return timeParts(n).h;
});
defScalar('MINUTE', 1, 1, (a, ctx) => {
  const n = serial(a[0], ctx);
  if (n < 0) return E.NUM;
  return timeParts(n).m;
});
defScalar('SECOND', 1, 1, (a, ctx) => {
  const n = serial(a[0], ctx);
  if (n < 0) return E.NUM;
  return timeParts(n).s;
});
defScalar('TIME', 3, 3, (a, ctx) => {
  const h = int(a[0], ctx),
    m = int(a[1], ctx),
    s = int(a[2], ctx);
  if (h < 0 || m < 0 || s < 0 || h > 32767 || m > 32767 || s > 32767) return E.NUM;
  const total = (h * 3600 + m * 60 + s) / 86400;
  return total - Math.floor(total);
});
defScalar('DATEVALUE', 1, 1, (a, ctx) => {
  const v = a[0];
  if (typeof v !== 'string') return E.VALUE;
  const n = parseDateTimeText(v, ctx.order, ctx.year);
  if (n === null) return E.VALUE;
  return Math.floor(n);
});
defScalar('TIMEVALUE', 1, 1, (a, ctx) => {
  const v = a[0];
  if (typeof v !== 'string') return E.VALUE;
  const t = parseTimeText(v);
  if (t !== null) return t - Math.floor(t);
  const n = parseDateTimeText(v, ctx.order, ctx.year);
  if (n === null) return E.VALUE;
  return n - Math.floor(n);
});
def('NOW', 0, 0, (_a, ctx) => ctx.now(), { volatile: true });
def('TODAY', 0, 0, (_a, ctx) => Math.floor(ctx.now()), { volatile: true });

// ------------------------------------------------------------ weekday logic

const WEEKDAY_START: Record<number, number> = {
  1: 0,
  2: 1,
  3: 1,
  11: 1,
  12: 2,
  13: 3,
  14: 4,
  15: 5,
  16: 6,
  17: 0
};
defScalar('WEEKDAY', 1, 2, (a, ctx) => {
  const n = serial(a[0], ctx);
  const type = intOr(a[1], ctx, 1);
  if (!(type in WEEKDAY_START)) return E.NUM;
  if (n < 0) return E.NUM;
  const dow0 = weekdayOf(n) - 1;
  const r = ((dow0 - WEEKDAY_START[type] + 7) % 7) + (type === 3 ? 0 : 1);
  return r;
});
function isoWeek(n: number): number {
  // Thursday of this week decides the ISO year.
  const wd = ((weekdayOf(n) + 5) % 7) + 1; // Mon=1..Sun=7
  const thursday = n + (4 - wd);
  const ty = serialToYmd(thursday)!.y;
  const jan1 = ymdToSerial(ty, 1, 1);
  return Math.floor((thursday - jan1) / 7) + 1;
}
defScalar('WEEKNUM', 1, 2, (a, ctx) => {
  const n = serial(a[0], ctx);
  const type = intOr(a[1], ctx, 1);
  const ymd = serialToYmd(n);
  if (!ymd) return E.NUM;
  if (type === 21) return isoWeek(Math.floor(n));
  const sdMap: Record<number, number> = {
    1: 0,
    2: 1,
    11: 1,
    12: 2,
    13: 3,
    14: 4,
    15: 5,
    16: 6,
    17: 0
  };
  if (!(type in sdMap)) return E.NUM;
  const jan1 = ymdToSerial(ymd.y, 1, 1);
  const offset = (weekdayOf(jan1) - 1 - sdMap[type] + 7) % 7;
  return Math.floor((Math.floor(n) - jan1 + offset) / 7) + 1;
});
defScalar('ISOWEEKNUM', 1, 1, (a, ctx) => {
  const n = serial(a[0], ctx);
  if (!serialToYmd(n)) return E.NUM;
  return isoWeek(Math.floor(n));
});

// ------------------------------------------------------------ month maths

function addMonths(n: number, months: number): number {
  const ymd = serialToYmd(n);
  if (!ymd) throw E.NUM;
  const total = ymd.y * 12 + (ymd.m - 1) + months;
  const y = Math.floor(total / 12),
    m = (total % 12) + 1;
  const day = Math.min(ymd.d, daysInMonth(y, m));
  const out = ymdToSerial(y, m, day);
  if (out < 0 || out > MAX_SERIAL) throw E.NUM;
  return out;
}
defScalar('EDATE', 2, 2, (a, ctx) => addMonths(Math.floor(serial(a[0], ctx)), int(a[1], ctx)));
defScalar('EOMONTH', 2, 2, (a, ctx) => {
  const start = Math.floor(serial(a[0], ctx));
  const ymd = serialToYmd(start);
  if (!ymd) return E.NUM;
  const total = ymd.y * 12 + (ymd.m - 1) + int(a[1], ctx);
  const y = Math.floor(total / 12),
    m = (total % 12) + 1;
  const out = ymdToSerial(y, m, daysInMonth(y, m));
  if (out < 0 || out > MAX_SERIAL) return E.NUM;
  return out;
});
defScalar('DAYS', 2, 2, (a, ctx) => Math.trunc(serial(a[0], ctx)) - Math.trunc(serial(a[1], ctx)));

function days360(s: number, e: number, european: boolean): number {
  const a = serialToYmd(s)!,
    b = serialToYmd(e)!;
  let d1 = a.d,
    d2 = b.d;
  const lastFeb = (y: number, m: number, d: number) => m === 2 && d === daysInMonth(y, 2);
  if (european) {
    if (d1 === 31) d1 = 30;
    if (d2 === 31) d2 = 30;
  } else {
    const startLastFeb = lastFeb(a.y, a.m, a.d);
    if (startLastFeb && lastFeb(b.y, b.m, b.d)) d2 = 30;
    if (startLastFeb) d1 = 30;
    if (d2 === 31 && d1 >= 30) d2 = 30;
    if (d1 === 31) d1 = 30;
  }
  return (b.y - a.y) * 360 + (b.m - a.m) * 30 + (d2 - d1);
}
defScalar('DAYS360', 2, 3, (a, ctx) => {
  const s = Math.floor(serial(a[0], ctx)),
    e = Math.floor(serial(a[1], ctx));
  if (!serialToYmd(s) || !serialToYmd(e)) return E.NUM;
  return days360(s, e, boolOr(a[2], false));
});
defScalar('DATEDIF', 3, 3, (a, ctx) => {
  const s = Math.floor(serial(a[0], ctx)),
    e = Math.floor(serial(a[1], ctx));
  const unit = str(a[2]).toUpperCase();
  if (s > e) return E.NUM;
  const sd = serialToYmd(s),
    ed = serialToYmd(e);
  if (!sd || !ed) return E.NUM;
  const months = (ed.y - sd.y) * 12 + (ed.m - sd.m) - (ed.d < sd.d ? 1 : 0);
  switch (unit) {
    case 'D':
      return e - s;
    case 'M':
      return months;
    case 'Y':
      return Math.floor(months / 12);
    case 'YM':
      return months % 12;
    case 'MD': {
      if (ed.d >= sd.d) return ed.d - sd.d;
      const pm = ed.m === 1 ? 12 : ed.m - 1;
      const py = ed.m === 1 ? ed.y - 1 : ed.y;
      const day = Math.min(sd.d, daysInMonth(py, pm));
      return e - ymdToSerial(py, pm, day);
    }
    case 'YD': {
      let y = ed.y;
      let anchor = ymdToSerial(y, sd.m, Math.min(sd.d, daysInMonth(y, sd.m)));
      if (anchor > e) {
        y -= 1;
        anchor = ymdToSerial(y, sd.m, Math.min(sd.d, daysInMonth(y, sd.m)));
      }
      return e - anchor;
    }
    default:
      return E.NUM;
  }
});
defScalar('YEARFRAC', 2, 3, (a, ctx) => {
  let s = Math.floor(serial(a[0], ctx)),
    e = Math.floor(serial(a[1], ctx));
  const basis = intOr(a[2], ctx, 0);
  if (basis < 0 || basis > 4) return E.NUM;
  if (!serialToYmd(s) || !serialToYmd(e)) return E.NUM;
  if (s > e) [s, e] = [e, s];
  if (s === e) return 0;
  switch (basis) {
    case 0:
      return days360(s, e, false) / 360;
    case 4:
      return days360(s, e, true) / 360;
    case 2:
      return (e - s) / 360;
    case 3:
      return (e - s) / 365;
    default: {
      const sd = serialToYmd(s)!,
        ed = serialToYmd(e)!;
      if (sd.y === ed.y) return (e - s) / (isLeapYear(sd.y) ? 366 : 365);
      const oneYearLater = ymdToSerial(sd.y + 1, sd.m, sd.d);
      if (e <= oneYearLater) {
        const feb29 =
          (isLeapYear(sd.y) && s <= ymdToSerial(sd.y, 2, 29)) ||
          (isLeapYear(ed.y) && e >= ymdToSerial(ed.y, 2, 29));
        return (e - s) / (feb29 ? 366 : 365);
      }
      let total = 0;
      for (let y = sd.y; y <= ed.y; y++) total += isLeapYear(y) ? 366 : 365;
      return (e - s) / (total / (ed.y - sd.y + 1));
    }
  }
});

// -------------------------------------------------------- working days

function weekendMask(arg: Arg | Value, ctx: EvalContext): boolean[] {
  // index 0 = Sunday ... 6 = Saturday
  if (arg === undefined || (arg as unknown) === null)
    return [true, false, false, false, false, false, true];
  if (typeof arg === 'string') {
    if (!/^[01]{7}$/.test(arg) || arg === '1111111') throw E.VALUE;
    const mask = new Array<boolean>(7).fill(false);
    for (let i = 0; i < 7; i++) mask[(i + 1) % 7] = arg[i] === '1';
    return mask;
  }
  const code = Math.trunc(num(arg, ctx));
  const mask = new Array<boolean>(7).fill(false);
  if (code >= 1 && code <= 7) {
    // 1 = Sat,Sun; 2 = Sun,Mon; ... 7 = Fri,Sat
    const first = code === 1 ? 6 : code - 2; // Sat for 1, Sun for 2, Mon for 3 ...
    mask[first] = true;
    mask[(first + 1) % 7] = true;
    return mask;
  }
  if (code >= 11 && code <= 17) {
    mask[code === 11 ? 0 : code - 11] = true;
    return mask;
  }
  throw E.NUM;
}
function holidaySet(arg: Value | undefined, ctx: EvalContext): Set<number> {
  const out = new Set<number>();
  if (arg === undefined) return out;
  walkArgs([arg], ctx, (v) => {
    if (isError(v)) throw v;
    if (v === null) return;
    const n = toNumber(v, ctx.coerce);
    if (isError(n)) throw n;
    out.add(Math.floor(n));
  });
  return out;
}
function isWorkday(n: number, mask: boolean[], holidays: Set<number>) {
  return !mask[weekdayOf(n) - 1] && !holidays.has(n);
}
function networkdays(a: Value[], ctx: EvalContext, intl: boolean): Scalar {
  const s = Math.floor(num(a[0], ctx)),
    e = Math.floor(num(a[1], ctx));
  const mask = intl
    ? weekendMask(isMissing(a[2]) ? undefined : (operand(a[2]) as Scalar), ctx)
    : weekendMask(undefined, ctx);
  const holidays = holidaySet(isMissing(a[intl ? 3 : 2]) ? undefined : a[intl ? 3 : 2], ctx);
  if (s < 0 || e < 0) return E.NUM;
  const lo = Math.min(s, e),
    hi = Math.max(s, e);
  if (hi - lo > 200_000) return E.NUM;
  let count = 0;
  for (let d = lo; d <= hi; d++) if (isWorkday(d, mask, holidays)) count++;
  return s <= e ? count : -count;
}
def('NETWORKDAYS', 2, 3, (a, ctx) => networkdays(a, ctx, false));
def('NETWORKDAYS.INTL', 2, 4, (a, ctx) => networkdays(a, ctx, true));
function workday(a: Value[], ctx: EvalContext, intl: boolean): Scalar {
  const start = Math.floor(num(a[0], ctx));
  const days = Math.trunc(num(a[1], ctx));
  const mask = intl
    ? weekendMask(isMissing(a[2]) ? undefined : (operand(a[2]) as Scalar), ctx)
    : weekendMask(undefined, ctx);
  const holidays = holidaySet(isMissing(a[intl ? 3 : 2]) ? undefined : a[intl ? 3 : 2], ctx);
  if (start < 0) return E.NUM;
  if (mask.every(Boolean)) return E.VALUE;
  let d = start;
  let left = Math.abs(days);
  const step = days >= 0 ? 1 : -1;
  let guard = 0;
  while (left > 0) {
    d += step;
    if (++guard > 400_000) return E.NUM;
    if (isWorkday(d, mask, holidays)) left--;
  }
  return d < 0 ? E.NUM : d;
}
def('WORKDAY', 2, 3, (a, ctx) => workday(a, ctx, false));
def('WORKDAY.INTL', 2, 4, (a, ctx) => workday(a, ctx, true));

defScalar('INTERVAL', 1, 1, (a, ctx) => {
  const total = Math.trunc(num(a[0], ctx));
  if (total < 0) return E.NUM;
  const years = Math.floor(total / 31536000);
  let rest = total - years * 31536000;
  const months = Math.floor(rest / 2592000);
  rest -= months * 2592000;
  const days = Math.floor(rest / 86400);
  rest -= days * 86400;
  const hours = Math.floor(rest / 3600);
  rest -= hours * 3600;
  const minutes = Math.floor(rest / 60);
  const seconds = rest - minutes * 60;
  return `P${years ? years + 'Y' : ''}${months ? months + 'M' : ''}${days ? days + 'D' : ''}T${hours ? hours + 'H' : ''}${minutes ? minutes + 'M' : ''}${seconds || (!hours && !minutes) ? seconds + 'S' : ''}`;
});
