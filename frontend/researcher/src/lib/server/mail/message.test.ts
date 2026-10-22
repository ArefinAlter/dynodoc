// @vitest-environment node
// The message builder: what it refuses (hostile input), and what it produces (UTF-8, RFC 2047,
// multipart/alternative, Message-ID, the header lint, the size limit).
import { describe, expect, it } from 'vitest';
import {
  MAX_MESSAGE_BYTES,
  MAX_RECIPIENTS,
  checkMessage,
  checkSender,
  composeMessage,
  lintMessage,
  newMessageId
} from './message.js';
import { MailError } from './errors.js';
import { decodePart, decodeWords, split } from './test-helpers/mime';

const ch = (code: number) => String.fromCharCode(code);
const FROM = { name: 'Acme Docs', address: 'docs@example.org' };
const ok = { to: 'olivia@example.test', subject: 'Hello', text: 'Body' };
const refusal = (run: () => unknown) => {
  try {
    run();
  } catch (error) {
    return error as MailError;
  }
  return undefined;
};

describe('checkMessage', () => {
  it('accepts a plain message and trims the subject', () => {
    expect(checkMessage({ ...ok, subject: '  Hello  ', html: '<p>Body</p>' })).toEqual({
      to: ['olivia@example.test'],
      subject: 'Hello',
      text: 'Body',
      html: '<p>Body</p>',
      replyTo: undefined
    });
  });
  it('refuses a line break or control character in the subject', () => {
    for (const subject of [
      'Hello\r\nBcc: attacker@example.org',
      'Hello\nBcc: attacker@example.org',
      'Hello\rX',
      `Hello${ch(0)}`,
      `Hello${ch(0x2028)}Bcc: x`,
      `Hello${ch(0x85)}`
    ]) {
      const error = refusal(() => checkMessage({ ...ok, subject }));
      expect(error, JSON.stringify(subject)).toBeInstanceOf(MailError);
      expect(error?.errorClass).toBe('invalid-message');
      expect(error?.message).toContain('subject');
      expect(error?.message).not.toContain('attacker');
    }
    expect(refusal(() => checkMessage({ ...ok, subject: '   ' }))?.message).toContain('subject');
  });
  it('refuses a hostile recipient or reply-to, and names the field, never the content', () => {
    for (const to of [
      'a@example.org\r\nBcc: attacker@example.org',
      'a@example.org\nBcc: x@example.org',
      'a@example.org, b@example.org',
      'a@example.org;b@example.org',
      'Name <a@example.org>',
      '"a b"@example.org',
      'a@[127.0.0.1]',
      ''
    ]) {
      const error = refusal(() => checkMessage({ ...ok, to }));
      expect(error?.errorClass, JSON.stringify(to)).toBe('invalid-message');
      expect(error?.message).toMatch(/recipient 1|no recipient/);
      expect(error?.message).not.toContain('attacker');
    }
    expect(
      refusal(() => checkMessage({ ...ok, replyTo: 'x@example.org\r\nBcc: y@example.org' }))
        ?.message
    ).toContain('reply-to');
  });
  it('caps the recipients, and removes a repeated one', () => {
    const many = (n: number) => Array.from({ length: n }, (_, i) => `user${i}@example.org`);
    expect(checkMessage({ ...ok, to: many(MAX_RECIPIENTS) }).to).toHaveLength(MAX_RECIPIENTS);
    expect(refusal(() => checkMessage({ ...ok, to: many(MAX_RECIPIENTS + 1) }))?.message).toMatch(
      /more than 10 recipients/
    );
    expect(refusal(() => checkMessage({ ...ok, to: [] }))?.message).toMatch(/no recipient/);
    expect(
      checkMessage({ ...ok, to: ['a@example.org', 'A@Example.org'.replace('A@', 'a@')] }).to
    ).toEqual(['a@example.org']);
  });
  it('refuses a control character in a body, an empty text, and an oversized body', () => {
    expect(refusal(() => checkMessage({ ...ok, text: `x${ch(0)}y` }))?.message).toContain('text');
    expect(refusal(() => checkMessage({ ...ok, html: `x${ch(7)}` }))?.message).toContain('html');
    expect(refusal(() => checkMessage({ ...ok, text: '  \n' }))?.message).toContain('text');
    expect(refusal(() => checkMessage({ ...ok, text: 'x'.repeat(100_001) }))?.message).toContain(
      'too long'
    );
  });
  it('refuses values that are not text at all', () => {
    expect(refusal(() => checkMessage({ ...ok, subject: 5 as never }))?.errorClass).toBe(
      'invalid-message'
    );
    expect(refusal(() => checkMessage({ ...ok, text: undefined as never }))?.errorClass).toBe(
      'invalid-message'
    );
  });
});

describe('checkSender', () => {
  it('validates the configured sender', () => {
    expect(checkSender({ name: ' Acme Docs ', address: 'Docs@Example.org' })).toEqual({
      name: 'Acme Docs',
      address: 'Docs@example.org'
    });
    expect(
      refusal(() => checkSender({ name: 'Acme\r\nBcc: x@example.org', address: 'a@example.org' }))
        ?.message
    ).toContain('sender name');
    expect(
      refusal(() => checkSender({ name: 'Acme', address: 'not an address' }))?.message
    ).toContain('sender address');
  });
});

describe('composeMessage', () => {
  const build = async (
    message: Partial<Parameters<typeof checkMessage>[0]> = {},
    replyTo?: string
  ) => {
    const id = newMessageId('example.org');
    const checked = checkMessage({ ...ok, ...message });
    return { id, ...(await composeMessage(FROM, replyTo, checked, id)) };
  };

  it('builds multipart/alternative with a text and an HTML part, UTF-8, headers in 7 bits', async () => {
    const subject = `Zürich ${ch(0x2713)} ${ch(0x201c)}Quarterly${ch(0x201d)} 東京`;
    const text = `Grüße aus Zürich ${ch(0x2713)}\n東京の皆さん`;
    const html = `<p>Gr&uuml;ße ${ch(0x2713)} 東京</p>`;
    const built = await build({ subject, text, html });
    const { headers, body } = split(built.raw);
    expect(headers.get('content-type')).toMatch(/^multipart\/alternative;/);
    expect(headers.get('mime-version')).toBe('1.0');
    expect(decodeWords(headers.get('subject') ?? '')).toBe(subject);
    expect(headers.get('subject')).toMatch(/=\?UTF-8\?[BQ]\?/i);
    expect(/[^\x20-\x7e]/.test([...headers.values()].join(''))).toBe(false);
    const boundary = /boundary="?([^";]+)"?/.exec(headers.get('content-type') ?? '')?.[1] ?? '';
    const parts = body.split(`--${boundary}`).slice(1, -1);
    expect(parts).toHaveLength(2);
    expect(parts[0]).toMatch(/content-type:\s*text\/plain;\s*charset=utf-8/i);
    expect(parts[1]).toMatch(/content-type:\s*text\/html;\s*charset=utf-8/i);
    // The wire form has CRLF line ends, whatever the template used.
    expect(decodePart(parts[0].replace(/^\r\n/, '')).replace(/\r\n/g, '\n')).toBe(text);
    expect(decodePart(parts[1].replace(/^\r\n/, ''))).toBe(html);
  });
  it('sets Message-ID under the sender domain, Auto-Submitted, Date, From and To; envelope matches', async () => {
    const built = await build({ to: ['a@example.test', 'b@example.test'] });
    const { headers } = split(built.raw);
    expect(headers.get('message-id')).toBe(built.id);
    expect(built.id).toMatch(/^<[0-9a-f]{32}@example\.org>$/);
    expect(headers.get('auto-submitted')).toBe('auto-generated');
    expect(headers.get('date')).toMatch(/^\w{3}, \d{1,2} \w{3} \d{4} /);
    expect(headers.get('from')).toBe('Acme Docs <docs@example.org>');
    expect(headers.get('to')).toBe('a@example.test, b@example.test');
    expect(built.envelope).toEqual({
      from: 'docs@example.org',
      to: ['a@example.test', 'b@example.test']
    });
    expect(headers.has('bcc')).toBe(false);
  });
  it('encodes a non-ASCII sender name (RFC 2047) and keeps the address plain', async () => {
    const checked = checkMessage(ok);
    const built = await composeMessage(
      { name: `Équipe Zürich`, address: 'docs@example.org' },
      undefined,
      checked,
      newMessageId('example.org')
    );
    const from = split(built.raw).headers.get('from') ?? '';
    expect(from).toMatch(/^=\?UTF-8\?[BQ]\?.*\?= <docs@example\.org>$/i);
    expect(decodeWords(from)).toBe('Équipe Zürich <docs@example.org>');
  });
  it('puts a request Reply-To ahead of the configured one, and the configured one when there is none', async () => {
    expect(
      split(
        (await build({ replyTo: 'inviter@example.test' }, 'configured@example.org')).raw
      ).headers.get('reply-to')
    ).toBe('inviter@example.test');
    expect(split((await build({}, 'configured@example.org')).raw).headers.get('reply-to')).toBe(
      'configured@example.org'
    );
    expect(split((await build({})).raw).headers.has('reply-to')).toBe(false);
  });
  it('sends a text-only message when there is no HTML', async () => {
    const { headers, body } = split((await build({ text: 'Plain only' })).raw);
    expect(headers.get('content-type')).toMatch(/^text\/plain/);
    expect(body).toContain('Plain only');
  });
  it('refuses a finished message over the size limit', async () => {
    // Quoted-printable makes each accented letter six bytes: 100,000 of them in two parts is over 256 KiB.
    const accented = ch(0xe9).repeat(100_000);
    const error = await build({ text: accented, html: accented }).catch((e) => e);
    expect(error).toBeInstanceOf(MailError);
    expect(error.errorClass).toBe('too-large');
    expect(MAX_MESSAGE_BYTES).toBe(256 * 1024);
  });
  it('lets a message just inside the limit through', async () => {
    const built = await build({ text: 'x'.repeat(50_000), html: `<p>${'y'.repeat(50_000)}</p>` });
    expect(built.raw.length).toBeLessThan(MAX_MESSAGE_BYTES);
  });
  it('even if the checks were skipped, a hostile subject or name cannot add a header', async () => {
    const hostile = 'Hello\r\nBcc: attacker@example.org\r\nX-Evil: 1';
    const attempt = await composeMessage(
      { name: 'Acme\r\nBcc: attacker@example.org', address: 'docs@example.org' },
      undefined,
      {
        to: ['olivia@example.test'],
        subject: hostile,
        text: 'Body',
        html: undefined,
        replyTo: undefined
      },
      newMessageId('example.org')
    ).catch((error) => error);
    if (attempt instanceof Error) expect(attempt).toBeInstanceOf(MailError);
    else {
      const { headers } = split(attempt.raw);
      expect(headers.has('bcc')).toBe(false);
      expect(headers.has('x-evil')).toBe(false);
    }
  });
});

describe('lintMessage', () => {
  const id = '<abc@example.org>';
  const good = [
    'From: Docs <docs@example.org>',
    'To: a@example.test',
    'Subject: Hello',
    `Message-ID: ${id}`,
    'Date: Thu, 22 Oct 2026 10:00:00 +0000',
    'MIME-Version: 1.0',
    'Content-Type: text/plain; charset=utf-8',
    'Content-Transfer-Encoding: 7bit',
    '',
    'Body'
  ];
  const lint = (lines: string[]) => lintMessage(Buffer.from(lines.join('\r\n')), id);
  it('accepts the headers the builder produces, folded or not', () => {
    expect(() => lint(good)).not.toThrow();
    expect(() =>
      lint([...good.slice(0, 2), 'Subject: Hello', ' continued', ...good.slice(3)])
    ).not.toThrow();
  });
  it('refuses an injected, repeated, missing or odd header', () => {
    const withExtra = (extra: string) => [...good.slice(0, 3), extra, ...good.slice(3)];
    expect(() => lint(withExtra('Bcc: attacker@example.org'))).toThrow(/unexpected header bcc/);
    expect(() => lint(withExtra('X-Evil: 1'))).toThrow(/unexpected header x-evil/);
    expect(() => lint(withExtra('Subject: second'))).toThrow(/appears twice/);
    expect(() => lint(withExtra('no colon here'))).toThrow(/malformed/);
    expect(() => lint(good.filter((line) => !line.startsWith('Date')))).toThrow(/date is missing/);
    expect(() =>
      lint(
        good.map((line) =>
          line.startsWith('Message-ID') ? 'Message-ID: <other@example.org>' : line
        )
      )
    ).toThrow(/Message-ID/);
    expect(() => lint(withExtra(`Subject-X: caf${ch(0xe9)}`))).toThrow(/non-ASCII/);
    expect(() => lint(['From: a', 'no blank line'])).toThrow(/no header block/);
  });
  it('is the same class of refusal the rest of the module uses', () => {
    expect(() => lint([...good.slice(0, 3), 'Bcc: x', ...good.slice(3)])).toThrowError(MailError);
  });
});

describe('newMessageId', () => {
  it('is unique and under the given domain', () => {
    const a = newMessageId('example.org');
    expect(a).toMatch(/^<[0-9a-f]{32}@example\.org>$/);
    expect(newMessageId('example.org')).not.toBe(a);
  });
});
