import { describe, expect, it } from 'vitest';
import {
  OFFLINE_HEADERS,
  OFFLINE_URL,
  SHELL_FILES,
  assetCacheName,
  isSkipWaiting,
  mayStore,
  offlineFallbackApplies,
  planFor,
  shellCacheName,
  staleCaches
} from './sw-policy';

const origin = 'https://app.dynodoc.online';
const headers = (values: Record<string, string> = {}) => ({
  get: (name: string) => values[name.toLowerCase()] ?? null
});
const request = (
  path: string,
  extra: {
    method?: string;
    mode?: string;
    destination?: string;
    headers?: Record<string, string>;
    origin?: string;
  } = {}
) => ({
  method: extra.method ?? 'GET',
  url: `${extra.origin ?? origin}${path}`,
  mode: extra.mode,
  destination: extra.destination,
  headers: headers(extra.headers)
});
const page = (path: string) => request(path, { mode: 'navigate', destination: 'document' });

describe('planFor', () => {
  it('answers page loads so an offline page can replace a failed one', () => {
    for (const path of ['/', '/workspace', '/projects', '/projects/1', '/login', '/features'])
      expect(planFor(page(path), origin), path).toBe('navigate');
  });

  it('never answers anything that is not a plain GET', () => {
    for (const method of ['POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'])
      expect(planFor(request('/workspace', { method, mode: 'navigate' }), origin), method).toBe(
        'bypass'
      );
  });

  it('leaves the API, the proxy, sign-in, shared links and the Office panes to the browser', () => {
    for (const path of [
      '/api/engine/v1/documents',
      '/api/connectors/pair',
      '/auth/login',
      '/auth/callback',
      '/admin',
      '/admin/users',
      '/connectors/word',
      '/integrations/google/start',
      '/view/abc123',
      '/print',
      '/telemetry',
      '/healthz',
      '/source',
      '/_app/version.json'
    ]) {
      expect(planFor(page(path), origin), `navigate ${path}`).toBe('bypass');
      expect(planFor(request(path), origin), `fetch ${path}`).toBe('bypass');
    }
  });

  it('does not match look-alike prefixes by accident', () => {
    expect(planFor(page('/apiary'), origin)).toBe('navigate');
    expect(planFor(page('/administration-guide'), origin)).toBe('navigate');
  });

  it('leaves event streams, range requests and credentials to the browser', () => {
    expect(
      planFor(request('/some/stream', { headers: { accept: 'text/event-stream' } }), origin)
    ).toBe('bypass');
    expect(
      planFor(
        request('/fonts/public-sans-latin.woff2', { headers: { range: 'bytes=0-9' } }),
        origin
      )
    ).toBe('bypass');
    expect(
      planFor(
        request('/_app/immutable/entry/app.js', { headers: { authorization: 'Bearer x' } }),
        origin
      )
    ).toBe('bypass');
  });

  it('ignores other origins and unparsable addresses', () => {
    expect(
      planFor(request('/_app/immutable/a.js', { origin: 'https://cdn.example' }), origin)
    ).toBe('bypass');
    expect(planFor({ ...request('/'), url: 'not a url' }, origin)).toBe('bypass');
  });

  it('serves the install-time files and content-hashed assets from cache', () => {
    for (const file of SHELL_FILES) expect(planFor(request(file), origin), file).toBe('shell');
    expect(planFor(request('/_app/immutable/chunks/abc.js'), origin)).toBe('static');
    expect(planFor(request('/fonts/public-sans-latin.woff2'), origin)).toBe('static');
  });

  it('does not take over frame loads or anything it does not recognise', () => {
    expect(
      planFor(request('/workspace', { mode: 'navigate', destination: 'iframe' }), origin)
    ).toBe('bypass');
    expect(planFor(request('/workspace'), origin)).toBe('bypass');
    expect(planFor(request('/examples/demo.docx'), origin)).toBe('bypass');
    expect(planFor(request('/site.webmanifest'), origin)).toBe('bypass');
  });
});

describe('mayStore', () => {
  const ok = (extra: Record<string, string> = {}, over: { status?: number; type?: string } = {}) =>
    mayStore({
      status: over.status ?? 200,
      type: over.type ?? 'basic',
      headers: headers({ 'content-type': 'text/javascript', ...extra })
    });

  it('stores a complete same-origin public file', () => {
    expect(ok({ 'cache-control': 'public, max-age=31536000, immutable' })).toBe(true);
    expect(ok()).toBe(true);
  });

  it('refuses errors, partial answers and other origins', () => {
    expect(ok({}, { status: 404 })).toBe(false);
    expect(ok({}, { status: 206 })).toBe(false);
    expect(ok({}, { status: 304 })).toBe(false);
    expect(ok({}, { type: 'opaque' })).toBe(false);
    expect(ok({}, { type: 'cors' })).toBe(false);
  });

  it('refuses anything private, personal or cookie-setting', () => {
    expect(ok({ 'cache-control': 'private, no-store' })).toBe(false);
    expect(ok({ 'cache-control': 'private' })).toBe(false);
    expect(ok({ 'set-cookie': 'dynodoc_session=abc; HttpOnly' })).toBe(false);
    expect(ok({ vary: '*' })).toBe(false);
  });

  it('never stores a page', () => {
    expect(ok({ 'content-type': 'text/html; charset=utf-8' })).toBe(false);
  });
});

describe('cache names', () => {
  it('removes the caches of other versions and leaves other sites alone', () => {
    const names = [
      shellCacheName('1'),
      assetCacheName('1'),
      shellCacheName('2'),
      assetCacheName('2'),
      'dynodoc-legacy',
      'workbox-precache-v2',
      'some-other-cache'
    ];
    expect(staleCaches(names, '2')).toEqual([
      shellCacheName('1'),
      assetCacheName('1'),
      'dynodoc-legacy'
    ]);
    expect(staleCaches([], '2')).toEqual([]);
  });
});

describe('offline fallback', () => {
  it('applies to a network failure only', () => {
    expect(offlineFallbackApplies(new TypeError('Failed to fetch'))).toBe(true);
    expect(offlineFallbackApplies(new DOMException('aborted', 'AbortError'))).toBe(false);
    expect(offlineFallbackApplies(new Error('boom'))).toBe(false);
    expect(offlineFallbackApplies('text')).toBe(false);
  });

  it('answers with its own policy: no script, nothing loaded, nothing stored', () => {
    expect(OFFLINE_URL).toBe('/offline.html');
    expect(SHELL_FILES).toContain(OFFLINE_URL);
    const policy = OFFLINE_HEADERS['content-security-policy'];
    expect(policy).toContain("default-src 'none'");
    expect(policy).not.toMatch(/script-src|connect-src/);
    expect(OFFLINE_HEADERS['cache-control']).toBe('no-store');
    expect(OFFLINE_HEADERS['content-type']).toMatch(/^text\/html/);
  });
});

describe('isSkipWaiting', () => {
  it('accepts only the one message', () => {
    expect(isSkipWaiting({ type: 'SKIP_WAITING' })).toBe(true);
    for (const data of [null, undefined, 'SKIP_WAITING', {}, { type: 'OTHER' }, 5])
      expect(isSkipWaiting(data)).toBe(false);
  });
});
