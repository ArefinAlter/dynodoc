import { MAX_FILE_BYTES } from '$lib/workspace/file-limits';
import { json } from '@sveltejs/kit';
import ExcelJS from 'exceljs';
import { activeIdentity, openProvider } from '$lib/server/session';
import {
  formId,
  questionnaireRows,
  responseCsv,
  type GoogleForm,
  type GoogleResponse
} from '$lib/server/google-forms-map';
import type { RequestHandler } from './$types';
let active = 0;
export const GET: RequestHandler = async ({ cookies, url }) => {
  const identity = await activeIdentity(cookies, url.protocol === 'https:');
  const connection = await openProvider(cookies.get('dynodoc_forms'));
  return json(
    { connected: !!identity && connection?.identity === identity },
    { headers: { 'cache-control': 'private, no-store' } }
  );
};
export const POST: RequestHandler = async ({ request, url, cookies }) => {
  if (request.headers.get('origin') !== url.origin)
    return json({ error: 'Invalid origin' }, { status: 403 });
  const identity = await activeIdentity(cookies, url.protocol === 'https:');
  if (!identity) return json({ error: 'Please sign in' }, { status: 401 });
  const text = await request.text();
  if (text.length > 2048) return json({ error: 'Request too large' }, { status: 413 });
  let input;
  try {
    input = JSON.parse(text);
  } catch {
    return json({ error: 'Invalid request' }, { status: 400 });
  }
  if (input.action === 'disconnect') {
    cookies.delete('dynodoc_forms', { path: '/api/google-forms' });
    return json({ ok: true });
  }
  const connection = await openProvider(cookies.get('dynodoc_forms'));
  if (connection?.identity !== identity || typeof connection?.token !== 'string')
    return json(
      { error: 'Connect Google Forms again. Connections expire after one hour.' },
      { status: 401 }
    );
  if (active >= 3)
    return json({ error: 'Other imports are running. Try again shortly.' }, { status: 429 });
  active++;
  try {
    const id = formId(String(input.form || ''));
    if (!['questionnaire', 'submissions'].includes(input.kind))
      throw new Error('Choose questions or responses.');
    let total = 0;
    async function read(path: string) {
      const response = await fetch(`https://forms.googleapis.com/v1/forms/${id}${path}`, {
        headers: { authorization: `Bearer ${connection!.token}` },
        redirect: 'error',
        signal: AbortSignal.timeout(20000)
      });
      if (!response.ok)
        throw new Error(
          'Google could not provide this form. Check your form ID, consent, and Google account permissions.'
        );
      const reader = response.body?.getReader();
      if (!reader) throw new Error('Empty provider response');
      const chunks: Uint8Array[] = [];
      for (;;) {
        const part = await reader.read();
        if (part.done) break;
        total += part.value.length;
        if (total > MAX_FILE_BYTES) {
          await reader.cancel();
          throw new Error('Import exceeds 10 MiB. Export a smaller dataset from Google Sheets.');
        }
        chunks.push(part.value);
      }
      return JSON.parse(Buffer.concat(chunks).toString('utf8'));
    }
    const form = (await read('')) as GoogleForm;
    let bytes: Buffer;
    let warnings: string[] = [];
    let extension = 'xlsx';
    if (input.kind === 'questionnaire') {
      const mapped = questionnaireRows(form);
      warnings = mapped.warnings;
      const workbook = new ExcelJS.Workbook();
      for (const [name, rows] of [
        ['survey', mapped.survey],
        ['choices', mapped.choices],
        ['settings', [{ form_title: form.info?.title || 'Google Form', form_id: id }]]
      ] as [string, Record<string, string>[]][]) {
        const sheet = workbook.addWorksheet(name);
        const keys = [...new Set(rows.flatMap((row) => Object.keys(row)))];
        if (!keys.length) keys.push('list_name', 'name', 'label');
        sheet.addRow(keys);
        rows.forEach((row) => sheet.addRow(keys.map((key) => row[key] || '')));
      }
      const source = workbook.addWorksheet('google_forms_source');
      source.addRow([
        'Source JSON, split into ordered text chunks. Not an executable questionnaire sheet.'
      ]);
      const raw = JSON.stringify(form);
      for (let i = 0; i < raw.length; i += 30000) source.addRow([raw.slice(i, i + 30000)]);
      bytes = Buffer.from(await workbook.xlsx.writeBuffer());
    } else {
      const responses: GoogleResponse[] = [];
      let next = '';
      const seen = new Set<string>();
      do {
        const q = new URLSearchParams({ pageSize: '500' });
        if (next) q.set('pageToken', next);
        const page = await read(`/responses?${q}`);
        responses.push(...(page.responses || []));
        if (responses.length > 2999)
          throw new Error(
            'Import is limited to 2,999 responses. Export a smaller dataset from Google Sheets.'
          );
        next = page.nextPageToken || '';
        if (next && seen.has(next)) throw new Error('Google returned a repeated response page.');
        seen.add(next);
      } while (next);
      bytes = Buffer.from(responseCsv(form, responses));
      extension = 'csv';
      warnings = [
        'Response IDs and original Google question IDs are preserved. Multi-answer and file references are JSON-valued cells; attached file contents are not downloaded.'
      ];
    }
    return json(
      {
        filename: `${(form.info?.title || 'Google Form').replace(/[^\p{L}\p{N} _-]/gu, '').slice(0, 80) || 'Google Form'}.${extension}`,
        content_type:
          extension === 'csv'
            ? 'text/csv'
            : 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
        content_base64: bytes.toString('base64'),
        warnings,
        source: {
          provider: 'google_forms',
          origin: 'https://forms.google.com',
          project: '',
          form: id,
          kind: input.kind,
          fetched_at: new Date().toISOString()
        }
      },
      { headers: { 'cache-control': 'private, no-store' } }
    );
  } catch (e) {
    return json(
      { error: e instanceof Error ? e.message : 'Could not import the Google Form.' },
      { status: 422 }
    );
  } finally {
    active--;
  }
};
