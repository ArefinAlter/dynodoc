// @vitest-environment node
// The SMTP sender against an SMTP server in this process (./test-helpers/fake-smtp.ts) that can
// use real TLS with a certificate made at test time. These tests talk to a real client and a
// real server over real sockets. They do not prove anything about any real mail provider.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createSmtpSender, resolveLocalOnly, transportOptions } from './smtp.js';
import { isLocalAddress } from './local-address.js';
import type { LogEntry } from './log.js';
import { startFakeSmtp, type FakeSmtp, type FakeSmtpOptions } from './test-helpers/fake-smtp';
import { selfSigned } from './test-helpers/x509';
import { MailError } from './errors.js';

vi.setConfig({ testTimeout: 30_000 });

const PASSWORD = 'correct horse battery staple';
const USER = 'dynodoc-sender';
const cert = selfSigned({ dns: ['localhost'], ip: ['127.0.0.1'] });
const wrongName = selfSigned({ dns: ['mail.other.example'] });
const expired = selfSigned({
  ip: ['127.0.0.1'],
  notBefore: new Date(Date.now() - 400 * 86_400_000),
  notAfter: new Date(Date.now() - 86_400_000)
});

const running: FakeSmtp[] = [];
async function server(options: FakeSmtpOptions = {}) {
  const started = await startFakeSmtp(options);
  running.push(started);
  return started;
}
afterEach(async () => {
  await Promise.all(running.splice(0).map((s) => s.close()));
  delete process.env.NODE_TLS_REJECT_UNAUTHORIZED;
});

const login = {
  credentials: { user: USER, pass: PASSWORD },
  authMethods: ['PLAIN', 'LOGIN'] as const
};
const tlsServer = (over: FakeSmtpOptions = {}) =>
  server({
    tls: cert,
    authMethods: [...login.authMethods],
    credentials: login.credentials,
    ...over
  });

function settings(target: FakeSmtp, over: Record<string, unknown> = {}) {
  return {
    host: '127.0.0.1',
    port: target.port,
    security: 'starttls' as const,
    user: USER,
    password: PASSWORD,
    from: { name: 'Acme Docs', address: 'docs@example.org' },
    replyTo: undefined,
    ca: [cert.cert],
    timeoutMs: 20_000,
    helloName: 'dynodoc.example.org',
    ...over
  };
}
type Sender = ReturnType<typeof createSmtpSender>;
function sender(
  target: FakeSmtp,
  over: Record<string, unknown> = {},
  deps: Record<string, unknown> = {}
) {
  const logs: LogEntry[] = [];
  const made: Sender = createSmtpSender(settings(target, over) as never, {
    log: (entry) => logs.push(entry),
    sleep: async () => undefined,
    random: () => 1,
    ...deps
  });
  return { send: made.send.bind(made), probe: made.probe.bind(made), logs };
}
const hello = {
  kind: 'signin' as const,
  to: 'olivia@example.test',
  subject: 'Sign in to Acme Docs',
  text: 'Sign in: https://docs.example.org/auth/verify?token=abc',
  html: '<p>Sign in</p>'
};
const failure = async (run: Promise<unknown>) => {
  const error = await run.then(
    () => undefined,
    (e) => e
  );
  expect(error).toBeInstanceOf(MailError);
  return error as MailError & { attempts?: number };
};
const base64 = (text: string) => Buffer.from(text).toString('base64');
const PLAIN_AUTH = base64(`\0${USER}\0${PASSWORD}`);
/** Nothing resembling the credentials was sent where it could be read. */
function expectNoCredentialsInClear(target: FakeSmtp) {
  const clear = target.plaintext();
  expect(clear).not.toContain(PASSWORD);
  expect(clear).not.toContain(PLAIN_AUTH);
  expect(clear).not.toContain(base64(PASSWORD));
  expect(clear).not.toContain(base64(USER));
  expect(clear.toUpperCase()).not.toContain('AUTH');
}

describe('STARTTLS (port 587 style)', () => {
  it('upgrades, authenticates inside TLS, and delivers the message', async () => {
    const target = await tlsServer({ requireAuth: true });
    const { send, logs } = sender(target);
    const { messageId } = await send(hello);
    expect(target.messages).toHaveLength(1);
    const [message] = target.messages;
    expect(message.secure).toBe(true);
    expect(message.authUser).toBe(USER);
    expect(message.from).toBe('docs@example.org');
    expect(message.to).toEqual(['olivia@example.test']);
    expect(message.data).toContain(`Message-ID: ${messageId}`);
    expect(messageId).toMatch(/^<[0-9a-f]{32}@example\.org>$/);
    expect(message.data).toMatch(/^Subject: Sign in to Acme Docs\r?$/m);
    expect(message.data).toContain('Auto-Submitted: auto-generated');
    expect(message.data).toContain('multipart/alternative');
    // EHLO name, STARTTLS before anything private.
    const commands = target.lines.map((l) => `${l.secure ? 'S' : 'P'} ${l.text.split(' ')[0]}`);
    expect(commands.slice(0, 4)).toEqual(['P EHLO', 'P STARTTLS', 'S EHLO', 'S AUTH']);
    expect(target.lines[0].text).toBe('EHLO dynodoc.example.org');
    expectNoCredentialsInClear(target);
    expect(logs).toEqual([
      { provider: 'smtp', outcome: 'sent', kind: 'signin', messageId, attempts: 1 }
    ]);
  });
  it('works against a relay that wants no login', async () => {
    const target = await server({ tls: cert });
    await sender(target, { user: undefined, password: undefined }).send(hello);
    expect(target.messages).toHaveLength(1);
    expect(target.messages[0].authUser).toBeNull();
  });
  it('uses AUTH LOGIN when that is all the server offers', async () => {
    const target = await tlsServer({ authMethods: ['LOGIN'], requireAuth: true });
    await sender(target).send(hello);
    expect(target.messages[0].authUser).toBe(USER);
    expect(target.lines.some((l) => l.secure && l.text === 'AUTH LOGIN')).toBe(true);
    expectNoCredentialsInClear(target);
  });
  it('sends the text and the HTML as alternatives, UTF-8, and a Reply-To when given', async () => {
    const target = await tlsServer();
    await sender(target, { replyTo: 'help@example.org' }).send({
      ...hello,
      subject: `Zürich ${String.fromCharCode(0x2713)} shared with you`,
      text: 'Grüße aus Zürich',
      html: '<p>Gr&uuml;ße</p>'
    });
    const { data } = target.messages[0];
    expect(data).toMatch(/^Reply-To: help@example\.org\r?$/m);
    expect(data).toMatch(/^From: Acme Docs <docs@example\.org>\r?$/m);
    expect(data).toMatch(/^Subject: =\?UTF-8\?[BQ]\?/m);
    expect(data).toMatch(/charset=utf-8/i);
    expect(data).not.toMatch(/<img|src=|url\(/i);
  });
});

describe('implicit TLS (port 465 style)', () => {
  it('connects encrypted from the first byte and authenticates', async () => {
    const target = await tlsServer({ implicitTls: true, requireAuth: true });
    await sender(target, { security: 'tls' }).send(hello);
    expect(target.messages).toHaveLength(1);
    expect(target.messages[0].secure).toBe(true);
    expect(target.messages[0].authUser).toBe(USER);
    expect(target.plaintext()).toBe('');
  });
  it('refuses a certificate for another name, or none we trust', async () => {
    const target = await server({
      tls: wrongName,
      implicitTls: true,
      authMethods: ['PLAIN'],
      credentials: login.credentials
    });
    expect(
      (await failure(sender(target, { security: 'tls', ca: [wrongName.cert] }).send(hello)))
        .errorClass
    ).toBe('tls-hostname');
    expect(
      (await failure(sender(target, { security: 'tls', ca: undefined }).send(hello))).errorClass
    ).toBe('tls-untrusted');
    expect(target.messages).toEqual([]);
    expect(target.lines.some((l) => l.text.startsWith('AUTH'))).toBe(false);
  });
  it('does not mistake a plain-text server for a TLS one', async () => {
    const target = await server({ authMethods: ['PLAIN'] });
    const error = await failure(sender(target, { security: 'tls' }).send(hello));
    expect(error.errorClass).toBe('tls-protocol');
    expect(error.attempts).toBe(1);
    expect(target.messages).toEqual([]);
  });
});

describe('certificates are checked, always', () => {
  it('refuses a self-signed certificate that is not in the trust list, before sending any password', async () => {
    const target = await tlsServer();
    const error = await failure(sender(target, { ca: undefined }).send(hello));
    expect(error.errorClass).toBe('tls-untrusted');
    expect(error.message).toMatch(/^TLS certificate not trusted: set SMTP_CA_FILE/);
    expect(error.attempts).toBe(1);
    expect(target.messages).toEqual([]);
    expect(target.lines.some((l) => l.text.startsWith('AUTH'))).toBe(false);
    expect(target.connections).toBe(1);
  });
  it('refuses a certificate that does not match the server name', async () => {
    const target = await server({
      tls: wrongName,
      authMethods: ['PLAIN'],
      credentials: login.credentials
    });
    const error = await failure(sender(target, { ca: [wrongName.cert] }).send(hello));
    expect(error.errorClass).toBe('tls-hostname');
    expect(error.message).toMatch(/does not match the server name/);
    expect(target.messages).toEqual([]);
    expect(target.lines.some((l) => l.text.startsWith('AUTH'))).toBe(false);
  });
  it('refuses an expired certificate', async () => {
    const target = await server({
      tls: expired,
      authMethods: ['PLAIN'],
      credentials: login.credentials
    });
    const error = await failure(sender(target, { ca: [expired.cert] }).send(hello));
    expect(error.errorClass).toBe('tls-expired');
    expect(target.messages).toEqual([]);
  });
  it('trusts only what SMTP_CA_FILE lists: a certificate from another authority is refused', async () => {
    const target = await tlsServer();
    const error = await failure(sender(target, { ca: [wrongName.cert] }).send(hello));
    expect(error.errorClass).toBe('tls-untrusted');
  });
  it('keeps checking even when NODE_TLS_REJECT_UNAUTHORIZED=0 is set in the process', async () => {
    process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';
    const target = await tlsServer();
    expect((await failure(sender(target, { ca: undefined }).send(hello))).errorClass).toBe(
      'tls-untrusted'
    );
    const wrong = await server({
      tls: wrongName,
      authMethods: ['PLAIN'],
      credentials: login.credentials
    });
    expect((await failure(sender(wrong, { ca: [wrongName.cert] }).send(hello))).errorClass).toBe(
      'tls-hostname'
    );
    expect(target.messages).toEqual([]);
    expect(wrong.messages).toEqual([]);
  });
  it('wants TLS 1.2 or newer', async () => {
    const target = await tlsServer({ tls: { ...cert, maxVersion: 'TLSv1.1' } });
    const error = await failure(sender(target).send(hello));
    expect(error.errorClass).toBe('tls-protocol');
    expect(target.messages).toEqual([]);
    // And the option the client is built with says so.
    expect(transportOptions(settings(target) as never, '127.0.0.1', 10_000).tls).toMatchObject({
      minVersion: 'TLSv1.2',
      rejectUnauthorized: true
    });
  });
});

describe('a password is never sent without encryption', () => {
  it('refuses a server that does not offer STARTTLS (a downgrade), and sends nothing private', async () => {
    const target = await server({
      authMethods: ['PLAIN', 'LOGIN'],
      credentials: login.credentials
    });
    const error = await failure(sender(target).send(hello));
    expect(error.errorClass).toBe('starttls-unavailable');
    expect(error.message).toContain('nothing was sent');
    expect(error.attempts).toBe(1);
    expect(target.messages).toEqual([]);
    expectNoCredentialsInClear(target);
    expect(target.lines.some((l) => l.text.startsWith('MAIL') || l.text.startsWith('RCPT'))).toBe(
      false
    );
  });
  it('refuses a server that offers STARTTLS and then refuses it', async () => {
    const target = await tlsServer({ refuseStarttls: true });
    const error = await failure(sender(target).send(hello));
    expect(error.errorClass).toBe('starttls-unavailable');
    expect(error.attempts).toBe(1);
    expect(target.lines.map((l) => l.text.split(' ')[0])).toEqual(['EHLO', 'STARTTLS']);
    expect(target.messages).toEqual([]);
    expectNoCredentialsInClear(target);
  });
  it('does not authenticate on an offer of AUTH made before TLS', async () => {
    const target = await tlsServer({
      authBeforeTls: true,
      acceptAuthBeforeTls: true,
      requireAuth: true
    });
    await sender(target).send(hello);
    expect(target.messages[0].authUser).toBe(USER);
    expectNoCredentialsInClear(target);
  });
  it('sends no credentials when AUTH was offered only before TLS and the secured session offers none', async () => {
    // The plain EHLO answer lists AUTH (and the server would take it there); the answer after
    // STARTTLS does not. The client must not log in on the earlier offer, in clear or at all.
    const target = await tlsServer({
      authBeforeTls: true,
      acceptAuthBeforeTls: true,
      authOnlyBeforeTls: true,
      requireAuth: true
    });
    const error = await failure(sender(target).send(hello));
    expect(error.errorClass).toBe('auth-required');
    expect(target.lines.some((l) => l.text.startsWith('AUTH'))).toBe(false);
    expect(target.messages).toEqual([]);
    expectNoCredentialsInClear(target);
    expect(JSON.stringify(target.lines)).not.toContain(PLAIN_AUTH);
  });
  it('would send them over plain text only to a loopback or private address, by explicit choice', async () => {
    const target = await server({
      authMethods: ['PLAIN'],
      credentials: login.credentials,
      requireAuth: true
    });
    await sender(target, { security: 'none-local', ca: undefined }).send(hello);
    expect(target.messages).toHaveLength(1);
    expect(target.messages[0].secure).toBe(false);
    expect(target.plaintext()).toContain(`AUTH PLAIN ${PLAIN_AUTH}`);
  });
  it('does not use STARTTLS in none-local mode even when it is offered', async () => {
    const target = await tlsServer();
    await sender(target, {
      security: 'none-local',
      ca: undefined,
      user: undefined,
      password: undefined
    }).send(hello);
    expect(target.lines.some((l) => l.text === 'STARTTLS')).toBe(false);
    expect(target.messages[0].secure).toBe(false);
  });
  it('refuses plain text to anything that is not local, without connecting', async () => {
    const target = await server({ authMethods: ['PLAIN'], credentials: login.credentials });
    const resolve = vi.fn(async () => [{ address: '203.0.113.9' }]);
    const error = await failure(
      sender(target, { security: 'none-local', host: 'relay.example.org' }, { resolve }).send(hello)
    );
    expect(error.errorClass).toBe('plaintext-refused');
    expect(error.attempts).toBe(1);
    expect(target.connections).toBe(0);
    const mixed = vi.fn(async () => [{ address: '127.0.0.1' }, { address: '8.8.8.8' }]);
    expect(
      (
        await failure(
          sender(
            target,
            { security: 'none-local', host: 'relay.example.org' },
            { resolve: mixed }
          ).send(hello)
        )
      ).errorClass
    ).toBe('plaintext-refused');
    expect(target.connections).toBe(0);
  });
  it('connects to the address it checked, for a name that resolves to a private one', async () => {
    const target = await server({});
    const resolve = vi.fn(async () => [{ address: '127.0.0.1' }]);
    await sender(
      target,
      { security: 'none-local', host: 'postfix', user: undefined, password: undefined },
      { resolve }
    ).send(hello);
    expect(resolve).toHaveBeenCalledTimes(1);
    expect(target.messages).toHaveLength(1);
  });
});

describe('addresses for none-local', () => {
  it('knows loopback, private, link-local and shared ranges, and nothing else', () => {
    for (const address of [
      '127.0.0.1',
      '127.255.255.254',
      '10.1.2.3',
      '172.16.0.1',
      '172.31.255.255',
      '192.168.0.1',
      '169.254.10.10',
      '100.64.0.1',
      '100.127.255.255',
      '::1',
      'fd00::1',
      'fc12::1',
      'fe80::1',
      '::ffff:10.0.0.1',
      '::ffff:127.0.0.1'
    ])
      expect(isLocalAddress(address), address).toBe(true);
    for (const address of [
      '8.8.8.8',
      '203.0.113.1',
      '172.15.0.1',
      '172.32.0.1',
      '192.167.0.1',
      '100.63.0.1',
      '100.128.0.1',
      '11.0.0.1',
      '2001:db8::1',
      '::ffff:8.8.8.8',
      'localhost',
      'example.org',
      '',
      '999.1.1.1'
    ])
      expect(isLocalAddress(address), address).toBe(false);
  });
  it('resolves a name and demands that every address be local', async () => {
    await expect(resolveLocalOnly('10.0.0.5')).resolves.toBe('10.0.0.5');
    await expect(
      resolveLocalOnly('relay', async () => [{ address: '10.0.0.7' }, { address: '10.0.0.8' }])
    ).resolves.toBe('10.0.0.7');
    await expect(
      resolveLocalOnly('relay', async () => [{ address: '10.0.0.7' }, { address: '1.1.1.1' }])
    ).rejects.toMatchObject({ errorClass: 'plaintext-refused' });
    await expect(resolveLocalOnly('relay', async () => [])).rejects.toMatchObject({
      errorClass: 'dns'
    });
    await expect(
      resolveLocalOnly('relay', async () => {
        throw Object.assign(new Error('getaddrinfo ENOTFOUND relay'), { code: 'EDNS' });
      })
    ).rejects.toMatchObject({ errorClass: 'dns' });
    await expect(resolveLocalOnly('8.8.8.8')).rejects.toMatchObject({
      errorClass: 'plaintext-refused'
    });
  });
  it('builds the client options from the security setting, and nothing weaker', () => {
    const target = { port: 25 } as FakeSmtp;
    const options = (security: string) =>
      transportOptions(settings(target, { security }) as never, '127.0.0.1', 10_000) as Record<
        string,
        unknown
      >;
    expect(options('tls')).toMatchObject({ secure: true, requireTLS: false, ignoreTLS: false });
    expect(options('starttls')).toMatchObject({
      secure: false,
      requireTLS: true,
      ignoreTLS: false
    });
    expect(options('none-local')).toMatchObject({
      secure: false,
      requireTLS: false,
      ignoreTLS: true
    });
    expect(options('none-local').tls).toBeUndefined();
    for (const security of ['tls', 'starttls'])
      expect(options(security).tls).toMatchObject({
        rejectUnauthorized: true,
        minVersion: 'TLSv1.2',
        ca: [cert.cert]
      });
    expect(options('tls')).toMatchObject({
      disableFileAccess: true,
      disableUrlAccess: true,
      logger: false,
      debug: false,
      name: 'dynodoc.example.org'
    });
  });
});

describe('login', () => {
  it('reports a wrong password as an authentication failure and does not retry it', async () => {
    const target = await tlsServer();
    const { send, logs } = sender(target, { password: 'wrong' });
    const error = await failure(send(hello));
    expect(error.errorClass).toBe('auth-failed');
    expect(error.message).toBe('authentication failed: check SMTP_USER and SMTP_PASSWORD');
    expect(error.attempts).toBe(1);
    expect(target.connections).toBe(1);
    expect(logs).toHaveLength(1);
    expect(logs[0]).toMatchObject({ outcome: 'failed', errorClass: 'auth-failed' });
  });
  it('reports a relay that wants a login as such', async () => {
    const target = await tlsServer({ requireAuth: true });
    const error = await failure(
      sender(target, { user: undefined, password: undefined }).send(hello)
    );
    expect(error.errorClass).toBe('auth-required');
  });
});

describe('retries', () => {
  it('retries a 4xx reply and then delivers', async () => {
    const target = await tlsServer({
      reply: {
        rcpt: ({ connection }) =>
          connection === 1 ? '451 4.7.1 Greylisted, try again later' : undefined
      }
    });
    const sleeps: number[] = [];
    const { send, logs } = sender(
      target,
      {},
      { sleep: async (ms: number) => void sleeps.push(ms) }
    );
    const { messageId } = await send(hello);
    expect(target.connections).toBe(2);
    expect(target.messages).toHaveLength(1);
    expect(sleeps).toEqual([1000]);
    expect(logs.map((l) => `${l.outcome}:${l.errorClass ?? ''}:${l.attempts}`)).toEqual([
      'retry:transient:1',
      'sent::2'
    ]);
    expect(logs.every((l) => l.messageId === messageId)).toBe(true);
  });
  it('retries a 4xx reply to the message itself', async () => {
    const target = await tlsServer({
      reply: { end: ({ connection }) => (connection === 1 ? '452 4.2.2 Mailbox full' : undefined) }
    });
    await sender(target).send(hello);
    expect(target.connections).toBe(2);
    expect(target.messages).toHaveLength(1);
  });
  it('gives up after three tries on a server that keeps saying later', async () => {
    const target = await tlsServer({ reply: { mail: () => '421 4.3.2 Service not available' } });
    const { send, logs } = sender(target);
    const error = await failure(send(hello));
    expect(error.errorClass).toBe('transient');
    expect(error.attempts).toBe(3);
    expect(target.connections).toBe(3);
    expect(logs.map((l) => l.outcome)).toEqual(['retry', 'retry', 'failed']);
  });
  it('retries a refused connection', async () => {
    const target = await tlsServer();
    const closed = target.port;
    await target.close();
    running.splice(running.indexOf(target), 1);
    const { send, logs } = sender({ ...target, port: closed } as FakeSmtp);
    const error = await failure(send(hello));
    expect(error.errorClass).toBe('connection-refused');
    expect(error.attempts).toBe(3);
    expect(logs.filter((l) => l.outcome === 'retry')).toHaveLength(2);
  });
  it.each([
    [
      'a mailbox that does not exist',
      { rcpt: () => '550 5.1.1 No such user' },
      'recipient-refused'
    ],
    ['a relay that refuses', { rcpt: () => '554 5.7.1 Relay access denied' }, 'relay-refused'],
    ['a sender it does not know', { mail: () => '550 5.7.1 Sender not allowed' }, 'sender-refused'],
    ['a message it rejects', { end: () => '554 5.7.0 Rejected as spam' }, 'message-rejected'],
    [
      'a message that is too big',
      { end: () => '552 5.3.4 Message size exceeds limit' },
      'too-large'
    ],
    ['a sender with a syntax error', { mail: () => '501 5.5.4 Syntax error' }, 'sender-refused']
  ] as [string, FakeSmtpOptions['reply'], string][])(
    'never retries %s (a 5xx reply)',
    async (_name, reply, errorClass) => {
      const target = await tlsServer({ reply });
      const { send, logs } = sender(target);
      const error = await failure(send(hello));
      expect(error.errorClass).toBe(errorClass);
      expect(error.attempts).toBe(1);
      expect(target.connections).toBe(1);
      expect(target.messages).toEqual([]);
      expect(logs).toHaveLength(1);
    }
  );
});

describe('time', () => {
  it('gives up on a server that never greets', async () => {
    const target = await server({ greetingDelayMs: Infinity });
    const started = Date.now();
    const error = await failure(
      sender(target, { security: 'none-local', ca: undefined, timeoutMs: 1500 }).send(hello)
    );
    expect(error.errorClass).toBe('timeout');
    expect(error.message).toMatch(/did not answer in time/);
    expect(Date.now() - started).toBeLessThan(15_000);
    expect(target.messages).toEqual([]);
  });
  it('gives up on a server that answers every command too slowly, within the total budget', async () => {
    const target = await tlsServer({ replyDelayMs: 700 });
    const started = Date.now();
    const error = await failure(sender(target, { timeoutMs: 2000 }).send(hello));
    expect(error.errorClass).toBe('timeout');
    expect(Date.now() - started).toBeLessThan(15_000);
    expect(target.messages).toEqual([]);
  });
  it('stops retrying when the budget is spent', async () => {
    const target = await tlsServer({ greetingDelayMs: Infinity });
    const { send, logs } = sender(target, { timeoutMs: 1500 });
    const error = await failure(send(hello));
    expect(error.attempts).toBe(1);
    expect(logs.map((l) => l.outcome)).toEqual(['failed']);
  });
});

describe('limits and hostile input are refused before any connection', () => {
  it('refuses more recipients than the cap, an oversized message, and every kind of header injection', async () => {
    const target = await tlsServer();
    const { send, logs } = sender(target);
    const many = Array.from({ length: 11 }, (_, i) => `user${i}@example.test`);
    expect((await failure(send({ ...hello, to: many }))).errorClass).toBe('invalid-message');
    const accented = String.fromCharCode(0xe9).repeat(100_000);
    expect((await failure(send({ ...hello, text: accented, html: accented }))).errorClass).toBe(
      'too-large'
    );
    for (const hostile of [
      { to: 'a@example.test\r\nBcc: attacker@example.org' },
      { to: 'a@example.test\nBcc: attacker@example.org' },
      { subject: 'Hello\r\nBcc: attacker@example.org' },
      { subject: 'Hello\nX-Evil: 1' },
      { replyTo: 'x@example.test\r\nBcc: attacker@example.org' }
    ])
      expect(
        (await failure(send({ ...hello, ...hostile }))).errorClass,
        JSON.stringify(hostile)
      ).toBe('invalid-message');
    // A hostile sender name never gets as far as a sender: the configuration is refused.
    expect(() =>
      sender(target, {
        from: { name: 'Acme\r\nBcc: attacker@example.org', address: 'docs@example.org' }
      })
    ).toThrow(/sender name contains a line break/);
    expect(target.connections).toBe(0);
    expect(target.messages).toEqual([]);
    expect(logs.every((l) => l.outcome === 'failed')).toBe(true);
  });
  it('delivers the largest single recipient list the cap allows', async () => {
    const target = await tlsServer();
    const ten = Array.from({ length: 10 }, (_, i) => `user${i}@example.test`);
    await sender(target).send({ ...hello, to: ten });
    expect(target.messages[0].to).toEqual(ten);
  });
});

describe('what is logged', () => {
  it('holds the provider, the outcome, a short class and the id, and no secret, address, subject or server text', async () => {
    const lines: string[] = [];
    const spies = [vi.spyOn(console, 'info'), vi.spyOn(console, 'warn')].map((spy) =>
      spy.mockImplementation((...args: unknown[]) => void lines.push(args.join(' ')))
    );
    try {
      const good = await tlsServer();
      await createSmtpSender(settings(good) as never).send(hello);
      const greylist = await tlsServer({
        reply: { rcpt: () => '550 5.1.1 <olivia@example.test> unknown user, password rejected' }
      });
      const error = await failure(createSmtpSender(settings(greylist) as never).send(hello));
      expect(error.serverResponse).toContain('olivia@example.test');
      const wrong = await tlsServer();
      await failure(
        createSmtpSender(settings(wrong, { password: 'not-the-password' }) as never).send(hello)
      );
    } finally {
      for (const spy of spies) spy.mockRestore();
    }
    const text = lines.join('\n');
    expect(lines.length).toBeGreaterThanOrEqual(3);
    for (const line of lines)
      expect(line).toMatch(
        /^\[dynodoc\] mail provider=smtp outcome=(sent|failed|retry)( kind=signin)?( class=[a-z-]+)?( attempts=\d)?( id=<[0-9a-f]{32}@example\.org>)?$/
      );
    for (const secret of [
      PASSWORD,
      'not-the-password',
      PLAIN_AUTH,
      'olivia',
      'example.test',
      'Sign in',
      'unknown user',
      'token=abc',
      'rejected'
    ])
      expect(text, secret).not.toContain(secret);
    expect(text).toContain('class=recipient-refused');
    expect(text).toContain('class=auth-failed');
  });
  it('puts no password, address or subject in any error message', async () => {
    const bad = await tlsServer({
      reply: { rcpt: () => '550 5.1.1 <olivia@example.test> unknown' }
    });
    const error = await failure(sender(bad).send(hello));
    for (const secret of [PASSWORD, 'olivia', 'example.test', 'Sign in'])
      expect(error.message).not.toContain(secret);
  });
});

describe('probe (the connection check)', () => {
  it('walks connect, TLS, EHLO, AUTH, MAIL FROM and RCPT TO, and delivers nothing', async () => {
    const target = await tlsServer({ requireAuth: true });
    const result = await sender(target).probe({ to: 'olivia@example.test' });
    expect(result.ok).toBe(true);
    expect(result.steps.map((s) => `${s.ok ? 'ok' : 'FAIL'} ${s.name}`)).toEqual([
      'ok connect, secure the connection and say EHLO',
      'ok authenticate',
      'ok MAIL FROM and RCPT TO'
    ]);
    expect(result.steps[0].detail).toMatch(/^encrypted with TLSv1\.[23], certificate verified$/);
    expect(target.messages).toEqual([]);
    const commands = target.lines.map((l) => l.text.split(' ')[0]);
    expect(commands).toEqual(['EHLO', 'STARTTLS', 'EHLO', 'AUTH', 'MAIL', 'RCPT', 'DATA']);
    expectNoCredentialsInClear(target);
  });
  it('checks only the connection and the login when no recipient is given', async () => {
    const target = await tlsServer();
    const result = await sender(target).probe();
    expect(result.ok).toBe(true);
    expect(result.steps.at(-1)).toMatchObject({
      ok: true,
      detail: expect.stringContaining('not checked')
    });
    expect(target.lines.some((l) => l.text.startsWith('MAIL'))).toBe(false);
  });
  it.each([
    [
      'an untrusted certificate',
      {},
      { ca: undefined },
      'tls-untrusted',
      'connect, secure the connection and say EHLO'
    ],
    ['a wrong password', {}, { password: 'nope' }, 'auth-failed', 'authenticate'],
    [
      'a relay that refuses the recipient',
      { reply: { rcpt: () => '554 5.7.1 Relay access denied' } },
      {},
      'relay-refused',
      'MAIL FROM and RCPT TO'
    ],
    [
      'a refused sender',
      { reply: { mail: () => '550 5.7.1 Sender not allowed' } },
      {},
      'sender-refused',
      'MAIL FROM and RCPT TO'
    ]
  ] as [string, Partial<FakeSmtpOptions>, Record<string, unknown>, string, string][])(
    'names %s',
    async (_name, serverOptions, over, errorClass, step) => {
      const target = await tlsServer(serverOptions);
      const result = await sender(target, over).probe({ to: 'olivia@example.test' });
      expect(result.ok).toBe(false);
      expect(result.error?.errorClass).toBe(errorClass);
      expect(result.steps.at(-1)).toMatchObject({ ok: false, name: step });
      expect(target.messages).toEqual([]);
    }
  );
  it('refuses to go on without STARTTLS and shows the server words only for the server step', async () => {
    const target = await server({ authMethods: ['PLAIN'], credentials: login.credentials });
    const result = await sender(target).probe({ to: 'olivia@example.test' });
    expect(result.ok).toBe(false);
    expect(result.error?.errorClass).toBe('starttls-unavailable');
    expect(result.steps[0].detail).toContain('502 5.5.1 STARTTLS is not available');
    expectNoCredentialsInClear(target);
  });
  it('reports a server that offers no AUTH after encryption, without sending a password', async () => {
    const target = await server({ tls: cert });
    const result = await sender(target).probe({ to: 'olivia@example.test' });
    expect(result.ok).toBe(false);
    expect(result.error?.errorClass).toBe('auth-required');
    expect(target.lines.some((l) => l.text.startsWith('AUTH'))).toBe(false);
  });
});
