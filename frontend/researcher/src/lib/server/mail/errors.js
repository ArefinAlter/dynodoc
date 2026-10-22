// What can go wrong when mail is sent, as a short class. The class is what the server log
// records (never the server's reply text, which can quote a recipient); `diagnosis()` is the
// plain sentence the mail check prints for an operator.

/** @typedef {keyof typeof DIAGNOSES} MailErrorClass */

const DIAGNOSES = {
  'tls-untrusted':
    'TLS certificate not trusted: set SMTP_CA_FILE to the certificate authority that signed the mail server certificate (or to the certificate itself)',
  'tls-hostname':
    'TLS certificate does not match the server name: SMTP_HOST must be a name that the certificate lists',
  'tls-expired':
    "TLS certificate of the mail server has expired or is not valid yet: ask the mail administrator to renew it, and check this machine's clock",
  'tls-protocol':
    'TLS handshake failed: check SMTP_PORT and SMTP_SECURITY (port 465 usually needs tls, port 587 usually needs starttls; TLS 1.2 or newer is required)',
  'starttls-unavailable':
    'the server does not offer STARTTLS, so the connection would not be encrypted and nothing was sent: use SMTP_SECURITY=tls on port 465, or ask the mail administrator to enable STARTTLS (a password is never sent without encryption)',
  timeout:
    'the mail server did not answer in time: check SMTP_HOST, SMTP_PORT and any firewall, and SMTP_SECURITY (a server that speaks TLS from the first byte, usually on port 465, needs SMTP_SECURITY=tls)',
  'connection-refused':
    'the mail server refused the connection: check SMTP_HOST and SMTP_PORT, and that the relay is running',
  dns: 'SMTP_HOST could not be found in DNS',
  'dns-temporary': 'DNS did not answer: try again in a moment',
  network: 'the connection to the mail server could not be made or was lost',
  'auth-failed': 'authentication failed: check SMTP_USER and SMTP_PASSWORD',
  'auth-required':
    'the server requires authentication before it accepts mail: set SMTP_USER and SMTP_PASSWORD',
  'relay-refused':
    'relay refused: the server will not take mail from this installation for that recipient; ask the mail administrator to allow it (check SMTP_FROM and which machines the relay trusts)',
  'sender-refused':
    'the server refused the sender address: SMTP_FROM must be an address the relay accepts',
  'recipient-refused': 'the server refused the recipient address',
  'message-rejected': 'the server rejected the message',
  'too-large': 'the message is larger than the limit',
  transient: 'the server is busy or asked for a later attempt',
  permanent: 'the server refused the message permanently',
  'plaintext-refused':
    'SMTP_SECURITY=none-local is only allowed for a loopback or private network address, and the host resolves elsewhere',
  'invalid-message': 'the message was refused before sending',
  config: 'the mail settings are incomplete or invalid',
  unknown: 'the mail could not be sent for a reason that is not recognised'
};

/** Classes worth another attempt: the server or the network may recover on its own. */
const RETRYABLE = new Set([
  'timeout',
  'connection-refused',
  'dns-temporary',
  'network',
  'transient'
]);

/** @param {string} errorClass */
export const diagnosis = (errorClass) =>
  DIAGNOSES[/** @type {MailErrorClass} */ (errorClass)] ?? DIAGNOSES.unknown;

export class MailError extends Error {
  /**
   * @param {MailErrorClass} errorClass
   * @param {{ detail?: string, serverResponse?: string, retryable?: boolean }} [options]
   *   `detail` is added to the message and must carry no secret or recipient.
   */
  constructor(errorClass, options = {}) {
    super(options.detail ? `${diagnosis(errorClass)} (${options.detail})` : diagnosis(errorClass));
    this.name = 'MailError';
    this.errorClass = errorClass;
    this.retryable = options.retryable ?? RETRYABLE.has(errorClass);
    /** The server's own words, sanitised; shown by the mail check, never logged. */
    this.serverResponse = options.serverResponse;
  }
}

/** A server reply as one printable line of limited length. @param {unknown} text */
export function printable(text) {
  return String(text ?? '')
    .replace(/[\u0000-\u001f\u007f-\u009f]+/g, ' ')
    .trim()
    .slice(0, 200);
}

const TLS_HOSTNAME = /ERR_TLS_CERT_ALTNAME_INVALID|does not match certificate'?s altnames|Hostname\/IP does not match/i;
const TLS_EXPIRED = /CERT_HAS_EXPIRED|certificate has expired|CERT_NOT_YET_VALID|not yet valid/i;
const TLS_UNTRUSTED =
  /DEPTH_ZERO_SELF_SIGNED_CERT|SELF_SIGNED_CERT_IN_CHAIN|UNABLE_TO_VERIFY_LEAF_SIGNATURE|UNABLE_TO_GET_ISSUER_CERT|CERT_UNTRUSTED|self[- ]signed|unable to verify the first certificate|unable to get (?:local )?issuer|certificate chain|CERT_REJECTED/i;
const TLS_PROTOCOL =
  /ERR_SSL_|SSL routines|wrong version number|protocol version|no protocols available|unsupported protocol|ERR_TLS_|handshake failure|ssl3_|tlsv1 alert/i;

/**
 * Sort an error from the SMTP client or the network into a MailError. Never throws, keeps no
 * recipient, and keeps the server's text only in `serverResponse`.
 * @param {any} error
 * @returns {MailError}
 */
export function classifyError(error) {
  if (error instanceof MailError) return error;
  const code = String(error?.code ?? '');
  const message = String(error?.message ?? '');
  const probe = `${code} ${message} ${String(error?.cause?.code ?? '')} ${String(error?.cause?.message ?? '')}`;
  const response = typeof error?.response === 'string' ? error.response : undefined;
  const serverResponse = response ? printable(response) : undefined;
  const status = Number(error?.responseCode) || Number(/^(\d{3})/.exec(response ?? '')?.[1]) || 0;
  const command = String(error?.command ?? '');
  const make = (/** @type {MailErrorClass} */ errorClass) =>
    new MailError(errorClass, { serverResponse });

  if (TLS_HOSTNAME.test(probe)) return make('tls-hostname');
  if (TLS_EXPIRED.test(probe)) return make('tls-expired');
  if (TLS_UNTRUSTED.test(probe)) return make('tls-untrusted');
  if (code === 'ETLS' && command === 'STARTTLS' && status) return make('starttls-unavailable');
  if (TLS_PROTOCOL.test(probe)) return make('tls-protocol');
  if (code === 'ETLS') return make('tls-protocol');
  if (/ECONNREFUSED/.test(probe)) return make('connection-refused');
  if (/ENOTFOUND/.test(probe)) return make('dns');
  if (/EAI_AGAIN/.test(probe)) return make('dns-temporary');
  if (code === 'ETIMEDOUT' || /time(?:d)? ?out/i.test(message)) return make('timeout');
  if (!status && /ECONNRESET|EPIPE|EHOSTUNREACH|ENETUNREACH|ECONNECTION|ESOCKET|EDNS/.test(probe))
    return make('network');
  if (code === 'EAUTH' || [534, 535, 538].includes(status)) return make('auth-failed');
  if (status === 530 || (status === 554 && /authentication required/i.test(response ?? '')))
    return make('auth-required');
  if (status >= 400 && status < 500) return make('transient');
  if (status === 552) return make('too-large');
  if (status >= 500) {
    const relay = /\b5\.7\.\d\b|relay/i.test(response ?? '') || [551, 554].includes(status);
    if (command === 'MAIL FROM') return make('sender-refused');
    if (command === 'RCPT TO') return make(relay ? 'relay-refused' : 'recipient-refused');
    if (code === 'EENVELOPE') return make(relay ? 'relay-refused' : 'recipient-refused');
    if (code === 'EMESSAGE' || command === 'DATA') return make('message-rejected');
    return make('permanent');
  }
  if (code === 'EENVELOPE') return make('recipient-refused');
  return make('unknown');
}
