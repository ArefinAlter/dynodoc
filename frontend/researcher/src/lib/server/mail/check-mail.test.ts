// @vitest-environment node
// scripts/check-mail.mjs: the connection check for an installation's mail settings, as a
// function and as the command `dynodoctl` will run.
import { afterAll, afterEach, describe, expect, it, vi } from 'vitest';
import { execFile } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkMail, render } from '../../../../scripts/check-mail.mjs';
import { generateEnv } from '../../../../scripts/dynodoc-env.mjs';
import { startFakeSmtp, type FakeSmtp, type FakeSmtpOptions } from './test-helpers/fake-smtp';
import { selfSigned } from './test-helpers/x509';

vi.setConfig({ testTimeout: 30_000 });

const SCRIPT = fileURLToPath(new URL('../../../../scripts/check-mail.mjs', import.meta.url));
const PASSWORD = 'relay password for the check';
const cert = selfSigned({ ip: ['127.0.0.1'] });
const root = mkdtempSync(join(tmpdir(), 'dynodoc-check-mail-'));
afterAll(() => rmSync(root, { recursive: true, force: true }));
const running: FakeSmtp[] = [];
afterEach(async () => {
  await Promise.all(running.splice(0).map((s) => s.close()));
});
let counter = 0;

/** An installation directory whose mail settings point at a fake server. */
async function install(
  serverOptions: FakeSmtpOptions = {},
  settings: { trust?: boolean; password?: string; mail?: boolean } = {}
) {
  const server = await startFakeSmtp({
    tls: cert,
    authMethods: ['PLAIN'],
    credentials: { user: 'sender', pass: PASSWORD },
    requireAuth: true,
    ...serverOptions
  });
  running.push(server);
  const dir = join(root, `install-${counter++}`);
  const caFile = join(root, `ca-${counter}.pem`);
  writeFileSync(caFile, cert.cert);
  generateEnv({
    dir,
    origin: 'https://docs.example.org',
    ...(settings.mail === false
      ? {}
      : {
          smtpHost: '127.0.0.1',
          smtpPort: String(server.port),
          smtpFrom: 'docs@example.org',
          smtpUser: 'sender',
          ...(settings.trust === false ? {} : { smtpCaFile: caFile })
        })
  });
  if (settings.mail !== false)
    writeFileSync(join(dir, 'secrets', 'smtp_password'), `${settings.password ?? PASSWORD}\n`, {
      mode: 0o600
    });
  return { server, dir };
}
const text = (result: unknown, options = {}) => render(result as never, options);

describe('checkMail', () => {
  it('passes a working server without sending anything, and prints no password', async () => {
    const { server, dir } = await install();
    const result = await checkMail({ dir, to: 'olivia@example.test' });
    expect(result).toMatchObject({ ok: true, exit: 0, transport: 'smtp' });
    expect(result.steps.map((s: { name: string }) => s.name)).toEqual([
      'connect, secure the connection and say EHLO',
      'authenticate',
      'MAIL FROM and RCPT TO'
    ]);
    expect(server.messages).toEqual([]);
    const printed = text(result, { to: 'olivia@example.test' });
    expect(printed).toContain('Mail check: starttls 127.0.0.1:');
    expect(printed).toContain(
      'ok    connect, secure the connection and say EHLO: encrypted with TLSv1.'
    );
    expect(printed).toContain('Nothing was sent. Add --send to send a test message.');
    expect(printed + JSON.stringify(result)).not.toContain(PASSWORD);
  });
  it('checks only the connection and login without --to, and says what to add', async () => {
    const { dir } = await install();
    const result = await checkMail({ dir });
    expect(result.exit).toBe(0);
    expect(text(result)).toContain('Add --to <address> to try a recipient too.');
  });
  it('sends one test message with --send, to that address only', async () => {
    const { server, dir } = await install();
    const result = await checkMail({ dir, to: 'olivia@example.test', send: true });
    expect(result).toMatchObject({ ok: true, exit: 0 });
    expect(server.messages).toHaveLength(1);
    expect(server.messages[0].to).toEqual(['olivia@example.test']);
    expect(server.messages[0].data).toContain('test of the mail settings');
    expect((result.sent as { messageId: string } | null)?.messageId).toMatch(
      /^<[0-9a-f]{32}@example\.org>$/
    );
    expect(text(result)).toContain('the mail server took the test message');
  });
  it('refuses --send without --to', async () => {
    const { dir } = await install();
    await expect(checkMail({ dir, send: true })).rejects.toThrow('--send needs --to');
  });
  it.each([
    [
      'a certificate it does not trust',
      {},
      { trust: false },
      'TLS certificate not trusted: set SMTP_CA_FILE'
    ],
    [
      'a wrong password',
      {},
      { password: 'not it' },
      'authentication failed: check SMTP_USER and SMTP_PASSWORD'
    ],
    [
      'a relay that refuses',
      { reply: { rcpt: () => '554 5.7.1 <olivia@example.test>: Relay access denied' } },
      {},
      'relay refused'
    ],
    [
      'a server without STARTTLS',
      { tls: undefined },
      {},
      'the server does not offer STARTTLS, so the connection would not be encrypted and nothing was sent'
    ]
  ] as [string, FakeSmtpOptions, Record<string, unknown>, string][])(
    'diagnoses %s in plain words',
    async (_name, serverOptions, settings, diagnosis) => {
      const { server, dir } = await install(serverOptions, settings);
      const result = await checkMail({ dir, to: 'olivia@example.test' });
      expect(result.exit).toBe(1);
      expect(result.ok).toBe(false);
      const printed = text(result, { to: 'olivia@example.test' });
      expect(printed).toContain(`Problem: ${diagnosis}`);
      expect(printed).toContain('FAIL');
      expect(printed + JSON.stringify(result)).not.toContain(PASSWORD);
      expect(server.messages).toEqual([]);
    }
  );
  it('shows the server’s own words for a refusal', async () => {
    const { dir } = await install({ reply: { rcpt: () => '554 5.7.1 Relay access denied' } });
    const printed = text(await checkMail({ dir, to: 'olivia@example.test' }));
    expect(printed).toContain('server said: 554 5.7.1 Relay access denied');
  });
  it('says there is nothing to check when mail is not SMTP, and stops on an invalid configuration', async () => {
    const { dir } = await install({}, { mail: false });
    const off = await checkMail({ dir });
    expect(off.exit).toBe(3);
    expect(text(off)).toBe(
      'The mail transport is none, not smtp: there is no SMTP server to check.'
    );
    const broken = await install();
    writeFileSync(join(broken.dir, 'secrets', 'smtp_password'), '');
    const invalid = await checkMail({ dir: broken.dir });
    expect(invalid.exit).toBe(1);
    expect(text(invalid)).toMatch(
      /invalid configuration \(1 problem\); the mail check needs a valid one:\n {2}- SMTP_PASSWORD_FILE .*is empty/
    );
  });
  it('needs a directory with a .env', async () => {
    await expect(checkMail({})).rejects.toThrow('--dir is required');
    await expect(checkMail({ dir: join(root, 'nowhere') })).rejects.toThrow('No .env in');
  });
});

describe('the command', () => {
  // Asynchronous on purpose: the fake mail server runs in this process and must keep answering.
  const run = (args: string[]) =>
    new Promise<{ status: number | null; stdout: string; stderr: string }>((resolve) =>
      execFile(
        process.execPath,
        [SCRIPT, ...args],
        { encoding: 'utf8', timeout: 60_000 },
        (error, stdout, stderr) =>
          resolve({ status: error ? ((error as { code?: number }).code ?? 1) : 0, stdout, stderr })
      )
    );
  it('prints the check and exits 0, or 1 with the diagnosis', async () => {
    const good = await install();
    const ok = await run(['--dir', good.dir, '--to', 'olivia@example.test']);
    expect(ok.status, ok.stderr).toBe(0);
    expect(ok.stdout).toContain(
      'Result: the server accepted the connection, the login and the recipient.'
    );
    const bad = await install({}, { password: 'wrong' });
    const failed = await run(['--dir', bad.dir]);
    expect(failed.status).toBe(1);
    expect(failed.stdout).toContain('Problem: authentication failed');
    for (const out of [ok.stdout + ok.stderr, failed.stdout + failed.stderr])
      expect(out).not.toContain(PASSWORD);
  });
  it('prints one JSON object for tools with --json', async () => {
    const { dir } = await install();
    const json = await run(['--dir', dir, '--to', 'olivia@example.test', '--json']);
    expect(json.status).toBe(0);
    const parsed = JSON.parse(json.stdout);
    expect(parsed).toMatchObject({ ok: true, transport: 'smtp', configured: true, error: null });
    expect(parsed.steps).toHaveLength(3);
    expect(json.stdout).not.toContain(PASSWORD);
  });
  it('exits 2 for bad usage and 3 when mail is not SMTP', async () => {
    expect((await run([])).status).toBe(2);
    expect((await run(['--dir', join(root, 'nowhere')])).stderr).toContain('No .env in');
    const { dir } = await install({}, { mail: false });
    expect((await run(['--dir', dir])).status).toBe(3);
    expect((await run(['--dir', dir, '--send'])).status).toBe(2);
  });
  it('is the same command the generated .env points to', async () => {
    const { dir } = await install({}, { mail: false });
    expect(readFileSync(join(dir, '.env'), 'utf8')).toContain(
      'node scripts/check-mail.mjs --dir <this directory> --to you@example.org'
    );
  });
});
