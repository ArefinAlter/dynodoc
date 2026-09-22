import { HyperFormula } from 'hyperformula';
import type { DocumentState, EventPayload } from '$lib/api/types';
import { workbookInput } from './sheet-model';
export interface WorkbookName {
  name: string;
  expression: string;
}
export function workbookNames(state: DocumentState): WorkbookName[] {
  const root = Object.values(state.nodes).find((n) => n.type === 'form' && !n.deleted);
  const names = root?.current_fields.named_ranges;
  if (!Array.isArray(names)) return [];
  const seen = new Set<string>();
  return names
    .filter((n): n is WorkbookName => {
      if (
        !n ||
        typeof n !== 'object' ||
        !/^[A-Za-z_][A-Za-z0-9_.]{0,63}$/.test(n.name) ||
        /^[A-Z]+[1-9]\d*$/i.test(n.name) ||
        typeof n.expression !== 'string' ||
        !n.expression.startsWith('=') ||
        n.expression.length > 1000 ||
        seen.has(n.name.toLowerCase())
      )
        return false;
      seen.add(n.name.toLowerCase());
      return true;
    })
    .slice(0, 100);
}
export function namedRangeOps(
  state: DocumentState,
  name: string,
  expression: string
): EventPayload[] {
  const root = Object.values(state.nodes).find((n) => n.type === 'form' && !n.deleted);
  if (!root) throw new Error('Workbook root is missing.');
  const current = workbookNames(state);
  const next = current.filter((n) => n.name.toLowerCase() !== name.toLowerCase());
  if (expression) {
    if (!/^[A-Za-z_][A-Za-z0-9_.]{0,63}$/.test(name) || /^[A-Z]+[1-9]\d*$/i.test(name))
      throw new Error(
        'Use a name of up to 64 letters, digits, underscores or dots, starting with a letter or underscore. Cell addresses are reserved.'
      );
    if (!expression.startsWith('=') || expression.length > 1000)
      throw new Error('Start the expression with = and use up to 1,000 characters.');
    const hf = HyperFormula.buildFromSheets(workbookInput(state), { licenseKey: 'gpl-v3' }, next);
    try {
      hf.addNamedExpression(name, expression);
    } finally {
      hf.destroy();
    }
    next.push({ name, expression });
  }
  if (next.length > 100) throw new Error('Use up to 100 workbook names.');
  return [{ type: 'FieldEdited', node_id: root.id, field: 'named_ranges', value: next }];
}
