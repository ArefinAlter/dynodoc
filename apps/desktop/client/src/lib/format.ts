// Dates and numbers as the Appearance settings ask (GitHub Desktop's formatting options). Pure.

export type DateFormat = 'medium' | 'iso' | 'dmy';
export type TimeFormat = '12h' | '24h';
export type NumberFormat = 'comma' | 'space' | 'dot';

export interface FormatPrefs {
  dateFormat: DateFormat;
  timeFormat: TimeFormat;
  numberFormat: NumberFormat;
  absoluteDates: boolean;
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const pad = (n: number) => String(n).padStart(2, '0');

export function formatDate(ms: number, p: Pick<FormatPrefs, 'dateFormat'>): string {
  const d = new Date(ms);
  const y = d.getUTCFullYear(), m = d.getUTCMonth(), day = d.getUTCDate();
  if (p.dateFormat === 'iso') return `${y}-${pad(m + 1)}-${pad(day)}`;
  if (p.dateFormat === 'dmy') return `${pad(day)}/${pad(m + 1)}/${y}`;
  return `${MONTHS[m]} ${day}, ${y}`;
}

export function formatTime(ms: number, p: Pick<FormatPrefs, 'timeFormat'>): string {
  const d = new Date(ms);
  const h = d.getUTCHours(), min = d.getUTCMinutes();
  if (p.timeFormat === '24h') return `${pad(h)}:${pad(min)}`;
  const h12 = h % 12 === 0 ? 12 : h % 12;
  return `${h12}:${pad(min)} ${h < 12 ? 'am' : 'pm'}`;
}

/** "3 min ago", "2 h ago", "yesterday", "5 d ago", then a date. */
export function relative(ms: number, now: number, p: FormatPrefs): string {
  if (p.absoluteDates) return `${formatDate(ms, p)} ${formatTime(ms, p)}`;
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 45) return 'just now';
  const min = Math.round(s / 60);
  if (min < 60) return `${min} min ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr} h ago`;
  const d = Math.round(hr / 24);
  if (d === 1) return 'yesterday';
  if (d < 14) return `${d} d ago`;
  return formatDate(ms, p);
}

/** Short age for list rows, as T3 Code prints it: "now", "12m", "3h", "2d", "5w". */
export function age(ms: number, now: number): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return 'now';
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  const d = Math.floor(h / 24);
  if (d < 14) return `${d}d`;
  return `${Math.floor(d / 7)}w`;
}

export function formatNumber(n: number, p: Pick<FormatPrefs, 'numberFormat'>): string {
  const neg = n < 0;
  const [int, frac] = Math.abs(n).toFixed(Number.isInteger(n) ? 0 : 2).split('.');
  const sep = p.numberFormat === 'space' ? ' ' : p.numberFormat === 'dot' ? '.' : ',';
  const dec = p.numberFormat === 'dot' ? ',' : '.';
  const grouped = int.replace(/\B(?=(\d{3})+(?!\d))/g, sep);
  return (neg ? '-' : '') + grouped + (frac ? dec + frac : '');
}

export function shortHash(hex: string, n = 8): string {
  return hex.slice(0, n);
}

export function plural(n: number, one: string, many = one + 's'): string {
  return `${n} ${n === 1 ? one : many}`;
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/);
  return ((parts[0]?.[0] ?? '') + (parts.length > 1 ? parts[parts.length - 1][0] : '')).toUpperCase();
}
