// How errors from the SMTP client and the network are sorted, and that the operator-facing
// sentences are the ones the guide promises.
import { describe, expect, it } from 'vitest';
import { MailError, classifyError, diagnosis, printable } from './errors.js';

const reply = (code: string, responseCode: number, command: string, response: string) =>
  Object.assign(new Error(`${command} failed: ${response}`), {
    code,
    responseCode,
    command,
    response
  });
const net = (code: string, message: string) => Object.assign(new Error(message), { code });

describe('classifyError', () => {
  const cases: [string, unknown, string, boolean][] = [
    ['untrusted: self-signed', net('ESOCKET', 'self-signed certificate'), 'tls-untrusted', false],
    ['untrusted: unknown issuer', net('ESOCKET', 'unable to verify the first certificate'), 'tls-untrusted', false],
    ['untrusted: in chain', net('ESOCKET', 'self-signed certificate in certificate chain'), 'tls-untrusted', false],
    ['expired', net('ESOCKET', 'certificate has expired'), 'tls-expired', false],
    [
      'host name',
      net('ESOCKET', "Hostname/IP does not match certificate's altnames: Host: a. is not in the cert's altnames: DNS:b"),
      'tls-hostname',
      false
    ],
    [
      'plaintext server on a TLS port',
      net('ESOCKET', '0A00010B:SSL routines:tls_validate_record_header:wrong version number'),
      'tls-protocol',
      false
    ],
    ['old TLS', net('ESOCKET', 'error:0A000102:SSL routines::unsupported protocol'), 'tls-protocol', false],
    [
      'no STARTTLS',
      reply('ETLS', 502, 'STARTTLS', '502 5.5.1 STARTTLS is not available'),
      'starttls-unavailable',
      false
    ],
    ['refused', net('ESOCKET', 'connect ECONNREFUSED 127.0.0.1:25'), 'connection-refused', true],
    ['not found', net('EDNS', 'getaddrinfo ENOTFOUND mail.example.invalid'), 'dns', false],
    ['dns trouble', net('EDNS', 'getaddrinfo EAI_AGAIN mail.example.org'), 'dns-temporary', true],
    ['timeout', Object.assign(net('ETIMEDOUT', 'Timeout'), { command: 'CONN' }), 'timeout', true],
    ['reset', net('ESOCKET', 'read ECONNRESET'), 'network', true],
    ['bad password', reply('EAUTH', 535, 'AUTH PLAIN', '535 5.7.8 Authentication credentials invalid'), 'auth-failed', false],
    ['auth required', reply('EENVELOPE', 530, 'MAIL FROM', '530 5.7.0 Authentication required'), 'auth-required', false],
    ['greylisted', reply('EENVELOPE', 451, 'RCPT TO', '451 4.7.1 Greylisted'), 'transient', true],
    ['mailbox busy', reply('EENVELOPE', 450, 'RCPT TO', '450 4.2.1 Mailbox busy'), 'transient', true],
    ['closing', reply('ECONNECTION', 421, 'EHLO', '421 4.3.2 Service shutting down'), 'transient', true],
    ['relay denied', reply('EENVELOPE', 554, 'RCPT TO', '554 5.7.1 Relay access denied'), 'relay-refused', false],
    ['policy', reply('EENVELOPE', 550, 'RCPT TO', '550 5.7.1 Not allowed'), 'relay-refused', false],
    ['unknown user', reply('EENVELOPE', 550, 'RCPT TO', '550 5.1.1 No such user'), 'recipient-refused', false],
    ['sender refused', reply('EENVELOPE', 550, 'MAIL FROM', '550 5.7.1 Sender not allowed'), 'sender-refused', false],
    ['spam', reply('EMESSAGE', 554, 'DATA', '554 5.7.0 Rejected as spam'), 'message-rejected', false],
    ['too big', reply('EMESSAGE', 552, 'DATA', '552 5.3.4 Message size exceeds limit'), 'too-large', false],
    ['plain 5xx', reply('EPROTOCOL', 500, 'EHLO', '500 5.5.1 Unknown'), 'permanent', false],
    ['nothing known', new Error('???'), 'unknown', false]
  ];
  it.each(cases)('%s', (_name, error, errorClass, retryable) => {
    const sorted = classifyError(error);
    expect(sorted).toBeInstanceOf(MailError);
    expect(sorted.errorClass).toBe(errorClass);
    expect(sorted.retryable).toBe(retryable);
  });
  it('never retries a 5xx and always retries a 4xx reply, whatever the command', () => {
    for (const command of ['MAIL FROM', 'RCPT TO', 'DATA', 'EHLO'])
      for (const status of [500, 501, 550, 553, 554])
        expect(classifyError(reply('EENVELOPE', status, command, `${status} no`)).retryable).toBe(false);
    for (const command of ['MAIL FROM', 'RCPT TO', 'DATA'])
      for (const status of [421, 450, 451, 452])
        expect(classifyError(reply('EENVELOPE', status, command, `${status} later`)).retryable).toBe(true);
  });
  it('keeps a MailError as it is, and keeps the server words only in serverResponse', () => {
    const own = new MailError('too-large');
    expect(classifyError(own)).toBe(own);
    const sorted = classifyError(reply('EENVELOPE', 550, 'RCPT TO', '550 5.1.1 <olivia@example.org> unknown'));
    expect(sorted.serverResponse).toContain('olivia@example.org');
    expect(sorted.message).not.toContain('olivia@example.org');
  });
  it('prints a server reply on one line without control characters, shortened', () => {
    expect(printable('550 no\r\n5.7.1 relay\u0000')).toBe('550 no 5.7.1 relay');
    expect(printable('x'.repeat(500))).toHaveLength(200);
  });
});

describe('diagnosis', () => {
  it('says what to do, in the words of the guide', () => {
    expect(diagnosis('tls-untrusted')).toMatch(/^TLS certificate not trusted: set SMTP_CA_FILE/);
    expect(diagnosis('auth-failed')).toMatch(/^authentication failed/);
    expect(diagnosis('relay-refused')).toMatch(/^relay refused/);
    expect(diagnosis('starttls-unavailable')).toMatch(/nothing was sent/);
    expect(diagnosis('no-such-class')).toBe(diagnosis('unknown'));
  });
  it('is the message of the error, with an optional detail that carries no content', () => {
    expect(new MailError('timeout').message).toBe(diagnosis('timeout'));
    expect(new MailError('invalid-message', { detail: 'subject is empty' }).message).toBe(
      `${diagnosis('invalid-message')} (subject is empty)`
    );
  });
});
