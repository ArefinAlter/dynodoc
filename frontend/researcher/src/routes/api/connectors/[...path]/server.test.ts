import { beforeEach, it, expect, vi } from 'vitest';
vi.mock('$lib/server/session', () => ({
  engineUrl: () => 'http://engine.test',
  accessToken: () => {
    throw new Error('Must never exchange browser credentials');
  }
}));
import { GET, POST } from './+server';
const DOC = '11111111-2222-3333-4444-555555555555';
let upstream: ReturnType<typeof vi.fn>;
beforeEach(() => {
  upstream = vi.fn(async () => new Response('{"through_seq":4}', { status: 200 }));
  vi.stubGlobal('fetch', upstream);
});
async function call(path: string, method = 'GET', auth?: string, search = '') {
  const request = new Request(`https://app.test/api/connectors/${path}${search}`, {
    method,
    headers: auth
      ? { authorization: auth, cookie: 'dynodoc_session=ignored' }
      : { cookie: 'dynodoc_session=ignored' },
    body: method === 'POST' ? '{}' : undefined
  });
  return (method === 'GET' ? GET : POST)({
    request,
    params: { path },
    url: new URL(request.url)
  } as never) as Promise<Response>;
}
it('requires the file-scoped bearer even if a session cookie exists', async () => {
  expect((await call(`documents/${DOC}/checkpoint`)).status).toBe(401);
  expect(upstream).not.toHaveBeenCalled();
});
it('forwards only a single valid historical checkpoint selector', async () => {
  const token = `Bearer ${'a'.repeat(64)}`;
  expect((await call(`documents/${DOC}/checkpoint`, 'GET', token, '?through_seq=2')).status).toBe(
    200
  );
  expect(upstream.mock.calls[0][0]).toBe(
    `http://engine.test/connector/documents/${DOC}/checkpoint?through_seq=2`
  );
  upstream.mockClear();
  for (const search of [
    '?through_seq=-1',
    '?through_seq=1.5',
    '?through_seq=2&through_seq=3',
    '?through_seq=9007199254740992',
    '?path=auth/me',
    '?through_seq='
  ]) {
    expect((await call(`documents/${DOC}/checkpoint`, 'GET', token, search)).status).toBe(400);
  }
  expect((await call(`documents/${DOC}/bundles`, 'POST', token, '?through_seq=2')).status).toBe(
    400
  );
  expect(upstream).not.toHaveBeenCalled();
});
it('cannot proxy merges, credential management, auth, traversal or a method mismatch', async () => {
  for (const path of [
    `documents/${DOC}/merge`,
    `documents/${DOC}/connectors`,
    'auth/me',
    '../healthz',
    `documents/${DOC}/bundles`
  ])
    expect((await call(path, 'GET', `Bearer ${'a'.repeat(64)}`)).status).toBe(404);
  expect(upstream).not.toHaveBeenCalled();
});
it('forwards only the scoped token and no cookies/service credentials', async () => {
  const token = `Bearer ${'a'.repeat(64)}`,
    r = await call(`documents/${DOC}/checkpoint`, 'GET', token);
  expect(r.status).toBe(200);
  expect(r.headers.get('cache-control')).toBe('private, no-store');
  expect(upstream.mock.calls[0][0]).toBe(
    `http://engine.test/connector/documents/${DOC}/checkpoint`
  );
  expect(upstream.mock.calls[0][1].headers).toEqual({
    authorization: token,
    'content-type': 'application/json'
  });
});
