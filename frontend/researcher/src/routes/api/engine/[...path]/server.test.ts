import { beforeEach, expect, it, vi } from 'vitest';
vi.mock('$lib/server/session', () => ({
  engineUrl: () => 'http://engine.test',
  accessToken: async () => 'session-token'
}));
import { PUT } from './+server';

const PROJECT = '11111111-2222-3333-4444-555555555555';
let upstream: ReturnType<typeof vi.fn>;
beforeEach(() => {
  upstream = vi.fn(async () => new Response('{"revision":4}', { status: 200 }));
  vi.stubGlobal('fetch', upstream);
});

it('forwards If-Match, so a settings save names the revision it was made from', async () => {
  const request = new Request(`https://app.test/api/engine/projects/${PROJECT}/agent-policy`, {
    method: 'PUT',
    headers: {
      origin: 'https://app.test',
      'content-type': 'application/json',
      'if-match': '"3"',
      cookie: 'dynodoc_session=ignored',
      'x-other': 'not forwarded'
    },
    body: '{"enabled":true}'
  });
  const response = await PUT({
    request,
    params: { path: `projects/${PROJECT}/agent-policy` },
    url: new URL(request.url),
    cookies: {}
  } as never);
  expect(response.status).toBe(200);
  const [url, init] = upstream.mock.calls[0] as [string, RequestInit];
  expect(url).toBe(`http://engine.test/projects/${PROJECT}/agent-policy`);
  const headers = init.headers as Headers;
  expect(headers.get('if-match')).toBe('"3"');
  expect(headers.get('authorization')).toBe('Bearer session-token');
  expect(headers.get('content-type')).toBe('application/json');
  expect(headers.get('cookie')).toBeNull();
  expect(headers.get('x-other')).toBeNull();
});

it('still refuses a write from another origin before anything is forwarded', async () => {
  const request = new Request(`https://app.test/api/engine/projects/${PROJECT}/agent-policy`, {
    method: 'PUT',
    headers: { origin: 'https://elsewhere.test', 'if-match': '"3"' },
    body: '{"enabled":true}'
  });
  const response = await PUT({
    request,
    params: { path: `projects/${PROJECT}/agent-policy` },
    url: new URL(request.url),
    cookies: {}
  } as never);
  expect(response.status).toBe(403);
  expect(upstream).not.toHaveBeenCalled();
});
