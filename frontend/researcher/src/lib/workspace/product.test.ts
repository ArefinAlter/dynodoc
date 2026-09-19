import { describe, it, expect } from 'vitest';
import { emptyState, applyPayload } from '$lib/materializer';
import { rootOp, itemOp, paragraph } from './files';
import { richContent, richChanges } from './changes';
import { sheetStructure } from './sheet-structure';
import { importLatex, latexDocument, toLatex } from './latex';
import {
  questionnaireRows,
  responseCsv,
  formId,
  type GoogleForm
} from '$lib/server/google-forms-map';
function workbook() {
  const state = emptyState();
  [
    rootOp('root', 'Data'),
    {
      type: 'NodeCreated' as const,
      node_id: 'data',
      node_type: 'section' as const,
      parent_id: 'root',
      pos: 'a',
      fields: { kind: 'sheet', label: 'Data', columns: 3 }
    },
    {
      type: 'NodeCreated' as const,
      node_id: 'summary',
      node_type: 'section' as const,
      parent_id: 'root',
      pos: 'b',
      fields: { kind: 'sheet', label: 'Summary', columns: 3 }
    },
    itemOp('data', 0, { cell_0: '7', cell_1: '=A1*2', format_0: { bold: true } }, 'row'),
    itemOp('data', 1, {}, 'next'),
    itemOp('summary', 0, { cell_0: '=Data!A1' }, 'total')
  ].forEach((op) => applyPayload(state, op));
  return state;
}
describe('spreadsheet structural changes', () => {
  it('inserts rows without losing identities and adjusts cross-sheet references', () => {
    const state = workbook();
    sheetStructure(state, 'data', 'insert-row', 0).forEach((op) => applyPayload(state, op));
    expect(state.nodes.row.id).toBe('row');
    expect(state.nodes.row.pos).toBe('00000001');
    expect(state.nodes.row.current_fields.cell_0).toBe('7');
    expect(state.nodes.row.current_fields.cell_1).toBe('=A2*2');
    expect(state.nodes.total.current_fields.cell_0).toMatch(/Data'?!A2/i);
  });
  it('shifts formatting and references when inserting a column', () => {
    const state = workbook();
    sheetStructure(state, 'data', 'insert-column', 0).forEach((op) => applyPayload(state, op));
    expect(state.nodes.row.current_fields.cell_0).toBe('');
    expect(state.nodes.row.current_fields.cell_1).toBe('7');
    expect(state.nodes.row.current_fields.cell_2).toBe('=B1*2');
    expect(state.nodes.row.current_fields.format_1).toEqual({ bold: true });
    expect(state.nodes.total.current_fields.cell_0).toMatch(/Data'?!B1/i);
  });
  it('updates formulas on rename and marks deleted sheet references', () => {
    const state = workbook();
    sheetStructure(state, 'data', 'rename', 0, 'Field data').forEach((op) =>
      applyPayload(state, op)
    );
    expect(state.nodes.total.current_fields.cell_0).toBe("='Field data'!A1");
    sheetStructure(state, 'data', 'delete-sheet').forEach((op) => applyPayload(state, op));
    expect(state.nodes.data.deleted).toBe(true);
    expect(state.nodes.row.deleted).toBe(true);
    expect(String(state.nodes.total.current_fields.cell_0)).toContain('#REF!');
    expect(() => sheetStructure(state, 'summary', 'delete-sheet')).toThrow(
      'Keep at least one sheet'
    );
  });
  it('rejects duplicate names and sheet boundaries', () => {
    const state = workbook();
    expect(() => sheetStructure(state, 'data', 'rename', 0, 'summary')).toThrow('unique');
    expect(() => sheetStructure(state, 'data', 'rename', 0, 'bad/name')).toThrow('unique');
  });
});
describe('document tabs and LaTeX', () => {
  it('changes one tab without deleting or editing another tab', () => {
    const state = emptyState();
    [
      rootOp('root', 'Study'),
      itemOp('root', 0, { content: paragraph('Main') }, 'main'),
      {
        type: 'NodeCreated' as const,
        node_id: 'tab',
        node_type: 'section' as const,
        parent_id: 'root',
        pos: 'a',
        fields: { kind: 'document_tab', label: 'Appendix' }
      },
      itemOp('tab', 0, { content: paragraph('Appendix') }, 'appendix')
    ].forEach((op) => applyPayload(state, op));
    const content = richContent(state, 'tab');
    content.content![0].content![0].text = 'Revised appendix';
    richChanges(state, content, 'tab').forEach((op) => applyPayload(state, op));
    expect(state.nodes.main.deleted).toBe(false);
    expect(state.nodes.main.current_fields.content).toEqual(paragraph('Main'));
    expect(state.nodes.appendix.current_fields.label).toBe('Revised appendix');
  });
  it('retains unrecognized LaTeX as source without executing it', () => {
    const result = importLatex(
      '\\section{Methods}\n\nA paragraph.\n\n\\input{private.tex}\n\n\\[x^2\\]',
      'study.tex'
    );
    const state = emptyState();
    result.ops.forEach((op) => applyPayload(state, op));
    const source = latexDocument(state, 'Study & methods');
    expect(source).toContain('\\title{Study \\& methods}');
    expect(source).toContain('\\section{Methods}');
    expect(source).toContain('\\[x^2\\]');
    expect(source).toContain('\\textbackslash{}input\\{private.tex\\}');
    expect(source).not.toContain('\\input{private.tex}');
  });
  it('exports math and escapes prose independently', () => {
    expect(toLatex({ type: 'inlineMath', attrs: { latex: '\\frac{1}{2}' } })).toBe(
      '$\\frac{1}{2}$'
    );
    expect(toLatex({ type: 'text', text: '50% & $2' })).toBe('50\\% \\& \\$2');
  });
});
const form: GoogleForm = {
  formId: 'long_form_id_123',
  info: { title: 'Survey' },
  items: [
    {
      title: 'Consent?',
      questionItem: {
        question: {
          questionId: 'a1',
          required: true,
          choiceQuestion: {
            type: 'RADIO',
            options: [
              { value: 'Yes', goToSectionId: 'next' },
              { value: 'Other', isOther: true }
            ]
          }
        }
      }
    },
    {
      title: 'Experience',
      questionGroupItem: {
        questions: [
          { questionId: 'b1', rowQuestion: { title: 'Support' } },
          { questionId: 'b2', rowQuestion: { title: 'Safety' } }
        ],
        grid: { columns: { type: 'CHECKBOX', options: [{ value: 'Often' }, { value: 'Rarely' }] } }
      }
    }
  ]
};
describe('Google Forms conversion', () => {
  it('keeps stable source IDs and exposes grid, branching and other-answer differences', () => {
    const mapped = questionnaireRows(form);
    expect(mapped.survey[0]).toMatchObject({
      name: 'q_a1',
      google_question_id: 'a1',
      required: 'yes',
      type: 'select_one q_a1'
    });
    expect(mapped.survey[1]).toMatchObject({
      name: 'q_b1',
      type: 'select_multiple q_b1',
      label: 'Experience — Support'
    });
    expect(mapped.warnings.join(' ')).toContain('branching');
    expect(mapped.warnings.join(' ')).toContain('Other');
    expect(mapped.warnings.join(' ')).toContain('Grid');
  });
  it('retains unknown historic answers and neutralizes formula-like CSV text', () => {
    const csv = responseCsv(form, [
      {
        responseId: 'r1',
        answers: {
          a1: { textAnswers: { answers: [{ value: '=1+1' }] } },
          removed: { textAnswers: { answers: [{ value: 'Past response' }] } },
          b1: { textAnswers: { answers: [{ value: 'Often' }, { value: 'Rarely' }] } }
        }
      }
    ]);
    expect(csv).toContain('Earlier question [removed]');
    expect(csv).toContain("'=1+1");
    expect(csv).toContain('Past response');
    expect(csv).toContain('Often');
  });
  it('accepts only editor IDs and rejects responder and external URLs', () => {
    expect(formId('https://docs.google.com/forms/d/long_form_id_123/edit')).toBe(
      'long_form_id_123'
    );
    expect(() => formId('https://docs.google.com/forms/d/e/some_published_id/viewform')).toThrow();
    expect(() => formId('https://evil.example/forms/d/long_form_id_123/edit')).toThrow();
    expect(() => formId('https://forms.gle/short')).toThrow();
  });
});
