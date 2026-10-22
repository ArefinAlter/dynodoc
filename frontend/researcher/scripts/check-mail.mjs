// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-nocheck -- a command-line tool, exercised by its tests; the typed code is src/lib/server/mail.
// Checks the mail settings of an installation against its real mail server, without starting
// the web server.
//
//   node scripts/check-mail.mjs --dir <dir> [--to someone@example.org] [--send] [--json]
//                                [--use-process-env]
//
// It reads <dir>/.env with the same validator the server runs at start, then does what a send
// does: connect, secure the connection (TLS, or STARTTLS, which it insists on), say EHLO, log in,
// and with --to give MAIL FROM and RCPT TO. It stops there: the connection is dropped when the
// server asks for the message text, so nothing is delivered. With --send (and --to) it then
// sends one short test message to that address. Every step is reported in plain words; a
// failure names the likely cause, for example "TLS certificate not trusted: set SMTP_CA_FILE".
// It never prints a password. The server's own words are shown for a refusal (they are the
// operator's to read, and may quote the --to address); the web server's log never has them.
//
// Exit code: 0 the check passed, 1 it failed, 2 it could not run (bad usage, no .env),
// 3 nothing to check (the mail transport is not SMTP). `--json` prints one object for tools.
// `dynodoctl` is meant to call this.
import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadConfig } from '../src/lib/server/config/schema.js';
import { parseEnvFile } from './dynodoc-env.mjs';

const flag = (args, name) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : undefined;
};

/**
 * @param {{ dir?: string, to?: string, send?: boolean, useProcessEnv?: boolean }} options
 * @param {{ createSmtpSender?: Function }} [deps] a stand-in for the sender, for tests
 */
export async function checkMail(options, deps = {}) {
  if (!options.dir) throw new Error('--dir is required.');
  const dir = resolve(options.dir);
  const envPath = join(dir, '.env');
  if (!existsSync(envPath)) throw new Error(`No .env in ${dir}`);
  if (options.send && !options.to)
    throw new Error('--send needs --to (where to send the test message).');
  const { values, problems: syntax } = parseEnvFile(readFileSync(envPath, 'utf8'));
  const config = loadConfig({ ...(options.useProcessEnv ? process.env : {}), ...values });
  const result = {
    ok: false,
    transport: config.config.mail.transport,
    configured: false,
    problems: [...syntax, ...config.problems],
    steps: [],
    sent: null,
    error: null
  };
  if (result.problems.length) return { ...result, exit: 1 };
  const smtp = config.config.mail.smtp;
  if (config.config.mail.transport !== 'smtp' || !smtp)
    return {
      ...result,
      exit: 3,
      note: `The mail transport is ${config.config.mail.transport}, not smtp: there is no SMTP server to check.`
    };
  result.configured = true;
  result.server = `${smtp.security} ${smtp.host}:${smtp.port}${smtp.user ? ' with a login' : ''}`;

  const createSmtpSender =
    deps.createSmtpSender ?? (await import('../src/lib/server/mail/smtp.js')).createSmtpSender;
  const sender = createSmtpSender(
    {
      host: smtp.host,
      port: smtp.port,
      security: smtp.security,
      user: smtp.user,
      password: smtp.password?.expose(),
      from: smtp.from,
      replyTo: smtp.replyTo,
      ca: smtp.ca ? [...smtp.ca] : undefined,
      timeoutMs: smtp.timeoutMs,
      helloName: smtp.helloName
    },
    { log: () => undefined }
  );
  const probe = await sender.probe({ to: options.to });
  result.steps = probe.steps;
  if (!probe.ok) {
    result.error = { class: probe.error.errorClass, message: probe.error.message };
    return { ...result, exit: 1 };
  }
  if (options.send) {
    try {
      const brand = config.config.site.brand;
      const { messageId } = await sender.send({
        kind: 'test',
        to: options.to,
        subject: `${brand}: test of the mail settings`,
        text: `This is a test message from ${brand}, sent by the mail check on ${new Date().toISOString().slice(0, 10)}.\n\nIf you can read it, outgoing mail works. You do not need to reply.`
      });
      result.sent = { messageId };
      result.steps = [
        ...result.steps,
        { name: 'send a test message', ok: true, detail: `accepted by the server as ${messageId}` }
      ];
    } catch (error) {
      result.steps = [
        ...result.steps,
        {
          name: 'send a test message',
          ok: false,
          detail: error.serverResponse ? `server said: ${error.serverResponse}` : undefined
        }
      ];
      result.error = { class: error.errorClass ?? 'unknown', message: error.message };
      return { ...result, exit: 1 };
    }
  }
  return { ...result, ok: true, exit: 0 };
}

export function render(result, options = {}) {
  const lines = [];
  if (result.problems.length) {
    lines.push(
      `invalid configuration (${result.problems.length} problem${result.problems.length === 1 ? '' : 's'}); the mail check needs a valid one:`,
      ...result.problems.map((p) => `  - ${p}`)
    );
    return lines.join('\n');
  }
  if (result.note) return result.note;
  lines.push(`Mail check: ${result.server}`);
  for (const step of result.steps)
    lines.push(
      `  ${step.ok ? 'ok  ' : 'FAIL'}  ${step.name}${step.detail ? `: ${step.detail}` : ''}`
    );
  if (result.error) lines.push('', `Problem: ${result.error.message}`);
  else if (result.sent)
    lines.push(
      '',
      'Result: the mail server took the test message. Ask the recipient to confirm it arrived.'
    );
  else
    lines.push(
      '',
      options.to
        ? 'Result: the server accepted the connection, the login and the recipient. Nothing was sent. Add --send to send a test message.'
        : 'Result: the server accepted the connection and the login. Nothing was sent. Add --to <address> to try a recipient too.'
    );
  return lines.join('\n');
}

async function main(argv) {
  const args = argv;
  try {
    const options = {
      dir: flag(args, '--dir'),
      to: flag(args, '--to'),
      send: args.includes('--send'),
      useProcessEnv: args.includes('--use-process-env')
    };
    const result = await checkMail(options);
    if (args.includes('--json')) {
      const { exit: _exit, ...printable } = result;
      console.log(JSON.stringify(printable));
    } else console.log(render(result, options));
    return result.exit;
  } catch (error) {
    console.error(error.message);
    console.error(
      'usage: check-mail.mjs --dir <dir> [--to <address>] [--send] [--json] [--use-process-env]'
    );
    return 2;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).then((code) => {
    process.exitCode = code;
  });
}
