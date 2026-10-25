// Simple line icons drawn for Dynodoc (24 by 24, stroked with the current colour). Each is a list
// of SVG path commands, circles or rectangles; nothing is fetched.
import { s } from './dom.ts';

type Shape = string | { c: [number, number, number] } | { r: [number, number, number, number, number] };

const I: Record<string, Shape[]> = {
  changes: ['M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z', 'M14 3v5h5', 'M9 14h6', 'M12 11v6'],
  history: ['M3 12a9 9 0 1 0 3-6.7', 'M3 4v4h4', 'M12 7v5l3 2'],
  request: [{ c: [6, 6, 2.2] }, { c: [6, 18, 2.2] }, { c: [18, 18, 2.2] }, 'M6 8.2v7.6', 'M18 15.8V11a3 3 0 0 0-3-3h-4', 'M13 5.5 10.5 8l2.5 2.5'],
  documents: ['M8 3h8l4 4v11a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z', 'M16 3v4h4', 'M3 7v12a2 2 0 0 0 2 2h9'],
  projects: ['M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'],
  connectors: ['M9 7V3', 'M15 7V3', 'M6 7h12v4a6 6 0 0 1-12 0z', 'M12 17v4'],
  activity: ['M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9', 'M10.3 21a1.9 1.9 0 0 0 3.4 0'],
  settings: [{ c: [12, 12, 3] }, 'M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z'],
  agent: ['M12 3l1.8 4.2L18 9l-4.2 1.8L12 15l-1.8-4.2L6 9l4.2-1.8z', 'M19 15l.8 1.7 1.7.8-1.7.8L19 20l-.8-1.7-1.7-.8 1.7-.8z', 'M5 16l.6 1.3 1.4.7-1.4.6L5 20l-.6-1.4L3 18l1.4-.7z'],
  branch: [{ c: [6, 5, 2.2] }, { c: [6, 19, 2.2] }, { c: [18, 7, 2.2] }, 'M6 7.2v9.6', 'M18 9.2c0 4-6 3.5-11 7.8'],
  repo: ['M5 4.5A2.5 2.5 0 0 1 7.5 2H19v16H7.5A2.5 2.5 0 0 0 5 20.5z', 'M5 20.5A2.5 2.5 0 0 0 7.5 23H19v-5', 'M9 6h6'],
  sync: ['M20 11a8 8 0 0 0-14.8-4', 'M4 4v4h4', 'M4 13a8 8 0 0 0 14.8 4', 'M20 20v-4h-4'],
  up: ['M12 19V5', 'M6 11l6-6 6 6'],
  down: ['M12 5v14', 'M6 13l6 6 6-6'],
  search: [{ c: [11, 11, 7] }, 'M20 20l-3.5-3.5'],
  plus: ['M12 5v14', 'M5 12h14'],
  check: ['M5 12.5l4.5 4.5L19 7.5'],
  x: ['M6 6l12 12', 'M18 6L6 18'],
  chevron: ['M6 9l6 6 6-6'],
  chevronRight: ['M9 6l6 6-6 6'],
  more: [{ c: [5, 12, 1.2] }, { c: [12, 12, 1.2] }, { c: [19, 12, 1.2] }],
  sidebar: [{ r: [3, 4, 18, 16, 2] }, 'M9 4v16'],
  sun: [{ c: [12, 12, 4] }, 'M12 2v2', 'M12 20v2', 'M4.9 4.9l1.4 1.4', 'M17.7 17.7l1.4 1.4', 'M2 12h2', 'M20 12h2', 'M4.9 19.1l1.4-1.4', 'M17.7 6.3l1.4-1.4'],
  moon: ['M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z'],
  shield: ['M12 3l8 3v6c0 4.5-3.4 8.3-8 9-4.6-.7-8-4.5-8-9V6z', 'M8.5 12l2.5 2.5 4.5-4.5'],
  shieldX: ['M12 3l8 3v6c0 4.5-3.4 8.3-8 9-4.6-.7-8-4.5-8-9V6z', 'M9.5 9.5l5 5', 'M14.5 9.5l-5 5'],
  lock: [{ r: [5, 11, 14, 10, 2] }, 'M8 11V7a4 4 0 0 1 8 0v4'],
  word: [{ r: [4, 3, 16, 18, 2] }, 'M7.5 8l1.5 8 3-6 3 6 1.5-8'],
  sheet: [{ r: [4, 3, 16, 18, 2] }, 'M4 9h16', 'M4 15h16', 'M10 3v18'],
  slides: [{ r: [3, 4, 18, 13, 2] }, 'M12 17v4', 'M8 21h8', 'M8 9h8', 'M8 12h5'],
  file: ['M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z', 'M14 3v5h5'],
  user: [{ c: [12, 8, 4] }, 'M4 21a8 8 0 0 1 16 0'],
  users: [{ c: [9, 8, 3.5] }, 'M2.5 20a6.5 6.5 0 0 1 13 0', 'M16 4.5a3.5 3.5 0 0 1 0 7', 'M18 14a6.5 6.5 0 0 1 3.5 6'],
  key: [{ c: [8, 15, 4] }, 'M11 12l9-9', 'M17 6l3 3', 'M15 8l2 2'],
  comment: ['M21 12a8 8 0 0 1-11.6 7.1L4 20l1.2-4.3A8 8 0 1 1 21 12z'],
  merge: [{ c: [6, 5, 2.2] }, { c: [6, 19, 2.2] }, { c: [18, 12, 2.2] }, 'M6 7.2v9.6', 'M6 7.5c0 3 4 4.5 9.8 4.5'],
  revert: ['M9 14L4 9l5-5', 'M4 9h10.5a5.5 5.5 0 0 1 0 11H11'],
  cherry: [{ c: [7, 17, 3] }, { c: [17, 17, 3] }, 'M7 14c1-5 4-8 9-10', 'M17 14c-.5-4-1-7-1-10'],
  compare: ['M8 3v13', 'M4 12l4 4 4-4', 'M16 21V8', 'M12 12l4-4 4 4'],
  graph: [{ c: [6, 6, 2] }, { c: [6, 18, 2] }, { c: [18, 12, 2] }, 'M6 8v8', 'M8 6.5c5 0 8 2 8.2 3.6', 'M8 17.5c5 0 8-2 8.2-3.6'],
  star: ['M12 3l2.8 5.7 6.2.9-4.5 4.4 1 6.2L12 17.3 6.5 20.2l1-6.2L3 9.6l6.2-.9z'],
  eye: ['M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z', { c: [12, 12, 3] }],
  link: ['M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1', 'M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1'],
  external: ['M14 4h6v6', 'M20 4l-9 9', 'M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5'],
  trash: ['M4 7h16', 'M10 11v6', 'M14 11v6', 'M5 7l1 13a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1l1-13', 'M9 7V4h6v3'],
  edit: ['M4 20h4L19 9l-4-4L4 16z', 'M13.5 6.5l4 4'],
  info: [{ c: [12, 12, 9] }, 'M12 11v6', 'M12 7.5v.5'],
  warning: ['M12 3l10 18H2z', 'M12 10v5', 'M12 18v.5'],
  issue: [{ c: [12, 12, 9] }, { c: [12, 12, 1.5] }],
  send: ['M4 12l16-8-6 16-2.5-6.5z'],
  stop: [{ r: [6, 6, 12, 12, 2] }],
  paperclip: ['M20 11.5l-8.3 8.3a5 5 0 0 1-7-7L13 4.5a3.3 3.3 0 0 1 4.7 4.7l-8.2 8.2a1.7 1.7 0 0 1-2.4-2.4l7.6-7.6'],
  skill: ['M4 5h16v14H4z', 'M8 9l3 3-3 3', 'M13 15h3'],
  keyboard: [{ r: [2, 6, 20, 12, 2] }, 'M6 10h.5', 'M10 10h.5', 'M14 10h.5', 'M18 10h.5', 'M7 14h10'],
  palette: [{ c: [12, 12, 9] }, { c: [8, 10, 1.2] }, { c: [12, 7.5, 1.2] }, { c: [16, 10, 1.2] }, 'M12 21a2 2 0 0 1 0-4h2a3 3 0 0 0 3-3'],
  bell: ['M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9', 'M10.3 21a1.9 1.9 0 0 0 3.4 0'],
  server: [{ r: [3, 4, 18, 7, 2] }, { r: [3, 13, 18, 7, 2] }, 'M7 7.5h.5', 'M7 16.5h.5'],
  accessibility: [{ c: [12, 4.5, 1.8] }, 'M5 8.5l7 1.5 7-1.5', 'M12 10v4l-3 7', 'M12 14l3 7'],
  prompt: [{ c: [12, 12, 9] }, 'M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6v.6', 'M12 17v.5'],
  advanced: ['M4 6h10', 'M18 6h2', { c: [16, 6, 2] }, 'M4 12h2', 'M10 12h10', { c: [8, 12, 2] }, 'M4 18h8', 'M16 18h4', { c: [14, 18, 2] }],
  admin: ['M12 3l8 3v6c0 4.5-3.4 8.3-8 9-4.6-.7-8-4.5-8-9V6z'],
  layers: ['M12 3l9 5-9 5-9-5z', 'M3 13l9 5 9-5'],
  split: [{ r: [3, 4, 18, 16, 2] }, 'M12 4v16'],
  unified: [{ r: [3, 4, 18, 16, 2] }, 'M3 12h18'],
  filter: ['M4 5h16l-6 7.5V19l-4 2v-8.5z'],
  pin: ['M9 4h6l-1 6 3 3H7l3-3z', 'M12 13v8'],
  dot: [{ c: [12, 12, 4] }],
  globe: [{ c: [12, 12, 9] }, 'M3 12h18', 'M12 3a14 14 0 0 1 0 18', 'M12 3a14 14 0 0 0 0 18'],
};

/** An icon by name; an unknown name draws an empty box rather than failing. */
export function icon(name: string, size = 16, cls = 'icon'): SVGElement {
  const shapes = I[name] ?? [];
  return s(
    'svg',
    { class: cls, width: size, height: size, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': 1.75, 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true', focusable: 'false' },
    shapes.map((sh) => {
      if (typeof sh === 'string') return s('path', { d: sh });
      if ('c' in sh) return s('circle', { cx: sh.c[0], cy: sh.c[1], r: sh.c[2] });
      return s('rect', { x: sh.r[0], y: sh.r[1], width: sh.r[2], height: sh.r[3], rx: sh.r[4] });
    }),
  );
}

/** The Dynodoc mark, drawn small for the sidebar's brand row. */
export function mark(size = 18): SVGElement {
  return s(
    'svg',
    { class: 'mark', width: size, height: size, viewBox: '0 0 24 24', 'aria-hidden': 'true' },
    s('rect', { x: 1, y: 1, width: 22, height: 22, rx: 6, class: 'mark-bg' }),
    s('path', { d: 'M7 6.5h5a5.5 5.5 0 0 1 0 11H7z', class: 'mark-fg', fill: 'none', 'stroke-width': 2.2 }),
    s('circle', { cx: 12, cy: 12, r: 1.8, class: 'mark-dot' }),
  );
}

export const ICON_NAMES = Object.keys(I);
