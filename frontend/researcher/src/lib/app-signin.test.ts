import { describe, expect, it } from 'vitest';
import {
  appLink,
  appSigninPath,
  isChallenge,
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
      good.replace(CHALLENGE, `${CHALLENGE.slice(0, 40)}%2d%2d%4d`),
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
