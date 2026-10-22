// The request hook and a local copy of Office.js: the panes learn about it, and only when the
// directory is configured. Without it every response is produced exactly as before.
import { afterAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { RequestEvent } from '@sveltejs/kit';

const env = vi.hoisted(() => ({}) as Record<string, string | undefined>);
const session = vi.hoisted(() => ({ accessToken: vi.fn(async () => null as string | null) }));
vi.mock('$env/dynamic/private', () => ({ env }));
vi.mock('$env/dynamic/public', () => ({ env: {} }));
vi.mock('$lib/server/session', () => session);

const root = mkdtempSync(join(tmpdir(), 'dynodoc-hooks-officejs-'));
writeFileSync(join(root, 'office.js'), '/* stand-in */');
afterAll(() => rmSync(root, { recursive: true, force: true }));

const SECRETS = {
  SESSION_SECRET: 'f4a6c1e8b3d7092a5c8e1f4b7d0a3c6ef4a6c1e8b3d7092a5c8e1f4b7d0a3c6e',
  WEB_SERVICE_KEY: '2b9d4f7a1c5e8036b9d4f7a1c5e8036a2b9d4f7a1c5e8036b9d4f7a1c5e8036a'
};
const SELFHOSTED = {
  ...SECRETS,
  DYNODOC_MODE: 'selfhosted',
  ORIGIN: 'https://dynodoc.corp.example',
  DYNODOC_MAIL_TRANSPORT: 'smtp',
  SMTP_HOST: 'mail.corp.example',
  SMTP_FROM: 'docs@corp.example'
};
const PAGE_CSP =
  "default-src 'none'; connect-src 'self'; script-src 'self' 'nonce-1'; frame-ancestors 'self'";
const MICROSOFT = /appsforoffice|onecdn|aspnetcdn/;

async function start(settings: Record<string, string>) {
  vi.resetModules();
  for (const key of Object.keys(env)) delete env[key];
  Object.assign(env, settings);
  const hooks = await import('./hooks.server');
  await hooks.init?.();
  return hooks;
}
async function request(hooks: Awaited<ReturnType<typeof start>>, href: string) {
  const url = new URL(href);
  const resolve = vi.fn(async (_event: unknown, options?: unknown) => {
    void options;
    return new Response(
      '<!doctype html><html><head><title>pane</title></head><body></body></html>',
      {
        headers: { 'content-security-policy': PAGE_CSP, 'content-type': 'text/html' }
      }
    );
  });
  const event = {
    url,
    route: { id: url.pathname },
    cookies: {},
    locals: {},
    request: new Request(url)
  } as unknown as RequestEvent;
  const response = await hooks.handle({ event, resolve: resolve as never });
  return { response, resolve };
}
/** Run the transform the hook handed to `resolve` over a page's first chunk. */
async function transformed(call: unknown[] | undefined) {
  const options = call?.[1] as
    | { transformPageChunk?: (input: { html: string; done: boolean }) => string }
    | undefined;
  return options?.transformPageChunk?.({
    html: '<!doctype html><html><head><title>pane</title></head>',
    done: false
  });
}

beforeEach(() => session.accessToken.mockReset().mockResolvedValue(null));

describe('hosted mode', () => {
  it('serves the panes exactly as before: resolve gets the event alone, the policy lists Microsoft', async () => {
    const hooks = await start(SECRETS);
    for (const pane of ['word', 'excel', 'powerpoint']) {
      const { response, resolve } = await request(
        hooks,
        `https://app.dynodoc.online/connectors/${pane}`
      );
      expect(resolve.mock.calls[0]).toHaveLength(1);
      const policy = response.headers.get('content-security-policy')!;
      expect(policy).toContain('https://appsforoffice.microsoft.com');
      expect(policy).toContain('https://ajax.aspnetcdn.com');
    }
  });
});

describe('self-hosted mode without a directory', () => {
  it('is the same as hosted for the panes: the CDN in the policy, no tag added', async () => {
    const hooks = await start(SELFHOSTED);
    const { response, resolve } = await request(
      hooks,
      'https://dynodoc.corp.example/connectors/word'
    );
    expect(resolve.mock.calls[0]).toHaveLength(1);
    expect(response.headers.get('content-security-policy')).toContain(
      'appsforoffice.microsoft.com'
    );
  });
});

describe('self-hosted mode with a directory', () => {
  const settings = { ...SELFHOSTED, DYNODOC_OFFICE_JS_DIR: root };

  it('tells each pane page to use the local copy, and drops every Microsoft host from its policy', async () => {
    const hooks = await start(settings);
    for (const pane of ['word', 'excel', 'powerpoint']) {
      const { response, resolve } = await request(
        hooks,
        `https://dynodoc.corp.example/connectors/${pane}`
      );
      const html = await transformed(resolve.mock.calls[0]);
      expect(html).toBe(
        '<!doctype html><html><head><meta name="dynodoc-office-js" content="/office-js/office.js" /><title>pane</title></head>'
      );
      const policy = response.headers.get('content-security-policy')!;
      expect(policy).not.toMatch(MICROSOFT);
      expect(policy).toContain("script-src 'self' 'nonce-1'");
      expect(response.headers.get('x-frame-options')).toBeNull();
      expect(policy).toContain('https://*.officeapps.live.com');
    }
  });
  it('leaves every other page alone: no tag, and the usual policy', async () => {
    const hooks = await start(settings);
    for (const path of ['/login', '/connectors', '/connectors/addins', '/healthz']) {
      const { response, resolve } = await request(hooks, `https://dynodoc.corp.example${path}`);
      expect(resolve.mock.calls[0], path).toHaveLength(1);
      expect(response.headers.get('content-security-policy'), path).not.toMatch(MICROSOFT);
    }
  });
});
