/** File interchange, not live account synchronization. See docs/research-compatibility.md. */
import Papa from 'papaparse';
import { ulid } from 'ulid';
import type { DocumentState, EventPayload, MaterializedNode } from '$lib/api/types';
import { rootOp, download, type ImportResult } from './files';
export type ResearchSheet = { name: string; rows: string[][] };
function records(rows: string[][]): Record<string, string>[] {
  const headers = rows[0] || [];
  return rows
    .slice(1)
    .filter((row) => row.some((v) => v !== ''))
    .map((row) => Object.fromEntries(headers.map((h, i) => [h.trim(), row[i] || ''])));
}
const labelKey = (row: Record<string, string>) =>
  Object.keys(row).find((k) => k === 'label') ||
  Object.keys(row).find((k) => k.startsWith('label::')) ||
  'label';
export function importXlsForm(sheets: ResearchSheet[], filename: string): ImportResult | null {
  const survey = sheets.find((s) => s.name.toLowerCase() === 'survey');
  if (!survey || !survey.rows[0]?.includes('type') || !survey.rows[0]?.includes('name'))
    return null;
  const choices = records(sheets.find((s) => s.name.toLowerCase() === 'choices')?.rows || []);
  const settings =
    records(sheets.find((s) => s.name.toLowerCase() === 'settings')?.rows || [])[0] || {};
  const rootId = ulid(),
    ops: EventPayload[] = [rootOp(rootId, settings.form_title || filename)];
  (ops[0] as Extract<EventPayload, { type: 'NodeCreated' }>).fields = {
    label: settings.form_title || filename,
    xlsform_settings: settings
  };
  const parents = [rootId],
    warnings = [
      'XLSForm structure, expressions, custom columns and translations are retained. Preview executes the documented expression subset and flags unsupported logic. Validate the exported form in ODK, KoboToolbox or SurveyCTO before deployment.'
    ];
  let index = 0;
  for (const row of records(survey.rows)) {
    const type = row.type?.trim();
    if (!type) continue;
    if (
      type === 'end_group' ||
      type === 'end group' ||
      type === 'end_repeat' ||
      type === 'end repeat'
    ) {
      if (parents.length < 2) throw new Error('XLSForm contains an unmatched end group/repeat.');
      parents.pop();
      continue;
    }
    const group = ['begin_group', 'begin group', 'begin_repeat', 'begin repeat'].includes(type),
      id = ulid(),
      key = labelKey(row);
    const [questionType, list] = type.split(/\s+/);
    ops.push({
      type: 'NodeCreated',
      node_id: id,
      node_type: group ? 'section' : 'item',
      parent_id: parents.at(-1),
      pos: String(index++).padStart(8, '0'),
      var_name: row.name || null,
      fields: {
        label: row[key] || row.name,
        hint: row.hint || row[Object.keys(row).find((k) => k.startsWith('hint::')) || ''] || '',
        question_type: questionType,
        group_type: group ? (type.includes('repeat') ? 'repeat' : 'group') : null,
        required: ['yes', 'true', '1'].includes(row.required),
        required_expression:
          row.required && !['yes', 'true', '1', 'no', 'false', '0'].includes(row.required)
            ? row.required
            : '',
        relevance: row.relevant || '',
        constraint: row.constraint || '',
        constraint_message: row.constraint_message || '',
        calculation: row.calculation || '',
        default: row.default || '',
        xlsform: row,
        label_column: key
      }
    });
    if (group) {
      parents.push(id);
      continue;
    }
    if (list && ['select_one', 'select_multiple', 'rank'].includes(questionType))
      for (const choice of choices.filter((c) => c.list_name === list)) {
        ops.push({
          type: 'ChoiceAdded',
          node_id: id,
          choice_id: ulid(),
          fields: {
            label: choice[labelKey(choice)] || choice.name,
            value: choice.name,
            xlsform: choice,
            label_column: labelKey(choice)
          }
        });
      }
  }
  if (parents.length !== 1) throw new Error('XLSForm contains an unclosed group or repeat.');
  if (ops.length > 5000)
    throw new Error('This form exceeds the demo’s 5,000-operation import limit.');
  return { kind: 'questionnaire', ops, warnings, filename, rootId };
}
const redcapHeaders = [
  'Variable / Field Name',
  'Form Name',
  'Section Header',
  'Field Type',
  'Field Label',
  'Choices, Calculations, OR Slider Labels',
  'Field Note',
  'Text Validation Type OR Show Slider Number',
  'Text Validation Min',
  'Text Validation Max',
  'Identifier?',
  'Branching Logic (Show field only if...)',
  'Required Field?',
  'Custom Alignment',
  'Question Number (surveys only)',
  'Matrix Group Name',
  'Matrix Ranking?',
  'Field Annotation'
];
export function importRedcap(rows: string[][], filename: string): ImportResult | null {
  if (!rows[0]?.includes('Variable / Field Name') || !rows[0]?.includes('Field Type')) return null;
  const rootId = ulid(),
    ops: EventPayload[] = [rootOp(rootId, filename)],
    groups = new Map<string, string>();
  let index = 0;
  for (const row of records(rows)) {
    const form = row['Form Name'] || 'survey';
    let parent = groups.get(form);
    if (!parent) {
      parent = ulid();
      groups.set(form, parent);
      ops.push({
        type: 'NodeCreated',
        node_id: parent,
        node_type: 'section',
        parent_id: rootId,
        pos: String(index++).padStart(8, '0'),
        fields: { label: form, redcap_form: form }
      });
    }
    const id = ulid(),
      sourceType = row['Field Type'];
    const type =
      (
        {
          radio: 'select_one',
          dropdown: 'select_one',
          checkbox: 'select_multiple',
          yesno: 'select_one',
          truefalse: 'select_one',
          notes: 'text',
          descriptive: 'note',
          calc: 'calculate'
        } as Record<string, string>
      )[sourceType] || 'text';
    ops.push({
      type: 'NodeCreated',
      node_id: id,
      node_type: 'item',
      parent_id: parent,
      pos: String(index++).padStart(8, '0'),
      var_name: row['Variable / Field Name'],
      fields: {
        label: row['Field Label'],
        hint: row['Field Note'],
        question_type: type,
        required: row['Required Field?'] === 'y',
        redcap: row,
        redcap_logic: row['Branching Logic (Show field only if...)'],
        calculation: sourceType === 'calc' ? row['Choices, Calculations, OR Slider Labels'] : ''
      }
    });
    const choices =
      sourceType === 'yesno'
        ? '1, Yes | 0, No'
        : sourceType === 'truefalse'
          ? '1, True | 0, False'
          : row['Choices, Calculations, OR Slider Labels'];
    if (type.startsWith('select_'))
      for (const part of choices.split('|').filter(Boolean)) {
        const at = part.indexOf(',');
        if (at < 0) throw new Error(`Invalid REDCap choice for ${row['Variable / Field Name']}`);
        ops.push({
          type: 'ChoiceAdded',
          node_id: id,
          choice_id: ulid(),
          fields: { value: part.slice(0, at).trim(), label: part.slice(at + 1).trim() }
        });
      }
  }
  return {
    kind: 'questionnaire',
    ops,
    filename,
    rootId,
    warnings: [
      'REDCap dictionary fields and source metadata are retained. REDCap branching logic, matrices, validation, action tags and calculations must be tested in REDCap; the Dynodoc preview does not execute them.'
    ]
  };
}
const live = (state: DocumentState) =>
  Object.values(state.nodes).filter((n) => !n.deleted && !state.removed_choices.includes(n.id));
const value = (n: MaterializedNode, key: string) => String(n.current_fields[key] ?? '');
function assertNames(nodes: MaterializedNode[]) {
  const used = new Set();
  for (const n of nodes.filter((n) => n.type === 'item' || n.type === 'section')) {
    if (!n.var_name || !/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(n.var_name))
      throw new Error(
        `Give “${value(n, 'label')}” a variable name using letters, numbers and underscores.`
      );
    if (used.has(n.var_name)) throw new Error(`Variable ${n.var_name} is used twice.`);
    used.add(n.var_name);
  }
}
export async function exportXlsForm(state: DocumentState, title: string) {
  const nodes = live(state);
  assertNames(nodes);
  if (nodes.some((n) => value(n, 'redcap_logic')))
    throw new Error(
      'This form contains REDCap branching logic. Translate it to XLSForm relevance before exporting to XLSForm.'
    );
  const { default: ExcelJS } = await import('exceljs');
  const book = new ExcelJS.Workbook(),
    survey: Record<string, unknown>[] = [],
    choices: Record<string, unknown>[] = [];
  const children = (id: string) =>
    nodes
      .filter((n) => n.parent_id === id && (n.type === 'item' || n.type === 'section'))
      .sort((a, b) => (a.pos < b.pos ? -1 : 1));
  function visit(n: MaterializedNode) {
    const row = { ...((n.current_fields.xlsform as Record<string, string>) || {}) } as Record<
      string,
      unknown
    >;
    row.name = n.var_name;
    row[String(n.current_fields.label_column || 'label')] = value(n, 'label');
    row.hint = value(n, 'hint');
    // Older imports kept conditional required only in the original XLSForm columns.
    const originalRequired = String(row.required || '');
    const requiredExpression =
      n.current_fields.required_expression ??
      (['yes', 'true', '1', 'no', 'false', '0'].includes(originalRequired) ? '' : originalRequired);
    row.required = String(requiredExpression) || (n.current_fields.required ? 'yes' : '');
    row.relevant = value(n, 'relevance');
    row.constraint = value(n, 'constraint');
    row.constraint_message = value(n, 'constraint_message');
    row.calculation = value(n, 'calculation');
    row.default = value(n, 'default');
    if (n.type === 'section') {
      const type = n.current_fields.group_type === 'repeat' ? 'repeat' : 'group';
      row.type = `begin_${type}`;
      survey.push(row);
      children(n.id).forEach(visit);
      survey.push({ type: `end_${type}` });
      return;
    }
    let type = value(n, 'question_type') || 'text';
    if (type === 'select_one' || type === 'select_multiple' || type === 'rank') {
      const list = `list_${n.var_name}`;
      type += ' ' + list;
      for (const choice of nodes.filter((c) => c.type === 'choice' && c.parent_id === n.id)) {
        choices.push({
          ...((choice.current_fields.xlsform as object) || {}),
          list_name: list,
          name: value(choice, 'value'),
          [String(choice.current_fields.label_column || 'label')]: value(choice, 'label')
        });
      }
    }
    row.type = type;
    survey.push(row);
  }
  const root = nodes.find((n) => n.type === 'form');
  if (root) children(root.id).forEach(visit);
  function add(name: string, rows: Record<string, unknown>[], first: string[]) {
    const headers = [...new Set([...first, ...rows.flatMap((r) => Object.keys(r))])];
    const sheet = book.addWorksheet(name);
    sheet.addRow(headers);
    rows.forEach((row) => sheet.addRow(headers.map((h) => row[h] ?? '')));
    sheet.getRow(1).font = { bold: true };
    sheet.columns.forEach((c) => (c.width = 24));
  }
  add('survey', survey, ['type', 'name', 'label']);
  add('choices', choices, ['list_name', 'name', 'label']);
  add(
    'settings',
    [
      {
        ...((root?.current_fields.xlsform_settings as object) || {}),
        form_title: title,
        form_id:
          (root?.current_fields.xlsform_settings as Record<string, string>)?.form_id ||
          'dynodoc_' + (root?.id || 'survey').slice(-8)
      }
    ],
    ['form_title', 'form_id']
  );
  download(
    (await book.xlsx.writeBuffer()) as ArrayBuffer,
    title + '.xlsx',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
  );
}
export function exportRedcap(state: DocumentState, title: string) {
  const nodes = live(state);
  assertNames(nodes.filter((n) => n.type === 'item'));
  if (nodes.some((n) => value(n, 'relevance') || value(n, 'constraint')))
    throw new Error(
      'Translate XLSForm relevance and validation to REDCap expressions before exporting this dictionary.'
    );
  const rows = nodes
    .filter((n) => n.type === 'item')
    .sort((a, b) => (a.pos < b.pos ? -1 : 1))
    .map((n) => {
      const source = (n.current_fields.redcap || {}) as Record<string, string>;
      const choices = nodes
        .filter((c) => c.type === 'choice' && c.parent_id === n.id)
        .map((c) => `${value(c, 'value')}, ${value(c, 'label')}`)
        .join(' | ');
      const type = value(n, 'question_type');
      const mapped =
        (
          {
            select_one: 'radio',
            select_multiple: 'checkbox',
            note: 'descriptive',
            calculate: 'calc'
          } as Record<string, string>
        )[type] || 'text';
      const row: Record<string, string> = {
        ...source,
        'Variable / Field Name': n.var_name || '',
        'Form Name': source['Form Name'] || 'survey',
        'Field Type': source['Field Type'] || mapped,
        'Field Label': value(n, 'label'),
        'Field Note': value(n, 'hint'),
        'Required Field?': n.current_fields.required ? 'y' : '',
        'Choices, Calculations, OR Slider Labels':
          type === 'calculate'
            ? value(n, 'calculation')
            : choices || source['Choices, Calculations, OR Slider Labels'] || '',
        'Branching Logic (Show field only if...)': value(n, 'redcap_logic')
      };
      return redcapHeaders.map((h) => row[h] || '');
    });
  download(Papa.unparse([redcapHeaders, ...rows]), title + '.redcap.csv', 'text/csv;charset=utf-8');
}
