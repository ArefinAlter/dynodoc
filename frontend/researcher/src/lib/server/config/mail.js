// Mail settings of the web server: which transport sends sign-in links and invitations, and
// the SMTP connection. Called by ./schema.js with its own problem and warning lists, so a bad
// mail setting is reported together with every other problem and never carries a secret.
// Pure like the rest of the module (it reads a CA file when SMTP_CA_FILE is set).
//
//   DYNODOC_MAIL_TRANSPORT  resend | smtp | none. Unset: resend when RESEND_API_KEY is set (what
//                           the hosted service has always done), otherwise none.
//   SMTP_HOST SMTP_PORT SMTP_SECURITY SMTP_USER SMTP_PASSWORD[_FILE] SMTP_FROM SMTP_FROM_NAME
//   SMTP_REPLY_TO SMTP_CA_FILE SMTP_TIMEOUT_SECONDS
//
// The sending limits (3 attempts, 10 recipients, 256 KiB, 1.2 as the lowest TLS version) are
// fixed in the mail code, not settings: nothing an operator should loosen.
import { X509Certificate } from 'node:crypto';
import { isIP } from 'node:net';
import {
  MAX_DISPLAY_NAME_LENGTH,
  domainOf,
  headerTextProblem,
  parseAddress
} from '../mail/address.js';
import { isLocalAddress } from '../mail/local-address.js';
import { readBoundedFile } from './secrets.js';

export const TRANSPORTS = ['resend', 'smtp', 'none'];
export const SECURITIES = ['tls', 'starttls', 'none-local'];
export const DEFAULT_PORTS = { tls: 465, starttls: 587, 'none-local': 25 };
export const SMTP_TIMEOUT_MIN = 5;
export const SMTP_TIMEOUT_MAX = 120;
export const SMTP_TIMEOUT_DEFAULT = 30;
export const CA_FILE_MAX_BYTES = 256 * 1024;
export const CA_MAX_CERTIFICATES = 100;

/** Every SMTP_* name some code reads; a name that resembles one of these gets a typo warning. */
export const SMTP_NAMES = [
  'SMTP_HOST',
  'SMTP_PORT',
  'SMTP_SECURITY',
  'SMTP_USER',
  'SMTP_PASSWORD',
  'SMTP_PASSWORD_FILE',
  'SMTP_FROM',
  'SMTP_FROM_NAME',
  'SMTP_REPLY_TO',
  'SMTP_CA_FILE',
  'SMTP_TIMEOUT_SECONDS'
];

/**
 * @typedef {object} SmtpConfig
 * @property {string} host
 * @property {number} port
 * @property {'tls' | 'starttls' | 'none-local'} security
 * @property {string | undefined} user
 * @property {import('./secrets.js').Secret | undefined} password
 * @property {{ name: string, address: string }} from
 * @property {string | undefined} replyTo
 * @property {string[] | undefined} ca PEM text of each certificate in SMTP_CA_FILE
 * @property {string | undefined} caFile the path, for messages
 * @property {number} timeoutMs
 * @property {string} helloName the name given in EHLO
 */
/**
 * @typedef {object} MailConfig
 * @property {'resend' | 'smtp' | 'none'} transport
 * @property {boolean} explicit DYNODOC_MAIL_TRANSPORT was set
 * @property {boolean} configured the transport can send
 * @property {SmtpConfig | null} smtp
 */

const shown = (/** @type {string} */ v) => (v.length > 40 ? `${v.slice(0, 40)}...` : v);

/** A DNS name or an IP address, without scheme, port, path or brackets. @param {string} host */
function hostProblem(host) {
  if (host.length > 253) return 'is longer than 253 characters';
  if (/^\[.*\]$/.test(host)) return 'must not be in brackets (write an IPv6 address bare)';
  if (isIP(host)) return undefined;
  if (!/^[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?$/.test(host) || host.includes('..'))
    return 'must be a host name or an IP address, with no scheme (smtp://), port, path or spaces';
  if (host.split('.').some((label) => label.length > 63))
    return 'has a label longer than 63 characters';
  return undefined;
}

/**
 * @param {string} name
 * @param {string} path
 * @param {string[]} problems
 * @param {string[]} warnings
 * @returns {string[] | undefined}
 */
function readCa(name, path, problems, warnings) {
  const file = readBoundedFile(name, path, {
    maxBytes: CA_FILE_MAX_BYTES,
    ownerOnly: false,
    allowOpen: true
  });
  if ('problem' in file) {
    problems.push(file.problem);
    return undefined;
  }
  const blocks = file.text.match(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g);
  if (!blocks) {
    problems.push(`${name} (${path}): holds no PEM certificate (-----BEGIN CERTIFICATE-----)`);
    return undefined;
  }
  if (blocks.length > CA_MAX_CERTIFICATES) {
    problems.push(`${name} (${path}): holds more than ${CA_MAX_CERTIFICATES} certificates`);
    return undefined;
  }
  const now = Date.now();
  for (const block of blocks) {
    try {
      const certificate = new X509Certificate(block);
      if (Date.parse(certificate.validTo) < now)
        warnings.push(
          `${name}: a certificate in it has expired (${shown(certificate.subject.replace(/\n/g, ', '))}); mail will fail if the server chain needs it`
        );
    } catch {
      problems.push(`${name} (${path}): holds a certificate that cannot be read`);
      return undefined;
    }
  }
  return blocks;
}

/**
 * @param {{
 *   lookup: (name: string) => string | undefined,
 *   get: (name: string) => string | undefined,
 *   secret: (name: string) => { secret: import('./secrets.js').Secret | undefined, failed: boolean },
 *   problems: string[],
 *   warnings: string[],
 *   self: boolean,
 *   devAuth: boolean,
 *   emailLink: boolean,
 *   brand: string,
 *   origin: string | undefined,
 *   resendKey: import('./secrets.js').Secret | undefined,
 *   emailFrom: string | undefined,
 *   inputsBroken: boolean
 * }} context
 * @returns {MailConfig}
 */
export function loadMail(context) {
  const { get, lookup, problems, warnings } = context;
  const before = problems.length;

  // --- Which transport ---
  let transport = /** @type {'resend' | 'smtp' | 'none'} */ (context.resendKey ? 'resend' : 'none');
  const rawTransport = get('DYNODOC_MAIL_TRANSPORT');
  const explicit = rawTransport !== undefined;
  if (explicit) {
    const value = rawTransport.trim().toLowerCase();
    if (TRANSPORTS.includes(value)) transport = /** @type {typeof transport} */ (value);
    else
      problems.push(
        `DYNODOC_MAIL_TRANSPORT must be resend, smtp or none (got "${shown(rawTransport)}")`
      );
  }

  // --- Settings of a transport that is not the one in use ---
  const smtpSet = SMTP_NAMES.filter((name) => get(name) !== undefined);
  if (transport !== 'smtp' && smtpSet.length)
    warnings.push(
      `${smtpSet.join(', ')} ${smtpSet.length === 1 ? 'is' : 'are'} set but the mail transport is ${transport}; the SMTP settings are ignored`
    );
  if (transport !== 'resend' && (context.resendKey || context.emailFrom))
    warnings.push(
      `RESEND_API_KEY / EMAIL_FROM are set but the mail transport is ${transport}; Resend is not used`
    );

  /** @type {SmtpConfig | null} */
  let smtp = null;
  let configured = false;

  if (transport === 'resend') {
    configured = Boolean(context.resendKey && context.emailFrom);
    if (explicit && !configured)
      problems.push(
        'DYNODOC_MAIL_TRANSPORT=resend needs RESEND_API_KEY (or RESEND_API_KEY_FILE) and EMAIL_FROM'
      );
  }

  if (transport === 'smtp') {
    const sectionStart = problems.length;
    const issue = (/** @type {string} */ text) => problems.push(text);

    const host = get('SMTP_HOST')?.trim();
    if (host === undefined)
      issue('SMTP_HOST is not set (the mail server, such as smtp.example.org)');
    else {
      const bad = hostProblem(host);
      if (bad) issue(`SMTP_HOST ${bad}`);
    }

    let security = /** @type {'tls' | 'starttls' | 'none-local'} */ ('starttls');
    const rawSecurity = get('SMTP_SECURITY');
    if (rawSecurity !== undefined) {
      const value = rawSecurity.trim().toLowerCase();
      if (SECURITIES.includes(value)) security = /** @type {typeof security} */ (value);
      else issue(`SMTP_SECURITY must be tls, starttls or none-local (got "${shown(rawSecurity)}")`);
    }

    let port = DEFAULT_PORTS[security];
    const rawPort = get('SMTP_PORT');
    if (rawPort !== undefined) {
      const value = rawPort.trim();
      if (/^\d{1,5}$/.test(value) && Number(value) >= 1 && Number(value) <= 65535)
        port = Number(value);
      else issue('SMTP_PORT must be a whole number from 1 to 65535');
    }

    if (security === 'none-local' && host !== undefined && !hostProblem(host)) {
      if (isIP(host) && !isLocalAddress(host))
        issue(
          `SMTP_HOST ${host} is not a loopback or private address; SMTP_SECURITY=none-local sends mail and any password without encryption and is allowed only for those (use tls or starttls)`
        );
    }

    // Login: both or neither.
    const rawUser = get('SMTP_USER');
    const user = rawUser === undefined ? undefined : rawUser.trim();
    if (user !== undefined) {
      const bad = headerTextProblem(user, 255);
      if (bad) issue(`SMTP_USER ${bad}`);
    }
    const { secret: password, failed } = context.secret('SMTP_PASSWORD');
    if (password && password.expose().includes(String.fromCharCode(0)))
      issue('SMTP_PASSWORD must not contain a NUL character');
    if (!failed) {
      if (user !== undefined && !password)
        issue(
          'SMTP_USER is set but there is no SMTP_PASSWORD (or SMTP_PASSWORD_FILE): set both or neither'
        );
      if (user === undefined && password)
        issue('SMTP_PASSWORD is set but there is no SMTP_USER: set both or neither');
    }
    if (security === 'none-local' && user !== undefined)
      warnings.push(
        `SMTP_SECURITY=none-local: ${host ?? 'the mail server'} receives the SMTP user name and password without encryption; use it only for this machine or a trusted private network`
      );

    // Sender.
    /** @type {{ name: string, address: string } | undefined} */
    let from;
    const rawFrom = get('SMTP_FROM');
    if (rawFrom === undefined)
      issue('SMTP_FROM is not set (the sender address, such as docs@example.org)');
    else {
      const parsed = parseAddress(rawFrom);
      if ('reason' in parsed) issue(`SMTP_FROM ${parsed.reason}`);
      else {
        const rawName = get('SMTP_FROM_NAME');
        const name = rawName === undefined ? context.brand : rawName.trim();
        const bad = headerTextProblem(name, MAX_DISPLAY_NAME_LENGTH);
        if (bad) issue(`SMTP_FROM_NAME ${bad}`);
        else from = { name, address: parsed.address };
      }
    }
    let replyTo;
    const rawReply = get('SMTP_REPLY_TO');
    if (rawReply !== undefined) {
      const parsed = parseAddress(rawReply);
      if ('reason' in parsed) issue(`SMTP_REPLY_TO ${parsed.reason}`);
      else replyTo = parsed.address;
    }

    // Trust and time.
    let ca;
    const caFile = get('SMTP_CA_FILE');
    if (caFile !== undefined) {
      if (security === 'none-local')
        warnings.push(
          'SMTP_CA_FILE is set but SMTP_SECURITY=none-local uses no TLS; it is ignored'
        );
      else ca = readCa('SMTP_CA_FILE', caFile, problems, warnings);
    }
    let timeoutSeconds = SMTP_TIMEOUT_DEFAULT;
    const rawTimeout = get('SMTP_TIMEOUT_SECONDS');
    if (rawTimeout !== undefined) {
      const value = rawTimeout.trim();
      if (
        /^\d{1,3}$/.test(value) &&
        Number(value) >= SMTP_TIMEOUT_MIN &&
        Number(value) <= SMTP_TIMEOUT_MAX
      )
        timeoutSeconds = Number(value);
      else
        issue(
          `SMTP_TIMEOUT_SECONDS must be a whole number from ${SMTP_TIMEOUT_MIN} to ${SMTP_TIMEOUT_MAX}`
        );
    }

    if (lookup('NODE_TLS_REJECT_UNAUTHORIZED') === '0')
      warnings.push(
        'NODE_TLS_REJECT_UNAUTHORIZED=0 is set: it switches off certificate checks for every TLS connection of this process except mail, which keeps checking; remove it'
      );

    if (problems.length === sectionStart && host !== undefined && from) {
      // The name given in EHLO: this installation's own host name if it has a real one.
      let helloName = domainOf(from.address);
      if (context.origin) {
        const originHost = new URL(context.origin).hostname;
        if (originHost.includes('.') && !isIP(originHost)) helloName = originHost;
      }
      smtp = {
        host,
        port,
        security,
        user,
        password,
        from,
        replyTo,
        ca,
        caFile,
        timeoutMs: timeoutSeconds * 1000,
        helloName
      };
      configured = true;
    }
  }

  // --- Email sign-in needs a way to send the link ---
  if (
    context.emailLink &&
    !configured &&
    !context.devAuth &&
    !context.inputsBroken &&
    problems.length === before
  ) {
    const message =
      'email sign-in is on but no mail transport is configured, so no one could sign in by email: set DYNODOC_MAIL_TRANSPORT=smtp with SMTP_HOST and SMTP_FROM, or turn email sign-in off with AUTH_EMAIL_LINK=off';
    (context.self ? problems : warnings).push(message);
  }

  return { transport, explicit, configured, smtp };
}
