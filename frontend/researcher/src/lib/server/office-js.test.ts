// @vitest-environment node
// Serving the operator's local copy of Office.js: only that directory, only by file name, and
// only when the directory is configured.
import { afterAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const env = vi.hoisted(() => ({}) as Record<string, string | undefined>);
vi.mock('$env/dynamic/private', () => ({ env }));
vi.mock('$env/dynamic/public', () => ({ env: {} }));
vi.mock('$app/environment', () => ({ dev: false }));

import { resetConfig } from '$lib/server/config';
import { GET } from '../../routes/office-js/[...path]/+server';
import { MAX_FILE_BYTES, entityTag, resolveOfficeJsFile } from './office-js';

const base = mkdtempSync(join(tmpdir(), 'dynodoc-officejs-'));
const dir = join(base, 'office-js');
const outside = join(base, 'outside');
mkdirSync(join(dir, 'en-us'), { recursive: true });
mkdirSync(join(dir, '.hidden'));
mkdirSync(outside);
writeFileSync(join(dir, 'office.js'), 'window.Office = { stand: "in" };');
writeFileSync(join(dir, 'en-us', 'office_strings.js'), 'var strings = {};');
writeFileSync(join(dir, 'page.html'), '<script>alert(1)</script>');
writeFileSync(join(dir, 'image.svg'), '<svg onload="alert(1)"/>');
writeFileSync(join(dir, '.secret.js'), 'secret');
writeFileSync(join(dir, '.hidden', 'a.js'), 'secret');
writeFileSync(join(outside, 'secret.js'), 'outside secret');
writeFileSync(join(base, 'sibling.js'), 'sibling');
// Creating a file link needs a privilege on some systems; a directory junction does not.
const tryLink = (target: string, path: string, type?: 'junction') => {
  try {
    symlinkSync(target, path, type);
    return true;
  } catch {
    return false;
  }
};
const directoryLinked = tryLink(outside, join(dir, 'link'), 'junction');
const fileLinked = tryLink(join(outside, 'secret.js'), join(dir, 'secret-link.js'));
afterAll(() => rmSync(base, { recursive: true, force: true }));

describe('finding a file', () => {
  it('serves a file in the directory and in a sub-directory, with a type by extension', async () => {
    const top = await resolveOfficeJsFile(dir, 'office.js');
    expect(top).toMatchObject({ type: 'text/javascript; charset=utf-8', size: 32 });
    expect(await resolveOfficeJsFile(dir, 'en-us/office_strings.js')).not.toBeNull();
  });
  it('refuses everything that is not a plain, visible file below the directory', async () => {
    const refused = [
      '',
      '/office.js',
      '../sibling.js',
      '../outside/secret.js',
      'en-us/../../sibling.js',
      'en-us/../office.js',
      './office.js',
      'en-us//office_strings.js',
      'en-us/',
      'en-us',
      '.secret.js',
      '.hidden/a.js',
      'office.js\0',
      'office.js\\..\\..\\sibling.js',
      '..\\sibling.js',
      'page.html',
      'image.svg',
      'missing.js',
      'office.exe',
      'a'.repeat(300) + '.js'
    ];
    for (const path of refused)
      expect(await resolveOfficeJsFile(dir, path), JSON.stringify(path)).toBeNull();
  });
  it.skipIf(!fileLinked)('refuses a file link that leaves the directory', async () => {
    expect(await resolveOfficeJsFile(dir, 'secret-link.js')).toBeNull();
  });
  it.skipIf(!directoryLinked)('refuses a directory link that leaves the directory', async () => {
    expect(await resolveOfficeJsFile(dir, 'link/secret.js')).toBeNull();
  });
  it('refuses a file over the size limit and a directory that does not exist', async () => {
    const big = join(dir, 'big.js');
    writeFileSync(big, Buffer.alloc(MAX_FILE_BYTES + 1));
    expect(await resolveOfficeJsFile(dir, 'big.js')).toBeNull();
    expect(await resolveOfficeJsFile(join(base, 'nowhere'), 'office.js')).toBeNull();
  });
});

const settings = (extra: Record<string, string> = {}) => ({
  SESSION_SECRET: 'f4a6c1e8b3d7092a5c8e1f4b7d0a3c6ef4a6c1e8b3d7092a5c8e1f4b7d0a3c6e',
  WEB_SERVICE_KEY: '2b9d4f7a1c5e8036b9d4f7a1c5e8036a2b9d4f7a1c5e8036b9d4f7a1c5e8036a',
  ...extra
});
function configure(values: Record<string, string>) {
  for (const key of Object.keys(env)) delete env[key];
  Object.assign(env, values);
  resetConfig();
}
const selfhosted = () =>
  settings({
    DYNODOC_MODE: 'selfhosted',
    ORIGIN: 'https://dynodoc.corp.example',
    DYNODOC_MAIL_TRANSPORT: 'smtp',
    SMTP_HOST: 'mail.corp.example',
    SMTP_FROM: 'docs@corp.example',
    DYNODOC_OFFICE_JS_DIR: dir
  });
const request = (path: string, headers: Record<string, string> = {}) =>
  GET({
    params: { path },
    request: new Request(`https://dynodoc.corp.example/office-js/${path}`, { headers })
  } as Parameters<typeof GET>[0]);
async function status(action: () => unknown) {
  try {
    return (await action()) instanceof Response ? 'response' : 'none';
  } catch (caught) {
    return (caught as { status: number }).status;
  }
}

describe('the route', () => {
  beforeEach(() => configure(selfhosted()));

  it('hands out the file with its type, a validator and a revalidate-every-time cache rule', async () => {
    const response = await request('office.js');
    expect(response.status).toBe(200);
    expect(response.headers.get('content-type')).toBe('text/javascript; charset=utf-8');
    expect(response.headers.get('cache-control')).toBe('no-cache');
    expect(response.headers.get('x-content-type-options')).toBe('nosniff');
    expect(await response.text()).toBe('window.Office = { stand: "in" };');
    expect(response.headers.get('etag')).toMatch(/^W\/"[0-9a-f]+-[0-9a-f]+"$/);
  });
  it('answers 304 to a matching validator, and the file again when it has changed', async () => {
    const served = (await resolveOfficeJsFile(dir, 'office.js'))!;
    const etag = entityTag(served);
    const same = await request('office.js', { 'if-none-match': etag });
    expect(same.status).toBe(304);
    expect(await same.text()).toBe('');
    expect((await request('office.js', { 'if-none-match': 'W/"0-0"' })).status).toBe(200);
  });
  it('is public: it reads no session', async () => {
    expect((await request('en-us/office_strings.js')).status).toBe(200);
  });
  it('answers 404 for everything the finder refuses', async () => {
    for (const path of ['../sibling.js', 'page.html', '.secret.js', 'missing.js', ''])
      expect(await status(() => request(path)), JSON.stringify(path)).toBe(404);
  });
  it('does not exist unless the directory is configured', async () => {
    configure({
      ...selfhosted(),
      DYNODOC_OFFICE_JS_DIR: undefined as unknown as string
    });
    expect(await status(() => request('office.js'))).toBe(404);
    configure(settings());
    expect(await status(() => request('office.js'))).toBe(404);
  });
});
