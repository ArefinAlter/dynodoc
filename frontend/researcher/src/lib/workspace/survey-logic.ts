/** Bounded interpreter for the documented XLSForm preview subset. Never uses eval. */
import type { DocumentState, MaterializedNode } from '$lib/api/types';
export type Answer = string | number | boolean | string[];
type Expr =
  | { kind: 'literal'; value: Answer }
  | { kind: 'ref'; name: string }
  | { kind: 'call'; name: string; args: Expr[] }
  | { kind: 'binary'; op: string; left: Expr; right: Expr }
  | { kind: 'unary'; op: string; value: Expr };
export class LogicError extends Error {}
const powers: Record<string, number> = {
  or: 1,
  and: 2,
  '=': 3,
  '!=': 3,
  '<>': 3,
  '<': 4,
  '>': 4,
  '<=': 4,
  '>=': 4,
  '+': 5,
  '-': 5,
  '*': 6,
  div: 6,
  mod: 6,
  '/': 6
};

const arity: Record<string, number[]> = {
  if: [3],
  selected: [2],
  'count-selected': [1],
  not: [1],
  true: [0],
  false: [0],
  number: [1],
  int: [1],
  string: [1],
  boolean: [1],
  'string-length': [1],
  round: [1, 2],
  coalesce: [2],
  concat: [],
  min: [],
  max: []
};

export function parseExpression(source: string): Expr {
  if (source.length > 4096) throw new LogicError('Expression exceeds the preview limit.');
  const tokens: string[] = [];
  const pattern =
    /\s*(\$\{[A-Za-z_][\w.-]*\}|\d+(?:\.\d+)?|\.\d+|'[^']*'|"[^"]*"|[A-Za-z_][\w-]*|!=|<=|>=|<>|[().,+*/=<>-])/y;
  let offset = 0;
  while (offset < source.length && source.slice(offset).trim()) {
    pattern.lastIndex = offset;
    const match = pattern.exec(source);
    if (!match)
      throw new LogicError(
        'Unsupported expression syntax near ' + source.slice(offset, offset + 30)
      );
    tokens.push(match[1]);
    offset = pattern.lastIndex;
    if (tokens.length > 512) throw new LogicError('Expression is too complex for preview.');
  }
  let index = 0;
  function expression(min = 0, depth = 0): Expr {
    if (depth > 40) throw new LogicError('Expression is nested too deeply.');
    const token = tokens[index++];
    let left: Expr;
    if (!token) throw new LogicError('Incomplete expression.');
    if (token === '(') {
      left = expression(0, depth + 1);
      if (tokens[index++] !== ')') throw new LogicError('Missing closing parenthesis.');
    } else if (token === '-' || token === '+')
      left = { kind: 'unary', op: token, value: expression(7, depth + 1) };
    else if (token.startsWith('${')) left = { kind: 'ref', name: token.slice(2, -1) };
    else if (token === '.') left = { kind: 'ref', name: '.' };
    else if (/^["']/.test(token)) left = { kind: 'literal', value: token.slice(1, -1) };
    else if (/^\d|^\.\d/.test(token)) left = { kind: 'literal', value: Number(token) };
    else if (tokens[index] === '(') {
      index++;
      const args: Expr[] = [];
      if (tokens[index] !== ')') {
        do {
          args.push(expression(0, depth + 1));
          if (tokens[index] !== ',') break;
          index++;
        } while (index < tokens.length);
      }
      if (tokens[index++] !== ')') throw new LogicError('Missing closing function parenthesis.');
      if (!Object.hasOwn(arity, token))
        throw new LogicError('Unsupported preview function: ' + token);
      if (arity[token].length && !arity[token].includes(args.length))
        throw new LogicError('Wrong number of arguments for ' + token);
      if (!arity[token].length && !args.length) throw new LogicError('Missing function arguments.');
      left = { kind: 'call', name: token, args };
    } else throw new LogicError('Unsupported name: ' + token);
    while (
      index < tokens.length &&
      powers[tokens[index]] !== undefined &&
      powers[tokens[index]] >= min
    ) {
      const op = tokens[index++];
      left = { kind: 'binary', op, left, right: expression(powers[op] + 1, depth + 1) };
    }
    return left;
  }
  const result = expression();
  if (index !== tokens.length) throw new LogicError('Unexpected expression content.');
  return result;
}
export const truth = (v: Answer): boolean =>
  Array.isArray(v) ? v.length > 0 : typeof v === 'string' ? v !== '' : Boolean(v);
const string = (v: Answer) => (Array.isArray(v) ? v.join(' ') : String(v));
function number(v: Answer) {
  const n = Number(v);
  if (!Number.isFinite(n)) throw new LogicError('A numeric value is required.');
  return n;
}
export function evaluate(
  source: string,
  read: (name: string) => Answer,
  current: Answer = ''
): Answer {
  function run(e: Expr): Answer {
    if (e.kind === 'literal') return e.value;
    if (e.kind === 'ref') return e.name === '.' ? current : read(e.name);
    if (e.kind === 'unary') return number(run(e.value)) * (e.op === '-' ? -1 : 1);
    if (e.kind === 'binary') {
      const a = run(e.left);
      if (e.op === 'and') return truth(a) && truth(run(e.right));
      if (e.op === 'or') return truth(a) || truth(run(e.right));
      const b = run(e.right);
      if (e.op === '=') return string(a) === string(b);
      if (e.op === '!=' || e.op === '<>') return string(a) !== string(b);
      const x = number(a),
        y = number(b);
      switch (e.op) {
        case '+':
          return x + y;
        case '-':
          return x - y;
        case '*':
          return x * y;
        case '/':
        case 'div':
          if (!y) throw new LogicError('Division by zero.');
          return x / y;
        case 'mod':
          if (!y) throw new LogicError('Division by zero.');
          return x % y;
        case '<':
          return x < y;
        case '>':
          return x > y;
        case '<=':
          return x <= y;
        case '>=':
          return x >= y;
      }
    }
    if (e.kind === 'call') {
      if (e.name === 'if') return run(e.args[truth(run(e.args[0])) ? 1 : 2]);
      if (e.name === 'coalesce') {
        const a = run(e.args[0]);
        return string(a) ? a : run(e.args[1]);
      }
      const a = e.args.map(run);
      switch (e.name) {
        case 'true':
          return true;
        case 'false':
          return false;
        case 'not':
          return !truth(a[0]);
        case 'selected':
          return string(a[0]).split(/\s+/).includes(string(a[1]));
        case 'count-selected':
          return string(a[0]).trim() ? string(a[0]).trim().split(/\s+/).length : 0;
        case 'number':
          return number(a[0]);
        case 'int':
          return Math.trunc(number(a[0]));
        case 'string':
          return string(a[0]);
        case 'boolean':
          return truth(a[0]);
        case 'string-length':
          return [...string(a[0])].length;
        case 'concat':
          return a.map(string).join('');
        case 'min':
          return Math.min(...a.map(number));
        case 'max':
          return Math.max(...a.map(number));
        case 'round': {
          const places = a.length === 2 ? number(a[1]) : 0;
          if (!Number.isInteger(places) || Math.abs(places) > 10)
            throw new LogicError('Unsupported rounding precision.');
          const factor = 10 ** places;
          return Math.round(number(a[0]) * factor) / factor;
        }
      }
    }
    throw new LogicError('Unsupported expression.');
  }
  const result = run(parseExpression(source));
  if (typeof result === 'number' && !Number.isFinite(result))
    throw new LogicError('Calculation is not finite.');
  return result;
}
export function previewSurvey(
  state: DocumentState,
  answers: Record<string, Answer>,
  validate = false
) {
  const nodes = Object.values(state.nodes).filter(
    (n) => !n.deleted && ['item', 'section', 'form'].includes(n.type)
  );
  const variables = new Map<string, MaterializedNode>();
  const values: Record<string, Answer> = {},
    visible: Record<string, boolean> = {},
    errors: Record<string, string[]> = {};
  const active = new Set<string>();
  const issue = (id: string, message: string) => {
    errors[id] = [...new Set([...(errors[id] || []), message])];
  };
  for (const n of nodes)
    if (n.var_name) {
      if (variables.has(n.var_name)) issue(n.id, 'Duplicate variable name: ' + n.var_name);
      variables.set(n.var_name, n);
    }
  function read(name: string): Answer {
    const n = variables.get(name);
    if (!n) throw new LogicError('Unknown variable: ' + name);
    return value(n);
  }
  function visit<T>(key: string, work: () => T): T {
    if (active.has(key)) throw new LogicError('Circular logic dependency.');
    active.add(key);
    try {
      return work();
    } finally {
      active.delete(key);
    }
  }
  function shown(n: MaterializedNode): boolean {
    if (Object.hasOwn(visible, n.id)) return visible[n.id];
    return (visible[n.id] = visit('visible:' + n.id, () => {
      if (n.parent_id && state.nodes[n.parent_id] && !shown(state.nodes[n.parent_id])) return false;
      const expression = String(n.current_fields.relevance || '');
      return !expression || truth(evaluate(expression, read));
    }));
  }
  function value(n: MaterializedNode): Answer {
    if (Object.hasOwn(values, n.id)) return values[n.id];
    return (values[n.id] = visit('value:' + n.id, () => {
      if (!shown(n)) return '';
      const calc = String(n.current_fields.calculation || '');
      if (calc && n.current_fields.redcap)
        throw new LogicError('REDCap calculation syntax must be validated in REDCap.');
      return calc
        ? evaluate(calc, read)
        : (answers[n.id] ?? String(n.current_fields.default ?? ''));
    }));
  }
  for (const n of nodes) {
    try {
      if (n.current_fields.group_type === 'repeat')
        issue(n.id, 'Repeat instances must be tested in the destination survey platform.');
      if (n.current_fields.redcap_logic)
        issue(n.id, 'REDCap branching logic must be tested in REDCap.');
      const show = shown(n),
        answer = value(n),
        type = String(n.current_fields.question_type || 'text');
      if (
        n.type === 'item' &&
        ![
          'text',
          'integer',
          'decimal',
          'date',
          'select_one',
          'select_multiple',
          'note',
          'calculate'
        ].includes(type)
      )
        issue(n.id, `${type} collection must be tested in the destination survey platform.`);
      const constraint = String(n.current_fields.constraint || '');
      // Parse even unanswered constraints so malformed or unsupported syntax is visible.
      if (constraint) parseExpression(constraint);
      if (!show || !validate || n.type !== 'item' || ['note', 'calculate'].includes(type)) continue;
      const rawRequired = String(
        n.current_fields.required_expression ??
          (n.current_fields.xlsform as Record<string, string> | undefined)?.required ??
          ''
      );
      const required =
        !!n.current_fields.required ||
        (!!rawRequired &&
          !['no', 'false', '0', 'yes', 'true', '1'].includes(rawRequired) &&
          truth(evaluate(rawRequired, read, answer)));
      const empty = string(answer).trim() === '';
      if (required && empty) issue(n.id, 'An answer is required.');
      if (!empty && type === 'integer' && !Number.isInteger(Number(answer)))
        issue(n.id, 'Enter a whole number.');
      if (!empty && type === 'decimal' && !Number.isFinite(Number(answer)))
        issue(n.id, 'Enter a number.');
      if (!empty && constraint && !truth(evaluate(constraint, read, answer)))
        issue(
          n.id,
          String(
            n.current_fields.constraint_message || 'This answer does not meet the validation rule.'
          )
        );
    } catch (e) {
      visible[n.id] = true;
      issue(n.id, e instanceof Error ? e.message : 'Logic could not be evaluated.');
    }
  }
  return { values, visible, errors, valid: !Object.keys(errors).length };
}
