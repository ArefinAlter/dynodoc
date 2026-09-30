/**
 * Day and night colour helpers for the workspace editors.
 *
 * Authored formatting (cell fills and fonts, text colours, highlights, shaded table
 * cells) is content. Nothing here rewrites it or reaches an export: these helpers
 * only choose which on-screen default applies next to it, so authored colours stay
 * readable when the interface is in night mode.
 */
import { Extension } from '@tiptap/core';
import type { Node as PMNode } from '@tiptap/pm/model';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

export interface Rgba {
  r: number;
  g: number;
  b: number;
  /** 0 (transparent) to 1 (opaque). */
  a: number;
}

/** Text on a light authored fill. */
export const DARK_INK = '#1f2328';
/** Text on a dark authored fill. */
export const LIGHT_INK = '#f3f5f8';

const NAMED: Record<string, string> = { black: '#000000', white: '#ffffff' };
const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

/**
 * Read `#rgb`, `#rrggbb`, `#aarrggbb` (alpha first, as spreadsheet ARGB values are
 * written), `rgb()`/`rgba()` in comma or space syntax, and the names black and white.
 * Anything else returns null.
 */
export function parseColor(value: unknown): Rgba | null {
  if (typeof value !== 'string') return null;
  const text = value.trim().toLowerCase();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/.exec(NAMED[text] ?? text);
  if (hex) {
    let digits = hex[1];
    if (digits.length === 3) digits = [...digits].map((d) => d + d).join('');
    const a = digits.length === 8 ? parseInt(digits.slice(0, 2), 16) / 255 : 1;
    if (digits.length === 8) digits = digits.slice(2);
    return {
      r: parseInt(digits.slice(0, 2), 16),
      g: parseInt(digits.slice(2, 4), 16),
      b: parseInt(digits.slice(4, 6), 16),
      a
    };
  }
  const call = /^rgba?\(([^)]*)\)$/.exec(text);
  if (!call) return null;
  const parts = call[1].split(/[\s,/]+/).filter(Boolean);
  if (parts.length < 3 || parts.length > 4) return null;
  const numbers = parts.map((p) => (/^\d*\.?\d+%?$/.test(p) ? parseFloat(p) : NaN));
  if (numbers.some((n) => Number.isNaN(n))) return null;
  const channel = (i: number) =>
    clamp(parts[i].endsWith('%') ? Math.round(numbers[i] * 2.55) : numbers[i], 0, 255);
  const alpha =
    parts.length === 4 ? clamp(parts[3].endsWith('%') ? numbers[3] / 100 : numbers[3], 0, 1) : 1;
  return { r: channel(0), g: channel(1), b: channel(2), a: alpha };
}

/** WCAG relative luminance from 0 (black) to 1 (white); null when the colour can't be read. */
export function luminance(color: unknown): number | null {
  const c = parseColor(color);
  if (!c) return null;
  const linear = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b);
}

/** WCAG contrast ratio (1 to 21); null when either colour can't be read. */
export function contrast(a: unknown, b: unknown): number | null {
  const x = luminance(a),
    y = luminance(b);
  if (x === null || y === null) return null;
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** DARK_INK or LIGHT_INK, whichever reads better on `fill`. Unreadable fills get DARK_INK. */
export function readableInk(fill: unknown): string {
  const dark = contrast(fill, DARK_INK),
    light = contrast(fill, LIGHT_INK);
  return dark === null || light === null || dark >= light ? DARK_INK : LIGHT_INK;
}

/**
 * Black and very dark grey (low luminance, little hue): the colours night mode shows
 * as the automatic ink, as Word does. Dark but clearly coloured text (navy, maroon)
 * is left as authored.
 */
export function isNearBlack(color: unknown): boolean {
  const c = parseColor(color);
  if (!c || c.a === 0) return false;
  return (
    (luminance(color) ?? 1) <= 0.055 && Math.max(c.r, c.g, c.b) - Math.min(c.r, c.g, c.b) <= 48
  );
}

/** A fill that paints something: readable and not fully transparent. */
export function isFill(fill: unknown): fill is string {
  const c = parseColor(fill);
  return !!c && c.a > 0;
}

export interface CellColors {
  background?: string;
  color?: string;
}

/**
 * On-screen colours for a spreadsheet cell (or a run of document text) with an
 * authored `fill` and text `color`. An undefined value means "use the interface
 * default" (the grid or page tokens). Authored values are returned unchanged:
 * - nothing authored: nothing;
 * - a fill but no text colour: the fill, with readable ink for it;
 * - near-black text and no fill, in night mode: nothing, so the default ink applies;
 * - otherwise the authored values.
 */
export function cellColors(
  { fill, color }: { fill?: unknown; color?: unknown },
  dark: boolean
): CellColors {
  const background = isFill(fill) ? fill : undefined;
  const text = parseColor(color) ? String(color) : undefined;
  if (!background) return !text || (dark && isNearBlack(text)) ? {} : { color: text };
  return { background, color: text ?? readableInk(background) };
}

/** Per-browser option that keeps document pages white in night mode. */
export const LIGHT_PAGES_KEY = 'dynodoc.light-pages';
export function readLightPages(): boolean {
  try {
    return localStorage.getItem(LIGHT_PAGES_KEY) === '1';
  } catch {
    return false;
  }
}
export function writeLightPages(on: boolean) {
  try {
    if (on) localStorage.setItem(LIGHT_PAGES_KEY, '1');
    else localStorage.removeItem(LIGHT_PAGES_KEY);
  } catch {
    /* Storage can be disabled; the option then lasts for this page only. */
  }
}

/** The colour a `<mark>` without a colour attribute shows (the browser default). */
const DEFAULT_HIGHLIGHT = '#ffff00';

/**
 * The on-screen text colour a document text run needs in night mode, or undefined
 * when the inherited colour is already right. `cellFill` is the shading of the table
 * cell the text is in.
 */
export function runInk(node: PMNode, cellFill?: string): string | undefined {
  let color: unknown, fill: string | undefined;
  for (const mark of node.marks) {
    const name = mark.type.name;
    // Tracked insertions and deletions keep their review colours.
    if (name === 'trackedChange') return undefined;
    if (name === 'textStyle') {
      if (parseColor(mark.attrs.color)) color = mark.attrs.color;
      if (isFill(mark.attrs.backgroundColor)) fill ??= mark.attrs.backgroundColor;
    }
    // The highlight is drawn inside the text-style span, so it paints over its background.
    if (name === 'highlight')
      fill = isFill(mark.attrs.color) ? mark.attrs.color : DEFAULT_HIGHLIGHT;
  }
  if (fill) return color ? undefined : readableInk(fill);
  // Shaded cells get their ink from the cell decoration; authored colours stay.
  if (cellFill || !color) return undefined;
  return isNearBlack(color) ? 'var(--color-page-ink)' : undefined;
}

/** Decorations that give night-mode documents readable ink; empty when inactive. */
export function inkDecorations(doc: PMNode, active: boolean): DecorationSet {
  if (!active) return DecorationSet.empty;
  const found: Decoration[] = [];
  const walk = (parent: PMNode, start: number, cellFill?: string) => {
    parent.forEach((node, offset) => {
      const pos = start + offset;
      if (node.isText) {
        const ink = runInk(node, cellFill);
        if (ink) found.push(Decoration.inline(pos, pos + node.nodeSize, { style: `color:${ink}` }));
        return;
      }
      let fill = cellFill;
      if (
        (node.type.name === 'tableCell' || node.type.name === 'tableHeader') &&
        isFill(node.attrs.backgroundColor)
      ) {
        fill = node.attrs.backgroundColor;
        found.push(
          Decoration.node(pos, pos + node.nodeSize, { style: `color:${readableInk(fill)}` })
        );
      }
      if (!node.isLeaf) walk(node, pos + 1, fill);
    });
  };
  walk(doc, 0);
  return DecorationSet.create(doc, found);
}

export const themeInkKey = new PluginKey<DecorationSet>('themeInk');

/**
 * Night-mode ink for documents, as decorations only (never document changes, so
 * history, collaboration and exports are untouched). Shaded table cells and text on
 * a highlight get readable ink for their fill; black or near-black authored text on
 * the page shows in the page ink. `active()` is read when the document changes and
 * when a transaction carries the `themeInkKey` meta (dispatch one when the mode changes).
 */
export const ThemeInk = Extension.create<{ active: () => boolean }>({
  name: 'themeInk',
  addOptions() {
    return { active: () => false };
  },
  addProseMirrorPlugins() {
    const options = this.options;
    return [
      new Plugin<DecorationSet>({
        key: themeInkKey,
        state: {
          init: (_, state) => inkDecorations(state.doc, options.active()),
          apply: (tr, value, _old, state) =>
            tr.docChanged || tr.getMeta(themeInkKey)
              ? inkDecorations(state.doc, options.active())
              : value
        },
        props: { decorations: (state) => themeInkKey.getState(state) }
      })
    ];
  }
});
