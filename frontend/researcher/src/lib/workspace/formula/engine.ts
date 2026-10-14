import { E, FormulaError, errorFromText } from './errors';
import { evaluate, usesFunction, type EvalContext } from './evaluator';
import { FUNCTIONS, registeredFunctionNames } from './functions';
import {
  MAX_COLS,
  MAX_ROWS,
  nameToCol,
  refToText,
  sheetPrefix,
  tryParse,
  walk,
  type Node,
  type RefNode
} from './parser';
import { serialToYmd, type DateOrder } from './dates';
import {
  ArrayValue,
  MultiRef,
  RefValue,
  round15,
  type CellSource,
  type CoerceOptions,
  type Scalar,
  type Value
} from './values';

const CLEAN = 0;
const DIRTY = 1;
const PENDING = 2;
const MAX_SPILL_CELLS = 1_000_000;

export interface EngineOptions {
  /** Clock for NOW/TODAY; returns a Date or an Excel serial number. */
  now?: () => Date | number;
  random?: () => number;
  /** How ambiguous numeric dates such as 03/04/2026 are read. Default day-first. */
  dateOrder?: DateOrder;
  /** Upper bound on evaluation work (range cells visited, array cells made) per recalculation. */
  maxWork?: number;
}
export type CellContent = string | number | boolean | null | undefined;
export interface CellAddress {
  sheet: number;
  row: number;
  col: number;
}
export interface NamedExpressionInput {
  name: string;
  expression: string;
  /** Sheet id for a sheet-scoped name; omitted for workbook scope. */
  scope?: number;
}

interface StaticRef {
  sheet: number;
  r1: number;
  c1: number;
  r2: number;
  c2: number;
}
interface FormulaInfo {
  src: string;
  ast: Node | null;
  error?: FormulaError;
  refs: StaticRef[];
  volatile: boolean;
}
interface Cell {
  raw: CellContent;
  v: Scalar;
  f?: FormulaInfo;
  state: number;
  /** Search bookkeeping for the cycle detector. */
  run: number;
  idx: number;
  low: number;
  sheet: number;
  row: number;
  col: number;
}
interface Sheet {
  id: number;
  name: string;
  rows: ((Cell | undefined)[] | undefined)[];
  /** Dimensions declared when the sheet was built, kept so row permutations line up. */
  height: number;
  width: number;
  formulaCells: Set<Cell>;
  /** Values written by array results into otherwise empty cells. */
  spill: Map<number, Scalar>;
  /** Formula cells grouped by column and sorted by row; rebuilt lazily. */
  colIndex?: Map<number, Cell[]>;
  /** One past the last column that holds a cell. */
  maxCol: number;
  /** Spill values from the previous pass, readable until this pass has produced its own. */
  predicted: Map<number, Scalar>;
}
interface NameDef {
  name: string;
  expression: string;
  scope: number | null;
  ast: Node | null;
  error?: FormulaError;
}
interface RangeEntry extends StaticRef {
  id: string;
  owners: Set<Cell>;
}
/** Dependent ranges of one sheet, bucketed by row blocks so edits find their dependents fast. */
interface RangeIndex {
  entries: Map<string, RangeEntry>;
  buckets: Map<number, RangeEntry[]>;
  /** Ranges too tall to bucket (whole columns, long lists). */
  long: RangeEntry[];
}
const BUCKET_SHIFT = 4;
const MAX_BUCKET_SPAN = 32;

const key = (row: number, col: number) => row * MAX_COLS + col;
const cellKey = (sheet: number, row: number, col: number) =>
  (sheet * MAX_ROWS + row) * MAX_COLS + col;

const CURRENCY = /^\$|\$$/;
const NUMERIC = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/;

/** How a raw cell string becomes a value: booleans, error literals, numbers, percent, text. */
export function parseConstant(raw: CellContent): Scalar {
  if (raw === null || raw === undefined) return null;
  if (typeof raw === 'number') return Number.isFinite(raw) ? raw : E.NUM;
  if (typeof raw === 'boolean') return raw;
  if (raw === '') return null;
  const lower = raw.toLowerCase();
  if (lower === 'true') return true;
  if (lower === 'false') return false;
  if (raw.charCodeAt(0) === 35 /* # */) {
    const e = errorFromText(raw);
    if (e) return e;
  }
  const t = raw.trim();
  if (t.length && /^[\d$+\-.]/.test(t)) {
    let core = t;
    let percent = false;
    if (core.endsWith('%')) {
      core = core.slice(0, -1).trim();
      percent = true;
    } else if (CURRENCY.test(core)) core = core.replace(CURRENCY, '').trim();
    if (NUMERIC.test(core)) {
      const n = percent ? Number(core) / 100 : Number(core);
      return Number.isFinite(n) ? n : E.NUM;
    }
  }
  return raw.charCodeAt(0) === 39 /* ' */ ? raw.slice(1) : raw;
}

export function isValidName(name: string): boolean {
  if (!/^[\p{L}_\\][\p{L}\p{N}_.\\?]{0,254}$/u.test(name)) return false;
  const cell = /^([A-Za-z]{1,3})(\d+)$/.exec(name);
  if (cell && nameToCol(cell[1]) < MAX_COLS && Number(cell[2]) >= 1 && Number(cell[2]) <= MAX_ROWS)
    return false;
  if (/^[RrCc]$/.test(name) || /^[Rr]\d*[Cc]\d*$/.test(name)) return false;
  return true;
}

function excelSerial(now: Date | number): number {
  if (typeof now === 'number') return now;
  const ms = now.getTime() - now.getTimezoneOffset() * 60000;
  return ms / 86400000 + 25569;
}

/** An in-process spreadsheet calculation engine: formulas, names, structural edits. */
export class FormulaEngine implements CellSource {
  private sheets: (Sheet | undefined)[] = [];
  private nameDefs = new Map<string, NameDef[]>();
  private cellDeps = new Map<number, Set<Cell>>();
  private rangeDeps = new Map<number, RangeIndex>();
  private indexValid = false;
  private volatileCells = new Set<Cell>();
  private spillers = new Map<
    Cell,
    { sheet: number; row: number; col: number; rows: number; cols: number }
  >();
  private batchDepth = 0;
  private pending = false;
  private work = 0;
  private cycle = new Set<Cell>();
  private readonly options: Required<Omit<EngineOptions, 'now'>> & { now: () => Date | number };
  private destroyed = false;
  private clock = 0;
  private runs = 0;

  constructor(options: EngineOptions = {}) {
    this.options = {
      now: options.now ?? (() => new Date()),
      random: options.random ?? Math.random,
      dateOrder: options.dateOrder ?? 'DMY',
      maxWork: options.maxWork ?? 150_000_000
    };
  }

  static buildFromSheets(
    sheets: Record<string, CellContent[][]>,
    names: NamedExpressionInput[] = [],
    options?: EngineOptions
  ): FormulaEngine {
    const engine = new FormulaEngine(options);
    for (const [name, rows] of Object.entries(sheets)) engine.createSheet(name, rows);
    for (const n of names) engine.storeName(n.name, n.expression, n.scope ?? null);
    engine.rebuild();
    return engine;
  }

  static getRegisteredFunctionNames(): string[] {
    return registeredFunctionNames();
  }

  get maxRows() {
    return MAX_ROWS;
  }
  get maxColumns() {
    return MAX_COLS;
  }

  // ------------------------------------------------------------------ sheets

  getSheetNames(): string[] {
    return this.sheets.filter((s): s is Sheet => !!s).map((s) => s.name);
  }
  getSheetId(name: string): number | undefined {
    const lower = name.toLowerCase();
    for (const s of this.sheets) if (s && s.name.toLowerCase() === lower) return s.id;
    return undefined;
  }
  getSheetName(id: number): string | undefined {
    return this.sheets[id]?.name;
  }
  sheetIdByName(name: string) {
    return this.getSheetId(name);
  }
  sheetName(sheet: number) {
    return this.sheets[sheet]?.name;
  }
  sheetCount() {
    return this.sheets.filter(Boolean).length;
  }

  private createSheet(name: string, content: CellContent[][] = []): Sheet {
    if (this.getSheetId(name) !== undefined) throw new Error(`Sheet "${name}" already exists.`);
    const sheet: Sheet = {
      id: this.sheets.length,
      name,
      rows: [],
      height: content.length,
      width: content.reduce((m, r) => Math.max(m, r?.length ?? 0), 0),
      formulaCells: new Set(),
      spill: new Map(),
      predicted: new Map(),
      maxCol: 0
    };
    this.sheets.push(sheet);
    content.forEach((row, r) => {
      if (!row) return;
      row.forEach((raw, c) => {
        if (raw === null || raw === undefined || raw === '') return;
        this.putCell(sheet, r, c, raw);
      });
    });
    return sheet;
  }

  addSheet(name: string, content: CellContent[][] = []): number {
    const sheet = this.createSheet(name, content);
    this.rebuild();
    return sheet.id;
  }

  // ----------------------------------------------------------------- content

  private putCell(sheet: Sheet, row: number, col: number, raw: CellContent): Cell | undefined {
    if (row < 0 || col < 0 || row >= MAX_ROWS || col >= MAX_COLS)
      throw new Error('Cell address is outside the sheet.');
    const old = sheet.rows[row]?.[col];
    if (old?.f) {
      sheet.colIndex = undefined;
      sheet.formulaCells.delete(old);
      this.volatileCells.delete(old);
      if (this.indexValid) this.unindex(old);
      this.dropSpill(old);
    }
    if (raw === null || raw === undefined || raw === '') {
      if (sheet.rows[row]) delete sheet.rows[row][col];
      return undefined;
    }
    const cell: Cell = {
      raw,
      v: null,
      state: CLEAN,
      run: 0,
      idx: 0,
      low: 0,
      sheet: sheet.id,
      row,
      col
    };
    if (typeof raw === 'string' && raw.charCodeAt(0) === 61 /* = */) {
      cell.f = this.compile(raw);
      cell.state = DIRTY;
      sheet.formulaCells.add(cell);
      sheet.colIndex = undefined;
    } else cell.v = parseConstant(raw);
    if (col >= sheet.maxCol) sheet.maxCol = col + 1;
    const rows = sheet.rows;
    while (rows.length <= row) rows.push(undefined);
    const line = (rows[row] ??= []);
    line[col] = cell;
    return cell;
  }

  private compile(src: string): FormulaInfo {
    const parsed = tryParse(src);
    if ('error' in parsed) {
      return {
        src,
        ast: null,
        error: new FormulaError('ERROR', parsed.message),
        refs: [],
        volatile: false
      };
    }
    return { src, ast: parsed.ast, refs: [], volatile: false };
  }

  setCellContents(address: CellAddress, content: CellContent | CellContent[][]): void {
    const sheet = this.sheets[address.sheet];
    if (!sheet) throw new Error('Unknown sheet.');
    const value = Array.isArray(content) ? content[0]?.[0] : content;
    if (this.spillers.size && sheet.rows[address.row]?.[address.col]) {
      const prior = sheet.rows[address.row]![address.col]!;
      if (this.spillers.has(prior)) this.needFull = true;
    }
    const cell = this.putCell(sheet, address.row, address.col, value);
    if (cell?.f) {
      this.link(cell);
      if (this.indexValid) this.index(cell);
    }
    this.afterChange(address.sheet, address.row, address.col, cell);
  }

  /** Run several edits and recalculate once at the end. */
  batch(fn: () => void): void {
    this.batchDepth++;
    try {
      fn();
    } finally {
      this.batchDepth--;
      if (this.batchDepth === 0 && this.pending) this.flush();
    }
  }

  private changed: CellAddress[] = [];
  private dirtyList: Cell[] = [];
  private needFull = false;
  private afterChange(sheet: number, row: number, col: number, cell: Cell | undefined) {
    this.changed.push({ sheet, row, col });
    if (cell?.f) {
      cell.state = DIRTY;
      this.dirtyList.push(cell);
    }
    this.pending = true;
    if (this.batchDepth === 0) this.flush();
  }

  /** Recalculate everything affected by pending edits. */
  private flush() {
    this.pending = false;
    const changed = this.changed;
    this.changed = [];
    const seeds = this.dirtyList;
    this.dirtyList = [];
    if (this.spillers.size > 0 || this.needFull) {
      this.needFull = false;
      this.recalcAll();
      return;
    }
    this.clock = excelSerial(this.options.now());
    this.ensureIndex();
    const queue: Cell[] = [];
    const dirty: Cell[] = [];
    const mark = (c: Cell) => {
      if (c.state === CLEAN) {
        c.state = DIRTY;
        dirty.push(c);
        queue.push(c);
      }
    };
    for (const c of seeds)
      if (c.state === DIRTY) {
        dirty.push(c);
        queue.push(c);
      }
    for (const a of changed) this.dependentsOf(a.sheet, a.row, a.col, mark);
    for (const v of this.volatileCells) mark(v);
    while (queue.length) {
      const c = queue.pop()!;
      this.dependentsOf(c.sheet, c.row, c.col, mark);
    }
    this.work = 0;
    this.cycle.clear();
    for (const c of dirty) if (c.state === DIRTY) this.resolve(c);
    if (this.spillers.size > 0) this.recalcAll();
  }

  private dependentsOf(sheet: number, row: number, col: number, visit: (c: Cell) => void) {
    const direct = this.cellDeps.get(cellKey(sheet, row, col));
    if (direct) for (const c of direct) visit(c);
    const ranges = this.rangeDeps.get(sheet);
    if (!ranges) return;
    const hit = (r: RangeEntry) => {
      if (row >= r.r1 && row <= r.r2 && col >= r.c1 && col <= r.c2)
        for (const c of r.owners) visit(c);
    };
    const bucket = ranges.buckets.get(row >> BUCKET_SHIFT);
    if (bucket) for (const r of bucket) hit(r);
    for (const r of ranges.long) hit(r);
  }

  // ---------------------------------------------------------------- linking

  private nameKey(name: string) {
    return name.toLowerCase();
  }

  /** Resolve sheet names in a formula and collect its static references and volatility. */
  private link(cell: Cell) {
    const f = cell.f!;
    f.refs = [];
    f.volatile = false;
    if (!f.ast) return;
    const seenNames = new Set<NameDef>();
    const collect = (ast: Node, owner: number, depth: number) => {
      walk(ast, (n) => {
        if (n.t === 'ref') {
          if (n.sheet !== null) {
            const id = this.getSheetId(n.sheet);
            n.sheetId = id === undefined ? -1 : id;
          } else n.sheetId = undefined;
          const sheet = n.sheetId === undefined ? owner : n.sheetId;
          if (sheet >= 0) f.refs.push({ sheet, r1: n.r1, c1: n.c1, r2: n.r2, c2: n.c2 });
        } else if (n.t === 'name') {
          if (n.sheet !== null) {
            const id = this.getSheetId(n.sheet);
            n.sheetId = id === undefined ? -1 : id;
          }
          const def = this.findName(n.name, n.sheet !== null ? (n.sheetId ?? -1) : null, owner);
          if (def && def.ast && !seenNames.has(def) && depth < 16) {
            seenNames.add(def);
            collect(def.ast, owner, depth + 1);
          }
        }
      });
    };
    collect(f.ast, cell.sheet, 0);
    f.volatile = usesFunction(f.ast, (k) => !!FUNCTIONS.get(k)?.volatile);
    if (f.volatile) this.volatileCells.add(cell);
    else this.volatileCells.delete(cell);
  }

  private findName(name: string, scopedSheet: number | null, from: number): NameDef | undefined {
    const defs = this.nameDefs.get(this.nameKey(name));
    if (!defs) return undefined;
    if (scopedSheet !== null) return defs.find((d) => d.scope === scopedSheet);
    return defs.find((d) => d.scope === from) ?? defs.find((d) => d.scope === null);
  }

  private rebuild() {
    this.cellDeps.clear();
    this.rangeDeps.clear();
    this.indexValid = false;
    this.volatileCells.clear();
    for (const def of [...this.nameDefs.values()].flat()) this.linkName(def);
    for (const s of this.sheets)
      if (s)
        for (const c of s.formulaCells) {
          this.link(c);
          c.state = DIRTY;
        }
    this.recalcAll();
  }

  private linkName(def: NameDef) {
    if (!def.ast) return;
    walk(def.ast, (n) => {
      if (n.t === 'ref') {
        if (n.sheet !== null) {
          const id = this.getSheetId(n.sheet);
          n.sheetId = id === undefined ? -1 : id;
        } else n.sheetId = undefined;
      } else if (n.t === 'name' && n.sheet !== null) {
        const id = this.getSheetId(n.sheet);
        n.sheetId = id === undefined ? -1 : id;
      }
    });
  }

  private ensureIndex() {
    if (this.indexValid) return;
    this.cellDeps.clear();
    this.rangeDeps.clear();
    for (const s of this.sheets) if (s) for (const c of s.formulaCells) this.index(c);
    this.indexValid = true;
  }
  private index(cell: Cell) {
    for (const r of cell.f!.refs) {
      if (r.r1 === r.r2 && r.c1 === r.c2) {
        const k = cellKey(r.sheet, r.r1, r.c1);
        let set = this.cellDeps.get(k);
        if (!set) this.cellDeps.set(k, (set = new Set()));
        set.add(cell);
        continue;
      }
      let index = this.rangeDeps.get(r.sheet);
      if (!index)
        this.rangeDeps.set(r.sheet, (index = { entries: new Map(), buckets: new Map(), long: [] }));
      const id = `${r.r1}:${r.c1}:${r.r2}:${r.c2}`;
      let entry = index.entries.get(id);
      if (!entry) {
        entry = { ...r, id, owners: new Set() };
        index.entries.set(id, entry);
        const first = r.r1 >> BUCKET_SHIFT,
          last = r.r2 >> BUCKET_SHIFT;
        if (last - first >= MAX_BUCKET_SPAN) index.long.push(entry);
        else
          for (let k = first; k <= last; k++) {
            let list = index.buckets.get(k);
            if (!list) index.buckets.set(k, (list = []));
            list.push(entry);
          }
      }
      entry.owners.add(cell);
    }
  }
  private unindex(cell: Cell) {
    for (const r of cell.f!.refs) {
      if (r.r1 === r.r2 && r.c1 === r.c2) {
        this.cellDeps.get(cellKey(r.sheet, r.r1, r.c1))?.delete(cell);
        continue;
      }
      const index = this.rangeDeps.get(r.sheet);
      const entry = index?.entries.get(`${r.r1}:${r.c1}:${r.r2}:${r.c2}`);
      if (!index || !entry) continue;
      entry.owners.delete(cell);
      if (entry.owners.size) continue;
      index.entries.delete(entry.id);
      const first = entry.r1 >> BUCKET_SHIFT,
        last = entry.r2 >> BUCKET_SHIFT;
      if (last - first >= MAX_BUCKET_SPAN) index.long = index.long.filter((e) => e !== entry);
      else
        for (let k = first; k <= last; k++) {
          const list = index.buckets.get(k);
          if (!list) continue;
          const at = list.indexOf(entry);
          if (at >= 0) list.splice(at, 1);
          if (!list.length) index.buckets.delete(k);
        }
    }
  }

  // ------------------------------------------------------------- evaluation

  private recalcAll() {
    this.clock = excelSerial(this.options.now());
    const formulas: Cell[] = [];
    for (const s of this.sheets) if (s) for (const c of s.formulaCells) formulas.push(c);
    let previous = this.spillSignature();
    // Array results can change what other formulas read, so repeat until the spill layout settles.
    for (let pass = 0; pass < 4; pass++) {
      for (const s of this.sheets)
        if (s) {
          s.predicted = s.spill;
          s.spill = new Map();
        }
      this.spillers.clear();
      this.work = 0;
      this.cycle.clear();
      // A stable order makes competing spills resolve the same way on every build.
      if (previous !== '')
        formulas.sort((x, y) => x.sheet - y.sheet || x.row - y.row || x.col - y.col);
      for (const c of formulas) c.state = DIRTY;
      for (const c of formulas) if (c.state === DIRTY) this.resolve(c);
      const next = this.spillSignature();
      if (next === previous) break;
      previous = next;
    }
    for (const s of this.sheets) if (s) s.predicted = new Map();
    this.dirtyList = [];
  }

  private spillSignature() {
    const parts: string[] = [];
    for (const [c, s] of this.spillers)
      parts.push(`${c.sheet}:${c.row}:${c.col}:${s.rows}x${s.cols}`);
    return parts.sort().join('|');
  }

  /**
   * Evaluate a dirty cell after everything it depends on. Strongly connected components
   * are found with an iterative Tarjan search, so a cycle is reported on exactly its members
   * regardless of the order cells happen to be visited in.
   */
  private resolve(root: Cell) {
    interface Frame {
      cell: Cell;
      deps: Cell[];
      i: number;
    }
    const frames: Frame[] = [];
    const members: Cell[] = [];
    const run = ++this.runs;
    let counter = 0;
    const open = (c: Cell) => {
      c.state = PENDING;
      c.run = run;
      c.idx = c.low = counter++;
      members.push(c);
      frames.push({ cell: c, deps: this.dirtyPrecedents(c), i: 0 });
    };
    open(root);
    while (frames.length) {
      const fr = frames[frames.length - 1];
      const c = fr.cell;
      if (fr.i < fr.deps.length) {
        const d = fr.deps[fr.i++];
        if (d.state === DIRTY) open(d);
        else if (d.state === PENDING) {
          if (d === c) this.cycle.add(c);
          else if (d.run === run) c.low = Math.min(c.low, d.idx);
          else this.cycle.add(c); // waiting on a cell of an enclosing evaluation
        }
        continue;
      }
      frames.pop();
      const parent = frames[frames.length - 1];
      if (parent) parent.cell.low = Math.min(parent.cell.low, c.low);
      if (c.low !== c.idx) continue;
      let k = members.length - 1;
      while (members[k] !== c) k--;
      const component = members.splice(k);
      const cyclic = component.length > 1 || this.cycle.has(c);
      for (const m of component) {
        this.dropSpill(m);
        if (cyclic) {
          m.v = new FormulaError('CYCLE', 'Circular reference');
          this.cycle.delete(m);
        } else m.v = this.evaluateCell(m);
        m.state = CLEAN;
      }
    }
  }

  private columnIndex(sheet: Sheet): Map<number, Cell[]> {
    if (sheet.colIndex) return sheet.colIndex;
    const map = new Map<number, Cell[]>();
    for (const c of sheet.formulaCells) {
      let list = map.get(c.col);
      if (!list) map.set(c.col, (list = []));
      list.push(c);
    }
    for (const list of map.values()) list.sort((x, y) => x.row - y.row);
    return (sheet.colIndex = map);
  }

  private dirtyPrecedents(cell: Cell): Cell[] {
    const out: Cell[] = [];
    const f = cell.f;
    if (!f) return out;
    for (const r of f.refs) {
      const sheet = this.sheets[r.sheet];
      if (!sheet) continue;
      if (r.r1 === r.r2 && r.c1 === r.c2) {
        const c = sheet.rows[r.r1]?.[r.c1];
        if (c && c.f && c.state !== CLEAN) out.push(c);
        continue;
      }
      if (!sheet.formulaCells.size) continue;
      const index = this.columnIndex(sheet);
      const visit = (list: Cell[]) => {
        let lo = 0,
          hi = list.length;
        while (lo < hi) {
          const mid = (lo + hi) >> 1;
          if (list[mid].row < r.r1) lo = mid + 1;
          else hi = mid;
        }
        for (let i = lo; i < list.length && list[i].row <= r.r2; i++)
          if (list[i].state !== CLEAN) out.push(list[i]);
      };
      if (r.c2 - r.c1 + 1 <= index.size) {
        for (let col = r.c1; col <= r.c2; col++) {
          const list = index.get(col);
          if (list) visit(list);
        }
      } else for (const [col, list] of index) if (col >= r.c1 && col <= r.c2) visit(list);
    }
    return out;
  }

  private evaluateCell(cell: Cell): Scalar {
    const f = cell.f!;
    if (f.error) return f.error;
    if (!f.ast) return E.ERROR;
    const ctx = new Ctx(this, cell.sheet, cell.row, cell.col);
    let result: Value;
    try {
      result = evaluate(f.ast, ctx, FUNCTIONS, true);
    } catch (e) {
      if (e instanceof FormulaError) return e;
      if (e instanceof RangeError) return new FormulaError('ERROR', 'Formula is too complex');
      throw e;
    }
    return this.settle(cell, result);
  }

  /** Reduce a formula result to the cell's own value, spilling arrays into free cells. */
  private settle(cell: Cell, result: Value): Scalar {
    if (result instanceof MultiRef) return E.VALUE;
    let matrix: ArrayValue | null = null;
    if (result instanceof RefValue) {
      if (result.isCell) {
        const v = result.get(0, 0);
        return v === null ? 0 : v;
      }
      if (
        result.rows * result.cols > MAX_SPILL_CELLS ||
        cell.row + result.rows > MAX_ROWS ||
        cell.col + result.cols > MAX_COLS
      )
        return E.SPILL;
      matrix = ArrayValue.of(result.rows, result.cols, (r, c) => result.get(r, c));
    } else if (result instanceof ArrayValue) matrix = result;
    else if (typeof result === 'symbol') return null;
    else return result === null ? 0 : (result as Scalar);
    if (matrix.rows === 1 && matrix.cols === 1) {
      const v = matrix.data[0];
      return v === null ? 0 : v;
    }
    return this.spill(cell, matrix);
  }

  private spill(cell: Cell, m: ArrayValue): Scalar {
    const sheet = this.sheets[cell.sheet]!;
    if (cell.row + m.rows > MAX_ROWS || cell.col + m.cols > MAX_COLS) return E.SPILL;
    // The range must be free of content and of other spills.
    for (let r = 0; r < m.rows; r++)
      for (let c = 0; c < m.cols; c++) {
        if (r === 0 && c === 0) continue;
        const row = cell.row + r,
          col = cell.col + c;
        if (sheet.rows[row]?.[col]) return new FormulaError('SPILL', 'Spill range is not empty');
        if (sheet.spill.has(key(row, col)))
          return new FormulaError('SPILL', 'Spill range is occupied');
      }
    for (let r = 0; r < m.rows; r++)
      for (let c = 0; c < m.cols; c++) {
        if (r === 0 && c === 0) continue;
        const v = m.get(r, c);
        sheet.spill.set(key(cell.row + r, cell.col + c), v === null ? 0 : v);
      }
    this.spillers.set(cell, {
      sheet: cell.sheet,
      row: cell.row,
      col: cell.col,
      rows: m.rows,
      cols: m.cols
    });
    const first = m.data[0];
    return first === null ? 0 : first;
  }

  private dropSpill(cell: Cell) {
    const s = this.spillers.get(cell);
    if (!s) return;
    const sheet = this.sheets[s.sheet];
    if (sheet)
      for (let r = 0; r < s.rows; r++)
        for (let c = 0; c < s.cols; c++) if (r || c) sheet.spill.delete(key(s.row + r, s.col + c));
    this.spillers.delete(cell);
  }

  // ------------------------------------------------------------ CellSource

  cellValue(sheetId: number, row: number, col: number): Scalar {
    const sheet = this.sheets[sheetId];
    if (!sheet) return E.REF;
    const cell = sheet.rows[row]?.[col];
    if (cell) {
      if (cell.state === CLEAN) return cell.v;
      if (cell.state === DIRTY) {
        this.resolve(cell);
        return cell.v;
      }
      return new FormulaError('CYCLE', 'Circular reference');
    }
    if (sheet.spill.size || sheet.predicted.size) {
      const k = key(row, col);
      const v = sheet.spill.get(k) ?? sheet.predicted.get(k);
      if (v !== undefined) return v;
    }
    return null;
  }
  eachCell(
    sheetId: number,
    r1: number,
    c1: number,
    r2: number,
    c2: number,
    fn: (value: Scalar, row: number, col: number) => void
  ) {
    const sheet = this.sheets[sheetId];
    if (!sheet) return;
    const lastRow = Math.min(r2, sheet.rows.length - 1);
    for (let r = Math.max(0, r1); r <= lastRow; r++) {
      const line = sheet.rows[r];
      if (!line) continue;
      const lastCol = Math.min(c2, line.length - 1);
      for (let c = Math.max(0, c1); c <= lastCol; c++) {
        const cell = line[c];
        if (cell) {
          const v = this.cellValue(sheetId, r, c);
          if (v !== null) fn(v, r, c);
        }
      }
    }
    const visit = (k: number, v: Scalar) => {
      const r = Math.floor(k / MAX_COLS),
        c = k % MAX_COLS;
      if (r >= r1 && r <= r2 && c >= c1 && c <= c2) fn(v, r, c);
    };
    if (sheet.spill.size) for (const [k, v] of sheet.spill) visit(k, v);
    if (sheet.predicted.size)
      for (const [k, v] of sheet.predicted) if (!sheet.spill.has(k)) visit(k, v);
  }
  isBlank(sheetId: number, row: number, col: number) {
    const sheet = this.sheets[sheetId];
    if (!sheet) return true;
    const cell = sheet.rows[row]?.[col];
    if (cell) return cell.f ? false : cell.v === null;
    const k = key(row, col);
    return !sheet.spill.has(k) && !sheet.predicted.has(k);
  }
  isFormulaCell(sheetId: number, row: number, col: number) {
    return !!this.sheets[sheetId]?.rows[row]?.[col]?.f;
  }
  formulaText(sheetId: number, row: number, col: number) {
    return this.sheets[sheetId]?.rows[row]?.[col]?.f?.src;
  }
  extent(sheetId: number) {
    const sheet = this.sheets[sheetId];
    if (!sheet) return { rows: 0, cols: 0 };
    let rows = sheet.rows.length;
    let cols = sheet.maxCol;
    for (const map of [sheet.spill, sheet.predicted])
      for (const k of map.keys()) {
        rows = Math.max(rows, Math.floor(k / MAX_COLS) + 1);
        cols = Math.max(cols, (k % MAX_COLS) + 1);
      }
    return { rows, cols };
  }

  // ---------------------------------------------------------------- reading

  /**
   * Values leave the engine at 15 significant digits, the precision Excel keeps and compares
   * at, so binary noise such as 0.30000000000000004 never reaches sorting, de-duplication or
   * conditional formatting. Evaluation itself runs on full doubles.
   */
  getCellValue(address: CellAddress): Scalar {
    this.settleAll();
    const v = this.cellValue(address.sheet, address.row, address.col);
    return typeof v === 'number' ? round15(v) : v;
  }
  getCellSerialized(address: CellAddress): CellContent {
    const cell = this.sheets[address.sheet]?.rows[address.row]?.[address.col];
    if (!cell) return null;
    return cell.f ? cell.f.src : cell.raw;
  }
  /** The size the sheet was built with, grown by inserted or spilled content. */
  getSheetDimensions(sheet: number) {
    const e = this.extent(sheet);
    const declared = this.sheets[sheet];
    return {
      height: Math.max(e.rows, declared?.height ?? 0),
      width: Math.max(e.cols, declared?.width ?? 0)
    };
  }
  getSheetValues(sheetId: number): Scalar[][] {
    this.settleAll();
    const { rows, cols } = this.extent(sheetId);
    const out: Scalar[][] = [];
    for (let r = 0; r < rows; r++) {
      const line: Scalar[] = new Array(cols);
      for (let c = 0; c < cols; c++) {
        const v = this.cellValue(sheetId, r, c);
        line[c] = typeof v === 'number' ? round15(v) : v;
      }
      out.push(line);
    }
    return out;
  }
  getSheetSerialized(sheetId: number): CellContent[][] {
    const sheet = this.sheets[sheetId];
    if (!sheet) return [];
    const { rows, cols } = this.extent(sheetId);
    const out: CellContent[][] = [];
    for (let r = 0; r < rows; r++) {
      const line: CellContent[] = new Array(cols).fill(null);
      const src = sheet.rows[r];
      if (src)
        src.forEach((cell, c) => {
          if (cell) line[c] = cell.f ? cell.f.src : cell.raw;
        });
      out.push(line);
    }
    return out;
  }
  private settleAll() {
    if (this.pending && this.batchDepth === 0) this.flush();
  }

  // ------------------------------------------------------------------ names

  private storeName(name: string, expression: string, scope: number | null): NameDef {
    const parsed = tryParse(expression);
    const def: NameDef = {
      name,
      expression,
      scope,
      ast: 'ast' in parsed ? parsed.ast : null,
      error: 'error' in parsed ? new FormulaError('ERROR', parsed.message) : undefined
    };
    const k = this.nameKey(name);
    const list = this.nameDefs.get(k) ?? [];
    const existing = list.findIndex((d) => d.scope === scope);
    if (existing >= 0) list[existing] = def;
    else list.push(def);
    this.nameDefs.set(k, list);
    return def;
  }

  /** Validate and add a name. Throws a readable Error when the definition cannot be used. */
  addNamedExpression(name: string, expression: string, scope?: number): void {
    if (!isValidName(name)) throw new Error(`"${name}" is not a valid name.`);
    if (typeof expression !== 'string' || !expression.startsWith('=') || expression.length < 2)
      throw new Error('Start the expression with =.');
    const defs = this.nameDefs.get(this.nameKey(name)) ?? [];
    if (defs.some((d) => d.scope === (scope ?? null)))
      throw new Error(`The name "${name}" is already defined.`);
    const parsed = tryParse(expression);
    if ('error' in parsed) throw new Error(parsed.message);
    let relative = false;
    walk(parsed.ast, (n) => {
      if (n.t === 'ref' && n.sheet === null) relative = true;
      else if (n.t === 'ref' && !n.abs.every(Boolean) && n.kind !== 'cols' && n.kind !== 'rows')
        relative = true;
    });
    if (relative)
      throw new Error('Use sheet-qualified absolute references such as Sheet1!$B$2 in a name.');
    this.storeName(name, expression, scope ?? null);
    this.rebuild();
  }
  removeNamedExpression(name: string, scope?: number): void {
    const k = this.nameKey(name);
    const list = (this.nameDefs.get(k) ?? []).filter((d) => d.scope !== (scope ?? null));
    if (list.length) this.nameDefs.set(k, list);
    else this.nameDefs.delete(k);
    this.rebuild();
  }
  getAllNamedExpressionsSerialized(): NamedExpressionInput[] {
    return [...this.nameDefs.values()].flat().map((d) => ({
      name: d.name,
      expression: d.expression,
      ...(d.scope === null ? {} : { scope: d.scope })
    }));
  }
  lookupName(name: string, sheetName: string | null, from: number) {
    let scoped: number | null = null;
    if (sheetName !== null) {
      const id = this.getSheetId(sheetName);
      if (id === undefined) return undefined;
      scoped = id;
    }
    const def = this.findName(name, scoped, from);
    return def?.ast ? { ast: def.ast } : undefined;
  }
  /** The value of a named expression, evaluated at the top-left of the workbook. */
  calculateNamedExpression(name: string, scope?: number): Scalar | undefined {
    const def = this.findName(name, scope ?? null, scope ?? 0);
    if (!def) return undefined;
    if (!def.ast) return def.error ?? E.ERROR;
    this.settleAll();
    const ctx = new Ctx(this, scope ?? 0, 0, 0);
    const v = evaluate(def.ast, ctx, FUNCTIONS, false);
    const cell: Cell = {
      raw: null,
      v: null,
      state: CLEAN,
      run: 0,
      idx: 0,
      low: 0,
      sheet: scope ?? 0,
      row: 0,
      col: 0
    };
    return this.settle(cell, v);
  }

  /** Evaluate reference text for INDIRECT; unknown sheets or names give #REF!. */
  evaluateReferenceText(text: string, sheet: number, row: number, col: number): Value {
    const parsed = tryParse(text);
    if ('error' in parsed) return E.REF;
    const ast = parsed.ast;
    if (!(ast.t === 'ref' || ast.t === 'name' || ast.t === 'rng')) return E.REF;
    walk(ast, (n) => {
      if (n.t === 'ref' || n.t === 'name') {
        if (n.sheet !== null) {
          const id = this.getSheetId(n.sheet);
          n.sheetId = id === undefined ? -1 : id;
        }
      }
    });
    const ctx = new Ctx(this, sheet, row, col);
    const v = evaluate(ast, ctx, FUNCTIONS, false);
    if (v instanceof RefValue || v instanceof MultiRef) return v;
    return E.REF;
  }

  // ------------------------------------------------------------- structure

  private mutateReferences(
    transform: (ref: RefNode, owner: Cell | null, targetSheet: number) => RefNode | 'REF' | null,
    only?: (cell: Cell) => boolean
  ) {
    const rewrite = (src: string, ast: Node | null, ownerSheet: number, owner: Cell | null) => {
      if (!ast) return src;
      const edits: { s: number; e: number; text: string }[] = [];
      walk(ast, (n) => {
        if (n.t !== 'ref') return;
        const target = n.sheet !== null ? (this.getSheetId(n.sheet) ?? -1) : ownerSheet;
        if (target < 0) return;
        const out = transform(n, owner, target);
        if (out === null) return;
        if (out === 'REF') {
          edits.push({ s: n.s, e: n.e, text: '#REF!' });
          return;
        }
        const slice = src.slice(n.s, n.e);
        const prefix =
          n.sheet !== null ? (/^(?:'(?:[^']|'')+'|[^!]+)!/.exec(slice)?.[0] ?? '') : '';
        edits.push({ s: n.s, e: n.e, text: prefix + refToText(out, null) });
      });
      if (!edits.length) return src;
      edits.sort((a, b) => b.s - a.s);
      let text = src;
      for (const ed of edits) text = text.slice(0, ed.s) + ed.text + text.slice(ed.e);
      return text;
    };
    for (const s of this.sheets)
      if (s)
        for (const c of s.formulaCells) {
          if (only && !only(c)) continue;
          const next = rewrite(c.f!.src, c.f!.ast, c.sheet, c);
          if (next !== c.f!.src) {
            c.f = this.compile(next);
            c.raw = next;
          }
        }
    for (const defs of this.nameDefs.values())
      for (const d of defs) {
        const next = rewrite(d.expression, d.ast, d.scope ?? -1, null);
        if (next !== d.expression) {
          const parsed = tryParse(next);
          d.expression = next;
          d.ast = 'ast' in parsed ? parsed.ast : null;
        }
      }
  }

  private checkSheet(sheet: number): Sheet {
    const s = this.sheets[sheet];
    if (!s) throw new Error('Unknown sheet.');
    return s;
  }

  addRows(sheet: number, ...spans: [number, number][]) {
    for (const [index, count] of spans) this.insertLines(sheet, 'row', index, count);
    this.rebuild();
  }
  removeRows(sheet: number, ...spans: [number, number][]) {
    // Remove from the bottom up so earlier indexes stay valid.
    for (const [index, count] of [...spans].sort((a, b) => b[0] - a[0]))
      this.deleteLines(sheet, 'row', index, count);
    this.rebuild();
  }
  addColumns(sheet: number, ...spans: [number, number][]) {
    for (const [index, count] of spans) this.insertLines(sheet, 'col', index, count);
    this.rebuild();
  }
  removeColumns(sheet: number, ...spans: [number, number][]) {
    for (const [index, count] of [...spans].sort((a, b) => b[0] - a[0]))
      this.deleteLines(sheet, 'col', index, count);
    this.rebuild();
  }

  private insertLines(sheetId: number, axis: 'row' | 'col', at: number, n: number) {
    const sheet = this.checkSheet(sheetId);
    if (n < 1 || at < 0) return;
    this.mutateReferences((ref, _o, target) => {
      if (target !== sheetId) return null;
      return shiftRef(ref, axis, at, n, 'insert');
    });
    this.clearSpills();
    if (axis === 'row') {
      sheet.rows.splice(at, 0, ...new Array(n).fill(undefined));
      sheet.height += n;
      this.renumber(sheet);
    } else {
      sheet.width += n;
      for (const line of sheet.rows)
        if (line && line.length > at) line.splice(at, 0, ...new Array(n).fill(undefined));
      this.renumber(sheet);
    }
  }
  private deleteLines(sheetId: number, axis: 'row' | 'col', at: number, n: number) {
    const sheet = this.checkSheet(sheetId);
    if (n < 1 || at < 0) return;
    this.mutateReferences((ref, _o, target) => {
      if (target !== sheetId) return null;
      return shiftRef(ref, axis, at, n, 'remove');
    });
    this.clearSpills();
    if (axis === 'row') {
      for (const removed of sheet.rows.slice(at, at + n))
        if (removed) for (const c of removed) if (c?.f) this.forget(sheet, c);
      sheet.rows.splice(at, n);
      sheet.height = Math.max(0, sheet.height - n);
    } else {
      sheet.width = Math.max(0, sheet.width - n);
      for (const line of sheet.rows)
        if (line) {
          for (const c of line.slice(at, at + n)) if (c?.f) this.forget(sheet, c);
          if (line.length > at) line.splice(at, n);
        }
    }
    this.renumber(sheet);
  }
  private forget(sheet: Sheet, cell: Cell) {
    sheet.colIndex = undefined;
    sheet.formulaCells.delete(cell);
    this.volatileCells.delete(cell);
  }
  private renumber(sheet: Sheet) {
    let maxCol = 0;
    sheet.colIndex = undefined;
    sheet.rows.forEach((line, r) => {
      if (!line) return;
      line.forEach((cell, c) => {
        if (cell) {
          cell.row = r;
          cell.col = c;
          if (c >= maxCol) maxCol = c + 1;
        }
      });
    });
    sheet.maxCol = maxCol;
  }
  private clearSpills() {
    for (const s of this.sheets) {
      s?.spill.clear();
      s?.predicted.clear();
    }
    this.spillers.clear();
  }

  /** Rename a sheet and rewrite every reference that named it. */
  renameSheet(sheetId: number, newName: string) {
    const sheet = this.checkSheet(sheetId);
    const clash = this.getSheetId(newName);
    if (clash !== undefined && clash !== sheetId)
      throw new Error(`Sheet "${newName}" already exists.`);
    const old = sheet.name.toLowerCase();
    const prefix = sheetPrefix(newName);
    const edit = (src: string, ast: Node | null) => {
      if (!ast) return src;
      const edits: { s: number; e: number; text: string }[] = [];
      walk(ast, (n) => {
        if (n.t === 'ref' && n.sheet !== null && n.sheet.toLowerCase() === old)
          edits.push({ s: n.s, e: n.e, text: refToText(n, newName) });
        else if (n.t === 'name' && n.sheet !== null && n.sheet.toLowerCase() === old) {
          const m = /^(?:'(?:[^']|'')+'|[^!]+)!/.exec(src.slice(n.s, n.e));
          if (m) edits.push({ s: n.s, e: n.s + m[0].length, text: prefix });
        }
      });
      edits.sort((a, b) => b.s - a.s);
      let text = src;
      for (const ed of edits) text = text.slice(0, ed.s) + ed.text + text.slice(ed.e);
      return text;
    };
    for (const s of this.sheets)
      if (s)
        for (const c of s.formulaCells) {
          const next = edit(c.f!.src, c.f!.ast);
          if (next !== c.f!.src) {
            c.f = this.compile(next);
            c.raw = next;
          }
        }
    for (const defs of this.nameDefs.values())
      for (const d of defs) {
        const next = edit(d.expression, d.ast);
        if (next !== d.expression) {
          const parsed = tryParse(next);
          d.expression = next;
          d.ast = 'ast' in parsed ? parsed.ast : null;
        }
      }
    sheet.name = newName;
    this.rebuild();
  }

  removeSheet(sheetId: number) {
    this.checkSheet(sheetId);
    this.mutateReferences((_ref, _o, target) => (target === sheetId ? 'REF' : null));
    const sheet = this.sheets[sheetId]!;
    for (const c of sheet.formulaCells) {
      this.volatileCells.delete(c);
      this.spillers.delete(c);
    }
    for (const [k, defs] of this.nameDefs) {
      const keep = defs.filter((d) => d.scope !== sheetId);
      if (keep.length) this.nameDefs.set(k, keep);
      else this.nameDefs.delete(k);
    }
    this.sheets[sheetId] = undefined;
    this.clearSpills();
    this.rebuild();
  }

  /**
   * Reorder the rows of a sheet. `permutation[old] = new`. Formulas in moved rows keep
   * their relative references (shifted with the row), as a sort does in Excel.
   */
  setRowOrder(sheetId: number, permutation: number[]) {
    const sheet = this.checkSheet(sheetId);
    const height = Math.max(sheet.rows.length, sheet.height, permutation.length);
    const delta = new Map<Cell, number>();
    sheet.rows.forEach((line, r) => {
      if (!line) return;
      const to = permutation[r] ?? r;
      if (to === r) return;
      for (const c of line) if (c?.f) delta.set(c, to - r);
    });
    this.mutateReferences(
      (ref, owner) => {
        const d = owner ? delta.get(owner) : undefined;
        if (!d) return null;
        return translateRef(ref, d, 0);
      },
      (c) => delta.has(c)
    );
    const rows: ((Cell | undefined)[] | undefined)[] = new Array(height).fill(undefined);
    sheet.rows.forEach((line, r) => {
      rows[permutation[r] ?? r] = line;
    });
    sheet.rows = rows;
    this.renumber(sheet);
    this.clearSpills();
    this.rebuild();
  }

  /** The formula or value that results from copying a cell by a row/column offset. */
  translatedContent(source: CellAddress, rowOffset: number, colOffset: number): CellContent {
    const cell = this.sheets[source.sheet]?.rows[source.row]?.[source.col];
    if (!cell) return null;
    if (!cell.f || !cell.f.ast) return cell.f ? cell.f.src : cell.raw;
    const src = cell.f.src;
    const edits: { s: number; e: number; text: string }[] = [];
    walk(cell.f.ast, (n) => {
      if (n.t !== 'ref') return;
      const out = translateRef(n, rowOffset, colOffset);
      if (out === null) return;
      if (out === 'REF') {
        edits.push({ s: n.s, e: n.e, text: '#REF!' });
        return;
      }
      const slice = src.slice(n.s, n.e);
      const prefix = n.sheet !== null ? (/^(?:'(?:[^']|'')+'|[^!]+)!/.exec(slice)?.[0] ?? '') : '';
      edits.push({ s: n.s, e: n.e, text: prefix + refToText(out, null) });
    });
    edits.sort((a, b) => b.s - a.s);
    let text = src;
    for (const ed of edits) text = text.slice(0, ed.s) + ed.text + text.slice(ed.e);
    return text;
  }

  /** Copy a cell's content (formulas translated) onto another cell. */
  copyCell(source: CellAddress, target: CellAddress) {
    const content = this.translatedContent(
      source,
      target.row - source.row,
      target.col - source.col
    );
    this.setCellContents(target, content);
  }

  destroy() {
    this.destroyed = true;
    this.sheets = [];
    this.nameDefs.clear();
    this.cellDeps.clear();
    this.rangeDeps.clear();
    this.spillers.clear();
    this.volatileCells.clear();
  }

  // ----------------------------------------------------------------- helpers

  /** @internal */
  _clock() {
    return this.clock;
  }
  /** @internal */
  _options() {
    return this.options;
  }
  /** @internal */
  _charge(units: number) {
    this.work += units;
    if (this.work > this.options.maxWork) {
      this.work = this.options.maxWork;
      throw new FormulaError('ERROR', 'Calculation budget exceeded');
    }
  }
  isDestroyed() {
    return this.destroyed;
  }
}

// --------------------------------------------------------- reference shifting

function shiftRef(
  ref: RefNode,
  axis: 'row' | 'col',
  at: number,
  n: number,
  mode: 'insert' | 'remove'
): RefNode | 'REF' | null {
  const rowAxis = axis === 'row';
  // whole-column references do not move on row edits and vice versa
  if (rowAxis && ref.kind === 'cols') return null;
  if (!rowAxis && ref.kind === 'rows') return null;
  const a = rowAxis ? ref.r1 : ref.c1;
  const b = rowAxis ? ref.r2 : ref.c2;
  let na = a,
    nb = b;
  if (mode === 'insert') {
    if (a >= at) na = a + n;
    if (b >= at) nb = b + n;
  } else {
    const end = at + n;
    if (a >= at && b < end) return 'REF';
    na = a < at ? a : a < end ? at : a - n;
    nb = b < at ? b : b < end ? at - 1 : b - n;
    if (nb < na) return 'REF';
  }
  if (na === a && nb === b) return null;
  const limit = rowAxis ? MAX_ROWS - 1 : MAX_COLS - 1;
  if (nb > limit) nb = limit;
  return rowAxis ? { ...ref, r1: na, r2: nb } : { ...ref, c1: na, c2: nb };
}

function translateRef(ref: RefNode, dr: number, dc: number): RefNode | 'REF' | null {
  if (!dr && !dc) return null;
  let { r1, c1, r2, c2 } = ref;
  const [ra1, ca1, ra2, ca2] = ref.abs;
  if (ref.kind !== 'cols') {
    if (!ra1) r1 += dr;
    if (!ra2) r2 += dr;
  }
  if (ref.kind !== 'rows') {
    if (!ca1) c1 += dc;
    if (!ca2) c2 += dc;
  }
  if (r1 === ref.r1 && r2 === ref.r2 && c1 === ref.c1 && c2 === ref.c2) return null;
  if (
    r1 < 0 ||
    r2 < 0 ||
    c1 < 0 ||
    c2 < 0 ||
    r1 >= MAX_ROWS ||
    r2 >= MAX_ROWS ||
    c1 >= MAX_COLS ||
    c2 >= MAX_COLS
  )
    return 'REF';
  return { ...ref, r1, c1, r2, c2 };
}

// ------------------------------------------------------------------ context

class Ctx implements EvalContext {
  depth = 0;
  readonly order: DateOrder;
  readonly year: number;
  readonly coerce: CoerceOptions;
  readonly src: CellSource;
  constructor(
    private readonly engine: FormulaEngine,
    readonly sheet: number,
    readonly row: number,
    readonly col: number
  ) {
    const o = engine._options();
    this.order = o.dateOrder;
    this.src = engine;
    this.year = serialToYmd(Math.floor(engine._clock()))?.y ?? 2000;
    this.coerce = { order: this.order, year: this.year };
  }
  now() {
    return this.engine._clock();
  }
  random() {
    return this.engine._options().random();
  }
  charge(units: number) {
    this.engine._charge(units);
  }
  lookupName(name: string, sheetName: string | null, from: number) {
    return this.engine.lookupName(name, sheetName, from);
  }
  sheetIdByName(name: string) {
    return this.engine.getSheetId(name);
  }
  evalReferenceText(text: string): Value {
    return this.engine.evaluateReferenceText(text, this.sheet, this.row, this.col);
  }
}
