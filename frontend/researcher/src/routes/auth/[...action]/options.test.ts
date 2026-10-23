// Which sign-in methods the server offers, and that the email route refuses when email
// sign-in is switched off (AUTH_EMAIL_LINK=off), whatever else is configured.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const env = vi.hoisted(() => ({}) as Record<string, string | undefined>);
vi.mock('$env/dynamic/private', () => ({ env }));
const engine = vi.hoisted(() => ({
  enginePost: vi.fn(async () => ({ magic_link_token: 'issued-token' })),
  accessToken: vi.fn(),
  saveSession: vi.fn(),
  readSession: vi.fn(),
  clearSession: vi.fn(),
  engineUrl: () => 'http://engine.invalid'
}));
vi.mock('$lib/server/session', () => engine);

const SESSION = '7c2e9a5d1f8b4036a7c2e9d5a1f8b4c36a7c2e9a5d1f8b4036a7c2e9d5a1f8b4';
const SERVICE = 'e0b6d3a9f5c28147e0b6d3a9f5c28147e0b6d3a9f5c28147e0b6d3a9f5c28147';
const tmp = mkdtempSync(join(tmpdir(), 'dynodoc-auth-'));

async function route(values: Record<string, string>) {
  vi.resetModules();
  for (const key of Object.keys(env)) delete env[key];
  Object.assign(env, { SESSION_SECRET: SESSION, WEB_SERVICE_KEY: SERVICE }, values);
  return (await import('./+server')) as unknown as {
    GET: (event: unknown) => Promise<Response>;
    POST: (event: unknown) => Promise<Response>;
  };
}
const options = async (values: Record<string, string>) => {
  const { GET } = await route(values);
  return (
    await GET({
      params: { action: 'options' },
      url: new URL('https://dynodoc.example.test/auth/options'),
      cookies: {}
    })
  ).json();
};
async function postEmail(values: Record<string, string>) {
  const { POST } = await route(values);
  const url = new URL('https://dynodoc.example.test/auth/email');
  return POST({
    params: { action: 'email' },
    url,
    cookies: {},
    request: new Request(url, {
      method: 'POST',
      headers: { origin: url.origin, 'content-type': 'application/json' },
      body: JSON.stringify({ email: 'olivia@example.test', display_name: 'Olivia' })
    })
  });
}
beforeEach(() => {
  engine.enginePost.mockClear();
  engine.saveSession.mockClear();
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => new Response('{}', { status: 200 }))
  );
});

const self = { DYNODOC_MODE: 'selfhosted', ORIGIN: 'https://dynodoc.example.test' };
const mail = { RESEND_API_KEY: 're_0123456789abcdef', EMAIL_FROM: 'Docs <docs@example.test>' };

describe('GET /auth/options', () => {
  it('offers email when a mail key and sender are set, and not otherwise', async () => {
    expect((await options({ ...self, ...mail })).email).toBe(true);
    expect((await options(self)).email).toBe(false);
    expect((await options({ ...self, RESEND_API_KEY: mail.RESEND_API_KEY })).email).toBe(false);
  });
  it('offers email sign-in nowhere when it is switched off', async () => {
    const off = await options({ ...self, ...mail, DEV_AUTH: 'true', AUTH_EMAIL_LINK: 'off' });
    expect(off).toMatchObject({ email: false, development: false });
  });
  it('offers the development sign-in only with DEV_AUTH=true, as before', async () => {
    expect((await options({ ...self, DEV_AUTH: 'true' })).development).toBe(true);
    expect((await options({ ...self, DEV_AUTH: 'yes' })).development).toBe(false);
  });
  it('offers Google when its id and secret are set, the secret perhaps as a file', async () => {
    expect((await options(self)).google).toBe(false);
    expect((await options({ ...self, GOOGLE_CLIENT_ID: 'id' })).google).toBe(false);
    const path = join(tmp, 'google-secret');
    writeFileSync(path, 'google-secret-value-0123456789\n', { mode: 0o600 });
    expect(
      (await options({ ...self, GOOGLE_CLIENT_ID: 'id', GOOGLE_CLIENT_SECRET_FILE: path })).google
    ).toBe(true);
  });
  it('keeps the admin destination of the admin site', async () => {
    expect((await options({ ...self, ADMIN_SITE: 'true' })).destination).toBe('/admin');
    expect((await options(self)).destination).toBe('/projects');
  });
});

describe('POST /auth/email', () => {
  it('refuses when email sign-in is switched off, before any mail or engine call', async () => {
    const response = await postEmail({ ...self, ...mail, AUTH_EMAIL_LINK: 'off' });
    expect(response.status).toBe(403);
    expect((await response.json()).error).toMatch(/turned off/);
    expect(engine.enginePost).not.toHaveBeenCalled();
    expect(fetch).not.toHaveBeenCalled();
  });
  it('refuses the development sign-in too when email sign-in is off', async () => {
    const response = await postEmail({ ...self, DEV_AUTH: 'true', AUTH_EMAIL_LINK: 'off' });
    expect(response.status).toBe(403);
    expect(engine.enginePost).not.toHaveBeenCalled();
  });
  it('is not configured without a mail key and without DEV_AUTH', async () => {
    expect((await postEmail(self)).status).toBe(503);
    expect(engine.enginePost).not.toHaveBeenCalled();
  });
  it('still returns the development token on a test stack', async () => {
    const response = await postEmail({ ...self, DEV_AUTH: 'true' });
    expect(await response.json()).toEqual({ development_token: 'issued-token' });
  });
  it('sends the mail with the key read from RESEND_API_KEY_FILE', async () => {
    const path = join(tmp, 'resend-key');
    writeFileSync(path, 're_from_a_file_0123456789\n', { mode: 0o600 });
    const response = await postEmail({
      ...self,
      RESEND_API_KEY_FILE: path,
      EMAIL_FROM: mail.EMAIL_FROM
    });
    expect(response.status).toBe(200);
    const [url, init] = vi.mocked(fetch).mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('https://api.resend.com/emails');
    expect((init.headers as Record<string, string>).Authorization).toBe(
      'Bearer re_from_a_file_0123456789'
    );
  });
});

// Confirming a sign-in link (an emailed one, or one the operator made with `engine-cli auth
// issue-link`, docs/decisions/018) is the same route and the same engine call; it works with email
// sign-in switched off, and its refusal does not offer a request that would be refused.
async function postVerify(values: Record<string, string>, token = 'a'.repeat(64)) {
  const { POST } = await route(values);
  const url = new URL('https://dynodoc.example.test/auth/verify');
  return POST({
    params: { action: 'verify' },
    url,
    cookies: {},
    request: new Request(url, {
      method: 'POST',
      headers: { origin: url.origin, 'content-type': 'application/json' },
      body: JSON.stringify({ token })
    })
  });
}
describe('POST /auth/verify', () => {
  it('confirms a link with email sign-in switched off, passing the token to the engine as is', async () => {
    engine.enginePost.mockResolvedValueOnce({ access_token: 'a', refresh_token: 'r' } as never);
    const response = await postVerify({ ...self, AUTH_EMAIL_LINK: 'off' }, 'b'.repeat(64));
    expect(response.status).toBe(200);
    expect(engine.enginePost).toHaveBeenCalledWith('/auth/verify', { token: 'b'.repeat(64) });
    expect(engine.saveSession).toHaveBeenCalledTimes(1);
  });
  it('tells a person where email sign-in is off to ask the administrator, not to request a link', async () => {
    engine.enginePost.mockRejectedValueOnce(new Error('401'));
    const off = await postVerify({ ...self, AUTH_EMAIL_LINK: 'off' });
    expect(off.status).toBe(401);
    const text = (await off.json()).error as string;
    expect(text).toMatch(/Ask your administrator/);
    expect(text).not.toMatch(/Request another link/);
    expect(engine.saveSession).not.toHaveBeenCalled();
  });
  it('keeps the old wording where email sign-in is on', async () => {
    engine.enginePost.mockRejectedValueOnce(new Error('401'));
    const on = await postVerify({ ...self, ...mail });
    expect(on.status).toBe(401);
    expect((await on.json()).error).toBe(
      'This link has expired or has already been used. Request another link.'
    );
  });
});

import { afterAll } from 'vitest';
afterAll(() => rmSync(tmp, { recursive: true, force: true }));
