import { E, errorFromText, type FormulaError } from './errors';

export const MAX_ROWS = 1048576;
export const MAX_COLS = 16384;
export const MAX_FORMULA_LENGTH = 8192;
const MAX_DEPTH = 64;

export type RefKind = 'cell' | 'range' | 'cols' | 'rows';

/** A literal A1-style reference. Coordinates are 0-based and inclusive. */
export interface RefNode {
  t: 'ref';
  s: number;
  e: number;
  sheet: string | null;
  kind: RefKind;
  r1: number;
  c1: number;
  r2: number;
  c2: number;
  /** Absolute flags: [row1, col1, row2, col2]. */
  abs: [boolean, boolean, boolean, boolean];
  /** Resolved at link time; -1 when the sheet does not exist. */
  sheetId?: number;
}
export interface NumNode {
  t: 'num';
  s: number;
  e: number;
  v: number;
}
export interface StrNode {
  t: 'str';
  s: number;
  e: number;
  v: string;
}
export interface BoolNode {
  t: 'bool';
  s: number;
  e: number;
  v: boolean;
}
export interface ErrNode {
  t: 'err';
  s: number;
  e: number;
  v: FormulaError;
}
export interface NameNode {
  t: 'name';
  s: number;
  e: number;
  name: string;
  sheet: string | null;
  sheetId?: number;
}
export interface CallNode {
  t: 'call';
  s: number;
  e: number;
  name: string;
  args: (Node | null)[];
}
export interface UnaryNode {
  t: 'un';
  s: number;
  e: number;
  op: '-' | '+' | '%' | '@';
  arg: Node;
}
export interface BinaryNode {
  t: 'bin';
  s: number;
  e: number;
  op: '+' | '-' | '*' | '/' | '^' | '&' | '=' | '<>' | '<' | '>' | '<=' | '>=';
  l: Node;
  r: Node;
}
export interface ArrayNode {
  t: 'arr';
  s: number;
  e: number;
  rows: Node[][];
}
export interface ParenNode {
  t: 'paren';
  s: number;
  e: number;
  x: Node;
}
/** `a:b` where an operand is not a plain A1 reference, e.g. `A1:INDEX(...)`. */
export interface RangeOpNode {
  t: 'rng';
  s: number;
  e: number;
  l: Node;
  r: Node;
}
export interface IntersectNode {
  t: 'isect';
  s: number;
  e: number;
  l: Node;
  r: Node;
}
export interface UnionNode {
  t: 'union';
  s: number;
  e: number;
  parts: Node[];
}
export type Node =
  | RefNode
  | NumNode
  | StrNode
  | BoolNode
  | ErrNode
  | NameNode
  | CallNode
  | UnaryNode
  | BinaryNode
  | ArrayNode
  | ParenNode
  | RangeOpNode
  | IntersectNode
  | UnionNode;

export class ParseError extends Error {
  constructor(
    message: string,
    readonly position: number
  ) {
    super(message);
  }
}

// ------------------------------------------------------------------ addresses

export function colToName(col: number): string {
  let s = '';
  for (let n = col + 1; n > 0; n = Math.floor((n - 1) / 26))
    s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
  return s;
}
export function nameToCol(name: string): number {
  let n = 0;
  for (const ch of name.toUpperCase()) n = n * 26 + ch.charCodeAt(0) - 64;
  return n - 1;
}

const SIMPLE_SHEET = /^[A-Za-z_][A-Za-z0-9_.]*$/;
/** Sheet prefix text including the trailing `!`, quoted when necessary. */
export function sheetPrefix(name: string): string {
  const confusable =
    /^[A-Za-z]{1,3}\d+$/.test(name) ||
    /^[Rr]\d*[Cc]\d*$/.test(name) ||
    /^(true|false)$/i.test(name);
  if (SIMPLE_SHEET.test(name) && !confusable) return name + '!';
  return `'${name.replace(/'/g, "''")}'!`;
}

export function refToText(ref: RefNode, sheet: string | null): string {
  const prefix = sheet === null ? '' : sheetPrefix(sheet);
  const col = (c: number, abs: boolean) => (abs ? '$' : '') + colToName(c);
  const row = (r: number, abs: boolean) => (abs ? '$' : '') + String(r + 1);
  const [ra1, ca1, ra2, ca2] = ref.abs;
  switch (ref.kind) {
    case 'cell':
      return prefix + col(ref.c1, ca1) + row(ref.r1, ra1);
    case 'range':
      return (
        prefix + col(ref.c1, ca1) + row(ref.r1, ra1) + ':' + col(ref.c2, ca2) + row(ref.r2, ra2)
      );
    case 'cols':
      return prefix + col(ref.c1, ca1) + ':' + col(ref.c2, ca2);
    default:
      return prefix + row(ref.r1, ra1) + ':' + row(ref.r2, ra2);
  }
}

// ------------------------------------------------------------------ tokenizer

type Token =
  | { k: 'num'; v: number; s: number; e: number; ws: boolean }
  | { k: 'str'; v: string; s: number; e: number; ws: boolean }
  | { k: 'err'; v: FormulaError; s: number; e: number; ws: boolean }
  | { k: 'ref'; v: RefNode; s: number; e: number; ws: boolean }
  | { k: 'id'; v: string; s: number; e: number; ws: boolean; sheet: string | null }
  | { k: 'op'; v: string; s: number; e: number; ws: boolean }
  | { k: 'eof'; s: number; e: number; ws: boolean };

const RE_NUMBER = /(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/y;
const RE_ERROR = /#(?:NULL!|DIV\/0!|VALUE!|REF!|NAME\?|NUM!|N\/A|CYCLE!|ERROR!|SPILL!|CALC!)/iy;
const RE_SHEET = /(?:'((?:[^']|'')+)'|([\p{L}_\\][\p{L}\p{N}_.\\]*))!/uy;
const RE_CELL = /(\$?)([A-Za-z]{1,3})(\$?)(\d+)(?![\p{L}\p{N}_.(])/uy;
const RE_COLS = /(\$?)([A-Za-z]{1,3}):(\$?)([A-Za-z]{1,3})(?![\p{L}\p{N}_.(])/uy;
const RE_ROWS = /(\$?)(\d+):(\$?)(\d+)(?![\p{L}\p{N}_.(])/uy;
const RE_ID = /[\p{L}_\\][\p{L}\p{N}_.\\?]*/uy;

/** Excel keeps 15 significant digits of a typed number literal and discards the rest. */
function literalNumber(text: string): number {
  const m = /^(\d*)\.?(\d*)(?:[eE]([+-]?\d+))?$/.exec(text);
  if (!m) return Number(text);
  const digits = m[1] + m[2];
  const lead = /^0*/.exec(digits)![0].length;
  const sig = digits.length - lead;
  if (sig <= 15) return Number(text);
  const kept = digits.slice(0, lead + 15);
  const exp = Number(m[3] ?? 0) - m[2].length + (digits.length - kept.length);
  return Number(`${kept}e${exp}`);
}

function exec(re: RegExp, src: string, at: number) {
  re.lastIndex = at;
  return re.exec(src);
}

function cellFrom(
  m: RegExpExecArray,
  s: number,
  pos: number,
  sheet: string | null
): { node: RefNode; end: number } | null {
  const col = nameToCol(m[2]);
  const row = Number(m[4]);
  if (col >= MAX_COLS || row < 1 || row > MAX_ROWS) return null;
  return {
    end: pos + m[0].length,
    node: {
      t: 'ref',
      s,
      e: pos + m[0].length,
      sheet,
      kind: 'cell',
      r1: row - 1,
      c1: col,
      r2: row - 1,
      c2: col,
      abs: [m[3] === '$', m[1] === '$', m[3] === '$', m[1] === '$']
    }
  };
}

/** Scan a reference (optionally sheet-qualified) at `at`, or return null. */
function scanReference(
  src: string,
  at: number
): { node: RefNode; end: number } | { sheetOnly: string; end: number } | null {
  let sheet: string | null = null;
  let pos = at;
  const sm = exec(RE_SHEET, src, pos);
  if (sm) {
    sheet = sm[1] !== undefined ? sm[1].replace(/''/g, "'") : sm[2];
    pos = at + sm[0].length;
  }
  let m = exec(RE_CELL, src, pos);
  if (m) {
    const c = cellFrom(m, at, pos, sheet);
    if (c) return c;
  }
  m = exec(RE_COLS, src, pos);
  if (m) {
    const c1 = nameToCol(m[2]),
      c2 = nameToCol(m[4]);
    if (c1 < MAX_COLS && c2 < MAX_COLS)
      return {
        end: pos + m[0].length,
        node: {
          t: 'ref',
          s: at,
          e: pos + m[0].length,
          sheet,
          kind: 'cols',
          r1: 0,
          r2: MAX_ROWS - 1,
          c1: Math.min(c1, c2),
          c2: Math.max(c1, c2),
          abs: [false, m[1] === '$', false, m[3] === '$']
        }
      };
  }
  m = exec(RE_ROWS, src, pos);
  if (m) {
    const r1 = Number(m[2]),
      r2 = Number(m[4]);
    if (r1 >= 1 && r2 >= 1 && r1 <= MAX_ROWS && r2 <= MAX_ROWS)
      return {
        end: pos + m[0].length,
        node: {
          t: 'ref',
          s: at,
          e: pos + m[0].length,
          sheet,
          kind: 'rows',
          r1: Math.min(r1, r2) - 1,
          r2: Math.max(r1, r2) - 1,
          c1: 0,
          c2: MAX_COLS - 1,
          abs: [m[1] === '$', false, m[3] === '$', false]
        }
      };
  }
  if (sheet !== null) return { sheetOnly: sheet, end: pos };
  return null;
}

function tokenize(src: string): Token[] {
  const out: Token[] = [];
  let i = 0;
  let ws = false;
  const n = src.length;
  while (i < n) {
    const ch = src[i];
    if (ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r') {
      ws = true;
      i++;
      continue;
    }
    const start = i;
    const push = (t: Token) => {
      out.push(t);
      ws = false;
    };
    if (ch === '"') {
      let j = i + 1;
      let v = '';
      for (;;) {
        if (j >= n) throw new ParseError('Unterminated string', start);
        if (src[j] === '"') {
          if (src[j + 1] === '"') {
            v += '"';
            j += 2;
            continue;
          }
          break;
        }
        v += src[j++];
      }
      i = j + 1;
      push({ k: 'str', v, s: start, e: i, ws });
      continue;
    }
    if (ch === '#') {
      const m = exec(RE_ERROR, src, i);
      if (!m) throw new ParseError('Unknown error literal', i);
      i += m[0].length;
      push({ k: 'err', v: errorFromText(m[0])!, s: start, e: i, ws });
      continue;
    }
    // References first: they may start with a digit ($, rows) or a letter/quote.
    if (ch === "'" || ch === '$' || /[\p{L}\p{N}_\\.]/u.test(ch)) {
      const ref = scanReference(src, i);
      if (ref && 'node' in ref) {
        i = ref.end;
        push({ k: 'ref', v: ref.node, s: start, e: i, ws });
        continue;
      }
      if (ref && 'sheetOnly' in ref) {
        // Sheet!Name or Sheet!#REF!
        const after = ref.end;
        if (src[after] === '#') {
          const m = exec(RE_ERROR, src, after);
          if (m) {
            i = after + m[0].length;
            push({ k: 'err', v: errorFromText(m[0])!, s: start, e: i, ws });
            continue;
          }
        }
        const m = exec(RE_ID, src, after);
        if (!m) throw new ParseError('Expected a name after the sheet', after);
        i = after + m[0].length;
        push({ k: 'id', v: m[0], s: start, e: i, ws, sheet: ref.sheetOnly });
        continue;
      }
      if (ch === "'") throw new ParseError('Unterminated sheet name', i);
      const num = exec(RE_NUMBER, src, i);
      if (num && /[\d.]/.test(ch)) {
        i += num[0].length;
        if (i < n && /[\p{L}\p{N}_]/u.test(src[i]))
          throw new ParseError('Unexpected text after number', i);
        push({ k: 'num', v: literalNumber(num[0]), s: start, e: i, ws });
        continue;
      }
      const id = exec(RE_ID, src, i);
      if (id) {
        i += id[0].length;
        push({ k: 'id', v: id[0], s: start, e: i, ws, sheet: null });
        continue;
      }
      throw new ParseError(`Unexpected character ${ch}`, i);
    }
    const two = src.slice(i, i + 2);
    if (two === '<>' || two === '<=' || two === '>=') {
      i += 2;
      push({ k: 'op', v: two, s: start, e: i, ws });
      continue;
    }
    if ('+-*/^&=<>%(),;:{}@'.includes(ch)) {
      i++;
      push({ k: 'op', v: ch, s: start, e: i, ws });
      continue;
    }
    throw new ParseError(`Unexpected character ${ch}`, i);
  }
  out.push({ k: 'eof', s: n, e: n, ws });
  return out;
}

// --------------------------------------------------------------------- parser

const BINARY_BP: Record<string, number> = {
  '=': 10,
  '<>': 10,
  '<': 10,
  '>': 10,
  '<=': 10,
  '>=': 10,
  '&': 20,
  '+': 30,
  '-': 30,
  '*': 40,
  '/': 40,
  '^': 50
};
const PERCENT_BP = 60;
const UNARY_BP = 70;
const AT_BP = 80;
const INTERSECT_BP = 85;
const RANGE_BP = 90;

class Parser {
  private i = 0;
  private depth = 0;
  constructor(
    private readonly src: string,
    private readonly tokens: Token[]
  ) {}

  private get tok() {
    return this.tokens[this.i];
  }
  private next() {
    return this.tokens[this.i++];
  }
  private isOp(v: string) {
    const t = this.tok;
    return t.k === 'op' && t.v === v;
  }

  parse(): Node {
    const node = this.expr(0);
    if (this.tok.k !== 'eof') throw new ParseError('Unexpected token', this.tok.s);
    return node;
  }

  private expr(minBp: number): Node {
    if (++this.depth > MAX_DEPTH * 2)
      throw new ParseError('Formula is nested too deeply', this.tok.s);
    try {
      let left = this.prefix();
      for (;;) {
        const t = this.tok;
        if (t.k !== 'op' && t.k !== 'ref' && t.k !== 'id') break;
        if (t.k === 'op') {
          const v = t.v;
          if (v === '%') {
            if (PERCENT_BP < minBp) break;
            this.next();
            left = { t: 'un', op: '%', arg: left, s: left.s, e: t.e };
            continue;
          }
          if (v === ':') {
            if (RANGE_BP < minBp) break;
            this.next();
            const right = this.expr(RANGE_BP + 1);
            left = this.range(left, right);
            continue;
          }
          const bp = BINARY_BP[v];
          if (bp === undefined || bp < minBp) {
            // A parenthesised operand directly after a reference means intersection.
            if (v === '(' && t.ws && isReferenceLike(left) && INTERSECT_BP >= minBp) {
              const right = this.expr(INTERSECT_BP + 1);
              left = { t: 'isect', l: left, r: right, s: left.s, e: right.e };
              continue;
            }
            break;
          }
          this.next();
          const right = this.expr(bp + 1); // all binary operators are left associative
          left = { t: 'bin', op: v as BinaryNode['op'], l: left, r: right, s: left.s, e: right.e };
          continue;
        }
        // Whitespace between two reference operands is the intersection operator.
        if (t.ws && isReferenceLike(left) && INTERSECT_BP >= minBp) {
          const right = this.expr(INTERSECT_BP + 1);
          left = { t: 'isect', l: left, r: right, s: left.s, e: right.e };
          continue;
        }
        break;
      }
      return left;
    } finally {
      this.depth--;
    }
  }

  private range(left: Node, right: Node): Node {
    if (left.t === 'ref' && right.t === 'ref' && left.kind === 'cell' && right.kind === 'cell') {
      const sameSheet =
        right.sheet === null ||
        left.sheet === null ||
        right.sheet.toLowerCase() === left.sheet.toLowerCase();
      if (sameSheet) {
        const r1 = Math.min(left.r1, right.r1),
          r2 = Math.max(left.r1, right.r1),
          c1 = Math.min(left.c1, right.c1),
          c2 = Math.max(left.c1, right.c1);
        const first = left.r1 <= right.r1 ? left : right;
        const firstC = left.c1 <= right.c1 ? left : right;
        const lastR = left.r1 <= right.r1 ? right : left;
        const lastC = left.c1 <= right.c1 ? right : left;
        return {
          t: 'ref',
          s: left.s,
          e: right.e,
          sheet: left.sheet ?? right.sheet,
          kind: 'range',
          r1,
          c1,
          r2,
          c2,
          abs: [first.abs[0], firstC.abs[1], lastR.abs[0], lastC.abs[1]]
        };
      }
    }
    return { t: 'rng', l: left, r: right, s: left.s, e: right.e };
  }

  private prefix(): Node {
    const t = this.next();
    switch (t.k) {
      case 'num':
        return { t: 'num', v: t.v, s: t.s, e: t.e };
      case 'str':
        return { t: 'str', v: t.v, s: t.s, e: t.e };
      case 'err':
        return { t: 'err', v: t.v, s: t.s, e: t.e };
      case 'ref':
        return t.v;
      case 'id': {
        if (this.isOp('(') && !this.tok.ws && t.sheet === null) return this.call(t);
        if (this.isOp('(') && t.sheet === null) return this.call(t);
        const lower = t.v.toLowerCase();
        if (t.sheet === null && (lower === 'true' || lower === 'false'))
          return { t: 'bool', v: lower === 'true', s: t.s, e: t.e };
        return { t: 'name', name: t.v, sheet: t.sheet, s: t.s, e: t.e };
      }
      case 'op': {
        if (t.v === '-' || t.v === '+') {
          const arg = this.expr(UNARY_BP);
          return { t: 'un', op: t.v, arg, s: t.s, e: arg.e };
        }
        if (t.v === '@') {
          const arg = this.expr(AT_BP);
          return { t: 'un', op: '@', arg, s: t.s, e: arg.e };
        }
        if (t.v === '(') {
          const first = this.expr(0);
          if (this.isOp(',')) {
            const parts = [first];
            while (this.isOp(',')) {
              this.next();
              parts.push(this.expr(0));
            }
            const close = this.next();
            if (close.k !== 'op' || close.v !== ')') throw new ParseError('Expected )', close.s);
            return { t: 'union', parts, s: t.s, e: close.e };
          }
          const close = this.next();
          if (close.k !== 'op' || close.v !== ')') throw new ParseError('Expected )', close.s);
          return { t: 'paren', x: first, s: t.s, e: close.e };
        }
        if (t.v === '{') return this.arrayLiteral(t.s);
        throw new ParseError(`Unexpected ${t.v}`, t.s);
      }
      default:
        throw new ParseError('Unexpected end of formula', t.s);
    }
  }

  private call(id: Extract<Token, { k: 'id' }>): Node {
    this.next(); // (
    const args: (Node | null)[] = [];
    if (this.isOp(')')) {
      const close = this.next();
      return { t: 'call', name: id.v, args, s: id.s, e: close.e };
    }
    for (;;) {
      if (this.isOp(',') || this.isOp(')')) args.push(null);
      else args.push(this.expr(0));
      const t = this.next();
      if (t.k === 'op' && t.v === ',') continue;
      if (t.k === 'op' && t.v === ')') return { t: 'call', name: id.v, args, s: id.s, e: t.e };
      throw new ParseError('Expected , or )', t.s);
    }
  }

  private arrayLiteral(start: number): Node {
    const rows: Node[][] = [[]];
    for (;;) {
      rows[rows.length - 1].push(this.expr(0));
      const t = this.next();
      if (t.k === 'op' && t.v === ',') continue;
      if (t.k === 'op' && t.v === ';') {
        rows.push([]);
        continue;
      }
      if (t.k === 'op' && t.v === '}') {
        const width = rows[0].length;
        if (rows.some((r) => r.length !== width))
          throw new ParseError('Array rows must have the same length', t.s);
        return { t: 'arr', rows, s: start, e: t.e };
      }
      throw new ParseError('Expected , ; or }', t.s);
    }
  }
}

function isReferenceLike(n: Node): boolean {
  switch (n.t) {
    case 'ref':
    case 'name':
    case 'rng':
    case 'isect':
    case 'call':
      return true;
    case 'paren':
      return isReferenceLike(n.x);
    case 'union':
      return true;
    default:
      return false;
  }
}

/** Parse a formula. The text may or may not start with `=`. */
export function parseFormula(text: string): Node {
  if (text.length > MAX_FORMULA_LENGTH) throw new ParseError('Formula is too long', 0);
  const offset = text.startsWith('=') ? 1 : 0;
  const src = offset ? ' ' + text.slice(1) : text; // keep offsets identical to the full text
  const tokens = tokenize(src);
  return new Parser(src, tokens).parse();
}

/** Try to parse; return the error as a value instead of throwing. */
export function tryParse(text: string): { ast: Node } | { error: FormulaError; message: string } {
  try {
    return { ast: parseFormula(text) };
  } catch (e) {
    if (e instanceof ParseError)
      return { error: E.ERROR, message: `Parsing error at ${e.position}: ${e.message}` };
    if (e instanceof RangeError) return { error: E.ERROR, message: 'Formula is too complex' };
    throw e;
  }
}

/** Visit every node, parents first. */
export function walk(node: Node, fn: (n: Node) => void): void {
  fn(node);
  switch (node.t) {
    case 'call':
      for (const a of node.args) if (a) walk(a, fn);
      break;
    case 'un':
      walk(node.arg, fn);
      break;
    case 'bin':
    case 'rng':
    case 'isect':
      walk(node.l, fn);
      walk(node.r, fn);
      break;
    case 'arr':
      for (const row of node.rows) for (const c of row) walk(c, fn);
      break;
    case 'paren':
      walk(node.x, fn);
      break;
    case 'union':
      for (const p of node.parts) walk(p, fn);
      break;
    default:
      break;
  }
}
