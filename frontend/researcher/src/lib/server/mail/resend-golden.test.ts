// The hosted service sends sign-in links and invitations through Resend. These tests pin the
// request it makes (URL, method, headers and the exact JSON text of the body) so the mail
// transport abstraction cannot change what hosted sends. They were written against the code
// before the abstraction and must keep passing unchanged.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const env = vi.hoisted(() => ({}) as Record<string, string | undefined>);
vi.mock('$env/dynamic/private', () => ({ env }));
const engine = vi.hoisted(() => ({
  enginePost: vi.fn(async () => ({ magic_link_token: 'issued token/+=' })),
  accessToken: vi.fn(async () => 'token'),
  saveSession: vi.fn(),
  readSession: vi.fn(),
  clearSession: vi.fn(),
  engineUrl: () => 'http://engine.test'
}));
vi.mock('$lib/server/session', () => engine);

const DOC = '11111111-2222-3333-4444-555555555555';
const HOSTED = 'https://app.dynodoc.online';
let resend: { url: string; init: RequestInit }[];

beforeEach(() => {
  resend = [];
  for (const key of Object.keys(env)) delete env[key];
  Object.assign(env, {
    RESEND_API_KEY: 're_golden_key',
    EMAIL_FROM: 'Dynodoc <team@example.test>'
  });
  vi.resetModules();
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init: RequestInit) => {
      const reply = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });
      if (url.startsWith('https://api.resend.com')) {
        resend.push({ url, init });
        return reply({ id: 'email' });
      }
      if (url.endsWith('/members')) return reply({ email: 'tariq@example.test', role: 'viewer' });
      if (url.endsWith('/auth/me'))
        return reply({
          identity_id: 'me-1',
          email: 'olivia@example.test',
          display_name: 'Olivia Owner'
        });
      if (url.endsWith(`/documents/${DOC}`)) return reply({ title: 'Group report' });
      return new Response('{}', { status: 404 });
    })
  );
});

const headersOf = (init: RequestInit) => init.headers as Record<string, string>;

describe('hosted sign-in link through Resend', () => {
  it('sends exactly this request', async () => {
    const { POST } = await import('../../../routes/auth/[...action]/+server');
    const url = new URL(`${HOSTED}/auth/email`);
    const response = await POST({
      params: { action: 'email' },
      url,
      cookies: {},
      request: new Request(url, {
        method: 'POST',
        headers: { origin: url.origin, 'content-type': 'application/json' },
        body: JSON.stringify({ email: ' Olivia@Example.test ', display_name: 'Olivia' })
      })
    } as never);
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ ok: true });
    expect(resend).toHaveLength(1);
    const { url: target, init } = resend[0];
    expect(target).toBe('https://api.resend.com/emails');
    expect(init.method).toBe('POST');
    expect(headersOf(init)).toEqual({
      Authorization: 'Bearer re_golden_key',
      'content-type': 'application/json'
    });
    expect(init.body).toBe(
      JSON.stringify({
        from: 'Dynodoc <team@example.test>',
        to: ['olivia@example.test'],
        subject: 'Sign in to Dynodoc',
        text: `Sign in to your Dynodoc workspace:\n\n${HOSTED}/auth/verify?token=issued%20token%2F%2B%3D\n\nThis link expires in 15 minutes. If you did not request it, you can ignore this message.`
      })
    );
    expect(init.signal).toBeInstanceOf(AbortSignal);
  });
});

describe('hosted invitation through Resend', () => {
  it('sends exactly this request', async () => {
    const { POST } = await import('../../../routes/api/invite/+server');
    const url = new URL(`${HOSTED}/api/invite`);
    const response = await POST({
      request: new Request(url, {
        method: 'POST',
        headers: { origin: url.origin, 'content-type': 'application/json' },
        body: JSON.stringify({
          document_id: DOC,
          email: 'Tariq@Example.test',
          role: 'contributor',
          note: 'Please review & "approve"'
        })
      }),
      url,
      cookies: {}
    } as never);
    expect(response.status).toBe(200);
    expect((await response.json()).email_status).toBe('sent');
    expect(resend).toHaveLength(1);
    const { url: target, init } = resend[0];
    expect(target).toBe('https://api.resend.com/emails');
    expect(init.method).toBe('POST');
    expect(headersOf(init)).toEqual({
      Authorization: 'Bearer re_golden_key',
      'content-type': 'application/json'
    });
    const link = `${HOSTED}/documents/${DOC}`;
    const signIn =
      'Sign in with this email address (tariq@example.test): choose “Continue with Google” if it is a Google account, or ask for an email sign-in link.';
    const inviter =
      'You are receiving this because olivia@example.test added you. If you were not expecting it, you can ignore this email.';
    const role = 'Contributor — you can comment, suggest and send changes for review';
    expect(init.body).toBe(
      JSON.stringify({
        from: 'Dynodoc <team@example.test>',
        to: ['tariq@example.test'],
        subject: 'Olivia Owner shared “Group report” with you on Dynodoc',
        text: [
          'Olivia Owner (olivia@example.test) shared “Group report” with you on Dynodoc.',
          `Your access: ${role}.`,
          '',
          'Their note:',
          'Please review & "approve"',
          '',
          `Open it: ${link}`,
          '',
          signIn,
          '',
          inviter
        ].join('\n'),
        html: `<div style="font-family:Arial,Helvetica,sans-serif;font-size:15px;line-height:1.55;color:#1d2f40;max-width:560px">
<p><strong>Olivia Owner (olivia@example.test)</strong> shared “Group report” with you on Dynodoc.</p>
<p>Your access: ${role}.</p>
<blockquote style="margin:16px 0;padding:10px 14px;border-left:3px solid #b9d0ee;background:#f3f6f9;white-space:pre-wrap">Please review &amp; &quot;approve&quot;</blockquote>
<p><a href="${link}" style="display:inline-block;padding:10px 18px;border-radius:8px;background:#205bb0;color:#ffffff;text-decoration:none;font-weight:bold">Open in Dynodoc</a></p>
<p style="color:#46617a;font-size:13px">${signIn}</p>
<p style="color:#64788b;font-size:12px">${inviter}</p>
</div>`,
        reply_to: 'olivia@example.test'
      })
    );
    expect(init.signal).toBeInstanceOf(AbortSignal);
  });
});
