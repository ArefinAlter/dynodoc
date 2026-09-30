import { beforeEach, describe, expect, it, vi } from 'vitest';

const env = vi.hoisted(() => ({
  RESEND_API_KEY: 'test-key',
  EMAIL_FROM: 'Dynodoc <team@example.test>'
}));
vi.mock('$env/dynamic/private', () => ({ env }));
vi.mock('$lib/server/session', () => ({
  accessToken: vi.fn(async () => 'token'),
  engineUrl: () => 'http://engine.test'
}));

import { POST } from './+server';

const DOC = '11111111-2222-3333-4444-555555555555';
type Call = { url: string; init?: RequestInit };
let calls: Call[];
let shareStatus: number;
let resendStatus: number;
beforeEach(() => {
  calls = [];
  shareStatus = 200;
  resendStatus = 200;
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      calls.push({ url, init });
      const reply = (status: number, body: unknown) =>
        new Response(JSON.stringify(body), { status });
      if (url.endsWith('/members'))
        return shareStatus === 200
          ? reply(200, { email: 'tariq@example.test', role: 'contributor' })
          : reply(shareStatus, { error: 'forbidden' });
      if (url.endsWith('/auth/me'))
        return reply(200, {
          identity_id: `me-${Math.random()}`,
          email: 'olivia@example.test',
          display_name: 'Olivia Owner'
        });
      if (url.endsWith(`/documents/${DOC}`)) return reply(200, { title: 'Group report' });
      if (url.startsWith('https://api.resend.com')) return reply(resendStatus, { id: 'email' });
      return reply(404, {});
    })
  );
});

function call(body: Record<string, unknown>, origin = 'https://app.dynodoc.online') {
  const url = new URL('https://app.dynodoc.online/api/invite');
  const request = new Request(url, {
    method: 'POST',
    headers: { origin, 'content-type': 'application/json' },
    body: JSON.stringify(body)
  });
  return POST({ request, url, cookies: {} } as never) as Promise<Response>;
}

describe('POST /api/invite', () => {
  it('shares first, then emails an invitation with the link and a reply-to', async () => {
    const response = await call({
      document_id: DOC,
      email: 'Tariq@Example.test ',
      role: 'contributor',
      note: 'Please review'
    });
    expect(response.status).toBe(200);
    expect((await response.json()).email_status).toBe('sent');
    expect(calls[0].url).toBe(`http://engine.test/documents/${DOC}/members`);
    expect(JSON.parse(String(calls[0].init?.body))).toEqual({
      email: 'tariq@example.test',
      role: 'contributor'
    });
    const email = JSON.parse(
      String(calls.find((c) => c.url.startsWith('https://api.resend.com'))!.init?.body)
    );
    expect(email.to).toEqual(['tariq@example.test']);
    expect(email.reply_to).toBe('olivia@example.test');
    expect(email.subject).toContain('“Group report”');
    expect(email.text).toContain(`https://app.dynodoc.online/documents/${DOC}`);
    expect(email.text).toContain('Please review');
  });
  it('sends nothing when sharing is refused', async () => {
    shareStatus = 403;
    const response = await call({ document_id: DOC, email: 'tariq@example.test', role: 'editor' });
    expect(response.status).toBe(403);
    expect(calls.some((c) => c.url.startsWith('https://api.resend.com'))).toBe(false);
  });
  it('respects the opt-out, removals and self-invitations', async () => {
    for (const body of [
      { document_id: DOC, email: 'tariq@example.test', role: 'editor', notify: false },
      { document_id: DOC, email: 'tariq@example.test', role: 'remove' },
      { document_id: DOC, email: 'olivia@example.test', role: 'editor' }
    ]) {
      expect((await (await call(body)).json()).email_status).toBe('skipped');
    }
    expect(calls.some((c) => c.url.startsWith('https://api.resend.com'))).toBe(false);
  });
  it('reports delivery failures without undoing the share', async () => {
    resendStatus = 500;
    const response = await call({ document_id: DOC, email: 'rina@example.test', role: 'viewer' });
    expect(response.status).toBe(200);
    expect((await response.json()).email_status).toBe('failed');
  });
  it('reports when email is not configured', async () => {
    const key = env.RESEND_API_KEY;
    env.RESEND_API_KEY = '';
    const response = await call({ document_id: DOC, email: 'eli@example.test', role: 'viewer' });
    env.RESEND_API_KEY = key;
    expect((await response.json()).email_status).toBe('not_configured');
  });
  it('rejects cross-site requests and bad input', async () => {
    expect(
      (await call({ document_id: DOC, email: 'x@y.z', role: 'viewer' }, 'https://evil.test')).status
    ).toBe(403);
    expect((await call({ document_id: 'nope', email: 'x@y.z', role: 'viewer' })).status).toBe(400);
  });
});
