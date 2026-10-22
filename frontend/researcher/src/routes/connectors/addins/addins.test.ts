// @vitest-environment node
// The add-in routes in both modes: who may fetch the files, who may see the instructions, and
// that a hosted process has neither. The settings are read through the real configuration module
// from a mocked environment, so what is tested is what the server does with its settings.
import { unzipSync } from 'fflate';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const env = vi.hoisted(() => ({}) as Record<string, string | undefined>);
const session = vi.hoisted(() => ({
  accessToken: vi.fn(async () => null as string | null),
  engineUrl: () => 'http://engine.test'
}));
vi.mock('$env/dynamic/private', () => ({ env }));
vi.mock('$env/dynamic/public', () => ({ env: {} }));
vi.mock('$app/environment', () => ({ dev: false }));
vi.mock('$lib/server/session', () => session);

import { resetConfig } from '$lib/server/config';
import { GET } from './[file]/+server';
import { load } from './+page.server';

const SECRETS = {
  SESSION_SECRET: 'f4a6c1e8b3d7092a5c8e1f4b7d0a3c6ef4a6c1e8b3d7092a5c8e1f4b7d0a3c6e',
  WEB_SERVICE_KEY: '2b9d4f7a1c5e8036b9d4f7a1c5e8036a2b9d4f7a1c5e8036b9d4f7a1c5e8036a'
};
const SELFHOSTED = {
  ...SECRETS,
  DYNODOC_MODE: 'selfhosted',
  ORIGIN: 'https://dynodoc.corp.example',
  DYNODOC_BRAND_NAME: 'Acme Docs',
  DYNODOC_ORG_NAME: 'Acme Research Ltd.',
  DYNODOC_SUPPORT_URL: 'https://help.corp.example/desk',
  DYNODOC_MAIL_TRANSPORT: 'smtp',
  SMTP_HOST: 'mail.corp.example',
  SMTP_FROM: 'docs@corp.example'
};
function configure(settings: Record<string, string | undefined>) {
  for (const key of Object.keys(env)) delete env[key];
  Object.assign(env, settings);
  resetConfig();
}
const download = (file: string) => GET({ params: { file } } as Parameters<typeof GET>[0]);
async function thrown(action: () => unknown) {
  try {
    await action();
  } catch (caught) {
    return caught as { status: number; location?: string; body?: { message?: string } };
  }
  throw new Error('Expected a redirect or an error.');
}
const pageEvent = () =>
  ({
    cookies: {},
    url: new URL('https://dynodoc.corp.example/connectors/addins'),
    setHeaders: vi.fn()
  }) as unknown as Parameters<typeof load>[0];
const fetchMock = vi.fn();

beforeEach(() => {
  session.accessToken.mockReset().mockResolvedValue(null);
  fetchMock.mockReset();
  vi.stubGlobal('fetch', fetchMock);
});

describe('the downloads in self-hosted mode', () => {
  beforeEach(() => configure(SELFHOSTED));

  it('are public: no session is read, and every file is served with the right type', async () => {
    const types: Record<string, string> = {
      'word.xml': 'application/xml; charset=utf-8',
      'excel-catalog.xml': 'application/xml; charset=utf-8',
      'powerpoint.xml': 'application/xml; charset=utf-8',
      'google-docs.zip': 'application/zip',
      'google-sheets.zip': 'application/zip',
      'google-slides.zip': 'application/zip',
      SHA256SUMS: 'text/plain; charset=utf-8'
    };
    for (const [name, type] of Object.entries(types)) {
      const response = await download(name);
      expect(response.status, name).toBe(200);
      expect(response.headers.get('content-type'), name).toBe(type);
      expect(response.headers.get('content-disposition'), name).toBe(
        `attachment; filename="${name}"`
      );
      expect(response.headers.get('x-content-type-options'), name).toBe('nosniff');
    }
    expect(session.accessToken).not.toHaveBeenCalled();
    expect(fetchMock).not.toHaveBeenCalled();
  });
  it('name the installation, its product name and its support link, and nothing hosted', async () => {
    const manifest = await (await download('word.xml')).text();
    expect(manifest).toContain('<AppDomain>https://dynodoc.corp.example</AppDomain>');
    expect(manifest).toContain('DisplayName DefaultValue="Acme Docs for Word"');
    expect(manifest).toContain('<ProviderName>Acme Research Ltd.</ProviderName>');
    expect(manifest).toContain('SupportUrl DefaultValue="https://help.corp.example/desk"');
    expect(manifest).not.toMatch(/dynodoc\.online/i);
    const catalog = await (await download('word-catalog.xml')).text();
    expect(catalog).not.toContain('VersionOverrides');
    const files = unzipSync(
      new Uint8Array(await (await download('google-docs.zip')).arrayBuffer())
    );
    expect(new TextDecoder().decode(files['appsscript.json'])).toContain(
      'https://dynodoc.corp.example/api/connectors/'
    );
    const sums = await (await download('SHA256SUMS')).text();
    expect(sums).toMatch(/^[0-9a-f]{64} {2}word\.xml$/m);
  });
  it('follow the settings: a new product name or address gives new files', async () => {
    const before = await (await download('word.xml')).text();
    configure({ ...SELFHOSTED, DYNODOC_BRAND_NAME: 'Other Docs' });
    expect(await (await download('word.xml')).text()).toContain('Other Docs for Word');
    configure({ ...SELFHOSTED, ORIGIN: 'https://moved.corp.example' });
    const moved = await (await download('word.xml')).text();
    expect(moved).toContain('https://moved.corp.example');
    expect(moved).not.toContain('https://dynodoc.corp.example');
    expect(moved).not.toBe(before);
  });
  it('answer 404 for anything that is not one of the generated files', async () => {
    for (const name of [
      '',
      'nope.xml',
      'WORD.XML',
      'word.xml/',
      '../word.xml',
      'word.xml\0',
      'packages'
    ])
      expect((await thrown(() => download(name))).status, JSON.stringify(name)).toBe(404);
  });
  it('explain in plain words when the address cannot be used, instead of serving a bad manifest', async () => {
    configure({ ...SELFHOSTED, ORIGIN: 'http://dynodoc.corp.example' });
    const response = await download('word.xml');
    expect(response.status).toBe(503);
    expect(response.headers.get('content-type')).toBe('text/plain; charset=utf-8');
    const text = await response.text();
    expect(text).toMatch(/must use https/);
    expect(text).toMatch(/Set ORIGIN/);
    expect(text).not.toMatch(/<OfficeApp/);
  });
  it('refuse an http://localhost address unless this is a test stack (DEV_AUTH)', async () => {
    configure({ ...SELFHOSTED, ORIGIN: 'http://localhost:4174' });
    expect((await download('word.xml')).status).toBe(503);
    configure({ ...SELFHOSTED, ORIGIN: 'http://localhost:4174', DEV_AUTH: 'true' });
    const response = await download('word.xml');
    expect(response.status).toBe(200);
    expect(await response.text()).toContain('<AppDomain>http://localhost:4174</AppDomain>');
    // Google can call https addresses only, so a test stack has no Google package.
    expect((await thrown(() => download('google-docs.zip'))).status).toBe(404);
  });
  it('refuse the hosted service as the address', async () => {
    configure({ ...SELFHOSTED, ORIGIN: 'https://app.dynodoc.online' });
    const response = await download('word.xml');
    expect(response.status).toBe(503);
    expect(await response.text()).toMatch(/hosted service/);
  });
});

describe('the downloads and the page in hosted mode', () => {
  beforeEach(() => configure(SECRETS));
  it('do not exist: the committed files in static/ are what the hosted service serves', async () => {
    for (const name of ['word.xml', 'word-catalog.xml', 'google-docs.zip', 'SHA256SUMS'])
      expect((await thrown(() => download(name))).status, name).toBe(404);
    expect((await thrown(() => load(pageEvent()))).status).toBe(404);
    expect(session.accessToken).not.toHaveBeenCalled();
  });
});

describe('the administrators page', () => {
  beforeEach(() => configure(SELFHOSTED));

  it('sends a visitor who is not signed in to sign in', async () => {
    const result = await thrown(() => load(pageEvent()));
    expect(result.status).toBe(303);
    expect(result.location).toBe('/login?next=admin');
    expect(fetchMock).not.toHaveBeenCalled();
  });
  it('sends a visitor whose session the API refuses to sign in again', async () => {
    session.accessToken.mockResolvedValue('token');
    fetchMock.mockResolvedValue(new Response(null, { status: 401 }));
    expect((await thrown(() => load(pageEvent()))).location).toBe('/login?next=admin');
  });
  it('refuses a signed-in person who is not an administrator, and shows them nothing', async () => {
    session.accessToken.mockResolvedValue('token');
    fetchMock.mockResolvedValue(new Response(null, { status: 403 }));
    const result = await thrown(() => load(pageEvent()));
    expect(result.status).toBe(403);
    expect(result.body?.message).toMatch(/Administrator access is required/);
  });
  it('does not guess when the administrator check fails', async () => {
    session.accessToken.mockResolvedValue('token');
    fetchMock.mockResolvedValue(new Response(null, { status: 500 }));
    expect((await thrown(() => load(pageEvent()))).status).toBe(503);
  });
  it('asks the API with the person’s own token, and shows an administrator the files', async () => {
    session.accessToken.mockResolvedValue('the-token');
    fetchMock.mockResolvedValue(new Response('{"items":[]}', { status: 200 }));
    const event = pageEvent();
    const data = (await load(event)) as Awaited<ReturnType<typeof load>> & Record<string, unknown>;
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe('http://engine.test/admin/records?kind=erasures&q=&offset=0');
    expect((init as RequestInit).headers).toEqual({ authorization: 'Bearer the-token' });
    expect(event.setHeaders).toHaveBeenCalledWith({ 'cache-control': 'private, no-store' });
    expect(data).toMatchObject({
      brand: 'Acme Docs',
      origin: 'https://dynodoc.corp.example',
      officeJsLocal: false,
      problem: null,
      manifestVersion: '1.0.1.0',
      supportUrl: 'https://help.corp.example/desk',
      google: { available: true }
    });
    const files = data.files as { name: string; id: string; sha256: string }[];
    expect(files.map((f) => f.name)).toContain('word-catalog.xml');
    expect(files.filter((f) => f.id)).toHaveLength(6);
    expect(JSON.stringify(data)).not.toMatch(/dynodoc\.online/i);
  });
  it('reports a problem with the address on the page instead of failing', async () => {
    configure({ ...SELFHOSTED, ORIGIN: 'http://dynodoc.corp.example' });
    session.accessToken.mockResolvedValue('t');
    fetchMock.mockResolvedValue(new Response('{"items":[]}', { status: 200 }));
    const data = await load(pageEvent());
    expect(data).toMatchObject({ files: [], problem: expect.stringMatching(/must use https/) });
  });
});
