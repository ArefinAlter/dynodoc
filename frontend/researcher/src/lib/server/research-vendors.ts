import Papa from 'papaparse';
import type { ResearchRequest, ResearchFile, ResearchChoice } from '$lib/workspace/connectors';
import {
  ResearchError,
  researchOrigin,
  recordsToCsv,
  identifier,
  type ResearchTransport
} from './research';
const metadataColumns = [
  ['field_name', 'Variable / Field Name'],
  ['form_name', 'Form Name'],
  ['section_header', 'Section Header'],
  ['field_type', 'Field Type'],
  ['field_label', 'Field Label'],
  ['select_choices_or_calculations', 'Choices, Calculations, OR Slider Labels'],
  ['field_note', 'Field Note'],
  ['text_validation_type_or_show_slider_number', 'Text Validation Type OR Show Slider Number'],
  ['text_validation_min', 'Text Validation Min'],
  ['text_validation_max', 'Text Validation Max'],
  ['identifier', 'Identifier?'],
  ['branching_logic', 'Branching Logic (Show field only if...)'],
  ['required_field', 'Required Field?'],
  ['custom_alignment', 'Custom Alignment'],
  ['question_number', 'Question Number (surveys only)'],
  ['matrix_group_name', 'Matrix Group Name'],
  ['matrix_ranking', 'Matrix Ranking?'],
  ['field_annotation', 'Field Annotation']
];
export function redcapDictionary(rows: Record<string, unknown>[]) {
  // Preserve expression strings. Dictionary imports do not execute spreadsheet cells.
  return Papa.unparse({
    fields: metadataColumns.map((c) => c[1]),
    data: rows.map((r) => metadataColumns.map((c) => String(r[c[0]] ?? '')))
  });
}
export async function vendorRead(
  input: ResearchRequest,
  transport: ResearchTransport
): Promise<{ items: ResearchChoice[] } | ResearchFile> {
  const base = researchOrigin(input.origin);
  if (input.provider === 'surveymonkey' && base.hostname !== 'api.surveymonkey.com')
    throw new ResearchError('Use https://api.surveymonkey.com for SurveyMonkey.');
  if (input.provider === 'surveycto' && !base.hostname.endsWith('.surveycto.com'))
    throw new ResearchError('Use your server address ending in .surveycto.com.');
  let total = 0;
  const request = async (path: string, body?: URLSearchParams) => {
    const url = new URL(path, base);
    if (url.origin !== base.origin)
      throw new ResearchError('The server returned a link to a different host.');
    let auth = '';
    if (input.provider === 'surveymonkey') auth = 'Bearer ' + input.token;
    if (input.provider === 'surveycto') {
      if (!input.username || input.username.length > 254 || /[:\r\n]/.test(input.username))
        throw new ResearchError('Enter a valid SurveyCTO API username.');
      auth = 'Basic ' + Buffer.from(input.username + ':' + input.token).toString('base64');
    }
    const response = await transport(
      url,
      auth,
      body ? { method: 'POST', body: body.toString() } : undefined
    );
    total += response.body.length;
    if (total > 10 * 1024 * 1024)
      throw new ResearchError('The export exceeds 10 MB. Use a filtered file export.');
    if (response.status === 401 || response.status === 403)
      throw new ResearchError('Your credentials do not have permission to export this project.');
    if (response.status !== 200)
      throw new ResearchError(
        `The research server could not return this export (${response.status}).`
      );
    const result = JSON.parse(response.body.toString('utf8'));
    if (result?.error)
      throw new ResearchError('The research tool refused the export. Check API permissions.');
    return result;
  };
  const redcap = async (content: string) => {
    const path = input.api_path || '/api/';
    if (path.length > 200 || !/^\/(?:[a-zA-Z0-9_-]+\/)*api\/$/.test(path))
      throw new ResearchError('Use the REDCap API path, such as /api/ or /redcap/api/.');
    return request(
      path,
      new URLSearchParams({
        token: input.token,
        content,
        action: 'export',
        format: 'json',
        type: 'flat',
        returnFormat: 'json',
        rawOrLabel: 'raw',
        rawOrLabelHeaders: 'raw',
        exportCheckboxLabel: 'false'
      })
    );
  };
  if (input.action !== 'import') {
    if (input.provider === 'redcap') {
      const project = await redcap('project');
      return {
        items: [
          {
            id: String(project.project_id || 'project'),
            name: String(project.project_title || 'REDCap project')
          }
        ]
      };
    }
    if (input.provider === 'surveycto')
      throw new ResearchError(
        'Enter the form ID from your SurveyCTO server to import its responses.'
      );
    const items: ResearchChoice[] = [];
    let next: string | undefined = '/v3/surveys?per_page=100';
    for (let i = 0; next && i < 30; i++) {
      const target = new URL(next, base);
      if (target.pathname !== '/v3/surveys')
        throw new ResearchError('Unexpected survey page link.');
      const result = await request(next);
      if (!Array.isArray(result.data))
        throw new ResearchError('The server did not return surveys.');
      items.push(
        ...result.data.map((r: Record<string, unknown>) => ({
          id: String(r.id),
          name: String(r.title || r.id)
        }))
      );
      next = result.links?.next;
    }
    if (next) throw new ResearchError('Too many surveys. Use a more narrowly scoped account.');
    return { items };
  }
  if (!['questionnaire', 'submissions'].includes(input.kind || ''))
    throw new ResearchError('Choose an import type.');
  let rows: Record<string, unknown>[], csv: string;
  if (input.provider === 'redcap') {
    rows = await redcap(input.kind === 'questionnaire' ? 'metadata' : 'record');
    if (!Array.isArray(rows)) throw new ResearchError('REDCap did not return export records.');
    csv = input.kind === 'questionnaire' ? redcapDictionary(rows) : recordsToCsv(rows);
  } else {
    if (input.kind !== 'submissions')
      throw new ResearchError(
        'This connection imports responses. Upload an XLSForm or dictionary for questionnaire editing.'
      );
    const id = identifier(input.form, input.provider === 'surveymonkey');
    if (input.provider === 'surveycto')
      rows = await request(`/api/v2/forms/data/wide/json/${id}?date=0`);
    else {
      rows = [];
      const path = `/v3/surveys/${id}/responses/bulk`;
      let next: string | undefined = path + '?per_page=100';
      for (let i = 0; next && i < 30; i++) {
        if (new URL(next, base).pathname !== path)
          throw new ResearchError('Unexpected response page link.');
        const result = await request(next);
        if (!Array.isArray(result.data))
          throw new ResearchError('The server did not return response records.');
        rows.push(...result.data);
        if (rows.length > 2999 || Number(result.total || 0) > 2999)
          throw new ResearchError(
            'Limit is 2,999 responses per import. Use a filtered file export.'
          );
        next = result.links?.next;
      }
      if (next) throw new ResearchError('Too many response pages. Use a filtered file export.');
    }
    if (!Array.isArray(rows))
      throw new ResearchError('The server did not return response records.');
    csv = recordsToCsv(rows);
  }
  if (rows.length > 2999)
    throw new ResearchError('Limit is 2,999 rows per import. Use a filtered file export.');
  if (!rows.length) throw new ResearchError('There are no records to import.');
  const content = Buffer.from(csv);
  if (content.length > 10 * 1024 * 1024) throw new ResearchError('The export exceeds 10 MB.');
  return {
    filename: `${input.provider}-${input.form || 'project'}-${input.kind}.csv`.replace(
      /[^\w.-]/g,
      '_'
    ),
    content_type: 'text/csv',
    content_base64: content.toString('base64'),
    source: {
      provider: input.provider,
      origin: base.origin,
      project: input.project || '',
      form: input.form || 'project',
      kind: input.kind!,
      fetched_at: new Date().toISOString()
    },
    warnings: [
      'Imported snapshot only. Changes in Dynodoc do not update the collection platform; credentials are not saved.',
      input.provider === 'redcap'
        ? 'REDCap metadata and raw values are preserved. Branching, calculations, longitudinal events, repeat instances and instrument behavior require validation in REDCap.'
        : input.provider === 'surveycto'
          ? 'Only API-exportable response fields are retrieved. Media, repeat tables and decryption of protected responses are not supported. The server’s default review-status filter applies.'
          : 'SurveyMonkey response IDs and nested pages/questions are preserved as JSON-valued columns. This is not a reconstructed survey designer or a question-label crosswalk.'
    ]
  };
}
