// The documented address subset and the header-text rules (./address.js).
// Invisible and non-ASCII characters are built with fromCharCode so the file shows what it tests.
import { describe, expect, it } from 'vitest';
import {
  MAX_ADDRESS_LENGTH,
  bodyTextProblem,
  domainOf,
  headerTextProblem,
  parseAddress
} from './address.js';

const ch = (code: number) => String.fromCharCode(code);
const NUL = ch(0);
const NEL = ch(0x85);
const DEL = ch(0x7f);
const BELL = ch(7);
const LINE_SEPARATOR = ch(0x2028);
const PARAGRAPH_SEPARATOR = ch(0x2029);
const RIGHT_TO_LEFT_OVERRIDE = ch(0x202e);
const LEFT_TO_RIGHT_ISOLATE = ch(0x2066);
const A_ACUTE = ch(0xe1);
const E_ACUTE = ch(0xe9);

describe('parseAddress', () => {
  it('accepts plain addresses and lower-cases the domain only', () => {
    expect(parseAddress('olivia@example.test')).toEqual({ address: 'olivia@example.test' });
    expect(parseAddress('  Olivia.O-Brien+docs@Mail.Example.ORG ')).toEqual({
      address: 'Olivia.O-Brien+docs@mail.example.org'
    });
    expect(parseAddress("o'brien@example.org")).toEqual({ address: "o'brien@example.org" });
    expect(parseAddress('a=b&c@sub.example.co.uk')).toEqual({ address: 'a=b&c@sub.example.co.uk' });
  });
  it('refuses every exotic form instead of repairing it', () => {
    const refused = [
      '',
      'no-at-sign.example.org',
      '@example.org',
      'olivia@',
      'a@b@example.org',
      'Olivia <olivia@example.org>',
      '<olivia@example.org>',
      '"olivia"@example.org',
      '"a@b"@example.org',
      'olivia@[192.0.2.1]',
      'olivia@192.0.2.1',
      'olivia@localhost',
      'olivia@example',
      'olivia@exa mple.org',
      'oli via@example.org',
      'olivia(comment)@example.org',
      'a%b@example.org',
      'a!b@example.org',
      'a|b@example.org',
      'a;b@example.org',
      'a,b@example.org',
      '.olivia@example.org',
      'olivia.@example.org',
      'oli..via@example.org',
      'olivia@-example.org',
      'olivia@example-.org',
      'olivia@exa..mple.org',
      'olivia@example.org.',
      `olivi${A_ACUTE}@example.org`,
      `olivia@exampl${E_ACUTE}.org`,
      `${'a'.repeat(65)}@example.org`,
      `a@${'b'.repeat(64)}.org`,
      `a@${'b'.repeat(MAX_ADDRESS_LENGTH)}.org`
    ];
    for (const value of refused)
      expect(parseAddress(value), JSON.stringify(value)).toHaveProperty('reason');
  });
  it('refuses line breaks and other control characters inside an address', () => {
    for (const value of [
      'a@example.org\r\nBcc: x@example.org',
      'a@example.org\nBcc: x@example.org',
      'a@example.org\r',
      'a\r\n@example.org',
      `a@example.org${NUL}`,
      'a@exam\tple.org'
    ])
      expect(parseAddress(value), JSON.stringify(value)).toHaveProperty('reason');
  });
  it('is not fooled by a non-string', () => {
    expect(parseAddress(undefined as never)).toHaveProperty('reason');
    expect(parseAddress(['a@example.org'] as never)).toHaveProperty('reason');
  });
  it('names the domain of a validated address', () => {
    expect(domainOf('docs@mail.example.org')).toBe('mail.example.org');
  });
});

describe('header text', () => {
  it('refuses a line break, any control or separator character and a direction override', () => {
    for (const hostile of [
      'Hi\r\nBcc: attacker@example.org',
      'Hi\nBcc: x',
      'Hi\rBcc: x',
      `Hi${NUL}`,
      `Hi${NEL}`,
      `Hi${LINE_SEPARATOR}Bcc: x`,
      `Hi${PARAGRAPH_SEPARATOR}`,
      `Hi${RIGHT_TO_LEFT_OVERRIDE}evil`,
      `Hi${LEFT_TO_RIGHT_ISOLATE}x`,
      `Hi${DEL}`,
      'tab\there'
    ])
      expect(headerTextProblem(hostile, 100), JSON.stringify(hostile)).toMatch(/control character/);
  });
  it('accepts ordinary text in any script and enforces the limit', () => {
    expect(headerTextProblem(`Z${ch(0xfc)}rich ${ch(0x2014)} ${ch(0x201c)}shared${ch(0x201d)} with you`, 100)).toBeUndefined();
    expect(headerTextProblem('x'.repeat(101), 100)).toMatch(/longer than 100/);
    expect(headerTextProblem(42, 100)).toBe('is not text');
  });
  it('lets bodies hold line breaks and tabs but no other control character', () => {
    expect(bodyTextProblem('line one\r\nline two\n\tindented')).toBeUndefined();
    expect(bodyTextProblem(`nul${NUL}`)).toMatch(/control/);
    expect(bodyTextProblem(`bell${BELL}`)).toMatch(/control/);
  });
});
