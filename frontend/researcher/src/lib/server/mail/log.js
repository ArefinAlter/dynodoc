// What the server logs about mail: the provider, the outcome, a short error class and the message
// id, as one line of key=value. Nothing that identifies a person (no recipient, no subject, no
// text of a reply from the mail server) and nothing secret ever goes through here.

/**
 * @typedef {object} LogEntry
 * @property {string} provider
 * @property {'sent' | 'failed' | 'retry'} outcome
 * @property {string} [kind] sign-in link, invitation or test
 * @property {string} [errorClass]
 * @property {string} [messageId]
 * @property {number} [attempts]
 */

/** @param {LogEntry} entry */
export function formatLog(entry) {
  const parts = [`provider=${entry.provider}`, `outcome=${entry.outcome}`];
  if (entry.kind) parts.push(`kind=${entry.kind}`);
  if (entry.errorClass) parts.push(`class=${entry.errorClass}`);
  if (entry.attempts) parts.push(`attempts=${entry.attempts}`);
  if (entry.messageId) parts.push(`id=${entry.messageId}`);
  return `[dynodoc] mail ${parts.join(' ')}`;
}

/** @param {LogEntry} entry */
export const defaultLog = (entry) =>
  (entry.outcome === 'sent' ? console.info : console.warn)(formatLog(entry));
