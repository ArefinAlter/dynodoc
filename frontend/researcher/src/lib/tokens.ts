/**
 * Design tokens for the Research IDE, ported from the prototype's design system
 * (docs/01 §6, docs/19). Warm parchment background, Fraunces / Source Serif 4 /
 * IBM Plex Sans typography, terracotta accent. Stage 12 stub — the full component
 * library is built in stage 19.
 */
export const color = {
  /** Warm parchment page background. */
  parchment: '#F4EFE6',
  /** Terracotta accent (links, active mode, primary actions). */
  terracotta: '#B14B2C',
  ink: '#2B2622',
  muted: '#6B635A',
  rule: '#DCD3C4'
} as const;

export const font = {
  /** Display / headings. */
  display: '"Fraunces", Georgia, serif',
  /** Body / item prose. */
  body: '"Source Serif 4", Georgia, serif',
  /** UI chrome (toolbars, inspector, labels). */
  ui: '"IBM Plex Sans", system-ui, sans-serif'
} as const;

export type Color = keyof typeof color;
export type Font = keyof typeof font;
