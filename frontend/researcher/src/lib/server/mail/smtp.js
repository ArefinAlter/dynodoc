// The SMTP mail provider, built on nodemailer 10 (MIT-0, no dependencies; the decision is in
// docs/SELF-HOSTED-DEPLOYMENT-GUIDE.md 7.2). nodemailer speaks the protocol; this file decides
// everything that matters for safety:
//   - how the connection is secured. `tls` is TLS from the first byte; `starttls` upgrades and
//     REFUSES to continue if the server does not offer it (requireTLS), so a server that
//     leaves STARTTLS out cannot make us send a password in clear; `none-local` is plaintext,
//     allowed only when every address the host resolves to is loopback or private. Certificate
//     and host name are always verified, TLS 1.2 is the minimum, and SMTP_CA_FILE replaces the
//     built-in list of authorities.
//   - time: connect, greeting and command timeouts, and one budget for the whole send.
//   - retries (./retry.js): 4xx replies and network errors only, a few times, with jitter.
//   - what is logged: provider, outcome, a short error class and the message id. Never a
//     password, a recipient, a subject or the server's reply text.
import { BlockList, isIP } from 'node:net';
import { lookup as dnsLookup } from 'node:dns/promises';
import { Readable } from 'node:stream';
import { createTransport } from 'nodemailer';
import SMTPConnection from 'nodemailer/lib/smtp-connection';
import { checkMessage, checkSender, composeMessage, newMessageId } from './message.js';
import { domainOf } from './address.js';
import { MailError, classifyError, printable } from './errors.js';
import { withRetry } from './retry.js';

/**
 * @typedef {object} SmtpSettings
 * @property {string} host
 * @property {number} port
 * @property {'tls' | 'starttls' | 'none-local'} security
 * @property {string | undefined} user
 * @property {string | undefined} password the password text (the caller exposes the Secret)
 * @property {{ name: string, address: string }} from
 * @property {string | undefined} replyTo
 * @property {string[] | undefined} ca PEM text of the authorities to trust instead of the built-in list
 * @property {number} timeoutMs the budget for one send, retries included
 * @property {string} helloName the name given in EHLO
 */

const PRIVATE = new BlockList();
for (const [network, prefix] of /** @type {[string, number][]} */ ([
  ['127.0.0.0', 8],
  ['10.0.0.0', 8],
  ['172.16.0.0', 12],
  ['192.168.0.0', 16],
  ['169.254.0.0', 16],
  ['100.64.0.0', 10]
]))
  PRIVATE.addSubnet(network, prefix, 'ipv4');
PRIVATE.addAddress('::1', 'ipv6');
PRIVATE.addSubnet('fc00::', 7, 'ipv6');
PRIVATE.addSubnet('fe80::', 10, 'ipv6');

/**
 * Loopback, private (RFC 1918, unique-local), link-local and shared (100.64/10) addresses.
 * @param {string} address
 */
export function isLocalAddress(address) {
  const family = isIP(address);
  if (family === 0) return false;
  const mapped = /^::ffff:(\d+\.\d+\.\d+\.\d+)$/i.exec(address);
  if (mapped) return PRIVATE.check(mapped[1], 'ipv4');
  return PRIVATE.check(address, family === 4 ? 'ipv4' : 'ipv6');
}

/**
 * The address to connect to for `none-local`: every address the host resolves to must be local,
 * and the connection goes to the one that was checked (no second lookup to be fooled by).
 * @param {string} host
 * @param {(host: string) => Promise<{ address: string }[]>} resolve
 */
export async function resolveLocalOnly(
  host,
  resolve = (name) => dnsLookup(name, { all: true, verbatim: true })
) {
  /** @type {{ address: string }[]} */
  let found;
  if (isIP(host)) found = [{ address: host }];
  else {
    try {
      found = await resolve(host);
    } catch (error) {
      throw classifyError(error);
    }
  }
  if (found.length === 0) throw new MailError('dns');
  if (!found.every((entry) => isLocalAddress(entry.address)))
    throw new MailError('plaintext-refused');
  return found[0].address;
}

/**
 * The nodemailer options for one connection.
 * @param {SmtpSettings} settings
 * @param {string} connectHost
 * @param {number} remainingMs
 */
export function transportOptions(settings, connectHost, remainingMs) {
  const within = (/** @type {number} */ cap) => Math.max(1000, Math.min(cap, remainingMs));
  const encrypted = settings.security !== 'none-local';
  return {
    host: connectHost,
    port: settings.port,
    secure: settings.security === 'tls',
    requireTLS: settings.security === 'starttls',
    ignoreTLS: settings.security === 'none-local',
    name: settings.helloName,
    ...(settings.user && settings.password !== undefined
      ? { auth: { user: settings.user, pass: settings.password } }
      : {}),
    connectionTimeout: within(10_000),
    greetingTimeout: within(10_000),
    socketTimeout: within(15_000),
    dnsTimeout: within(5_000),
    ...(encrypted
      ? {
          tls: {
            minVersion: /** @type {const} */ ('TLSv1.2'),
            rejectUnauthorized: true,
            ...(settings.ca ? { ca: settings.ca } : {})
          }
        }
      : {}),
    disableFileAccess: true,
    disableUrlAccess: true,
    logger: false,
    debug: false
  };
}

/**
 * @template T
 * @param {Promise<T>} promise
 * @param {number} ms
 * @param {() => void} onTimeout
 * @returns {Promise<T>}
 */
function within(promise, ms, onTimeout) {
  /** @type {ReturnType<typeof setTimeout>} */ let timer;
  const limit = new Promise((_, reject) => {
    timer = setTimeout(() => {
      onTimeout();
      reject(Object.assign(new Error('the send ran out of time'), { code: 'ETIMEDOUT' }));
    }, ms);
  });
  return Promise.race([promise, limit]).finally(() => clearTimeout(timer)).then((value) => /** @type {T} */ (value));
}

/** @typedef {{ provider: string, outcome: 'sent' | 'failed' | 'retry', kind?: string, errorClass?: string, messageId?: string, attempts?: number }} LogEntry */
/** One line, key=value, nothing that identifies a person or a secret. @param {LogEntry} entry */
export function formatLog(entry) {
  const parts = [`provider=${entry.provider}`, `outcome=${entry.outcome}`];
  if (entry.kind) parts.push(`kind=${entry.kind}`);
  if (entry.errorClass) parts.push(`class=${entry.errorClass}`);
  if (entry.attempts) parts.push(`attempts=${entry.attempts}`);
  if (entry.messageId) parts.push(`id=${entry.messageId}`);
  return `[dynodoc] mail ${parts.join(' ')}`;
}
/** @param {LogEntry} entry */
const defaultLog = (entry) =>
  (entry.outcome === 'sent' ? console.info : console.warn)(formatLog(entry));

/**
 * @param {SmtpSettings} settings
 * @param {{
 *   log?: (entry: LogEntry) => void,
 *   now?: () => number,
 *   sleep?: (ms: number) => Promise<void>,
 *   random?: () => number,
 *   resolve?: (host: string) => Promise<{ address: string }[]>
 * }} [deps]
 */
export function createSmtpSender(settings, deps = {}) {
  const from = checkSender(settings.from);
  const log = deps.log ?? defaultLog;
  const messageIdDomain = domainOf(from.address);

  const target = async () =>
    settings.security === 'none-local'
      ? resolveLocalOnly(settings.host, deps.resolve)
      : settings.host;

  /**
   * @param {{ raw: Buffer, envelope: { from: string, to: string[] } }} built
   * @param {number} remainingMs
   */
  async function deliver(built, remainingMs) {
    const connectHost = await target();
    const transport = createTransport(transportOptions(settings, connectHost, remainingMs));
    try {
      const info = await within(
        transport.sendMail({ raw: built.raw, envelope: built.envelope }),
        remainingMs,
        () => transport.close()
      );
      if (info.rejected?.length || !info.accepted?.length)
        throw new MailError('recipient-refused');
      return info;
    } finally {
      transport.close();
    }
  }

  return {
    /**
     * Send one message. Resolves with its Message-ID; rejects with a MailError.
     * @param {import('./message.js').OutgoingMessage & { kind?: string }} message
     */
    async send(message) {
      const messageId = newMessageId(messageIdDomain);
      let attempts = 0;
      try {
        const checked = checkMessage(message);
        const built = await composeMessage(from, settings.replyTo, checked, messageId);
        const result = await withRetry(({ remainingMs }) => deliver(built, remainingMs), {
          budgetMs: settings.timeoutMs,
          now: deps.now,
          sleep: deps.sleep,
          random: deps.random,
          onRetry: ({ attempt, errorClass }) =>
            log({
              provider: 'smtp',
              outcome: 'retry',
              kind: message.kind,
              errorClass,
              messageId,
              attempts: attempt
            })
        });
        attempts = result.attempts;
        log({ provider: 'smtp', outcome: 'sent', kind: message.kind, messageId, attempts });
        return { messageId };
      } catch (raw) {
        const error = classifyError(raw);
        log({
          provider: 'smtp',
          outcome: 'failed',
          kind: message.kind,
          errorClass: error.errorClass,
          messageId,
          attempts: /** @type {any} */ (error).attempts ?? attempts
        });
        throw error;
      }
    },

    /**
     * Talk to the server as a send would, up to and including MAIL FROM and RCPT TO, without
     * delivering anything: the connection is dropped when the server asks for the message
     * text, which a server must treat as no message. Each step is reported with its outcome.
     * @param {{ to?: string }} [options]
     * @returns {Promise<{ ok: boolean, steps: { name: string, ok: boolean, detail?: string }[], error?: MailError }>}
     */
    async probe(options = {}) {
      /** @type {{ name: string, ok: boolean, detail?: string }[]} */ const steps = [];
      const fail = (/** @type {string} */ name, /** @type {unknown} */ raw) => {
        const error = classifyError(raw);
        steps.push({
          name,
          ok: false,
          detail: error.serverResponse ? `server said: ${error.serverResponse}` : undefined
        });
        return { ok: false, steps, error };
      };
      let connectHost;
      try {
        connectHost = await target();
      } catch (error) {
        return fail('resolve the server address', error);
      }
      const connection = new SMTPConnection(
        /** @type {any} */ (transportOptions(settings, connectHost, settings.timeoutMs))
      );
      connection.on('error', () => undefined);
      try {
        await new Promise((resolve, reject) => {
          connection.once('error', reject);
          connection.connect(() => {
            connection.removeListener('error', reject);
            resolve(undefined);
          });
        });
      } catch (error) {
        connection.close();
        return fail('connect, secure the connection and say EHLO', error);
      }
      const protocol = /** @type {any} */ (connection)._socket?.getProtocol?.();
      steps.push({
        name: 'connect, secure the connection and say EHLO',
        ok: true,
        detail:
          settings.security === 'none-local'
            ? 'unencrypted by choice (none-local, a loopback or private address)'
            : `encrypted${protocol ? ` with ${protocol}` : ''}, certificate verified`
      });
      try {
        if (settings.user && settings.password !== undefined) {
          if (!connection.allowsAuth) {
            steps.push({
              name: 'authenticate',
              ok: false,
              detail: 'the server does not offer AUTH (after encryption); no password was sent'
            });
            connection.close();
            return {
              ok: false,
              steps,
              error: new MailError('auth-required', { detail: 'the server offers no AUTH' })
            };
          }
          await new Promise((resolve, reject) =>
            connection.login({ user: settings.user, pass: settings.password }, (error) =>
              error ? reject(error) : resolve(undefined)
            )
          );
          steps.push({ name: 'authenticate', ok: true });
        } else steps.push({ name: 'authenticate', ok: true, detail: 'no SMTP_USER set; skipped' });
        if (options.to) {
          let reached = false;
          /** @type {Readable} */
          const never = new Readable({
            read() {
              reached = true;
              connection.close();
            }
          });
          await new Promise((resolve, reject) =>
            connection.send({ from: from.address, to: [options.to ?? ''] }, never, (error) => {
              if (reached) resolve(undefined);
              else reject(error ?? new MailError('unknown'));
            })
          );
          steps.push({
            name: 'MAIL FROM and RCPT TO',
            ok: true,
            detail: 'accepted; the connection was dropped before any message text was sent'
          });
        } else
          steps.push({
            name: 'MAIL FROM and RCPT TO',
            ok: true,
            detail: 'not checked (give --to an address to try)'
          });
        connection.close();
        return { ok: true, steps };
      } catch (error) {
        connection.close();
        const classified = classifyError(error);
        const during = classified.errorClass.startsWith('auth')
          ? 'authenticate'
          : 'MAIL FROM and RCPT TO';
        return fail(during, error);
      }
    },

    messageIdDomain,
    /** Printable settings for diagnostics: never the password. */
    describe: () =>
      printable(
        `${settings.security} ${settings.host}:${settings.port}${settings.user ? ' with a login' : ''}`
      )
  };
}
