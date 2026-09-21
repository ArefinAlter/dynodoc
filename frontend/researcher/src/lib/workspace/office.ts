/** Persisted document settings. Units are millimetres, independent of screen zoom. */
export interface PageSetup {
  size: 'A4' | 'Letter' | 'Legal';
  landscape: boolean;
  margin: number;
  header: string;
  footer: string;
  pageNumbers: boolean;
  language: string;
}
export const defaultPage: PageSetup = {
  size: 'A4',
  landscape: false,
  margin: 25.4,
  header: '',
  footer: '',
  pageNumbers: true,
  language: 'en'
};
export const scriptFontStack =
  "'Noto Sans Bengali','Noto Naskh Arabic','Noto Sans Devanagari','Noto Sans Hebrew',sans-serif";
export function pageSetup(value: unknown): PageSetup {
  const v = (value && typeof value === 'object' ? value : {}) as Partial<PageSetup>;
  return {
    size: ['A4', 'Letter', 'Legal'].includes(v.size || '') ? v.size! : 'A4',
    landscape: v.landscape === true,
    margin: bounded(v.margin, 10, 45, 25.4),
    header: String(v.header || '').slice(0, 200),
    footer: String(v.footer || '').slice(0, 200),
    pageNumbers: v.pageNumbers !== false,
    language: /^[a-z]{2,3}(-[a-zA-Z]{2,8})?$/.test(v.language || '') ? v.language! : 'en'
  };
}
export function pageDimensions(page: PageSetup) {
  const dimensions = { A4: [210, 297], Letter: [215.9, 279.4], Legal: [215.9, 355.6] }[page.size];
  return page.landscape ? [...dimensions].reverse() : dimensions;
}
export function bounded(value: unknown, min: number, max: number, fallback: number) {
  const n = Number(value);
  return Number.isFinite(n) && value !== null && value !== '' && value !== undefined
    ? Math.max(min, Math.min(max, n))
    : fallback;
}
export function safeColor(value: unknown, fallback = '') {
  return /^#[0-9a-f]{6}$/i.test(String(value)) ? String(value) : fallback;
}
export function safeFont(value: unknown) {
  const s = String(value || '').trim();
  return /^[\p{L}\p{N} _,-]{1,100}$/u.test(s) ? s : 'Arial';
}
export const officeFonts = [
  'Arial',
  'Aptos',
  'Calibri',
  'Cambria',
  'Georgia',
  'Times New Roman',
  'Verdana',
  'Tahoma',
  'Trebuchet MS',
  'Courier New',
  'Noto Sans',
  'Noto Sans Bengali',
  'Noto Naskh Arabic',
  'Noto Sans Devanagari',
  'Noto Sans Hebrew',
  'Nirmala UI',
  'Microsoft YaHei',
  'Yu Gothic'
];
export interface DocumentFont {
  family: string;
  uploadId: string;
}
export function documentFonts(value: unknown): DocumentFont[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter(
      (f): f is DocumentFont =>
        !!f &&
        typeof f === 'object' &&
        typeof f.family === 'string' &&
        safeFont(f.family) === f.family &&
        /^[0-9a-f-]{36}$/i.test(String(f.uploadId))
    )
    .slice(0, 16);
}
