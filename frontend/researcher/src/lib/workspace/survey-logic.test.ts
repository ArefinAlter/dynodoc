import { describe, it, expect } from 'vitest';
import { evaluate, previewSurvey } from './survey-logic';
import { emptyState, applyPayload } from '$lib/materializer';
import { importXlsForm } from './research-files';
function fixture() {
  const imported = importXlsForm(
    [
      {
        name: 'survey',
        rows: [
          ['type', 'name', 'label', 'relevant', 'constraint', 'calculation', 'required'],
          ['integer', 'age', 'Age', '', '. >= 18', '', 'yes'],
          ['text', 'work', 'Work', '${age} >= 18', '', '', 'yes'],
          ['calculate', 'next', 'Next age', '', '', '${age} + 1', '']
        ]
      }
    ],
    'study.xlsx'
  )!;
  const state = emptyState();
  imported.ops.forEach((op) => applyPayload(state, op));
  const id = (name: string) => Object.values(state.nodes).find((n) => n.var_name === name)!.id;
  return { state, id };
}
describe('supported respondent logic', () => {
  it('evaluates precedence, selected values, current answer and lazy conditions', () => {
    expect(evaluate('2 + 3 * 4', () => '')).toBe(14);
    expect(evaluate("selected(${choice}, 'a') and . >= 18", () => ['a', 'b'], 20)).toBe(true);
    expect(evaluate("if(true(), concat('Hi', ' there'), 1 div 0)", () => '')).toBe('Hi there');
    expect(evaluate('count-selected(${choice})', () => ['a', 'b'])).toBe(2);
  });
  it('rejects unsafe syntax, excessive depth, unknown functions and nonfinite arithmetic', () => {
    for (const expression of [
      'globalThis.process.exit()',
      'fetch(1)',
      '1 div 0',
      '('.repeat(60) + '1' + ')'.repeat(60)
    ])
      expect(() => evaluate(expression, () => '')).toThrow();
  });
  it('reacts to relevance, requires visible answers, checks constraints and computes derived values', () => {
    const { state, id } = fixture();
    const young = previewSurvey(state, { [id('age')]: '16' }, true);
    expect(young.visible[id('work')]).toBe(false);
    expect(young.errors[id('work')]).toBeUndefined();
    expect(young.errors[id('age')]).toBeDefined();
    expect(young.values[id('next')]).toBe(17);
    const adult = previewSurvey(state, { [id('age')]: '22', [id('work')]: 'Research' }, true);
    expect(adult.visible[id('work')]).toBe(true);
    expect(adult.values[id('next')]).toBe(23);
    expect(adult.valid).toBe(true);
    expect(previewSurvey(state, { [id('age')]: '22' }, true).errors[id('work')]).toBeDefined();
  });
  it('reports unresolved references, cyclic calculations and unsupported vendor logic', () => {
    const { state, id } = fixture();
    state.nodes[id('age')].current_fields.calculation = '${next}';
    expect(previewSurvey(state, {}, true).valid).toBe(false);
    state.nodes[id('age')].current_fields.calculation = '${missing}';
    expect(
      Object.values(previewSurvey(state, {}, true).errors)
        .flat()
        .join(' ')
    ).toContain('Unknown variable');
    state.nodes[id('work')].current_fields.redcap_logic = '[age] > 18';
    expect(
      Object.values(previewSurvey(state, {}, true).errors)
        .flat()
        .join(' ')
    ).toContain('REDCap');
  });
  it('reports unsupported functions even in an unanswered optional constraint', () => {
    const { state, id } = fixture();
    state.nodes[id('age')].current_fields.constraint = 'vendor-function(.)';
    state.nodes[id('age')].current_fields.required = false;
    expect(previewSurvey(state, {}, false).errors[id('age')].join(' ')).toContain(
      'Unsupported preview function'
    );
  });
  it('evaluates conditional required rules independently from static required', () => {
    const { state, id } = fixture();
    state.nodes[id('work')].current_fields.required = false;
    state.nodes[id('work')].current_fields.required_expression = '${age} >= 30';
    expect(previewSurvey(state, { [id('age')]: '22' }, true).errors[id('work')]).toBeUndefined();
    expect(previewSurvey(state, { [id('age')]: '35' }, true).errors[id('work')]).toContain(
      'An answer is required.'
    );
  });
  it('does not mutate instrument state or save respondent answers', () => {
    const { state, id } = fixture();
    const original = JSON.stringify(state);
    previewSurvey(state, { [id('age')]: '25' }, true);
    expect(JSON.stringify(state)).toBe(original);
  });
});
