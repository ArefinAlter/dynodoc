import { ulid } from 'ulid';
import type { DocumentState, EventPayload, MaterializedNode } from '$lib/api/types';
import { between } from './changes';
import { stableJson } from './equality';

/** Slide decks reuse the ordinary node tree: root `form` → slide `section` → shape `item`.
 * Independently editable properties are separate fields so drafts merge per shape/field. */
export type SlideSize = '16:9' | '4:3';
export type SlideLayout = 'title' | 'title-content' | 'section' | 'two-content' | 'blank';
export type ShapeKind = 'text' | 'image' | 'shape';
export type ShapeRole = 'title' | 'body' | 'subtitle';
export type ThemeName = keyof typeof themes;
export interface ShapeStyle {
  size?: number;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  color?: string;
  align?: 'left' | 'center' | 'right';
  bullets?: boolean;
  fill?: string;
}
export interface ShapeFields {
  kind: ShapeKind;
  role?: ShapeRole;
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
  style: ShapeStyle;
  src?: string;
  alt?: string;
}
export interface SlideShape extends ShapeFields {
  id: string;
  pos: string;
}
export interface Slide {
  id: string;
  pos: string;
  label: string;
  layout: SlideLayout;
  notes: string;
  background?: string;
  hidden: boolean;
  shapes: SlideShape[];
}
export interface Deck {
  rootId?: string;
  title: string;
  size: SlideSize;
  theme: ThemeName;
  slides: Slide[];
}

/** Every slide is 7.5 in (540 pt) tall; point sizes scale against that height. */
export const SLIDE_HEIGHT_PT = 540;
export const MAX_IMAGE_BYTES = 1024 * 1024;
export const slideSizes = {
  '16:9': { width: 13.333, height: 7.5, ratio: 16 / 9, label: 'Widescreen (16:9)' },
  '4:3': { width: 10, height: 7.5, ratio: 4 / 3, label: 'Standard (4:3)' }
} as const;
export const layouts: { value: SlideLayout; label: string }[] = [
  { value: 'title', label: 'Title slide' },
  { value: 'title-content', label: 'Title and content' },
  { value: 'section', label: 'Section header' },
  { value: 'two-content', label: 'Two content' },
  { value: 'blank', label: 'Blank' }
];
export const themes = {
  office: {
    label: 'Office',
    background: '#ffffff',
    title: '#1f1f1f',
    text: '#333333',
    accent: '#c43e1c',
    font: 'Segoe UI'
  },
  slate: {
    label: 'Slate',
    background: '#1f2933',
    title: '#ffffff',
    text: '#e4e7eb',
    accent: '#f0b429',
    font: 'Segoe UI'
  },
  ocean: {
    label: 'Ocean',
    background: '#eaf2f8',
    title: '#0b3c5d',
    text: '#1d2731',
    accent: '#328cc1',
    font: 'Calibri'
  },
  forest: {
    label: 'Forest',
    background: '#f1f6ef',
    title: '#1e4d2b',
    text: '#2f3e33',
    accent: '#107c41',
    font: 'Georgia'
  },
  sunrise: {
    label: 'Sunrise',
    background: '#fff6ec',
    title: '#8a2c0d',
    text: '#3d2c24',
    accent: '#e8710a',
    font: 'Calibri'
  }
} as const;
export const placeholderText: Record<ShapeRole, string> = {
  title: 'Click to add title',
  subtitle: 'Click to add subtitle',
  body: 'Click to add text'
};

const layoutNames = new Set<string>(layouts.map((l) => l.value));
export const safeColor = (value: unknown) =>
  typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value) ? value.toLowerCase() : undefined;
/** Only inline raster images: no remote requests, scripts or tracking pixels from shared decks. */
export const safeImage = (value: unknown) =>
  typeof value === 'string' && /^data:image\/(png|jpeg|gif|webp);base64,/i.test(value)
    ? value
    : undefined;
const number = (value: unknown, fallback: number, min: number, max: number) => {
  const n = Number(value);
  return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : fallback;
};
export const round = (n: number) => Math.round(n * 100) / 100;
export function readStyle(value: unknown): ShapeStyle {
  const v = (value && typeof value === 'object' ? value : {}) as Record<string, unknown>;
  const style: ShapeStyle = {};
  if (v.size !== undefined && v.size !== null) style.size = number(v.size, 18, 6, 200);
  for (const key of ['bold', 'italic', 'underline', 'bullets'] as const)
    if (typeof v[key] === 'boolean') style[key] = v[key] as boolean;
  const color = safeColor(v.color),
    fill = safeColor(v.fill);
  if (color) style.color = color;
  if (fill) style.fill = fill;
  if (v.align === 'left' || v.align === 'center' || v.align === 'right') style.align = v.align;
  return style;
}
export function themeName(value: unknown): ThemeName {
  return typeof value === 'string' && value in themes ? (value as ThemeName) : 'office';
}
const order = (a: MaterializedNode, b: MaterializedNode) =>
  a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
export function readShape(n: MaterializedNode): SlideShape {
  const f = n.current_fields;
  const kind: ShapeKind = f.kind === 'image' || f.kind === 'shape' ? f.kind : 'text';
  const role =
    f.role === 'title' || f.role === 'body' || f.role === 'subtitle' ? f.role : undefined;
  return {
    id: n.id,
    pos: n.pos,
    kind,
    ...(role ? { role } : {}),
    text: typeof f.text === 'string' ? f.text : String(f.label ?? ''),
    x: number(f.x, 10, -100, 200),
    y: number(f.y, 10, -100, 200),
    w: number(f.w, 30, 0.5, 300),
    h: number(f.h, 20, 0.5, 300),
    style: readStyle(f.style),
    ...(safeImage(f.src) ? { src: safeImage(f.src) } : {}),
    ...(typeof f.alt === 'string' ? { alt: f.alt } : {})
  };
}
export function readDeck(state: DocumentState): Deck {
  const live = Object.values(state.nodes).filter((n) => !n.deleted);
  const root = live.find((n) => n.type === 'form');
  const children = new Map<string, MaterializedNode[]>();
  for (const n of live)
    if (n.parent_id) {
      const list = children.get(n.parent_id);
      if (list) list.push(n);
      else children.set(n.parent_id, [n]);
    }
  const slides = (root ? children.get(root.id) || [] : [])
    .filter((n) => n.type === 'section' && n.current_fields.kind === 'slide')
    .sort(order)
    .map((n): Slide => {
      const f = n.current_fields;
      const background = safeColor(f.background);
      return {
        id: n.id,
        pos: n.pos,
        label: String(f.label ?? ''),
        layout: layoutNames.has(String(f.layout)) ? (f.layout as SlideLayout) : 'title-content',
        notes: typeof f.notes === 'string' ? f.notes : '',
        ...(background ? { background } : {}),
        hidden: f.hidden === true,
        shapes: (children.get(n.id) || [])
          .filter((c) => c.type === 'item')
          .sort(order)
          .map(readShape)
      };
    });
  return {
    rootId: root?.id,
    title: String(root?.current_fields.label ?? ''),
    size: root?.current_fields.slide_size === '4:3' ? '4:3' : '16:9',
    theme: themeName(root?.current_fields.theme),
    slides
  };
}

/** Plain text labels keep history, search and comment lists readable. */
export const shapeLabel = (shape: Pick<ShapeFields, 'kind' | 'text' | 'alt'>) =>
  (shape.kind === 'image' ? shape.alt || 'Image' : shape.text.replace(/\s+/g, ' ').trim()).slice(
    0,
    120
  );
export const slideLabel = (title: string) =>
  title.replace(/\s+/g, ' ').trim().slice(0, 120) || 'Untitled slide';
export function titleOf(slide: Pick<Slide, 'shapes'>) {
  return slide.shapes.find((s) => s.role === 'title')?.text || '';
}

const text = (
  role: ShapeRole,
  x: number,
  y: number,
  w: number,
  h: number,
  style: ShapeStyle
): ShapeFields => ({ kind: 'text', role, text: '', x, y, w, h, style });
/** Placeholder geometry in percent of the slide, close to PowerPoint's default layouts. */
export function layoutShapes(layout: SlideLayout): ShapeFields[] {
  const heading = text('title', 6.25, 5.5, 87.5, 18, { size: 36 });
  const body = (x: number, w: number) =>
    text('body', x, 26.5, w, 63, { size: layout === 'two-content' ? 22 : 24, bullets: true });
  switch (layout) {
    case 'title':
      return [
        text('title', 8.33, 18, 83.33, 32, { size: 44, align: 'center' }),
        text('subtitle', 12.5, 53, 75, 22, { size: 24, align: 'center' })
      ];
    case 'section':
      return [
        text('title', 6.25, 25, 87.5, 36, { size: 44 }),
        text('subtitle', 6.25, 63, 87.5, 18, { size: 20 })
      ];
    case 'two-content':
      return [heading, body(6.25, 42.5), body(51.25, 42.5)];
    case 'blank':
      return [];
    default:
      return [heading, body(6.25, 87.5)];
  }
}
function shapeCreated(slideId: string, fields: ShapeFields, pos: string, id = ulid()) {
  return {
    type: 'NodeCreated',
    node_id: id,
    node_type: 'item',
    parent_id: slideId,
    pos,
    fields: { ...fields, label: shapeLabel(fields) }
  } satisfies EventPayload;
}
const sequence = (i: number) => String(i).padStart(4, '0');

export function newSlideOps(
  deck: Deck,
  layout: SlideLayout,
  afterId?: string,
  title = ''
): { ops: EventPayload[]; id: string } {
  if (!deck.rootId) return { ops: [], id: '' };
  let index = afterId ? deck.slides.findIndex((s) => s.id === afterId) : -1;
  if (index < 0) index = deck.slides.length - 1;
  const id = ulid();
  const shapes = layoutShapes(layout).map((s) => (s.role === 'title' ? { ...s, text: title } : s));
  return {
    id,
    ops: [
      {
        type: 'NodeCreated',
        node_id: id,
        node_type: 'section',
        parent_id: deck.rootId,
        pos: between(deck.slides[index]?.pos || '', deck.slides[index + 1]?.pos),
        fields: { label: slideLabel(title), kind: 'slide', layout, notes: '' }
      },
      ...shapes.map((s, i) => shapeCreated(id, s, sequence(i)))
    ]
  };
}
export function duplicateSlideOps(deck: Deck, slideId: string) {
  const index = deck.slides.findIndex((s) => s.id === slideId);
  const slide = deck.slides[index];
  if (!slide || !deck.rootId) return { ops: [] as EventPayload[], id: '' };
  const id = ulid();
  const ops: EventPayload[] = [
    {
      type: 'NodeCreated',
      node_id: id,
      node_type: 'section',
      parent_id: deck.rootId,
      pos: between(slide.pos, deck.slides[index + 1]?.pos),
      fields: {
        label: slide.label,
        kind: 'slide',
        layout: slide.layout,
        notes: slide.notes,
        ...(slide.background ? { background: slide.background } : {}),
        ...(slide.hidden ? { hidden: true } : {})
      }
    }
  ];
  for (const shape of slide.shapes) {
    const { id: _id, pos, ...fields } = shape;
    void _id;
    ops.push(shapeCreated(id, fields, pos));
  }
  return { ops, id };
}
/** The engine refuses to delete a parent with live children, so shapes go first. */
export function deleteSlideOps(state: DocumentState, slideId: string): EventPayload[] {
  const ids: string[] = [];
  const visit = (id: string) => {
    for (const n of Object.values(state.nodes)) if (n.parent_id === id && !n.deleted) visit(n.id);
    ids.push(id);
  };
  if (state.nodes[slideId] && !state.nodes[slideId].deleted) visit(slideId);
  return ids.map((node_id) => ({ type: 'NodeDeleted', node_id }));
}
/** Reorder with a single dense key between the new neighbours; untouched slides keep keys. */
export function moveOps(
  items: { id: string; pos: string }[],
  id: string,
  to: number,
  parentId: string
): EventPayload[] {
  const from = items.findIndex((s) => s.id === id);
  if (from < 0 || to < 0 || to >= items.length || to === from) return [];
  const rest = items.filter((s) => s.id !== id);
  const left = rest[to - 1]?.pos || '',
    right = rest[to]?.pos;
  const pos = between(left, right);
  if (pos > left && (!right || pos < right))
    return [{ type: 'NodeMoved', node_id: id, new_parent_id: parentId, new_pos: pos }];
  // Concurrent inserts can leave equal keys; renumber only in that rare case.
  rest.splice(to, 0, items[from]);
  return rest.map((s, i) => ({
    type: 'NodeMoved',
    node_id: s.id,
    new_parent_id: parentId,
    new_pos: String(i).padStart(8, '0')
  }));
}
export function moveSlideOps(deck: Deck, slideId: string, delta: number) {
  const index = deck.slides.findIndex((s) => s.id === slideId);
  return deck.rootId ? moveOps(deck.slides, slideId, index + delta, deck.rootId) : [];
}
export function arrangeShapeOps(slide: Slide, shapeId: string, place: 'front' | 'back') {
  return moveOps(slide.shapes, shapeId, place === 'front' ? slide.shapes.length - 1 : 0, slide.id);
}
export function addShapeOps(slide: Slide, fields: ShapeFields) {
  const id = ulid();
  return { id, ops: [shapeCreated(slide.id, fields, between(slide.shapes.at(-1)?.pos || ''), id)] };
}
/** One FieldEdited per changed field; derived labels follow text changes. */
export function editShapeOps(
  slide: Slide,
  shape: SlideShape,
  patch: Partial<ShapeFields>
): EventPayload[] {
  const ops: EventPayload[] = [];
  for (const [field, value] of Object.entries(patch) as [keyof ShapeFields, unknown][]) {
    if (value === undefined) continue;
    const next = typeof value === 'number' ? round(value) : value;
    if (stableJson(shape[field] ?? null) !== stableJson(next))
      ops.push({ type: 'FieldEdited', node_id: shape.id, field, value: next });
  }
  const textEdit = ops.some((op) => op.type === 'FieldEdited' && op.field === 'text');
  if (textEdit || (patch.alt !== undefined && patch.alt !== shape.alt)) {
    const label = shapeLabel({ ...shape, ...patch });
    ops.push({ type: 'FieldEdited', node_id: shape.id, field: 'label', value: label });
  }
  if (textEdit && shape.role === 'title') {
    const label = slideLabel(patch.text || '');
    if (label !== slide.label)
      ops.push({ type: 'FieldEdited', node_id: slide.id, field: 'label', value: label });
  }
  return ops;
}
export function editSlideOps(slide: Slide, patch: Record<string, unknown>): EventPayload[] {
  return Object.entries(patch)
    .filter(
      ([field, value]) =>
        stableJson((slide as unknown as Record<string, unknown>)[field] ?? null) !==
        stableJson(value ?? null)
    )
    .map(([field, value]) => ({ type: 'FieldEdited', node_id: slide.id, field, value }));
}
/** Switching layout moves matching placeholders, adds missing ones and removes only empty
 * placeholders the new layout does not use. Typed content is never discarded. */
export function applyLayoutOps(slide: Slide, layout: SlideLayout): EventPayload[] {
  const ops: EventPayload[] = [];
  if (slide.layout !== layout)
    ops.push({ type: 'FieldEdited', node_id: slide.id, field: 'layout', value: layout });
  const unused = [...slide.shapes.filter((s) => s.role)];
  let last = slide.shapes.at(-1)?.pos || '';
  for (const preset of layoutShapes(layout)) {
    const index = unused.findIndex(
      (s) =>
        s.role === preset.role ||
        (preset.role !== 'title' && s.role !== 'title' && s.kind === 'text')
    );
    if (index >= 0) {
      const [shape] = unused.splice(index, 1);
      const { x, y, w, h, role } = preset;
      const retyped =
        shape.role === role
          ? {}
          : {
              role,
              style: { ...shape.style, size: preset.style.size, bullets: !!preset.style.bullets }
            };
      ops.push(...editShapeOps(slide, shape, { x, y, w, h, ...retyped }));
    } else {
      last = between(last);
      ops.push(shapeCreated(slide.id, preset, last));
    }
  }
  for (const shape of unused)
    if (!shape.text.trim()) ops.push({ type: 'NodeDeleted', node_id: shape.id });
  return ops;
}
/** Starter deck: root plus one title slide carrying the document name. */
export function initialPresentationOps(title: string, rootId = ulid()): EventPayload[] {
  const root: EventPayload = {
    type: 'NodeCreated',
    node_id: rootId,
    node_type: 'form',
    pos: '000000',
    fields: { label: title, slide_size: '16:9', theme: 'office' }
  };
  const deck: Deck = { rootId, title, size: '16:9', theme: 'office', slides: [] };
  return [root, ...newSlideOps(deck, 'title', undefined, title).ops];
}
export interface ImportedSlide {
  layout: SlideLayout;
  notes: string;
  background?: string;
  hidden: boolean;
  shapes: ShapeFields[];
}
/** Creation operations for an imported deck (initial import may exceed the edit batch size). */
export function deckOps(
  title: string,
  size: SlideSize,
  slides: ImportedSlide[],
  rootId = ulid()
): EventPayload[] {
  const ops: EventPayload[] = [
    {
      type: 'NodeCreated',
      node_id: rootId,
      node_type: 'form',
      pos: '000000',
      fields: { label: title, slide_size: size, theme: 'office' }
    }
  ];
  slides.forEach((slide, i) => {
    const id = ulid();
    const heading = slide.shapes.find((s) => s.role === 'title')?.text || '';
    ops.push({
      type: 'NodeCreated',
      node_id: id,
      node_type: 'section',
      parent_id: rootId,
      pos: String(i).padStart(8, '0'),
      fields: {
        label: slideLabel(heading),
        kind: 'slide',
        layout: slide.layout,
        notes: slide.notes,
        ...(slide.background ? { background: slide.background } : {}),
        ...(slide.hidden ? { hidden: true } : {})
      }
    });
    slide.shapes.forEach((shape, j) => ops.push(shapeCreated(id, shape, sequence(j))));
  });
  return ops;
}
/** Best-effort layout name when a source file does not state one. */
export function inferLayout(shapes: Pick<ShapeFields, 'role'>[]): SlideLayout {
  const roles = shapes.map((s) => s.role);
  const bodies = roles.filter((r) => r === 'body').length;
  if (!roles.some(Boolean)) return 'blank';
  if (bodies >= 2) return 'two-content';
  if (bodies === 1) return 'title-content';
  if (roles.includes('subtitle')) return 'title';
  return 'title-content';
}
export function defaultSize(shape: Pick<ShapeFields, 'role' | 'kind'>) {
  return shape.role === 'title' ? 36 : shape.role ? 24 : 18;
}
/** Resolved presentation values shared by the editor, slide show, print and PPTX export. */
export function resolvedStyle(shape: ShapeFields, theme: ThemeName) {
  const t = themes[theme];
  return {
    size: shape.style.size ?? defaultSize(shape),
    color:
      shape.style.color ??
      (shape.kind === 'shape' ? '#ffffff' : shape.role === 'title' ? t.title : t.text),
    fill: shape.style.fill ?? (shape.kind === 'shape' ? t.accent : undefined),
    align: shape.style.align ?? (shape.kind === 'shape' ? 'center' : 'left'),
    valign: shape.role === 'title' || shape.kind === 'shape' ? 'middle' : 'top',
    font: t.font
  } as const;
}
