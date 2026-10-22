// Turning a message request into the bytes that go to the mail server. Three stages, each able
// to refuse:
//   1. checkMessage: every address, name, subject and body is validated; anything with a CR, LF
//      or other control character in a header, more recipients than the cap, or an address
//      outside the documented subset (./address.js) is refused, not repaired.
//   2. composeMessage: nodemailer builds the MIME (multipart/alternative with a text and an HTML
//      part, UTF-8, RFC 2047 encoded words in headers, quoted-printable or base64 bodies). The
//      size is checked on the finished bytes.
//   3. lintMessage: the finished header block is parsed again and refused unless it holds only
//      the headers this module sets, each as one logical line. A hole in stage 1 would still
//      not let a header be injected into a message that is sent.
// The same bytes are then sent as they are, so what was checked is what goes out.
import { randomBytes } from 'node:crypto';
import { createTransport } from 'nodemailer';
import {
  MAX_DISPLAY_NAME_LENGTH,
  MAX_SUBJECT_LENGTH,
  bodyTextProblem,
  headerTextProblem,
  parseAddress
} from './address.js';
import { MailError } from './errors.js';

export const MAX_RECIPIENTS = 10;
export const MAX_MESSAGE_BYTES = 256 * 1024;
const MAX_BODY_CHARS = 100_000;

/**
 * @typedef {object} OutgoingMessage
 * @property {string | string[]} to
 * @property {string} subject
 * @property {string} text
 * @property {string} [html]
 * @property {string} [replyTo]
 */
/**
 * @typedef {object} CheckedMessage
 * @property {string[]} to
 * @property {string} subject
 * @property {string} text
 * @property {string | undefined} html
 * @property {string | undefined} replyTo
 */

/** @param {string} detail */
const refuse = (detail) => new MailError('invalid-message', { detail });

/**
 * Validate a message request. Messages name the field, never its content or an address.
 * @param {OutgoingMessage} message
 * @returns {CheckedMessage}
 */
export function checkMessage(message) {
  const list = Array.isArray(message.to) ? message.to : [message.to];
  if (list.length === 0) throw refuse('no recipient');
  if (list.length > MAX_RECIPIENTS) throw refuse(`more than ${MAX_RECIPIENTS} recipients`);
  /** @type {string[]} */ const to = [];
  list.forEach((raw, index) => {
    const parsed = parseAddress(raw);
    if ('reason' in parsed) throw refuse(`recipient ${index + 1} ${parsed.reason}`);
    if (!to.includes(parsed.address)) to.push(parsed.address);
  });
  const subjectProblem = headerTextProblem(message.subject, MAX_SUBJECT_LENGTH);
  if (subjectProblem || message.subject.trim() === '')
    throw refuse(`subject ${subjectProblem ?? 'is empty'}`);
  for (const [field, value] of /** @type {[string, unknown][]} */ ([
    ['text', message.text],
    ['html', message.html ?? '']
  ])) {
    const problem = bodyTextProblem(value);
    if (problem) throw refuse(`${field} ${problem}`);
    if (String(value).length > MAX_BODY_CHARS) throw refuse(`${field} is too long`);
  }
  if (message.text.trim() === '') throw refuse('text is empty');
  let replyTo;
  if (message.replyTo !== undefined && message.replyTo !== '') {
    const parsed = parseAddress(message.replyTo);
    if ('reason' in parsed) throw refuse(`reply-to ${parsed.reason}`);
    replyTo = parsed.address;
  }
  return {
    to,
    subject: message.subject.trim(),
    text: message.text,
    html: message.html || undefined,
    replyTo
  };
}

/**
 * Check the sender once, when the sender is created.
 * @param {{ name: string, address: string }} from
 */
export function checkSender(from) {
  const parsed = parseAddress(from.address);
  if ('reason' in parsed) throw refuse(`sender address ${parsed.reason}`);
  const problem = headerTextProblem(from.name, MAX_DISPLAY_NAME_LENGTH);
  if (problem) throw refuse(`sender name ${problem}`);
  return { name: from.name.trim(), address: parsed.address };
}

/** A Message-ID under the sender's own domain. @param {string} domain */
export const newMessageId = (domain) => `<${randomBytes(16).toString('hex')}@${domain}>`;

const ALLOWED_HEADERS = new Set([
  'from',
  'to',
  'reply-to',
  'subject',
  'message-id',
  'date',
  'mime-version',
  'content-type',
  'content-transfer-encoding',
  'auto-submitted'
]);

/**
 * Refuse a finished message whose header block is not exactly what this module builds.
 * @param {Buffer} raw
 * @param {string} messageId
 */
export function lintMessage(raw, messageId) {
  const text = raw.toString('latin1');
  const end = text.indexOf('\r\n\r\n');
  if (end < 0) throw refuse('the finished message has no header block');
  const head = text.slice(0, end);
  // Headers are 7-bit: non-ASCII text must have been encoded (RFC 2047) by now.
  // eslint-disable-next-line no-control-regex
  if (/[^\t\r\n -~]/.test(head)) throw refuse('a header holds raw non-ASCII or control text');
  const logical = head.replace(/\r\n[ \t]+/g, ' ').split('\r\n');
  /** @type {Map<string, string>} */ const seen = new Map();
  for (const line of logical) {
    const match = /^([!-9;-~]+):[ \t]*(.*)$/.exec(line);
    if (!match) throw refuse('a header line is malformed');
    const name = match[1].toLowerCase();
    if (!ALLOWED_HEADERS.has(name)) throw refuse(`unexpected header ${name}`);
    if (seen.has(name)) throw refuse(`header ${name} appears twice`);
    seen.set(name, match[2]);
  }
  for (const required of ['from', 'to', 'subject', 'message-id', 'date', 'mime-version'])
    if (!seen.has(required)) throw refuse(`header ${required} is missing`);
  if (seen.get('message-id') !== messageId) throw refuse('the Message-ID is not the one requested');
}

/**
 * Build and check the bytes of a message.
 * @param {{ name: string, address: string }} from
 * @param {string | undefined} configuredReplyTo
 * @param {CheckedMessage} message
 * @param {string} messageId
 * @returns {Promise<{ raw: Buffer, envelope: { from: string, to: string[] } }>}
 */
export async function composeMessage(from, configuredReplyTo, message, messageId) {
  const builder = createTransport({
    streamTransport: true,
    buffer: true,
    newline: 'windows',
    disableFileAccess: true,
    disableUrlAccess: true
  });
  // A Reply-To on the request (an invitation answers to the person who invited) wins over the
  // configured default (SMTP_REPLY_TO), which covers messages that have none.
  const replyTo = message.replyTo ?? configuredReplyTo;
  const info = /** @type {{ message: Buffer }} */ (
    await builder.sendMail({
      from: { name: from.name, address: from.address },
      to: message.to.map((address) => ({ name: '', address })),
      ...(replyTo ? { replyTo: { name: '', address: replyTo } } : {}),
      subject: message.subject,
      text: message.text,
      ...(message.html ? { html: message.html } : {}),
      messageId,
      headers: { 'Auto-Submitted': 'auto-generated' },
      envelope: { from: from.address, to: message.to }
    })
  );
  const raw = info.message;
  if (raw.length > MAX_MESSAGE_BYTES)
    throw new MailError('too-large', { detail: `limit ${MAX_MESSAGE_BYTES} bytes` });
  lintMessage(raw, messageId);
  return { raw, envelope: { from: from.address, to: message.to } };
}
