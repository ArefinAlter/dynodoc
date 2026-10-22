// Page metrics: nothing is requested unless the configuration names a destination and this
// host. Hosted keeps its destination and host list; self-hosted, by default, has none.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const vitals = vi.hoisted(() => ({ loaded: vi.fn() }));
vi.mock('web-vitals', () => {
  vitals.loaded();
  return { onLCP: vi.fn(), onCLS: vi.fn(), onINP: vi.fn(), onFCP: vi.fn(), onTTFB: vi.fn() };
});
import { observeVitals } from './telemetry';

const hosted = {
  endpoint: 'https://app.dynodoc.online/telemetry',
  hosts: ['dynodoc.online', 'www.dynodoc.online', 'app.dynodoc.online', 'admin.dynodoc.online']
};
let fetchMock: ReturnType<typeof vi.fn>;
function visit(hostname: string, path = '/', extra: Record<string, unknown> = {}) {
  vi.stubGlobal('location', { hostname, pathname: path });
  vi.stubGlobal('navigator', { doNotTrack: null, globalPrivacyControl: false, ...extra });
}
beforeEach(() => {
  fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
  vi.stubGlobal('fetch', fetchMock);
  vi.stubGlobal('crypto', { randomUUID: () => 'a21f5eb4-28cf-4f78-a2c0-528cd74c8e17' });
  document.cookie = 'dynodoc_metrics=; max-age=0; path=/';
  vitals.loaded.mockClear();
});
afterEach(() => vi.unstubAllGlobals());

describe('observeVitals', () => {
  it('does nothing, and loads nothing, when the configuration has no destination', async () => {
    visit('dynodoc.online');
    for (const target of [null, undefined]) await observeVitals(target);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(vitals.loaded).not.toHaveBeenCalled();
  });
  it('reports from a hosted host to the hosted destination, as before', async () => {
    visit('app.dynodoc.online', '/login');
    await observeVitals(hosted);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('https://app.dynodoc.online/telemetry');
    expect(init).toMatchObject({ method: 'POST', credentials: 'omit', keepalive: true });
    expect(init.headers).toEqual({ 'content-type': 'application/json' });
    expect(JSON.parse(init.body as string)).toEqual({
      page_id: 'a21f5eb4-28cf-4f78-a2c0-528cd74c8e17',
      page: 'login',
      metric: 'visit',
      value: 1
    });
  });
  it('stays quiet on a host that is not listed', async () => {
    for (const host of [
      'localhost',
      '127.0.0.1',
      'evil.example.test',
      'dynodoc.online.evil.test'
    ]) {
      visit(host);
      await observeVitals(hosted);
    }
    expect(fetchMock).not.toHaveBeenCalled();
  });
  it('respects the browser opt-outs and the cookie', async () => {
    visit('dynodoc.online', '/', { doNotTrack: '1' });
    await observeVitals(hosted);
    visit('dynodoc.online', '/', { globalPrivacyControl: true });
    await observeVitals(hosted);
    visit('dynodoc.online');
    document.cookie = 'dynodoc_metrics=off; path=/';
    await observeVitals(hosted);
    expect(fetchMock).not.toHaveBeenCalled();
  });
  it('reports a self-hosted installation to itself when the operator turned it on', async () => {
    visit('dynodoc.example.test', '/workspace');
    await observeVitals({ endpoint: '/telemetry', hosts: ['dynodoc.example.test'] });
    expect(fetchMock.mock.calls[0][0]).toBe('/telemetry');
    expect(JSON.parse((fetchMock.mock.calls[0][1] as RequestInit).body as string).page).toBe(
      'workspace'
    );
  });
});
