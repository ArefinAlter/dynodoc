// @vitest-environment node
// The transport as the routes use it: sign-in links and invitations through SMTP (to an SMTP
// server in this process), with the mail off, and with Resend. Self-hosted mode builds links
// on its configured origin; hosted keeps the origin of the request.
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { startFakeSmtp, type FakeSmtp } from './test-helpers/fake-smtp';
import { readMessage } from './test-helpers/mime';

// Loading the routes the first time takes a few seconds on a busy machine.
vi.setConfig({ testTimeout: 30_000 });

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

const SESSION = '7c2e9a5d1f8b4036a7c2e9d5a1f8b4c36a7c2e9a5d1f8b4036a7c2e9d5a1f8b4';
const SERVICE = 'e0b6d3a9f5c28147e0b6d3a9f5c28147e0b6d3a9f5c28147e0b6d3a9f5c28147';
const ORIGIN = 'https://docs.example.org';
const DOC = '11111111-2222-3333-4444-555555555555';
const SPACE = '99999999-2222-3333-4444-555555555555';

let smtp: FakeSmtp;
beforeAll(async () => {
  smtp = await startFakeSmtp({
    authMethods: ['PLAIN'],
    credentials: { user: 'sender', pass: 'relay-password' },
    requireAuth: true
  });
});
afterAll(() => smtp.close());

const self = (extra: Record<string, string> = {}) => ({
  SESSION_SECRET: SESSION,
  WEB_SERVICE_KEY: SERVICE,
  DYNODOC_MODE: 'selfhosted',
  ORIGIN,
  DYNODOC_BRAND_NAME: 'Acme Docs',
  ...extra
});
const viaSmtp = (extra: Record<string, string> = {}) =>
  self({
    DYNODOC_MAIL_TRANSPORT: 'smtp',
    SMTP_HOST: '127.0.0.1',
    SMTP_PORT: String(smtp.port),
    SMTP_SECURITY: 'none-local',
    SMTP_USER: 'sender',
    SMTP_PASSWORD: 'relay-password',
    SMTP_FROM: 'docs@example.org',
    SMTP_FROM_NAME: 'Acme Docs',
    ...extra
  });
const mailOff = (extra: Record<string, string> = {}) => self({ DEV_AUTH: 'true', ...extra });

async function load(values: Record<string, string>) {
  vi.resetModules();
  for (const key of Object.keys(env)) delete env[key];
  Object.assign(env, values);
  return {
    auth: (await import('../../../routes/auth/[...action]/+server')) as unknown as {
      GET: (event: unknown) => Promise<Response>;
      POST: (event: unknown) => Promise<Response>;
    },
    invite: (await import('../../../routes/api/invite/+server')) as unknown as {
      POST: (event: unknown) => Promise<Response>;
    },
    mail: await import('./index')
  };
}
async function signIn(routes: Awaited<ReturnType<typeof load>>, requestOrigin = ORIGIN) {
  const url = new URL(`${requestOrigin}/auth/email`);
  return routes.auth.POST({
    params: { action: 'email' },
    url,
    cookies: {},
    request: new Request(url, {
      method: 'POST',
      headers: { origin: url.origin, 'content-type': 'application/json' },
      body: JSON.stringify({ email: 'Olivia@Example.test', display_name: 'Olivia' })
    })
  });
}

let engineCalls: string[];
let spaceKind = 'project';
beforeEach(() => {
  smtp.messages.length = 0;
  smtp.lines.length = 0;
  engine.enginePost.mockClear();
  engineCalls = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string) => {
      engineCalls.push(url);
      const reply = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
      if (url.endsWith('/members')) return reply({ email: 'tariq@example.test', role: 'viewer' });
      if (url.endsWith('/auth/me'))
        return reply({
          identity_id: `me-${Math.random()}`,
          email: 'olivia@example.test',
          display_name: 'Olivia Owner'
        });
      if (url.endsWith(`/documents/${DOC}`)) return reply({ title: '<b>Group</b> report' });
      if (url.endsWith('/spaces'))
        return reply({ items: [{ id: SPACE, name: 'Field team', kind: spaceKind }] });
      return reply({}, 404);
    })
  );
});
afterEach(() => vi.restoreAllMocks());

async function invite(
  routes: Awaited<ReturnType<typeof load>>,
  body: Record<string, unknown>,
  requestOrigin = ORIGIN
) {
  const url = new URL(`${requestOrigin}/api/invite`);
  const response = await routes.invite.POST({
    request: new Request(url, {
      method: 'POST',
      headers: { origin: url.origin, 'content-type': 'application/json' },
      body: JSON.stringify({ role: 'contributor', email: 'tariq@example.test', ...body })
    }),
    url,
    cookies: {}
  });
  return { status: response.status, json: await response.json() };
}

describe('sign-in link through SMTP', () => {
  it('sends the link on the configured origin, from the configured sender, and says so', async () => {
    const routes = await load(viaSmtp());
    const response = await signIn(routes, 'https://other.example');
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ ok: true });
    expect(smtp.messages).toHaveLength(1);
    const [message] = smtp.messages;
    expect(message.to).toEqual(['olivia@example.test']);
    expect(message.from).toBe('docs@example.org');
    expect(message.authUser).toBe('sender');
    const read = readMessage(message.data);
    expect(read.subject).toBe('Sign in to Acme Docs');
    expect(read.headers.get('from')).toBe('Acme Docs <docs@example.org>');
    const link = `${ORIGIN}/auth/verify?token=issued%20token%2F%2B%3D`;
    expect(read.text).toContain(link);
    expect(read.text).toContain('Sign in to your Acme Docs workspace:');
    expect(read.html).toContain(`href="${link}"`);
    expect(message.data).toContain('multipart/alternative');
    expect(`${read.text}${read.html}`).not.toContain('other.example');
    expect(`${read.text}${read.html}${message.data}`).not.toContain('dynodoc.online');
  });
  it('offers email sign-in when mail can be sent', async () => {
    const routes = await load(viaSmtp());
    const options = await (
      await routes.auth.GET({
        params: { action: 'options' },
        url: new URL(`${ORIGIN}/auth/options`),
        cookies: {}
      })
    ).json();
    expect(options).toMatchObject({ email: true, development: false });
  });
  it('answers 503 in plain words, with nothing from the mail server, when it cannot be sent', async () => {
    const routes = await load(viaSmtp({ SMTP_PASSWORD: 'wrong-password-123' }));
    const response = await signIn(routes);
    expect(response.status).toBe(503);
    const text = await response.text();
    expect(text).toBe(
      JSON.stringify({
        error: 'We could not send your sign-in link. Please wait a few minutes and try again.'
      })
    );
    expect(smtp.messages).toEqual([]);
  });
});

describe('mail off', () => {
  it('says that email sign-in is not configured, and does nothing else', async () => {
    const routes = await load(self({ AUTH_EMAIL_LINK: 'on' }));
    expect(routes.mail.mailState()).toEqual({ transport: 'none', configured: false });
    const response = await signIn(routes);
    expect(response.status).toBe(503);
    expect((await response.json()).error).toBe('Email sign-in is not configured yet.');
    expect(engine.enginePost).not.toHaveBeenCalled();
    expect(smtp.connections).toBeGreaterThanOrEqual(0);
    expect(smtp.messages).toEqual([]);
  });
  it('does not offer email sign-in in the options', async () => {
    const routes = await load(mailOff());
    const options = await (
      await routes.auth.GET({
        params: { action: 'options' },
        url: new URL(`${ORIGIN}/auth/options`),
        cookies: {}
      })
    ).json();
    expect(options).toMatchObject({ email: false, development: true });
  });
  it('still gives the development token on a test stack', async () => {
    const routes = await load(mailOff());
    expect(await (await signIn(routes)).json()).toEqual({ development_token: 'issued token/+=' });
  });
  it('refuses to send, with an error that names no one', async () => {
    const routes = await load(mailOff());
    const error = await routes.mail
      .sendMail({ kind: 'invitation', to: 'a@example.test', subject: 's', text: 't' })
      .catch((e: Error) => e);
    expect((error as { errorClass?: string }).errorClass).toBe('config');
    expect(String((error as Error).message)).not.toContain('example.test');
  });
  it('shows the person who invited the link to pass on, instead of sending an email', async () => {
    const routes = await load(mailOff());
    const document = await invite(routes, { document_id: DOC });
    expect(document.json).toMatchObject({
      email_status: 'not_configured',
      invitation_link: `${ORIGIN}/documents/${DOC}`
    });
    const project = await invite(routes, { space_id: SPACE });
    expect(project.json.invitation_link).toBe(`${ORIGIN}/projects/${SPACE}`);
    spaceKind = 'organization';
    const workspace = await invite(routes, { space_id: SPACE });
    spaceKind = 'project';
    expect(workspace.json.invitation_link).toBe(`${ORIGIN}/workspace`);
    expect(smtp.messages).toEqual([]);
  });
  it('still gives the link of a document if the title cannot be looked up, and none to someone who opted out', async () => {
    const routes = await load(mailOff());
    const lookup = vi.fn(async (url: string) => {
      if (url.endsWith('/members')) return new Response('{"role":"viewer"}', { status: 200 });
      if (url.endsWith('/auth/me'))
        return new Response('{"identity_id":"x","email":"olivia@example.test"}', { status: 200 });
      throw new Error('engine unreachable');
    });
    vi.stubGlobal('fetch', lookup);
    expect((await invite(routes, { document_id: DOC })).json.invitation_link).toBe(
      `${ORIGIN}/documents/${DOC}`
    );
    expect((await invite(routes, { space_id: SPACE })).json).not.toHaveProperty('invitation_link');
    const optedOut = await invite(routes, { document_id: DOC, notify: false });
    expect(optedOut.json.email_status).toBe('skipped');
    expect(optedOut.json).not.toHaveProperty('invitation_link');
    expect((await invite(routes, { document_id: DOC, role: 'remove' })).json).not.toHaveProperty(
      'invitation_link'
    );
  });
});

describe('invitation through SMTP', () => {
  it('sends it from the configured sender, with the inviter as Reply-To and a link on the configured origin', async () => {
    const routes = await load(viaSmtp({ SMTP_REPLY_TO: 'records@example.org' }));
    const { json } = await invite(
      routes,
      { document_id: DOC, note: 'Please check <section 2>' },
      'https://other.example'
    );
    expect(json.email_status).toBe('sent');
    expect(json).not.toHaveProperty('invitation_link');
    const [message] = smtp.messages;
    expect(message.to).toEqual(['tariq@example.test']);
    const read = readMessage(message.data);
    expect(read.headers.get('reply-to')).toBe('olivia@example.test');
    expect(read.subject).toBe('Olivia Owner shared “<b>Group</b> report” with you on Acme Docs');
    expect(read.text).toContain(`Open it: ${ORIGIN}/documents/${DOC}`);
    expect(read.html).toContain(`href="${ORIGIN}/documents/${DOC}"`);
    expect(`${read.text}${read.html}`).not.toContain('other.example');
    expect(`${read.subject}${read.text}${read.html}`).not.toContain('Dynodoc');
    // The title and the note are text in the text part and escaped in the HTML part.
    expect(read.text).toContain('Please check <section 2>');
    expect(read.html).toContain('&lt;b&gt;Group&lt;/b&gt; report');
    expect(read.html).toContain('Please check &lt;section 2&gt;');
    expect(read.html).not.toContain('<b>Group</b>');
    expect(read.html).not.toContain('<section');
  });
  it('falls back to the configured Reply-To only where the message has none', async () => {
    const routes = await load(viaSmtp({ SMTP_REPLY_TO: 'records@example.org' }));
    await routes.mail.sendMail({
      kind: 'signin',
      to: 'olivia@example.test',
      subject: 'x',
      text: 'y'
    });
    expect(readMessage(smtp.messages[0].data).headers.get('reply-to')).toBe('records@example.org');
  });
  it('tells the person who invited to pass the link on when the email could not be sent', async () => {
    const routes = await load(viaSmtp({ SMTP_PORT: '1', SMTP_TIMEOUT_SECONDS: '5' }));
    const { json } = await invite(routes, { document_id: DOC });
    expect(json).toMatchObject({
      email_status: 'failed',
      invitation_link: `${ORIGIN}/documents/${DOC}`
    });
  }, 30_000);
});

describe('links are built on the right origin', () => {
  it('uses the request origin in hosted mode, as it always has, and the configured one when self-hosted', async () => {
    const hosted = await load({ SESSION_SECRET: SESSION, WEB_SERVICE_KEY: SERVICE });
    expect(hosted.mail.linkOrigin('https://app.dynodoc.online')).toBe('https://app.dynodoc.online');
    const selfHosted = await load(viaSmtp());
    expect(selfHosted.mail.linkOrigin('https://evil.example')).toBe(ORIGIN);
    expect(selfHosted.mail.linkOrigin(ORIGIN)).toBe(ORIGIN);
  });
});

describe('Resend through the same entry point', () => {
  it('logs the provider and outcome only', async () => {
    const lines: string[] = [];
    vi.spyOn(console, 'info').mockImplementation((...args) => void lines.push(args.join(' ')));
    vi.spyOn(console, 'warn').mockImplementation((...args) => void lines.push(args.join(' ')));
    const routes = await load({
      SESSION_SECRET: SESSION,
      WEB_SERVICE_KEY: SERVICE,
      RESEND_API_KEY: 're_0123456789abcdef',
      EMAIL_FROM: 'Dynodoc <team@example.test>'
    });
    expect(routes.mail.mailState()).toEqual({ transport: 'resend', configured: true });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('{"id":"em_123"}', { status: 200 }))
    );
    await routes.mail.sendMail({
      kind: 'signin',
      to: 'olivia@example.test',
      subject: 'S',
      text: 'T'
    });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('{}', { status: 503 }))
    );
    await expect(
      routes.mail.sendMail({
        kind: 'invitation',
        to: 'olivia@example.test',
        subject: 'S',
        text: 'T'
      })
    ).rejects.toThrow('Email delivery failed (503)');
    const mailLines = lines.filter((line) => line.includes('mail provider='));
    expect(mailLines).toEqual([
      '[dynodoc] mail provider=resend outcome=sent kind=signin id=em_123',
      '[dynodoc] mail provider=resend outcome=failed kind=invitation class=http-503'
    ]);
    expect(lines.join('\n')).not.toMatch(/olivia|re_0123|team@/);
  });
});
