/** Shared cream, blue and green interface tokens; see src/app.css. */
export const color = {
  parchment: '#FAF9F6',
  surface: '#FFFEFA',
  surfaceMuted: '#F3EEE3',
  terracotta: '#205BB0',
  accentBg: '#E8F1FC',
  ink: '#263E53',
  muted: '#64788B',
  subtle: '#77899A',
  rule: '#DCE3E8',
  borderStrong: '#B8C9DB',
  suggest: '#107C41',
  suggestBg: '#EAF4ED',
  warn: '#A7792F',
  agent: '#21734A'
} as const;

export const font = {
  display: '"Public Sans", "Segoe UI", "Helvetica Neue", Arial, sans-serif',
  body: '"Public Sans", "Segoe UI", "Helvetica Neue", Arial, sans-serif',
  ui: '"Public Sans", "Segoe UI", "Helvetica Neue", Arial, sans-serif'
} as const;

export const fontMono = '"Cascadia Code", "SFMono-Regular", Consolas, monospace' as const;

export type Color = keyof typeof color;
export type Font = keyof typeof font;
