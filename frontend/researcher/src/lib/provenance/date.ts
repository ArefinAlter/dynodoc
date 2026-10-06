/** A civil day, not an instant. The engine's imported XLSX representation uses
 * midnight UTC; native hosts translate that day in their own date system. */
export function dateText(value: string): string {
  if (value === '' || value.startsWith('=')) return value;
  if (!/^\d{4}-\d{2}-\d{2}(T00:00:00\.000Z)?$/.test(value))
    throw new Error('Date cells need an ISO date without a time. Use file push for timestamps.');
  const day = value.slice(0, 10),
    iso = day + 'T00:00:00.000Z',
    parsed = new Date(iso);
  if (
    Number(day.slice(0, 4)) < 1900 ||
    !Number.isFinite(parsed.getTime()) ||
    parsed.toISOString() !== iso
  )
    throw new Error('Invalid calendar date. Use a date from 1900 through 9999.');
  return iso;
}

export interface DateResult {
  value: number;
  error: string | null;
  load: (fields: string) => void;
}
export interface DateFunctions {
  year: (serial: number) => DateResult;
  month: (serial: number) => DateResult;
  day: (serial: number) => DateResult;
  date: (year: number, month: number, day: number) => DateResult;
}
interface DateContext {
  workbook: { functions: DateFunctions };
  sync: () => Promise<void>;
}
function result(r: DateResult): number {
  if (r.error || !Number.isSafeInteger(r.value))
    throw new Error('Excel could not round-trip this calendar date. Use file push.');
  return r.value;
}
/** Stable ExcelApi 1.2 functions evaluate inside the workbook. Never assume a
 * 1900/1904 epoch or convert the serial using the computer's timezone. */
export async function readExcelDates(ctx: DateContext, values: unknown[]): Promise<string[]> {
  const parts = values.map((value) => {
    if (value === '') return null;
    if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0)
      throw new Error('A date has a time or changed type. Use file push.');
    const f = ctx.workbook.functions,
      parts = [f.year(value), f.month(value), f.day(value)];
    parts.forEach((p) => p.load('value,error'));
    return parts;
  });
  if (parts.some(Boolean)) await ctx.sync();
  const text = parts.map((p) =>
    p
      ? dateText(
          [result(p[0]), result(p[1]), result(p[2])]
            .map((n, i) => String(n).padStart(i === 0 ? 4 : 2, '0'))
            .join('-')
        )
      : ''
  );
  const back = await writeExcelDates(ctx, text);
  if (back.some((serial, i) => serial !== values[i]))
    throw new Error('Excel could not round-trip this calendar date. Use file push.');
  return text;
}
export async function writeExcelDates(
  ctx: DateContext,
  values: string[]
): Promise<(number | '')[]> {
  const results = values.map((value) => {
    const iso = dateText(value);
    if (iso === '') return null;
    if (iso.startsWith('=')) throw new Error('Write date formulas through the formula API.');
    const [y, m, d] = iso.slice(0, 10).split('-').map(Number),
      r = ctx.workbook.functions.date(y, m, d);
    r.load('value,error');
    return r;
  });
  if (results.some(Boolean)) await ctx.sync();
  const serials = results.map((r) => {
    if (!r) return '';
    const serial = result(r);
    if (serial < 0)
      throw new Error('This date is outside the workbook date system. Use file push.');
    return serial;
  });
  // Preflight the reverse conversion before any caller queues a cell write.
  const back = serials.map((serial) => {
    if (serial === '') return null;
    const f = ctx.workbook.functions,
      p = [f.year(serial), f.month(serial), f.day(serial)];
    p.forEach((r) => r.load('value,error'));
    return p;
  });
  if (back.some(Boolean)) await ctx.sync();
  if (
    back.some((p, i) =>
      p
        ? dateText(p.map((r, j) => String(result(r)).padStart(j === 0 ? 4 : 2, '0')).join('-')) !==
          dateText(values[i])
        : values[i] !== ''
    )
  )
    throw new Error('Excel could not round-trip this calendar date. Use file push.');
  return serials;
}
