import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  APP_VERSION,
  appKind,
  normaliseAppVersion,
  appLink,
  appSigninPath,
  isChallenge,
  loopbackLink,
  isCode,
  isState,
  isVerifier,
  parseAppSignin,
  parseFinishFragment
} from './app-signin';
import { safeNext } from './auth-next';

// RFC 7636 appendix B.
const VERIFIER = 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk';
const CHALLENGE = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';
const STATE = 'b3JkZXJseS1zdGF0ZS12YWx1ZQ';
const CODE = '0123456789abcdef'.repeat(4);
const query = (s: string) => new URLSearchParams(s);
const good = `challenge=${CHALLENGE}&state=${STATE}&platform=android`;

describe('the request an app puts in the sign-in page address', () => {
  it('accepts exactly three valid values, in any order', () => {
    const want = { challenge: CHALLENGE, state: STATE, platform: 'android' };
    expect(parseAppSignin(query(good))).toEqual(want);
    expect(parseAppSignin(query(`platform=android&state=${STATE}&challenge=${CHALLENGE}`))).toEqual(
      want
    );
    for (const platform of ['desktop', 'android', 'ios'])
      expect(parseAppSignin(query(good.replace('android', platform)))?.platform).toBe(platform);
  });

  it('refuses everything else', () => {
    const bad = [
      '',
      'challenge=x',
      `${good}&next=//evil.example`,
      `${good}&challenge=${CHALLENGE}`,
      good.replace('android', 'windows'),
      good.replace('android', 'Android'),
      good.replace('android', ''),
      good.replace(CHALLENGE, CHALLENGE.slice(1)),
      good.replace(CHALLENGE, `${CHALLENGE}A`),
      // The last character of a canonical challenge carries only four bits.
      good.replace(CHALLENGE, `${CHALLENGE.slice(0, 42)}N`),
      good.replace(CHALLENGE, CHALLENGE.replace('-', '+')),
      good.replace(CHALLENGE, `${CHALLENGE.slice(0, 40)}%2b%2b%4d`),
      good.replace(STATE, 'tiny'),
      good.replace(STATE, 'a'.repeat(65)),
      good.replace(STATE, `${STATE}%0a`),
      good.replace(STATE, `${STATE}<script>`),
      `state=${STATE}&platform=android`,
      `challenge=${CHALLENGE}&platform=android`,
      `challenge=${CHALLENGE}&state=${STATE}`
    ];
    for (const text of bad) expect(parseAppSignin(query(text)), text).toBeNull();
  });

  it('is rebuilt in one spelling, so what is carried through sign-in is exactly the request', () => {
    const request = parseAppSignin(query(`platform=ios&state=${STATE}&challenge=${CHALLENGE}`))!;
    expect(appSigninPath(request)).toBe(
      `/app-signin?challenge=${CHALLENGE}&state=${STATE}&platform=ios`
    );
  });
});

describe('the loopback port of a desktop app (decision 020)', () => {
  const desktop = `challenge=${CHALLENGE}&state=${STATE}&platform=desktop`;

  it('is accepted as a fourth value for a desktop app, in any order', () => {
    const want = { challenge: CHALLENGE, state: STATE, platform: 'desktop', redirectPort: 49321 };
    expect(parseAppSignin(query(`${desktop}&redirect_port=49321`))).toEqual(want);
    expect(parseAppSignin(query(`redirect_port=49321&${desktop}`))).toEqual(want);
    expect(parseAppSignin(query(`${desktop}&redirect_port=1024`))?.redirectPort).toBe(1024);
    expect(parseAppSignin(query(`${desktop}&redirect_port=65535`))?.redirectPort).toBe(65535);
  });

  it('is refused for a phone, and when it is not a plain unprivileged port, or not alone', () => {
    const bad = [
      `${good}&redirect_port=49321`,
      `${good.replace('android', 'ios')}&redirect_port=49321`,
      `${desktop}&redirect_port=1023`,
      `${desktop}&redirect_port=65536`,
      `${desktop}&redirect_port=0`,
      `${desktop}&redirect_port=049321`,
      `${desktop}&redirect_port=+49321`,
      `${desktop}&redirect_port=4.9e4`,
      `${desktop}&redirect_port=0x1234`,
      `${desktop}&redirect_port=`,
      `${desktop}&redirect_port=49321&redirect_port=49322`,
      `${desktop}&redirect=49321`,
      `${desktop}&redirect_port=49321&next=//evil.example`
    ];
    for (const text of bad) expect(parseAppSignin(query(text)), text).toBeNull();
  });

  it('is carried through sign-in in the same spelling, and only when present', () => {
    const request = parseAppSignin(query(`redirect_port=49321&${desktop}`))!;
    expect(appSigninPath(request)).toBe(`/app-signin?${desktop}&redirect_port=49321`);
    expect(appSigninPath(parseAppSignin(query(desktop))!)).toBe(`/app-signin?${desktop}`);
  });

  it('gives a link to this computer only, with the code and the state and nothing else', () => {
    expect(loopbackLink(49321, CODE, STATE)).toBe(
      `http://127.0.0.1:49321/signin?code=${CODE}&state=${STATE}`
    );
    expect(() => loopbackLink(80, CODE, STATE)).toThrow();
    expect(() => loopbackLink(49321, 'nope', STATE)).toThrow();
    expect(() => loopbackLink(49321, CODE, 'a b')).toThrow();
    expect(() => loopbackLink(1.5, CODE, STATE)).toThrow();
  });
});

describe('shapes of the other values', () => {
  it('codes, verifiers, states and challenges are bounded', () => {
    expect(isCode(CODE)).toBe(true);
    for (const bad of [
      CODE.toUpperCase(),
      CODE.slice(1),
      `${CODE}0`,
      `${CODE.slice(1)}g`,
      '',
      null,
      7,
      { toString: () => CODE }
    ])
      expect(isCode(bad)).toBe(false);
    expect(isVerifier(VERIFIER)).toBe(true);
    expect(isVerifier('A.~_-'.repeat(9))).toBe(true);
    for (const bad of [
      'a'.repeat(42),
      'a'.repeat(129),
      `${VERIFIER}+`,
      `${VERIFIER}=`,
      `${VERIFIER}\n`
    ])
      expect(isVerifier(bad)).toBe(false);
    expect(isState(STATE)).toBe(true);
    expect(isState('a'.repeat(15))).toBe(false);
    expect(isState(`${STATE}\n`)).toBe(false);
    expect(isChallenge(CHALLENGE)).toBe(true);
    expect(isChallenge(`${CHALLENGE}\n`)).toBe(false);
  });

  it('the app link carries a code and a state and nothing else', () => {
    expect(appLink(CODE, STATE)).toBe(`dynodoc://signin?code=${CODE}&state=${STATE}`);
    expect(() => appLink(`${CODE}&x=1`, STATE)).toThrow();
    expect(() => appLink(CODE, `${STATE}&x=1`)).toThrow();
    expect(() => appLink('', STATE)).toThrow();
  });
});

describe('the address fragment the app opens the finish page with', () => {
  const frag = `#code=${CODE}&state=${STATE}&verifier=${VERIFIER}`;
  it('needs the three values, each once', () => {
    expect(parseFinishFragment(frag)).toEqual({ code: CODE, state: STATE, verifier: VERIFIER });
    expect(parseFinishFragment(`#verifier=${VERIFIER}&state=${STATE}&code=${CODE}`)).not.toBeNull();
    for (const bad of [
      '',
      '#',
      frag.slice(1),
      `${frag}&x=1`,
      `${frag}&code=${CODE}`,
      `#code=${CODE}&state=${STATE}`,
      `#code=${CODE}&verifier=${VERIFIER}`,
      frag.replace(CODE, CODE.toUpperCase()),
      frag.replace(VERIFIER, 'short'),
      frag.replace(STATE, 'short'),
      `${frag}${'a'.repeat(400)}`
    ])
      expect(parseFinishFragment(bad), bad).toBeNull();
  });
});

describe('returning to the sign-in page after signing in', () => {
  it('safeNext carries only a valid request, in its one spelling', () => {
    const path = `/app-signin?challenge=${CHALLENGE}&state=${STATE}&platform=desktop`;
    expect(safeNext(path)).toBe(path);
    for (const bad of [
      '/app-signin',
      '/app-signin?',
      `${path}&next=//evil.example`,
      `${path}#x`,
      `/app-signin?platform=desktop&state=${STATE}&challenge=${CHALLENGE}`,
      `/app-signin/finish#code=${CODE}`,
      `/app-signin/../admin?${path.split('?')[1]}`,
      `//evil.example${path}`,
      path.replace('desktop', 'linux')
    ])
      expect(safeNext(bad), bad).toBeNull();
  });
});

describe('the app’s own window names itself in its User-Agent', () => {
  const BASE =
    'Mozilla/5.0 (Linux; Android 16; Pixel 6) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Mobile Safari/537.36';
  it('is one token at the end, naming one of three kinds', () => {
    expect(appKind(`${BASE} DynodocApp/0.1.0 (android)`)).toBe('android');
    expect(appKind(`${BASE} DynodocApp/10.20.300 (desktop)`)).toBe('desktop');
    expect(appKind('DynodocApp/1.2.3 (ios)')).toBe('ios');
    expect(appKind(`${BASE} DynodocApp/0.1.0 (ios) extra`)).toBe('ios');
  });
  it('is nothing else', () => {
    for (const bad of [
      null,
      undefined,
      '',
      BASE,
      `${BASE} DynodocApp/0.1.0`,
      `${BASE} DynodocApp/0.1.0 (web)`,
      `${BASE} DynodocApp/0.1.0 (Android)`,
      `${BASE} DynodocApp/0.1 (android)`,
      `${BASE} DynodocApp/0.1.0.0 (android)`,
      `${BASE} DynodocApp/0.1.0 (android)x`,
      `${BASE} xDynodocApp/0.1.0 (android)`,
      `${BASE} dynodocapp/0.1.0 (android)`,
      `${BASE} DynodocApp/0.1.0 (android) DynodocApp/0.1.0 (ios)`,
      `${BASE} ${'x'.repeat(1100)} DynodocApp/0.1.0 (android)`
    ])
      expect(appKind(bad), String(bad).slice(-50)).toBeNull();
  });
});

describe('the version in the app’s token: one grammar for the app and the server', () => {
  const fixture = JSON.parse(
    readFileSync(
      resolve(process.cwd(), '..', '..', 'apps', 'desktop', 'fixtures', 'app-token.json'),
      'utf8'
    )
  ) as { cases: { raw: string; version: string }[] };
  const BASE = 'Mozilla/5.0 (Linux; Android 16) Chrome/133.0.0.0 Mobile Safari/537.36';
  const generated = [
    { raw: `1.2.3-${'x'.repeat(5000)}`, version: '1.2.3' },
    { raw: `${'9'.repeat(400)}.0.0`, version: '0.0.0' }
  ];

  it('normalises every fixture the way the producers do', () => {
    expect(fixture.cases.length).toBeGreaterThan(15);
    for (const { raw, version } of [...fixture.cases, ...generated])
      expect(normaliseAppVersion(raw), JSON.stringify(raw.slice(0, 40))).toBe(version);
  });

  it('accepts the token of every normalised version, for every kind, and nothing it was not given', () => {
    for (const { raw, version } of [...fixture.cases, ...generated]) {
      expect(version).toMatch(APP_VERSION);
      for (const kind of ['desktop', 'android', 'ios'] as const)
        expect(
          appKind(`${BASE} DynodocApp/${version} (${kind})`),
          `${raw.slice(0, 30)} ${kind}`
        ).toBe(kind);
    }
  });

  it('refuses a token carrying a version that was not normalised, which is why producers normalise', () => {
    for (const raw of [
      '0.1.0-beta.1',
      '10.0.0+build5',
      '1.2.3.4',
      '',
      '1.2',
      'v1.2.3',
      '1234567890.0.0'
    ])
      expect(appKind(`${BASE} DynodocApp/${raw} (android)`), JSON.stringify(raw)).toBeNull();
    // Dates and long numbers within the grammar are accepted: nothing here has a three-digit limit.
    expect(appKind(`${BASE} DynodocApp/2026.10.22 (desktop)`)).toBe('desktop');
    expect(appKind(`${BASE} DynodocApp/123456789.0.1 (ios)`)).toBe('ios');
  });
});
