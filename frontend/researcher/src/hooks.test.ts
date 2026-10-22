// The request hook in both modes: hosted behaviour pinned to what the service did before
// DYNODOC_MODE existed, and the self-hosted differences (no marketing pages, never indexed).
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestEvent } from '@sveltejs/kit';

const env = vi.hoisted(() => ({}) as Record<string, string | undefined>);
const publicEnv = vi.hoisted(() => ({}) as Record<string, string | undefined>);
const session = vi.hoisted(() => ({ accessToken: vi.fn(async () => null as string | null) }));
vi.mock('$env/dynamic/private', () => ({ env }));
vi.mock('$env/dynamic/public', () => ({ env: publicEnv }));
vi.mock('$lib/server/session', () => session);

const secrets = {
  SESSION_SECRET: 'f4a6c1e8b3d7092a5c8e1f4b7d0a3c6ef4a6c1e8b3d7092a5c8e1f4b7d0a3c6e',
  WEB_SERVICE_KEY: '2b9d4f7a1c5e8036b9d4f7a1c5e8036a2b9d4f7a1c5e8036b9d4f7a1c5e8036a'
};
const PAGE_CSP =
  "default-src 'none'; connect-src 'self'; script-src 'self' 'nonce-1'; frame-ancestors 'self'";

async function start(
  settings: Record<string, string>,
  pub: Record<string, string> = {},
  withSecrets = true
) {
  vi.resetModules();
  for (const bag of [env, publicEnv]) for (const key of Object.keys(bag)) delete bag[key];
  Object.assign(env, withSecrets ? secrets : {}, settings);
  Object.assign(publicEnv, pub);
  const hooks = await import('./hooks.server');
  await hooks.init?.();
  return hooks;
}
async function request(
  hooks: Awaited<ReturnType<typeof start>>,
  href: string,
  options: { route?: string | null; page?: boolean } = {}
) {
  const url = new URL(href);
  const resolve = vi.fn(async () =>
    options.page === false
      ? new Response('body')
      : new Response('page', { headers: { 'content-security-policy': PAGE_CSP } })
  );
  const event = {
    url,
    route: { id: options.route ?? null },
    cookies: {},
    locals: {},
    request: new Request(url)
  } as unknown as RequestEvent;
  const response = await hooks.handle({ event, resolve });
  return { response, resolve, locals: event.locals };
}

beforeEach(() => session.accessToken.mockReset().mockResolvedValue(null));

describe('hosted mode', () => {
  it('serves the public homepage host, which runs with no secret at all', async () => {
    const hooks = await start({}, { PUBLIC_WORKSPACE_URL: 'https://app.dynodoc.online' }, false);
    const home = await request(hooks, 'https://dynodoc.online/', { route: '/(site)' });
    expect(home.response.status).toBe(200);
    expect(home.response.headers.has('x-robots-tag')).toBe(false);
    const workspace = await request(hooks, 'https://dynodoc.online/projects', {
      route: '/projects'
    });
    expect(workspace.response.headers.get('location')).toBe('https://app.dynodoc.online/projects');
  });
  it('sends the root of the workspace host to the public host', async () => {
    const hooks = await start({});
    const { response, resolve } = await request(hooks, 'https://app.dynodoc.online/', {
      route: '/(site)'
    });
    expect(response.status).toBe(303);
    expect(response.headers.get('location')).toBe('https://dynodoc.online');
    expect(resolve).not.toHaveBeenCalled();
  });
  it('serves the marketing pages and lets only the public host index them', async () => {
    const hooks = await start({});
    const open = await request(hooks, 'https://dynodoc.online/privacy', {
      route: '/(site)/privacy'
    });
    expect(open.response.status).toBe(200);
    expect(open.response.headers.has('x-robots-tag')).toBe(false);
    for (const host of ['app.dynodoc.online', 'admin.dynodoc.online', 'preview.example.test']) {
      const held = await request(hooks, `https://${host}/privacy`, { route: '/(site)/privacy' });
      expect(held.response.status, host).toBe(200);
      expect(held.response.headers.get('x-robots-tag'), host).toBe('noindex, nofollow');
    }
    const login = await request(hooks, 'https://dynodoc.online/login', { route: '/login' });
    expect(login.response.headers.get('x-robots-tag')).toBe('noindex, nofollow');
    expect(open.locals).toEqual({ indexable: true });
  });
  it('keeps the page policy exactly: the hosted connect-src origin is added by the configuration', async () => {
    const hooks = await start({});
    const { response } = await request(hooks, 'https://dynodoc.online/', { route: '/(site)' });
    expect(response.headers.get('content-security-policy')).toBe(
      "default-src 'none'; connect-src 'self' https://app.dynodoc.online; script-src 'self' 'nonce-1'; frame-ancestors 'self'"
    );
    expect(response.headers.get('x-frame-options')).toBe('SAMEORIGIN');
    expect(response.headers.get('x-content-type-options')).toBe('nosniff');
    expect(response.headers.get('referrer-policy')).toBe('strict-origin-when-cross-origin');
    expect(response.headers.get('strict-transport-security')).toBe('max-age=31536000');
  });
  it('adds nothing to a response that has no page policy', async () => {
    const hooks = await start({});
    const { response } = await request(hooks, 'https://dynodoc.online/healthz', {
      route: '/healthz',
      page: false
    });
    expect(response.headers.get('content-security-policy')).toBe(
      "default-src 'none'; base-uri 'none'; object-src 'none'; form-action 'self'; frame-ancestors 'self'"
    );
  });
  it('sends workspace visits from the public host to the workspace host, when configured', async () => {
    const hooks = await start({}, { PUBLIC_WORKSPACE_URL: 'https://app.dynodoc.online/' });
    const away = await request(hooks, 'https://dynodoc.online/projects?x=1', {
      route: '/projects'
    });
    expect(away.response.status).toBe(303);
    expect(away.response.headers.get('location')).toBe('https://app.dynodoc.online/projects?x=1');
    const here = await request(hooks, 'https://app.dynodoc.online/projects', {
      route: '/projects'
    });
    expect(here.response.headers.get('location')).toBe('/login?next=%2Fprojects');
  });
  it('signs in before the workspace and keeps the admin site on /admin', async () => {
    const hooks = await start({});
    const { response } = await request(hooks, 'https://app.dynodoc.online/workspace', {
      route: '/workspace'
    });
    expect(response.headers.get('location')).toBe('/login?next=%2Fworkspace');
    session.accessToken.mockResolvedValue('token');
    const signedIn = await request(hooks, 'https://app.dynodoc.online/workspace', {
      route: '/workspace'
    });
    expect(signedIn.response.status).toBe(200);
    const admin = await start({ ADMIN_SITE: 'true' });
    const root = await request(admin, 'https://admin.dynodoc.online/', { route: '/(site)' });
    expect(root.response.headers.get('location')).toBe('/admin');
  });
  it('lets the Office panes connect to the configured connector origins as before', async () => {
    const hooks = await start({ CONNECTOR_ORIGINS: 'https://other.example.test, javascript:x' });
    const pane = await request(hooks, 'https://app.dynodoc.online/connectors/word', {
      route: '/connectors/word'
    });
    const csp = pane.response.headers.get('content-security-policy')!;
    expect(csp).toContain(
      "connect-src 'self' https://app.dynodoc.online https://appsforoffice.microsoft.com"
    );
    expect(csp).toContain('https://other.example.test');
    expect(csp).not.toContain('javascript');
  });
});

describe('self-hosted mode', () => {
  const settings = { DYNODOC_MODE: 'selfhosted', ORIGIN: 'https://dynodoc.example.test' };
  it('answers 404 for every marketing and search-engine route, without a redirect', async () => {
    const hooks = await start(settings);
    for (const [path, route] of [
      ['/privacy', '/(site)/privacy'],
      ['/terms', '/(site)/terms'],
      ['/features', '/(site)/features'],
      ['/support', '/(site)/support'],
      ['/features/__data.json', '/(site)/features'],
      ['/sitemap.xml', '/sitemap.xml'],
      ['/llms.txt', '/llms.txt'],
      ['/llms-full.txt', '/llms-full.txt'],
      ['/source', '/source']
    ]) {
      const { response, resolve } = await request(hooks, `https://dynodoc.example.test${path}`, {
        route
      });
      expect(response.status, path).toBe(404);
      expect(response.headers.has('location'), path).toBe(false);
      expect(resolve, path).not.toHaveBeenCalled();
    }
  });
  it('sends / to sign-in, or to the projects of a signed-in person', async () => {
    const hooks = await start(settings);
    const anonymous = await request(hooks, 'https://dynodoc.example.test/', { route: '/(site)' });
    expect(anonymous.response.status).toBe(303);
    expect(anonymous.response.headers.get('location')).toBe('/login');
    session.accessToken.mockResolvedValue('token');
    const signedIn = await request(hooks, 'https://dynodoc.example.test/', { route: '/(site)' });
    expect(signedIn.response.headers.get('location')).toBe('/projects');
    // The destination itself must not send a visitor back to /: no loop.
    const login = await request(hooks, 'https://dynodoc.example.test/login', { route: '/login' });
    expect(login.response.status).toBe(200);
  });
  it('is never indexable, whatever the host or path', async () => {
    const hooks = await start(settings);
    for (const [href, route] of [
      ['https://dynodoc.example.test/login', '/login'],
      ['https://dynodoc.example.test/robots.txt', '/robots.txt'],
      ['https://dynodoc.online/robots.txt', '/robots.txt'],
      ['https://dynodoc.online/privacy', '/(site)/privacy']
    ]) {
      const { response, locals } = await request(hooks, href, { route });
      expect(response.headers.get('x-robots-tag'), href).toBe('noindex, nofollow');
      expect(locals, href).toEqual({ indexable: false });
    }
  });
  it('serves a disallow-all robots.txt', async () => {
    const { GET } = await import('./routes/robots.txt/+server');
    const get = GET as unknown as (event: unknown) => Response;
    const text = await get({
      url: new URL('https://dynodoc.online/robots.txt'),
      locals: { indexable: false }
    }).text();
    expect(text).toBe('User-agent: *\nDisallow: /\n');
  });
  it('derives connect-src from the configured origins, with no hosted hostname', async () => {
    const hooks = await start({
      ...settings,
      DYNODOC_ALT_ORIGINS: 'https://docs.example.test'
    });
    const { response } = await request(hooks, 'https://dynodoc.example.test/login', {
      route: '/login'
    });
    const csp = response.headers.get('content-security-policy')!;
    expect(csp).toBe(
      "default-src 'none'; connect-src 'self' https://dynodoc.example.test https://docs.example.test; script-src 'self' 'nonce-1'; frame-ancestors 'self'"
    );
    expect(csp).not.toContain('dynodoc.online');
  });
  it('does not redirect to another host unless PUBLIC_WORKSPACE_URL is set', async () => {
    const hooks = await start(settings);
    const { response } = await request(hooks, 'https://dynodoc.example.test/projects', {
      route: '/projects'
    });
    expect(response.headers.get('location')).toBe('/login?next=%2Fprojects');
    const configured = await start(settings, { PUBLIC_WORKSPACE_URL: 'https://app.example.test' });
    const away = await request(configured, 'https://dynodoc.example.test/projects', {
      route: '/projects'
    });
    expect(away.response.headers.get('location')).toBe('https://app.example.test/projects');
  });
  it('does not redirect the root of an app.dynodoc.online-named host', async () => {
    const hooks = await start({ ...settings, ORIGIN: 'https://app.dynodoc.online' });
    session.accessToken.mockResolvedValue(null);
    const { response } = await request(hooks, 'https://app.dynodoc.online/', { route: '/(site)' });
    expect(response.headers.get('location')).toBe('/login');
  });
});
