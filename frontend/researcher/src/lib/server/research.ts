import { MAX_FILE_BYTES } from '$lib/workspace/file-limits';
import { lookup } from 'node:dns/promises';
import { request as httpsRequest } from 'node:https';
import { isIP } from 'node:net';
import Papa from 'papaparse';
import type { ResearchRequest, ResearchFile, ResearchChoice } from '$lib/workspace/connectors';

export class ResearchError extends Error {}

const maximumBytes = MAX_FILE_BYTES;
type Reply = { status: number; body: Buffer };
export type ResearchTransport = (
  url: URL,
  authorization: string,
  init?: { method: 'POST'; body: string }
) => Promise<Reply>;

export function researchOrigin(input: string): URL {
  const url = new URL(input);
  if (
    url.protocol !== 'https:' ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== '/' ||
    (url.port && url.port !== '443') ||
    isIP(url.hostname) ||
    !url.hostname.includes('.') ||
    url.hostname.endsWith('.localhost')
  )
    throw new ResearchError(
      'Enter the HTTPS server address without a path, port or account details.'
    );
  return url;
}

export function publicIPv4(address: string): boolean {
  if (isIP(address) !== 4) return false;
  const [a, b, c] = address.split('.').map(Number);
  return !(
    a === 0 ||
    a === 10 ||
    a === 127 ||
    a >= 224 ||
    (a === 100 && b >= 64 && b <= 127) ||
    (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) ||
    (a === 192 && b === 0 && (c === 0 || c === 2)) ||
    (a === 192 && b === 88 && c === 99) ||
    (a === 198 && (b === 18 || b === 19)) ||
    (a === 198 && b === 51 && c === 100) ||
    (a === 203 && b === 0 && c === 113)
  );
}

// Resolve once, reject non-public addresses and pin the verified address to this TLS
// connection. Redirects are not followed, so credentials cannot leave the selected host.
export const researchGet: ResearchTransport = async (url, authorization, init) => {
  const addresses = await lookup(url.hostname, { family: 4, all: true });
  if (!addresses.length || addresses.some(({ address }) => !publicIPv4(address)))
    throw new ResearchError('This connector requires a publicly reachable HTTPS research server.');
  return new Promise((resolve, reject) => {
    const req = httpsRequest(
      url,
      {
        method: init?.method || 'GET',
        agent: false,
        family: 4,
        servername: url.hostname,
        headers: {
          ...(authorization ? { authorization } : {}),
          accept: '*/*',
          'accept-encoding': 'identity',
          ...(init
            ? {
                'content-type': 'application/x-www-form-urlencoded',
                'content-length': Buffer.byteLength(init.body)
              }
            : {})
        },
        lookup: (_hostname, _options, callback) => callback(null, addresses[0].address, 4),
        signal: AbortSignal.timeout(20_000)
      },
      (response) => {
        const chunks: Buffer[] = [];
        let size = 0;
        response.on('data', (chunk: Buffer) => {
          size += chunk.length;
          if (size > maximumBytes)
            req.destroy(
              new ResearchError('The research export exceeds 3 MB. Use a smaller export.')
            );
          else chunks.push(chunk);
        });
        response.on('error', reject);
        response.on('end', () =>
          resolve({ status: response.statusCode || 502, body: Buffer.concat(chunks) })
        );
      }
    );
    req.on('error', reject);
    req.end(init?.body);
  });
};

export function identifier(value: unknown, numeric = false): string {
  if (
    typeof value !== 'string' ||
    !value ||
    value.length > 200 ||
    value === '.' ||
    value === '..' ||
    /[\\/?#]/.test(value) ||
    Array.from(value).some((c) => c.charCodeAt(0) < 32) ||
    (numeric && !/^\d+$/.test(value))
  )
    throw new ResearchError('Choose a valid project and form.');
  return encodeURIComponent(value);
}
function parse(body: Buffer): Record<string, unknown> | unknown[] {
  const value: unknown = JSON.parse(body.toString('utf8'));
  if (!value || typeof value !== 'object')
    throw new ResearchError('The server returned an unexpected response.');
  return value as Record<string, unknown> | unknown[];
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new ResearchError('The server returned an unexpected record.');
  return value as Record<string, unknown>;
}

export function recordsToCsv(records: unknown[]): string {
  const flat = records.map((row) => {
    const result: Record<string, string> = Object.create(null);
    function visit(value: unknown, prefix: string, depth: number) {
      if (depth > 12)
        throw new ResearchError(
          'Nested responses require a separate export from the research tool.'
        );
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        for (const [key, item] of Object.entries(value))
          visit(item, prefix ? `${prefix}/${key}` : key, depth + 1);
      } else
        result[prefix] = Array.isArray(value)
          ? JSON.stringify(value)
          : value == null
            ? ''
            : String(value);
    }
    visit(object(row), '', 0);
    return result;
  });
  const columns = [...new Set(flat.flatMap((row) => Object.keys(row)))];
  if (columns.length > 100)
    throw new ResearchError(
      'This export has more than 100 columns. Select fewer fields in your research tool.'
    );
  if (!columns.length) throw new ResearchError('This form does not have responses to import yet.');
  return Papa.unparse(
    { fields: columns, data: flat.map((r) => columns.map((c) => r[c] ?? '')) },
    { escapeFormulae: true }
  );
}

export async function researchRead(
  input: ResearchRequest,
  transport = researchGet
): Promise<{ items: ResearchChoice[] } | ResearchFile> {
  const base = researchOrigin(input.origin);
  if (
    !['odk', 'kobo', 'redcap', 'surveycto', 'surveymonkey'].includes(input.provider) ||
    !['projects', 'forms', 'import'].includes(input.action)
  )
    throw new ResearchError('Choose a supported research tool and action.');
  if (
    typeof input.token !== 'string' ||
    !input.token.trim() ||
    input.token.length > 4096 ||
    (input.provider !== 'surveycto' && /\s/.test(input.token))
  )
    throw new ResearchError('Enter the API key or ODK session token without spaces.');
  if (['redcap', 'surveycto', 'surveymonkey'].includes(input.provider))
    return (await import('./research-vendors')).vendorRead(input, transport);
  const authorization = `${input.provider === 'odk' ? 'Bearer' : 'Token'} ${input.token}`;
  let downloaded = 0;
  const get = async (path: string) => {
    const url = new URL(path, base);
    if (url.origin !== base.origin)
      throw new ResearchError('The server returned a link to a different host.');
    const reply = await transport(url, authorization);
    downloaded += reply.body.length;
    if (downloaded > maximumBytes)
      throw new ResearchError('The research export exceeds 3 MB. Use a smaller export.');
    if (reply.status === 401 || reply.status === 403)
      throw new ResearchError('The token does not have permission to read this project.');
    if (reply.status === 404)
      throw new ResearchError(
        'The form or XLSX source was not found. Check its ID; forms created from XML or XLS need a separate XLSX upload.'
      );
    if (reply.status !== 200)
      throw new ResearchError(
        `The research server could not return this export (${reply.status}).`
      );
    return reply.body;
  };
  if (input.action === 'projects' || input.action === 'forms') {
    const path =
      input.provider === 'kobo'
        ? '/api/v2/assets/?asset_type=survey&limit=100&format=json'
        : input.action === 'projects'
          ? '/v1/projects'
          : `/v1/projects/${identifier(input.project, true)}/forms`;
    const items: ResearchChoice[] = [];
    let next: string | null = path;
    for (let page = 0; next && page < 10; page++) {
      const payload = parse(await get(next));
      const rows = Array.isArray(payload) ? payload : payload.results;
      if (!Array.isArray(rows))
        throw new ResearchError('The server did not return a project list.');
      for (const row of rows) {
        const value = object(row);
        if (input.provider === 'kobo' && value.asset_type && value.asset_type !== 'survey')
          continue;
        const id = String(
          input.provider === 'kobo'
            ? value.uid
            : input.action === 'projects'
              ? value.id
              : value.xmlFormId
        );
        if (id && id !== 'undefined') items.push({ id, name: String(value.name || id) });
      }
      next = !Array.isArray(payload) && typeof payload.next === 'string' ? payload.next : null;
      if (next && !new URL(next, base).pathname.startsWith('/api/v2/assets/'))
        throw new ResearchError('Unexpected project page link.');
    }
    if (next)
      throw new ResearchError(
        'This account has too many projects to list. Use an account scoped to the pilot projects.'
      );
    return { items };
  }
  if (input.kind !== 'questionnaire' && input.kind !== 'submissions')
    throw new ResearchError('Choose a questionnaire or responses.');
  const form = identifier(input.form),
    project = input.provider === 'odk' ? identifier(input.project, true) : '';
  const stem =
    input.provider === 'odk' ? `/v1/projects/${project}/forms/${form}` : `/api/v2/assets/${form}`;
  let content: Buffer, ext: string;
  const warnings: string[] = [];
  if (input.kind === 'questionnaire') {
    // Kobo's "xls" renderer returns XLSX (its extension is retained for API compatibility).
    content = await get(stem + (input.provider === 'odk' ? '.xlsx' : '.xls'));
    if (content[0] !== 0x50 || content[1] !== 0x4b)
      throw new ResearchError(
        'This form is not an XLSX file. Export an XLSX copy from the research tool and upload it.'
      );
    ext = 'xlsx';
    warnings.push(
      'Questionnaire media and external datasets are not downloaded. Validate the exported form in your collection platform before fieldwork.'
    );
  } else {
    const records: unknown[] = [];
    const dataPath = stem + (input.provider === 'odk' ? '.svc/Submissions' : '/data/');
    let next: string | null =
      dataPath + (input.provider === 'odk' ? '?$top=500' : '?format=json&limit=500');
    for (let page = 0; next && page < 10; page++) {
      if (new URL(next, base).pathname !== dataPath)
        throw new ResearchError('Unexpected response page link.');
      const payload = object(parse(await get(next)));
      const rows = input.provider === 'odk' ? payload.value : payload.results;
      if (!Array.isArray(rows))
        throw new ResearchError('The server did not return response records.');
      records.push(...rows);
      if (records.length > 2999 || Number(payload.count || payload['@odata.count'] || 0) > 2999)
        throw new ResearchError(
          'The pilot supports up to 2,999 responses per import. Use a filtered CSV/XLSX export for this study.'
        );
      const link = input.provider === 'odk' ? payload['@odata.nextLink'] : payload.next;
      next = typeof link === 'string' && link ? link : null;
    }
    if (next)
      throw new ResearchError('The export has too many pages. Use a filtered CSV/XLSX export.');
    content = Buffer.from(recordsToCsv(records));
    ext = 'csv';
    warnings.push(
      'This is a snapshot of current responses. Nested objects become columns and arrays are retained as JSON text. Separate ODK repeat tables, media attachments and encrypted responses are not imported. Editing this copy does not update the collection platform.'
    );
  }
  if (content.length > maximumBytes) throw new ResearchError('The research export exceeds 3 MB.');
  const filename = `${input.provider}-${input.form}-${input.kind}.${ext}`.replace(/[^\w.-]/g, '_');
  return {
    filename,
    content_type:
      ext === 'csv'
        ? 'text/csv'
        : 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    content_base64: content.toString('base64'),
    warnings,
    source: {
      provider: input.provider,
      origin: base.origin,
      project: input.project || '',
      form: input.form!,
      kind: input.kind,
      fetched_at: new Date().toISOString()
    }
  };
}
