// The browser half of signing a Dynodoc app (desktop or phone) in. Shapes only; no secrets are
// made or kept here. docs/decisions/017-native-app-sign-in-handover.md has the whole flow.
//
//   app  --opens the system browser at-->  /app-signin?challenge=..&state=..&platform=..
//   page --POST /auth/app-signin, person pressed Continue--> a one-time code
//   page --navigates to-->  dynodoc://signin?code=..&state=..     (the app takes over)
//   app  --opens in its own window-->  /app-signin/finish#code=..&state=..&verifier=..
//   page --POST /auth/app-signin/redeem-->  a session cookie in the app's window
//
// A desktop app that keeps its own session (the standalone desktop, decision 020) adds ONE optional
// value to the request, `redirect_port`, and gets the code on its own loopback listener instead of a
// custom-scheme link: `http://127.0.0.1:<port>/signin?code=..&state=..`. Then the app itself trades
// code, state and verifier at /auth/app-signin/redeem (there is no window to finish in). Nothing
// else about the request, the code or the redeem route changes.

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
  /** Desktop only: the port of the app's loopback listener on this computer (127.0.0.1). */
  redirectPort?: number;
}

/** What the app may ask for: an unprivileged port, as plain decimal digits with no sign or zeros. */
const PORT = /^[1-9][0-9]{3,4}$/;
export const isRedirectPort = (v: unknown): v is number =>
  typeof v === 'number' && Number.isInteger(v) && v >= 1024 && v <= 65535;

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
  redirectPort?: unknown;
}): AppSignin | null {
  const { challenge, state, platform, redirectPort } = values;
  if (!isChallenge(challenge) || !isState(state) || !isPlatform(platform)) return null;
  if (redirectPort === undefined) return { challenge, state, platform };
  // A loopback listener is a thing of a computer: a phone's app is reached by its own link.
  if (!isRedirectPort(redirectPort) || platform !== 'desktop') return null;
  return { challenge, state, platform, redirectPort };
}

/**
 * The request the app put in the page address: exactly these three values, each once, and for a
 * desktop app one more, `redirect_port`, each once.
 */
export function parseAppSignin(query: URLSearchParams): AppSignin | null {
  const keys = [...query.keys()];
  const withPort = keys.length === 4;
  if ((keys.length !== 3 && !withPort) || new Set(keys).size !== keys.length) return null;
  if (withPort) {
    const port = query.get('redirect_port');
    if (port === null || !PORT.test(port)) return null;
    return asAppSignin({
      challenge: query.get('challenge'),
      state: query.get('state'),
      platform: query.get('platform'),
      redirectPort: Number(port)
    });
  }
  return asAppSignin({
    challenge: query.get('challenge'),
    state: query.get('state'),
    platform: query.get('platform')
  });
}

/** The page address for a request, rebuilt from its validated parts in one fixed order. */
export function appSigninPath(request: AppSignin): string {
  const base = `/app-signin?challenge=${request.challenge}&state=${request.state}&platform=${request.platform}`;
  return request.redirectPort === undefined
    ? base
    : `${base}&redirect_port=${request.redirectPort}`;
}

/** The link that hands the code to the app. Built only from validated parts. */
export function appLink(code: string, state: string): string {
  if (!isCode(code) || !isState(state)) throw new Error('not a hand-over code');
  return `dynodoc://signin?code=${code}&state=${state}`;
}

/**
 * The address that hands the code to a desktop app's own listener: this computer only
 * (127.0.0.1), one fixed path, the two values and nothing else. Built only from validated parts.
 */
export function loopbackLink(port: number, code: string, state: string): string {
  if (!isRedirectPort(port) || !isCode(code) || !isState(state))
    throw new Error('not a hand-over code');
  return `http://127.0.0.1:${port}/signin?code=${code}&state=${state}`;
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
// route act only for such a request. It is NOT authentication and not a secret (anyone can send it,
// and the kind of app the API compares comes from this very header): it is a speed bump that keeps
// a crafted link, which cannot make a browser send it, from signing an ordinary browser in as
// someone else (decision 017).
//
// ONE grammar for the version, shared by the three producers (Rust `signin::app_token`, Kotlin
// `AppToken`) and this reader, with fixtures all three run (apps/desktop/fixtures/app-token.json):
// `<major>.<minor>.<patch>`, each one to nine digits. A producer takes the leading dotted triple of
// its version and drops whatever follows (`-beta.1`, `+build5`, a fourth number), and writes
// `0.0.0` when there is none; so the token is never malformed and never silently refused.
const VERSION = String.raw`\d{1,9}\.\d{1,9}\.\d{1,9}`;
export const APP_VERSION = new RegExp(`^${VERSION}$`);
const APP_AGENT = new RegExp(
  String.raw`(?:^|\s)DynodocApp/${VERSION} \((desktop|android|ios)\)(?=\s|$)`,
  'g'
);

/**
 * What a producer of the token does with its own version (the web app only READS the token; this
 * is the reference the fixtures check the Rust and Kotlin producers against): the leading dotted
 * triple of one to nine digits each, anything after it dropped, `0.0.0` when there is none.
 */
export function normaliseAppVersion(raw: string): string {
  const leading = /^(\d{1,9})\.(\d{1,9})\.(\d{1,9})(?!\d)/.exec(raw);
  return leading ? `${leading[1]}.${leading[2]}.${leading[3]}` : '0.0.0';
}

/** The kind of app a User-Agent names, or null: exactly one such token, nothing longer than 1 KiB. */
export function appKind(userAgent: string | null | undefined): Platform | null {
  if (!userAgent || userAgent.length > 1024) return null;
  const found = [...userAgent.matchAll(APP_AGENT)];
  return found.length === 1 ? (found[0][1] as Platform) : null;
}
