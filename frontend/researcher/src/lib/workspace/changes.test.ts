import { describe, it, expect } from 'vitest';
import { richContent, richChanges, stateChanges } from './changes';
import { emptyState, applyPayload } from '$lib/materializer';
import { rootOp, itemOp, paragraph } from './files';
import { importXlsForm, importRedcap } from './research-files';
function fixture() {
  const state = emptyState();
  applyPayload(state, rootOp('root', 'Study'));
  applyPayload(state, itemOp('root', 0, { label: 'First', content: paragraph('First') }, 'first'));
  applyPayload(
    state,
    itemOp('root', 1, { label: 'Second', content: paragraph('Second') }, 'second')
  );
  return state;
}
describe('block editing', () => {
  it('treats JSONB key ordering as unchanged content', () => {
    const a = fixture(),
      b = fixture();
    b.nodes.first.current_fields = {
      content: { content: [{ text: 'First', type: 'text' }], type: 'paragraph' },
      label: 'First'
    };
    expect(stateChanges(a, b)).toEqual([]);
  });
  it('keeps stable IDs and does not move untouched paragraphs when typing', () => {
    const state = fixture(),
      content = richContent(state);
    const first = content.content![0];
    first.content![0].text = 'Edited';
    const ops = richChanges(state, content);
    expect(ops.filter((o) => o.type === 'NodeMoved')).toEqual([]);
    expect(ops.some((o) => o.type === 'NodeCreated')).toBe(false);
    ops.forEach((op) => applyPayload(state, op));
    expect(state.nodes.first.current_fields.label).toBe('Edited');
    expect(state.nodes.second.current_fields.label).toBe('Second');
  });
  it('restores a removed block using the same identity', () => {
    const before = fixture(),
      deleted = fixture();
    applyPayload(deleted, { type: 'NodeDeleted', node_id: 'first' });
    const ops = stateChanges(deleted, before);
    expect(ops).toContainEqual({ type: 'NodeRestored', node_id: 'first' });
  });
  it('inserts between stable blocks and keeps lexical ordering', () => {
    const state = fixture(),
      content = richContent(state);
    content.content!.splice(1, 0, {
      type: 'paragraph',
      attrs: { blockId: 'new' },
      content: [{ type: 'text', text: 'Inserted' }]
    });
    const ops = richChanges(state, content);
    ops.forEach((op) => applyPayload(state, op));
    expect(richContent(state).content!.map((b) => b.attrs?.blockId)).toEqual([
      'first',
      'new',
      'second'
    ]);
  });
});
describe('research imports', () => {
  it('preserves XLSForm groups, expressions, translations and choices', () => {
    const result = importXlsForm(
      [
        {
          name: 'survey',
          rows: [
            ['type', 'name', 'label::English', 'relevant'],
            ['begin_group', 'people', 'People', ''],
            ['select_one yesno', 'consent', 'Consent?', '${age} >= 18'],
            ['end_group', '', '', '']
          ]
        },
        {
          name: 'choices',
          rows: [
            ['list_name', 'name', 'label::English'],
            ['yesno', '1', 'Yes'],
            ['yesno', '0', 'No']
          ]
        }
      ],
      'study.xlsx'
    )!;
    expect(result.kind).toBe('questionnaire');
    const state = emptyState();
    result.ops.forEach((op) => applyPayload(state, op));
    const item = Object.values(state.nodes).find((n) => n.var_name === 'consent')!;
    expect(item.current_fields.relevance).toBe('${age} >= 18');
    expect(item.current_fields.label).toBe('Consent?');
    expect(Object.values(state.nodes).filter((n) => n.parent_id === item.id)).toHaveLength(2);
    expect(state.nodes[item.parent_id!].var_name).toBe('people');
  });
  it('refuses unmatched XLSForm group endings', () => {
    expect(() =>
      importXlsForm(
        [
          {
            name: 'survey',
            rows: [
              ['type', 'name'],
              ['end_group', '']
            ]
          }
        ],
        'bad.xlsx'
      )
    ).toThrow('unmatched');
  });
  it('recognizes REDCap dictionaries and preserves original branching expressions', () => {
    const result = importRedcap(
      [
        [
          'Variable / Field Name',
          'Form Name',
          'Field Type',
          'Field Label',
          'Choices, Calculations, OR Slider Labels',
          'Branching Logic (Show field only if...)'
        ],
        ['consent', 'baseline', 'radio', 'Do you agree?', '1, Yes | 0, No', '[age] >= 18']
      ],
      'redcap.csv'
    )!;
    const state = emptyState();
    result.ops.forEach((op) => applyPayload(state, op));
    const item = Object.values(state.nodes).find((n) => n.var_name === 'consent')!;
    expect(item.current_fields.redcap_logic).toBe('[age] >= 18');
    expect(Object.values(state.nodes).filter((n) => n.type === 'choice')).toHaveLength(2);
  });
});
