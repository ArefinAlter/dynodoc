// @vitest-environment node
// The mail settings: which transport, and every rule and error of the SMTP connection.
import { afterAll, describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { inspect } from 'node:util';
import { formatProblems, loadConfig } from './schema.js';
import { selfSigned } from '../mail/test-helpers/x509';

const SESSION = 'a3f1c9d27b6e4058912ce7d4b0a6f358a3f1c9d27b6e4058912ce7d4b0a6f358';
const SERVICE = '0e8b5d3c9a71f26480bd7e1c35a9f2640e8b5d3c9a71f26480bd7e1c35a9f264';
const dir = mkdtempSync(join(tmpdir(), 'dynodoc-mail-config-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
let counter = 0;
const file = (content: string) => {
  const path = join(dir, `f-${counter++}`);
  writeFileSync(path, content, { mode: 0o600 });
  return path;
};

type Env = Record<string, string | undefined>;
const base: Env = { SESSION_SECRET: SESSION, WEB_SERVICE_KEY: SERVICE };
const self: Env = {
  ...base,
  DYNODOC_MODE: 'selfhosted',
  ORIGIN: 'https://docs.example.org'
};
const smtp = (extra: Env = {}, start: Env = self): Env => ({
  ...start,
  DYNODOC_MAIL_TRANSPORT: 'smtp',
  SMTP_HOST: 'mail.example.org',
  SMTP_FROM: 'docs@example.org',
  ...extra
});
/** Demand exactly these problems, each once, as substrings. */
function problems(env: Env, ...expected: string[]) {
  const result = loadConfig(env);
  expect(result.problems, result.problems.join('\n')).toHaveLength(expected.length);
  for (const part of expected)
    expect(
      result.problems.filter((p) => p.includes(part)),
      `${part}\n${result.problems.join('\n')}`
    ).toHaveLength(1);
  return result;
}
const warnings = (env: Env) => loadConfig(env).warnings;
const warned = (env: Env, part: string) =>
  expect(
    warnings(env).some((w) => w.includes(part)),
    `${part}\n${warnings(env).join('\n')}`
  ).toBe(true);

describe('which transport', () => {
  it('is none when nothing is set, and resend when a Resend key is (the hosted default)', () => {
    expect(loadConfig({ ...base, AUTH_EMAIL_LINK: 'off' }).config.mail).toEqual({
      transport: 'none',
      explicit: false,
      configured: false,
      smtp: null
    });
    const resend = loadConfig({
      ...base,
      RESEND_API_KEY: 're_0123456789abcdef',
      EMAIL_FROM: 'Dynodoc <team@example.test>'
    });
    expect(resend.problems).toEqual([]);
    expect(resend.warnings).toEqual([]);
    expect(resend.config.mail).toMatchObject({ transport: 'resend', explicit: false, configured: true });
    // A key without a sender is, as before, a warning and not a working transport.
    const half = loadConfig({ ...base, RESEND_API_KEY: 're_0123456789abcdef' });
    expect(half.config.mail).toMatchObject({ transport: 'resend', configured: false });
    expect(half.warnings.join('\n')).toContain('RESEND_API_KEY and EMAIL_FROM need both');
  });
  it('can be chosen, case does not matter, anything else is an error', () => {
    expect(loadConfig(smtp({ DYNODOC_MAIL_TRANSPORT: ' SMTP ' })).config.mail.transport).toBe('smtp');
    expect(
      loadConfig({ ...base, DYNODOC_MAIL_TRANSPORT: 'none', AUTH_EMAIL_LINK: 'off' }).config.mail
    ).toMatchObject({ transport: 'none', explicit: true });
    problems(
      { ...base, DYNODOC_MAIL_TRANSPORT: 'sendmail' },
      'DYNODOC_MAIL_TRANSPORT must be resend, smtp or none (got "sendmail")'
    );
    problems(
      { ...base, DYNODOC_MAIL_TRANSPORT: 'smtps' },
      'DYNODOC_MAIL_TRANSPORT must be resend, smtp or none'
    );
  });
  it('insists that an explicit resend has its key and sender', () => {
    problems(
      { ...base, DYNODOC_MAIL_TRANSPORT: 'resend' },
      'DYNODOC_MAIL_TRANSPORT=resend needs RESEND_API_KEY'
    );
    expect(
      loadConfig({
        ...base,
        DYNODOC_MAIL_TRANSPORT: 'resend',
        RESEND_API_KEY_FILE: file('re_0123456789abcdef\n'),
        EMAIL_FROM: 'a@b.test'
      }).config.mail.configured
    ).toBe(true);
  });
  it('says when settings of another transport are set and will be ignored', () => {
    warned(
      { ...base, AUTH_EMAIL_LINK: 'off', SMTP_HOST: 'mail.example.org', SMTP_FROM: 'a@b.test' },
      'SMTP_HOST, SMTP_FROM are set but the mail transport is none; the SMTP settings are ignored'
    );
    warned(
      smtp({ RESEND_API_KEY: 're_0123456789abcdef', EMAIL_FROM: 'a@b.test' }),
      'Resend is not used'
    );
    warned(
      { ...base, DYNODOC_MAIL_TRANSPORT: 'none', RESEND_API_KEY: 're_0123456789abcdef', AUTH_EMAIL_LINK: 'off' },
      'the mail transport is none; Resend is not used'
    );
  });
});

describe('SMTP settings', () => {
  it('accepts a minimal setup and fills in the rest', () => {
    const { config, problems: found, warnings: warn } = loadConfig(smtp());
    expect(found).toEqual([]);
    expect(warn).toEqual([]);
    expect(config.mail.configured).toBe(true);
    expect(config.mail.smtp).toMatchObject({
      host: 'mail.example.org',
      port: 587,
      security: 'starttls',
      user: undefined,
      password: undefined,
      from: { name: 'Dynodoc', address: 'docs@example.org' },
      replyTo: undefined,
      ca: undefined,
      timeoutMs: 30_000,
      helloName: 'docs.example.org'
    });
  });
  it('picks the port from the security, and the sender name from the brand', () => {
    expect(loadConfig(smtp({ SMTP_SECURITY: 'tls' })).config.mail.smtp?.port).toBe(465);
    expect(
      loadConfig(smtp({ SMTP_SECURITY: 'none-local', SMTP_HOST: '127.0.0.1' })).config.mail.smtp?.port
    ).toBe(25);
    expect(loadConfig(smtp({ SMTP_PORT: '2525' })).config.mail.smtp?.port).toBe(2525);
    expect(
      loadConfig(smtp({ DYNODOC_BRAND_NAME: 'Acme Docs' })).config.mail.smtp?.from.name
    ).toBe('Acme Docs');
    expect(
      loadConfig(smtp({ DYNODOC_BRAND_NAME: 'Acme Docs', SMTP_FROM_NAME: 'Acme Records' })).config.mail
        .smtp?.from.name
    ).toBe('Acme Records');
  });
  it('requires a host and a sender', () => {
    problems(smtp({ SMTP_HOST: undefined }), 'SMTP_HOST is not set');
    problems(smtp({ SMTP_FROM: undefined }), 'SMTP_FROM is not set');
    problems(
      smtp({ SMTP_HOST: undefined, SMTP_FROM: undefined }),
      'SMTP_HOST is not set',
      'SMTP_FROM is not set'
    );
  });
  it('refuses a host that is not a plain name or address', () => {
    for (const host of [
      'smtp://mail.example.org',
      'mail.example.org:587',
      'mail.example.org/path',
      'user@mail.example.org',
      'mail .example.org',
      '[::1]',
      'mail..example.org',
      '-mail.example.org',
      `${'a'.repeat(64)}.example.org`,
      `${'a.'.repeat(130)}org`
    ])
      problems(smtp({ SMTP_HOST: host }), 'SMTP_HOST');
    for (const host of ['mail.example.org', 'localhost', '127.0.0.1', '::1', '203.0.113.7', 'relay'])
      expect(loadConfig(smtp({ SMTP_HOST: host })).problems, host).toEqual([]);
  });
  it('checks the security and the port', () => {
    problems(smtp({ SMTP_SECURITY: 'ssl' }), 'SMTP_SECURITY must be tls, starttls or none-local (got "ssl")');
    problems(smtp({ SMTP_SECURITY: 'none' }), 'SMTP_SECURITY must be');
    expect(loadConfig(smtp({ SMTP_SECURITY: ' TLS ' })).config.mail.smtp?.security).toBe('tls');
    for (const port of ['0', '65536', '-1', '25a', '1.5', 'abc', '587 587', '123456'])
      problems(smtp({ SMTP_PORT: port }), 'SMTP_PORT must be a whole number from 1 to 65535');
    for (const port of ['1', '25', '65535']) expect(loadConfig(smtp({ SMTP_PORT: port })).problems).toEqual([]);
  });
  it('needs a user name and a password together, and takes the password from a file', () => {
    problems(smtp({ SMTP_USER: 'sender' }), 'SMTP_USER is set but there is no SMTP_PASSWORD');
    problems(smtp({ SMTP_PASSWORD: 'secret-value' }), 'SMTP_PASSWORD is set but there is no SMTP_USER');
    const ok = loadConfig(smtp({ SMTP_USER: 'sender', SMTP_PASSWORD: 'p@ss word!' }));
    expect(ok.problems).toEqual([]);
    expect(ok.config.mail.smtp?.user).toBe('sender');
    expect(ok.config.mail.smtp?.password?.expose()).toBe('p@ss word!');
    const fromFile = loadConfig(smtp({ SMTP_USER: 'sender', SMTP_PASSWORD_FILE: file('from-a-file\r\n') }));
    expect(fromFile.problems).toEqual([]);
    expect(fromFile.config.mail.smtp?.password?.expose()).toBe('from-a-file');
  });
  it('reports a bad password file once, with the API rules, without the pairing complaint', () => {
    problems(
      smtp({ SMTP_USER: 'sender', SMTP_PASSWORD_FILE: join(dir, 'missing') }),
      'SMTP_PASSWORD_FILE'
    );
    problems(
      smtp({ SMTP_USER: 'sender', SMTP_PASSWORD_FILE: file('') }),
      'SMTP_PASSWORD_FILE'
    );
    problems(
      smtp({ SMTP_USER: 'sender', SMTP_PASSWORD: 'a', SMTP_PASSWORD_FILE: file('b') }),
      'SMTP_PASSWORD and SMTP_PASSWORD_FILE are both set'
    );
    problems(
      smtp({ SMTP_USER: 'sender', SMTP_PASSWORD: `pass${String.fromCharCode(0)}word` }),
      'SMTP_PASSWORD must not contain a NUL character'
    );
  });
  it('refuses a user name with a line break or control character', () => {
    problems(
      smtp({ SMTP_USER: 'sender\r\nRCPT TO:<x@example.org>', SMTP_PASSWORD: 'p' }),
      'SMTP_USER contains a line break or another control character'
    );
  });
  it('validates the sender, its name and the reply-to', () => {
    problems(smtp({ SMTP_FROM: 'Docs <docs@example.org>' }), 'SMTP_FROM has a local part with characters outside');
    problems(smtp({ SMTP_FROM: 'docs@localhost' }), 'SMTP_FROM has a domain that is not a plain DNS name');
    problems(
      smtp({ SMTP_FROM: 'docs@example.org\r\nBcc: x@example.org' }),
      'SMTP_FROM'
    );
    problems(smtp({ SMTP_FROM_NAME: 'Acme\r\nBcc: x@example.org' }), 'SMTP_FROM_NAME contains a line break');
    problems(smtp({ SMTP_FROM_NAME: 'x'.repeat(121) }), 'SMTP_FROM_NAME is longer than 120');
    problems(smtp({ SMTP_REPLY_TO: 'not an address' }), 'SMTP_REPLY_TO');
    expect(loadConfig(smtp({ SMTP_REPLY_TO: 'Help@Example.org' })).config.mail.smtp?.replyTo).toBe(
      'Help@example.org'
    );
  });
  it('bounds the timeout', () => {
    expect(loadConfig(smtp({ SMTP_TIMEOUT_SECONDS: '5' })).config.mail.smtp?.timeoutMs).toBe(5000);
    expect(loadConfig(smtp({ SMTP_TIMEOUT_SECONDS: '120' })).config.mail.smtp?.timeoutMs).toBe(120_000);
    for (const value of ['4', '121', '0', '-5', '10s', 'x', '1e2'])
      problems(smtp({ SMTP_TIMEOUT_SECONDS: value }), 'SMTP_TIMEOUT_SECONDS must be a whole number from 5 to 120');
  });
  it('gives EHLO the installation host name when it has a real one, else the sender domain', () => {
    expect(loadConfig(smtp()).config.mail.smtp?.helloName).toBe('docs.example.org');
    expect(
      loadConfig(smtp({ ORIGIN: 'http://localhost:3000' })).config.mail.smtp?.helloName
    ).toBe('example.org');
    expect(
      loadConfig(smtp({ ORIGIN: 'http://192.0.2.10:3000' })).config.mail.smtp?.helloName
    ).toBe('example.org');
    expect(loadConfig(smtp({ ORIGIN: undefined }, base)).config.mail.smtp?.helloName).toBe(
      'example.org'
    );
  });
  it('warns about a misspelt SMTP name, and knows the real ones', () => {
    warned(smtp({ SMTP_HOSTT: 'x' }), 'SMTP_HOSTT is not a known setting; did you mean SMTP_HOST?');
    warned(smtp({ SMTP_SECURTY: 'tls' }), 'did you mean SMTP_SECURITY?');
    expect(loadConfig(smtp({ SMTP_UNRELATED_THING: 'x' })).warnings).toEqual([]);
    expect(
      loadConfig(
        smtp({
          SMTP_PORT: '587',
          SMTP_USER: 'u',
          SMTP_PASSWORD_FILE: file('p'),
          SMTP_FROM_NAME: 'n',
          SMTP_REPLY_TO: 'r@example.org',
          SMTP_TIMEOUT_SECONDS: '30'
        })
      ).warnings
    ).toEqual([]);
  });
});

describe('plain text for a local relay', () => {
  const local = (extra: Env = {}) => smtp({ SMTP_SECURITY: 'none-local', SMTP_HOST: '127.0.0.1', ...extra });
  it('is allowed for loopback and private addresses and for names (checked when mail is sent)', () => {
    for (const host of ['127.0.0.1', '127.1.2.3', '10.0.0.5', '172.16.9.9', '192.168.1.1', '169.254.1.1', '100.64.0.1', '::1', 'fd12:3456::1', 'fe80::1', 'localhost', 'postfix'])
      expect(loadConfig(local({ SMTP_HOST: host })).problems, host).toEqual([]);
  });
  it('is refused for any other address, in the words the guide gives', () => {
    for (const host of ['203.0.113.7', '8.8.8.8', '172.32.0.1', '2001:db8::1', '100.128.0.1', '192.169.0.1'])
      problems(
        local({ SMTP_HOST: host }),
        `SMTP_HOST ${host} is not a loopback or private address; SMTP_SECURITY=none-local sends mail and any password without encryption and is allowed only for those (use tls or starttls)`
      );
  });
  it('warns, loudly, that a password goes out unencrypted', () => {
    const env = local({ SMTP_USER: 'sender', SMTP_PASSWORD: 'p' });
    warned(env, 'SMTP_SECURITY=none-local: 127.0.0.1 receives the SMTP user name and password without encryption');
    expect(warnings(local())).toEqual([]);
  });
  it('ignores a CA file, with a warning', () => {
    warned(local({ SMTP_CA_FILE: join(dir, 'missing') }), 'SMTP_CA_FILE is set but SMTP_SECURITY=none-local uses no TLS');
    expect(loadConfig(local({ SMTP_CA_FILE: join(dir, 'missing') })).problems).toEqual([]);
  });
});

describe('the certificate authority file', () => {
  const authority = selfSigned({ dns: ['mail.example.org'] }).cert;
  it('reads every certificate in a PEM file', () => {
    const second = selfSigned({ dns: ['other.example.org'] }).cert;
    const path = file(`# my authorities\n${authority}\n${second}`);
    const { config, problems: found, warnings: warn } = loadConfig(smtp({ SMTP_CA_FILE: path }));
    expect(found).toEqual([]);
    expect(warn).toEqual([]);
    expect(config.mail.smtp?.ca).toHaveLength(2);
    expect(config.mail.smtp?.ca?.[0]).toContain('BEGIN CERTIFICATE');
    expect(config.mail.smtp?.caFile).toBe(path);
  });
  it('reports a missing file, a file with no certificate, and a damaged one', () => {
    problems(smtp({ SMTP_CA_FILE: join(dir, 'nope') }), 'SMTP_CA_FILE');
    problems(smtp({ SMTP_CA_FILE: file('just text') }), 'holds no PEM certificate');
    problems(
      smtp({ SMTP_CA_FILE: file('-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n') }),
      'holds a certificate that cannot be read'
    );
    problems(smtp({ SMTP_CA_FILE: file('x'.repeat(256 * 1024 + 1)) }), 'is larger than 262144 bytes');
  });
  it('warns when an authority in the file has expired', () => {
    const expired = selfSigned({
      dns: ['old.example.org'],
      notBefore: new Date(Date.now() - 400 * 86_400_000),
      notAfter: new Date(Date.now() - 86_400_000)
    }).cert;
    const result = loadConfig(smtp({ SMTP_CA_FILE: file(expired) }));
    expect(result.problems).toEqual([]);
    expect(result.warnings.join('\n')).toContain('SMTP_CA_FILE: a certificate in it has expired');
  });
});

describe('the sign-in link needs mail', () => {
  const message = 'email sign-in is on but no mail transport is configured';
  it('stops a self-hosted server, and says both ways out', () => {
    const result = problems(self, message);
    expect(result.problems[0]).toContain('DYNODOC_MAIL_TRANSPORT=smtp');
    expect(result.problems[0]).toContain('AUTH_EMAIL_LINK=off');
  });
  it('is only a warning in the hosted mode', () => {
    const result = loadConfig(base);
    expect(result.problems).toEqual([]);
    expect(result.warnings.join('\n')).toContain(message);
  });
  it('is satisfied by SMTP, by Resend, by turning email sign-in off, and by the test stack switch', () => {
    expect(loadConfig(smtp()).problems).toEqual([]);
    expect(loadConfig({ ...self, RESEND_API_KEY: 're_0123456789abcdef', EMAIL_FROM: 'a@b.test' }).problems).toEqual([]);
    expect(loadConfig({ ...self, AUTH_EMAIL_LINK: 'off' }).problems).toEqual([]);
    expect(loadConfig({ ...self, AUTH_EMAIL_LINK: 'off' }).warnings).toEqual([]);
    const dev = loadConfig({ ...self, DEV_AUTH: 'true' });
    expect(dev.problems).toEqual([]);
    expect(dev.warnings.join('\n')).not.toContain(message);
  });
  it('does not repeat itself when the mail settings are already wrong', () => {
    const result = loadConfig(smtp({ SMTP_HOST: undefined }));
    expect(result.problems.filter((p) => p.includes(message))).toEqual([]);
    expect(
      loadConfig({ ...self, DYNODOC_MAIL_TRANSPORT: 'resend' }).problems.filter((p) => p.includes(message))
    ).toEqual([]);
    expect(
      loadConfig({ ...self, RESEND_API_KEY: 'a', RESEND_API_KEY_FILE: file('b') }).problems.filter((p) =>
        p.includes(message)
      )
    ).toEqual([]);
  });
});

describe('secrecy and the process environment', () => {
  it('never prints the password, in problems, warnings, JSON or inspect output', () => {
    const password = 'smtp-password-0123456789';
    const result = loadConfig(
      smtp({ SMTP_USER: 'sender', SMTP_PASSWORD: password, SMTP_PORT: 'bad', SMTP_HOSTT: 'x' })
    );
    const text = [formatProblems(result.problems), ...result.warnings].join('\n');
    expect(text).not.toContain(password);
    const good = loadConfig(smtp({ SMTP_USER: 'sender', SMTP_PASSWORD: password })).config;
    for (const shown of [JSON.stringify(good), inspect(good, { depth: 8 }), String(good.mail.smtp?.password)])
      expect(shown).not.toContain(password);
    expect(good.mail.smtp?.password?.expose()).toBe(password);
    expect(() => {
      (good.mail.smtp as { host: string }).host = 'elsewhere';
    }).toThrow();
  });
  it('warns that NODE_TLS_REJECT_UNAUTHORIZED=0 is set, and that mail is not affected', () => {
    warned(smtp({ NODE_TLS_REJECT_UNAUTHORIZED: '0' }), 'NODE_TLS_REJECT_UNAUTHORIZED=0 is set');
    expect(warnings(smtp({ NODE_TLS_REJECT_UNAUTHORIZED: '1' }))).toEqual([]);
  });
  it('reports every mail problem together with the others', () => {
    const result = loadConfig({
      ...self,
      SESSION_SECRET: 'short',
      DYNODOC_MAIL_TRANSPORT: 'smtp',
      SMTP_HOST: 'smtp://x',
      SMTP_PORT: '0',
      SMTP_SECURITY: 'ssl',
      SMTP_FROM: 'nope',
      SMTP_USER: 'u'
    });
    expect(result.problems).toHaveLength(6);
    expect(formatProblems(result.problems).startsWith('invalid configuration (6 problems):')).toBe(true);
  });
});
