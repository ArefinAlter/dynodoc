/**
 * Design tokens for the Research IDE, ported from the prototype's design system
 * (docs/01 section 6, docs/19). Warm parchment background, Fraunces / Source Serif 4 /
 * IBM Plex Sans typography, terracotta accent.
 */
export const color = {
  parchment: '#F4EFE6',
  surface: '#FBF8F1',
  surfaceMuted: '#F0E7D8',
  terracotta: '#B14B2C',
  accentBg: '#F5E6DD',
  ink: '#2B2622',
  muted: '#6B635A',
  subtle: '#8A8170',
  rule: '#DCD3C4',
  borderStrong: '#C4B8A4',
  suggest: '#7B8F50',
  suggestBg: '#EDF1DD',
  warn: '#C18A3F',
  agent: '#4A6741'
} as const;

export const font = {
  display: '"Fraunces", Georgia, serif',
  body: '"Source Serif 4", Georgia, serif',
  ui: '"IBM Plex Sans", system-ui, sans-serif'
} as const;

export const fontMono = '"IBM Plex Mono", ui-monospace, monospace' as const;

export type Color = keyof typeof color;
export type Font = keyof typeof font;
