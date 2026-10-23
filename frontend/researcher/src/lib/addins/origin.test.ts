// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { parseAddinOrigin, requireAddinOrigin } from './origin.js';

const accepted = [
  'https://dynodoc.corp.example',
  'https://docs.example.org:8443',
  'https://intranet',
  'https://10.1.2.3',
  'https://[2001:db8::1]',
  'https://xn--bcher-kva.example'
];
const refused: [string, unknown, RegExp][] = [
  ['not text', 42, /not set/],
  ['null', null, /not set/],
  ['empty', '', /not set/],
  ['user name', 'https://admin@dynodoc.corp.example', /user name or password/],
  ['user and password', 'https://admin:secret@dynodoc.corp.example', /user name or password/],
  ['look-alike user name', 'https://dynodoc.corp.example@evil.example', /user name or password/],
  ['path', 'https://dynodoc.corp.example/app', /only the scheme/],
  ['trailing slash', 'https://dynodoc.corp.example/', /only the scheme/],
  ['query', 'https://dynodoc.corp.example?x=1', /only the scheme/],
  ['fragment', 'https://dynodoc.corp.example#x', /only the scheme/],
  ['default port written out', 'https://dynodoc.corp.example:443', /only the scheme/],
  ['upper case', 'https://Dynodoc.Corp.Example', /only the scheme/],
  ['leading zeros in the port', 'https://dynodoc.corp.example:08443', /only the scheme/],
  ['port 0', 'https://dynodoc.corp.example:0', /port 0/],
  ['port too large', 'https://dynodoc.corp.example:65536', /not a URL/],
  ['two ports', 'https://dynodoc.corp.example:443:80', /not a URL/],
  ['space', 'https://dynodoc corp.example', /space|control|not a URL/],
  ['trailing space', 'https://dynodoc.corp.example ', /space|control/],
  ['newline', 'https://dynodoc.corp.example\n', /space|control/],
  ['backslash', 'https://dynodoc.corp.example\\@evil.example', /not part of an origin/],
  ['quote', 'https://dynodoc.corp.example"', /not part of an origin/],
  ['angle bracket', 'https://dynodoc.corp.example<', /not part of an origin/],
  ['non-ASCII name', 'https://bücher.example', /outside ASCII/],
  ['trailing dot', 'https://dynodoc.corp.example.', /dot/],
  ['plain http', 'http://dynodoc.corp.example', /https/],
  ['plain http on a private address', 'http://10.0.0.5', /https/],
  ['another scheme', 'ftp://dynodoc.corp.example', /https/],
  ['script scheme', 'javascript:alert(1)', /https|origin/],
  ['relative', '/connectors', /not a URL/],
  ['too long', `https://${'a'.repeat(260)}.example`, /longer than/]
];

describe('an add-in origin', () => {
  it.each(accepted)('accepts %s as written', (origin) => {
    expect(parseAddinOrigin(origin)).toEqual({ ok: true, origin });
  });
  it.each(refused)('refuses %s', (_name, value, reason) => {
    const result = parseAddinOrigin(value);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toMatch(reason);
  });
  it('accepts plain http for localhost only when the caller allows a test stack', () => {
    for (const origin of ['http://localhost:4174', 'http://127.0.0.1:4174', 'http://[::1]:4174']) {
      expect(parseAddinOrigin(origin).ok, origin).toBe(false);
      expect(parseAddinOrigin(origin, { allowLoopbackHttp: true }), origin).toEqual({
        ok: true,
        origin
      });
    }
    // The allowance is for loopback names, not for every private address or look-alike.
    for (const origin of [
      'http://192.168.1.4',
      'http://localhost.example',
      'http://127.0.0.1.example',
      'http://dynodoc.corp.example'
    ])
      expect(parseAddinOrigin(origin, { allowLoopbackHttp: true }).ok, origin).toBe(false);
  });
  it('refuses a forbidden domain and its sub-domains, and nothing that merely ends alike', () => {
    for (const origin of [
      'https://hosted.test',
      'https://app.hosted.test',
      'https://a.b.hosted.test'
    ])
      expect(parseAddinOrigin(origin, { forbidDomain: 'hosted.test' }).ok, origin).toBe(false);
    for (const origin of ['https://nothosted.test', 'https://hosted.test.example'])
      expect(parseAddinOrigin(origin, { forbidDomain: 'hosted.test' }).ok, origin).toBe(true);
  });
  it('throws a sentence for the caller that wants an exception', () => {
    expect(() => requireAddinOrigin('http://example.org')).toThrow(/^The origin must use https/);
    expect(requireAddinOrigin('https://example.org')).toBe('https://example.org');
  });
});
