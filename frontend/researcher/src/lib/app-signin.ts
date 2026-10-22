// The browser half of signing a Dynodoc app (desktop or phone) in. Shapes only; no secrets are
// made or kept here. docs/decisions/017-native-app-sign-in-handover.md has the whole flow.
//
//   app  --opens the system browser at-->  /app-signin?challenge=..&state=..&platform=..
//   page --POST /auth/app-signin, person pressed Continue--> a one-time code
//   page --navigates to-->  dynodoc://signin?code=..&state=..     (the app takes over)
//   app  --opens in its own window-->  /app-signin/finish#code=..&state=..&verifier=..
//   page --POST /auth/app-signin/redeem-->  a session cookie in the app's window
//
// Every value is checked for length and characters before it is used for anything, on both
// sides and again by the API.

export const PLATFORMS = ['desktop', 'android', 'ios'] as const;
export type Platform = (typeof PLATFORMS)[number];

/** What the person is told: which kind of device the app is on. */
export const PLATFORM_LABEL: Record<Platform, string> = {
  desktop: 'computer',
  android: 'Android phone',
  ios: 'iPhone'
};

export interface AppSignin {
  challenge: string;
  state: string;
  platform: Platform;
}

// base64url(SHA-256(verifier)): 43 characters; the last one carries four bits, so only these
// sixteen can end a canonical encoding of 32 bytes.
const CHALLENGE = /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/;
const STATE = /^[A-Za-z0-9_-]{16,64}$/;
const CODE = /^[0-9a-f]{64}$/;
const VERIFIER = /^[A-Za-z0-9._~-]{43,128}$/;

export const isChallenge = (v: unknown): v is string => typeof v === 'string' && CHALLENGE.test(v);
export const isState = (v: unknown): v is string => typeof v === 'string' && STATE.test(v);
export const isCode = (v: unknown): v is string => typeof v === 'string' && CODE.test(v);
export const isVerifier = (v: unknown): v is string => typeof v === 'string' && VERIFIER.test(v);
export const isPlatform = (v: unknown): v is Platform =>
  typeof v === 'string' && (PLATFORMS as readonly string[]).includes(v);

/** The three values of a request, checked. Anything else (a missing value, a wrong type) is null. */
export function asAppSignin(values: {
  challenge?: unknown;
  state?: unknown;
  platform?: unknown;
}): AppSignin | null {
  const { challenge, state, platform } = values;
  if (!isChallenge(challenge) || !isState(state) || !isPlatform(platform)) return null;
  return { challenge, state, platform };
}

/** The request the app put in the page address: exactly these three values, each once. */
export function parseAppSignin(query: URLSearchParams): AppSignin | null {
  const keys = [...query.keys()];
  if (keys.length !== 3 || new Set(keys).size !== 3) return null;
  return asAppSignin({
    challenge: query.get('challenge'),
    state: query.get('state'),
    platform: query.get('platform')
  });
}

/** The page address for a request, rebuilt from its validated parts in one fixed order. */
export function appSigninPath(request: AppSignin): string {
  return `/app-signin?challenge=${request.challenge}&state=${request.state}&platform=${request.platform}`;
}

/** The link that hands the code to the app. Built only from validated parts. */
export function appLink(code: string, state: string): string {
  if (!isCode(code) || !isState(state)) throw new Error('not a hand-over code');
  return `dynodoc://signin?code=${code}&state=${state}`;
}

export interface Finish {
  code: string;
  state: string;
  verifier: string;
}

/** The fragment the app puts on `/app-signin/finish`: code, state and verifier, each once. */
export function parseFinishFragment(hash: string): Finish | null {
  if (hash.length > 400 || !hash.startsWith('#')) return null;
  const params = new URLSearchParams(hash.slice(1));
  const keys = [...params.keys()];
  if (keys.length !== 3 || new Set(keys).size !== 3) return null;
  const code = params.get('code');
  const state = params.get('state');
  const verifier = params.get('verifier');
  if (!isCode(code) || !isState(state) || !isVerifier(verifier)) return null;
  return { code, state, verifier };
}

// The Dynodoc app's own window says what it is in its User-Agent: the web view's normal one with
// `DynodocApp/<version> (desktop|android|ios)` added at the end. The finish page and the redeem
// route act only for such a request. It is not a secret (anyone can send it); it keeps a crafted
// link from signing an ordinary browser in as someone else (decision 017).
const APP_AGENT =
  /(?:^|\s)DynodocApp\/\d{1,3}\.\d{1,3}\.\d{1,3} \((desktop|android|ios)\)(?=\s|$)/g;

/** The kind of app a User-Agent names, or null: exactly one such token, nothing longer than 1 KiB. */
export function appKind(userAgent: string | null | undefined): Platform | null {
  if (!userAgent || userAgent.length > 1024) return null;
  const found = [...userAgent.matchAll(APP_AGENT)];
  return found.length === 1 ? (found[0][1] as Platform) : null;
}
