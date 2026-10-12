/** Presentation helpers for the Projects area: relative time, day groups, formats and hashes. */

export const plural = (n: number, one: string, many = `${one}s`) =>
  `${n.toLocaleString()} ${n === 1 ? one : many}`;

/** "just now", "5 minutes ago", "3 days ago", "2 months ago". Empty for an unreadable date. */
export function relativeTime(iso: string | null | undefined, now = Date.now()): string {
  const at = iso ? Date.parse(iso) : NaN;
  if (!Number.isFinite(at)) return '';
  const seconds = Math.round((now - at) / 1000);
  if (seconds < 45) return 'just now';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${plural(minutes, 'minute')} ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${plural(hours, 'hour')} ago`;
  const days = Math.round(hours / 24);
  if (days === 1) return 'yesterday';
  if (days < 30) return `${plural(days, 'day')} ago`;
  const months = Math.round(days / 30);
  if (months < 12) return `${plural(months, 'month')} ago`;
  return `${plural(Math.round(days / 365), 'year')} ago`;
}
export function absoluteTime(iso: string | null | undefined): string {
  const at = iso ? new Date(iso) : null;
  return at && Number.isFinite(at.getTime()) ? at.toLocaleString() : '';
}
const pad = (n: number) => String(n).padStart(2, '0');
/** Local calendar day as YYYY-MM-DD. */
export function dayKey(iso: string): string {
  const d = new Date(iso);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
export function dayLabel(key: string, now = new Date()): string {
  if (key === dayKey(now.toISOString())) return 'Today';
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (key === dayKey(yesterday.toISOString())) return 'Yesterday';
  const [y, m, d] = key.split('-').map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric'
  });
}
export interface DayGroup<T> {
  key: string;
  label: string;
  items: T[];
}
/** Consecutive items sharing a local day, in the order given. */
export function groupByDay<T>(
  items: T[],
  at: (item: T) => string,
  now = new Date()
): DayGroup<T>[] {
  const groups: DayGroup<T>[] = [];
  for (const item of items) {
    const stamp = at(item);
    if (!Number.isFinite(Date.parse(stamp))) continue;
    const key = dayKey(stamp);
    const last = groups[groups.length - 1];
    if (last && last.key === key) last.items.push(item);
    else groups.push({ key, label: dayLabel(key, now), items: [item] });
  }
  return groups;
}

export type FormatKey = 'word' | 'excel' | 'powerpoint' | 'other';
export interface FormatInfo {
  key: FormatKey;
  label: string;
  /** What this kind is called next to a file. */
  noun: string;
}
const FORMATS: Record<FormatKey, FormatInfo> = {
  word: { key: 'word', label: 'Word', noun: 'Word or Docs document' },
  excel: { key: 'excel', label: 'Excel', noun: 'Excel or Sheets workbook' },
  powerpoint: { key: 'powerpoint', label: 'PowerPoint', noun: 'PowerPoint or Slides deck' },
  other: { key: 'other', label: 'Other', noun: 'Other file' }
};
export const formatOf = (kind: string): FormatInfo =>
  FORMATS[
    kind === 'document'
      ? 'word'
      : kind === 'spreadsheet'
        ? 'excel'
        : kind === 'presentation'
          ? 'powerpoint'
          : 'other'
  ];
export interface FormatShare {
  format: FormatInfo;
  count: number;
  percent: number;
}
export function formatBreakdown(files: { kind: string }[]): FormatShare[] {
  const counts = new Map<FormatKey, number>();
  for (const f of files) {
    const key = formatOf(f.kind).key;
    counts.set(key, (counts.get(key) || 0) + 1);
  }
  const total = files.length;
  return (Object.keys(FORMATS) as FormatKey[])
    .filter((key) => counts.get(key))
    .map((key) => ({
      format: FORMATS[key],
      count: counts.get(key) || 0,
      percent: total ? ((counts.get(key) || 0) / total) * 100 : 0
    }))
    .sort((a, b) => b.count - a.count);
}

/** Hex form of a hash that the API sends as bytes or as text. */
export function hashHex(value: number[] | string | null | undefined): string {
  if (!value) return '';
  if (typeof value === 'string') return value.replace(/^\\x/, '');
  return value.map((b) => (b & 255).toString(16).padStart(2, '0')).join('');
}
export const shortHash = (hex: string) => hex.slice(0, 7);

/** First word of a display name, or the part of an address before the @. */
export function displayName(name: string | null | undefined, email?: string | null): string {
  const trimmed = (name || '').trim();
  if (trimmed) return trimmed;
  return (email || '').split('@')[0] || 'Project member';
}
/** A stable hue (0-359) so a person keeps one avatar colour. */
export function avatarHue(name: string): number {
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.charCodeAt(0)) % 360;
  return hash;
}
