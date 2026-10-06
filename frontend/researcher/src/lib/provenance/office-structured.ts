/** Native Office adapters for existing cells/formulas and slide shape text.
 * Read/write APIs are stable ExcelApi 1.4 / PowerPointApi 1.4 subsets. */
import {
  bindIdentical,
  sameBlocks,
  blockKey,
  structuredTargets,
  structuredGroups,
  type Block,
  type Checkpoint,
  type StructuredTarget
} from './protocol';
interface Collection<T> {
  items: T[];
  load: (fields: string) => void;
}
interface Range {
  values: (string | number | boolean)[][];
  formulas: (string | number | boolean)[][];
  rowIndex: number;
  columnIndex: number;
  rowCount: number;
  columnCount: number;
  isNullObject: boolean;
  load: (fields: string) => void;
  worksheet: { name: string; load: (fields: string) => void };
  getCell: (row: number, column: number) => Range;
}
interface NamedItem {
  name: string;
  isNullObject: boolean;
  load: (fields: string) => void;
  getRange: () => Range;
}
interface Names extends Collection<NamedItem> {
  getItemOrNullObject: (name: string) => NamedItem;
  add: (name: string, range: Range) => unknown;
}
interface Worksheet {
  name: string;
  getUsedRangeOrNullObject: (valuesOnly: boolean) => Range;
  getRangeByIndexes: (r: number, c: number, rows: number, cols: number) => Range;
}
interface ExcelContext {
  workbook: { worksheets: Collection<Worksheet>; names: Names };
  sync: () => Promise<void>;
}
export interface ExcelRuntime {
  run: <T>(callback: (context: ExcelContext) => Promise<T>) => Promise<T>;
}
interface Tag {
  key: string;
  value: string;
}
interface Tags extends Collection<Tag> {
  add: (key: string, value: string) => unknown;
}
interface Shape {
  type: string;
  tags: Tags;
  textFrame: { textRange: { text: string; load: (fields: string) => void } };
}
interface Slide {
  tags: Tags;
  shapes: Collection<Shape>;
}
interface PowerPointContext {
  presentation: { slides: Collection<Slide> };
  sync: () => Promise<void>;
}
export interface PowerPointRuntime {
  run: <T>(callback: (context: PowerPointContext) => Promise<T>) => Promise<T>;
}
export interface StructuredAdapter {
  configure: (checkpoint: Checkpoint) => void;
  bind: (blocks: Block[]) => Promise<unknown>;
  read: () => Promise<Block[]>;
  apply: (expected: Block[], next: Block[]) => Promise<unknown>;
}
function validateNext(actual: Block[], expected: Block[], next: Block[]) {
  if (
    !sameBlocks(actual, expected) ||
    actual.length !== next.length ||
    next.some((b, i) => blockKey(b) !== blockKey(actual[i]))
  )
    throw new Error('The local file changed after preview. Capture again before applying.');
}
export function cellText(value: unknown, formula: unknown, type?: string): string {
  if (typeof formula === 'string' && formula.startsWith('=')) {
    if (type === 'text')
      throw new Error('A text cell became a formula. Use file push to record its type.');
    return formula;
  }
  if (!['string', 'number', 'boolean'].includes(typeof value))
    throw new Error('Unsupported cell type. Use file push.');
  if (typeof value === 'string' && value.startsWith('='))
    throw new Error('Literal formula-like text requires file push.');
  if (
    value !== '' &&
    ((type === 'text' && typeof value !== 'string') ||
      (type === 'number' && typeof value !== 'number') ||
      (type === 'boolean' && typeof value !== 'boolean'))
  )
    throw new Error('A cell type changed. Use file push to include its format and value together.');
  return String(value);
}
export function cellValue(text: string, type?: string): string | number | boolean {
  if (text.startsWith('=')) return text;
  if (text === '') return '';
  if (type === 'number') {
    const n = Number(text);
    if (!Number.isFinite(n))
      throw new Error('This update changes a numeric cell type. Use file push.');
    return n;
  }
  if (type === 'boolean') {
    if (!['true', 'false'].includes(text))
      throw new Error('This update changes a boolean cell type. Use file push.');
    return text === 'true';
  }
  // Untyped imported cells follow the engine's existing scalar conversion.
  if (!type && text.trim() && Number.isFinite(Number(text))) return Number(text);
  if (!type && ['true', 'false'].includes(text)) return text === 'true';
  return text;
}
export class ExcelAdapter implements StructuredAdapter {
  private targets: StructuredTarget[] = [];
  private groups: string[] = [];
  constructor(
    private runtime: ExcelRuntime,
    private guard: () => Promise<void> = async () => {}
  ) {}
  configure(c: Checkpoint) {
    if (c.kind !== 'spreadsheet') throw new Error('Use a spreadsheet checkpoint.');
    this.targets = structuredTargets(c);
    this.groups = structuredGroups(c).map((g) => g.name);
  }
  private async rows(ctx: ExcelContext, binding: boolean) {
    ctx.workbook.worksheets.load('items/name');
    ctx.workbook.names.load('items/name');
    await ctx.sync();
    const known = new Set(this.targets.map((t) => 'dynodoc_' + t.id));
    if (ctx.workbook.names.items.some((n) => n.name.startsWith('dynodoc_') && !known.has(n.name)))
      throw new Error('This workbook has identities from another history.');
    const groups = this.groups;
    if (
      ctx.workbook.worksheets.items.length !== groups.length ||
      groups.some((g) => !ctx.workbook.worksheets.items.some((s) => s.name === g))
    )
      throw new Error('Sheet names or structure changed. Use file push.');
    const rows = [...new Map(this.targets.map((t) => [t.id, t])).values()].map((t) => {
      const sheet = ctx.workbook.worksheets.items.find((s) => s.name === t.group)!;
      const columns = this.targets.filter((x) => x.id === t.id).length;
      const range = sheet.getRangeByIndexes(t.index, 0, 1, columns),
        name = ctx.workbook.names.getItemOrNullObject('dynodoc_' + t.id);
      range.load('values,formulas,rowIndex,columnIndex,rowCount,columnCount');
      name.load('name,isNullObject');
      return { t, range, name };
    });
    const used = ctx.workbook.worksheets.items.map((s) => {
      const r = s.getUsedRangeOrNullObject(true);
      r.load('isNullObject,rowIndex,columnIndex,rowCount,columnCount');
      return { s, r };
    });
    await ctx.sync();
    for (const { s, r } of used) {
      const ts = this.targets.filter((t) => t.group === s.name);
      if (
        !r.isNullObject &&
        (r.rowIndex + r.rowCount > Math.max(...ts.map((t) => t.index)) + 1 ||
          r.columnIndex + r.columnCount > Math.max(...ts.map((t) => t.column!)) + 1)
      )
        throw new Error('New rows or columns require file push.');
    }
    const anchored = rows
      .filter((r) => !r.name.isNullObject)
      .map((r) => {
        const anchor = r.name.getRange();
        anchor.load('rowIndex,columnIndex,rowCount,columnCount');
        anchor.worksheet.load('name');
        return { r, anchor };
      });
    await ctx.sync();
    for (const { r, anchor } of anchored)
      if (
        anchor.worksheet.name !== r.t.group ||
        anchor.rowIndex !== r.range.rowIndex ||
        anchor.columnIndex !== 0 ||
        anchor.rowCount !== 1 ||
        anchor.columnCount !== r.range.columnCount
      )
        throw new Error('A stable row identity moved or changed. Use file push.');
    if (!binding && rows.some((r) => r.name.isNullObject))
      throw new Error('A row identity is missing. Use file push.');
    return rows;
  }
  private readRows(rows: Awaited<ReturnType<ExcelAdapter['rows']>>): Block[] {
    return this.targets.map((t) => {
      const r = rows.find((r) => r.t.id === t.id)!.range;
      return {
        id: t.id,
        field: t.field,
        text: cellText(r.values[0][t.column!], r.formulas[0][t.column!], t.cellType)
      };
    });
  }
  async bind(blocks: Block[]) {
    await this.guard();
    return this.runtime.run(async (ctx) => {
      const rows = await this.rows(ctx, true);
      bindIdentical(this.readRows(rows), blocks);
      await this.guard();
      for (const r of rows)
        if (r.name.isNullObject) ctx.workbook.names.add('dynodoc_' + r.t.id, r.range);
      await ctx.sync();
    });
  }
  async read() {
    await this.guard();
    return this.runtime.run(async (ctx) => {
      const rows = await this.rows(ctx, false);
      await this.guard();
      return this.readRows(rows);
    });
  }
  async apply(expected: Block[], next: Block[]) {
    await this.guard();
    return this.runtime.run(async (ctx) => {
      const rows = await this.rows(ctx, false);
      validateNext(this.readRows(rows), expected, next);
      const writes = next.map((b, i) => ({
        b,
        i,
        value: cellValue(b.text, this.targets[i].cellType)
      }));
      await this.guard();
      for (const { b, i, value } of writes)
        if (b.text !== expected[i].text) {
          const t = this.targets[i],
            cell = rows.find((r) => r.t.id === t.id)!.range.getCell(0, t.column!);
          if (b.text.startsWith('=')) cell.formulas = [[b.text]];
          else cell.values = [[value]];
        }
      await ctx.sync();
    });
  }
}
const identityTag = 'DYNODOC_ID';
export class PowerPointAdapter implements StructuredAdapter {
  private targets: StructuredTarget[] = [];
  constructor(
    private runtime: PowerPointRuntime,
    private guard: () => Promise<void> = async () => {}
  ) {}
  configure(c: Checkpoint) {
    if (c.kind !== 'presentation') throw new Error('Use a presentation checkpoint.');
    this.targets = structuredTargets(c);
    this.slideIds = Object.values(c.state.nodes)
      .filter((n) => !n.deleted && n.type === 'section' && n.current_fields.kind === 'slide')
      .sort((a, b) => (a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : a.id.localeCompare(b.id)))
      .map((n) => n.id);
  }
  private async shapes(ctx: PowerPointContext, binding: boolean) {
    ctx.presentation.slides.load('items');
    await ctx.sync();
    const slides = ctx.presentation.slides.items;
    // Empty slides still matter; the configured checkpoint records their count below.
    if (slides.length !== this.slideIds.length)
      throw new Error('Slides were inserted or removed. Use file push.');
    for (const slide of slides) {
      slide.shapes.load('items/type');
      slide.tags.load('items/key,items/value');
    }
    await ctx.sync();
    const shapes: Shape[] = [];
    slides.forEach((slide, i) => {
      const tags = slide.tags.items.filter((t) => t.key.toUpperCase() === identityTag);
      if (
        tags.length > 1 ||
        (tags.length && tags[0].value !== this.slideIds[i]) ||
        (!binding && !tags.length)
      )
        throw new Error('A slide identity changed or is missing. Use file push.');
      const local = slide.shapes.items.filter((s) =>
        ['TextBox', 'GeometricShape', 'Placeholder'].includes(s.type)
      );
      if (
        slide.shapes.items.some(
          (s) => !['TextBox', 'GeometricShape', 'Placeholder', 'Image'].includes(s.type)
        )
      )
        throw new Error('Grouped, table or unsupported slide shapes require file push.');
      if (local.length !== this.targets.filter((t) => t.groupIndex === i).length)
        throw new Error('Slide text shape structure changed. Use file push.');
      for (const s of local) {
        s.tags.load('items/key,items/value');
        s.textFrame.textRange.load('text');
        shapes.push(s);
      }
    });
    await ctx.sync();
    shapes.forEach((s, i) => {
      const tags = s.tags.items.filter((t) => t.key.toUpperCase() === identityTag);
      if (
        tags.length > 1 ||
        (tags.length && tags[0].value !== this.targets[i].id) ||
        (!binding && !tags.length)
      )
        throw new Error('A slide shape identity changed or is missing. Use file push.');
    });
    return shapes;
  }
  private slideIds: string[] = [];
  private blocks(shapes: Shape[]): Block[] {
    return shapes.map((s, i) => ({
      id: this.targets[i].id,
      field: 'text',
      text: s.textFrame.textRange.text.replace(/\r/g, '\n')
    }));
  }
  async bind(blocks: Block[]) {
    await this.guard();
    return this.runtime.run(async (ctx) => {
      const shapes = await this.shapes(ctx, true);
      bindIdentical(this.blocks(shapes), blocks);
      await this.guard();
      ctx.presentation.slides.items.forEach((s, i) => {
        if (!s.tags.items.some((t) => t.key.toUpperCase() === identityTag))
          s.tags.add(identityTag, this.slideIds[i]);
      });
      shapes.forEach((s, i) => {
        if (!s.tags.items.some((t) => t.key.toUpperCase() === identityTag))
          s.tags.add(identityTag, this.targets[i].id);
      });
      await ctx.sync();
    });
  }
  async read() {
    await this.guard();
    return this.runtime.run(async (ctx) => {
      const shapes = await this.shapes(ctx, false);
      await this.guard();
      return this.blocks(shapes);
    });
  }
  async apply(expected: Block[], next: Block[]) {
    await this.guard();
    return this.runtime.run(async (ctx) => {
      const shapes = await this.shapes(ctx, false);
      validateNext(this.blocks(shapes), expected, next);
      await this.guard();
      shapes.forEach((s, i) => {
        if (next[i].text !== expected[i].text) s.textFrame.textRange.text = next[i].text;
      });
      await ctx.sync();
    });
  }
}
