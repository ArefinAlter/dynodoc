// The web server's half of signing a Dynodoc app in (docs/decisions/017).
//
//   the confirmation page's FORM POST   the person pressed Continue; makes a one-time code for the
//                                       account signed in HERE and returns the link to the app
//   POST /auth/app-signin/redeem        the app's window trades code + state + verifier for a session
//
// Asking for a code is possible only from a real, user-activated, top-level form submission of
// this very site: the browser itself says so in `Sec-Fetch-*` headers that a script cannot set or
// forge (`fetch`, XHR and a form a script submits without a click all differ in at least one of
// them). Without that, a script on any page of this origin (a cross-site-scripting flaw) could ask
// for a code, keep the code and the verifier it chose, and trade them from its own client for a
// 30-day session that it can carry away: the session cookie never reaches a script, but a code does.
// A browser too old to send the headers gets a plain page saying so, not a code.
//
// Redeeming is possible only from the Dynodoc app's own window, which tells the server so in its
// User-Agent (`DynodocApp/<version> (desktop|android|ios)`). A request without that token is refused
// before anything else. The token is not a secret: someone who forges it only signs THEMSELVES in.
// What it stops is a crafted link that signs a visitor's ordinary browser in as the attacker.
//
// Neither route puts a code, a verifier or a state in a log, an error message or a URL.
import { json } from '@sveltejs/kit';
import type { Cookies } from '@sveltejs/kit';
import { createHash } from 'node:crypto';
import {
  appKind,
  appLink,
  isCode,
  isState,
  isVerifier,
  loopbackLink,
  type AppSignin,
  type Platform
} from '$lib/app-signin';
import { getConfig } from './config';
import { accessToken, engineUrl, enginePost, saveSession, type Tokens } from './session';

interface Context {
  request: Request;
  url: URL;
  cookies: Cookies;
  getClientAddress?: () => string;
}

const NO_STORE = { 'cache-control': 'private, no-store' };
const reply = (body: unknown, status: number) => json(body, { status, headers: NO_STORE });

/** The one message every failed redeem gets: it says nothing about which check failed. */
export const REFUSED =
  'This sign-in could not be completed. Go back to the app and choose Sign in with your browser again.';

/** Same origin, and JSON: a form posted from another site is neither. */
function sameOriginJson({ request, url }: Context): 'origin' | 'type' | null {
  if (request.headers.get('origin') !== url.origin) return 'origin';
  if (!(request.headers.get('content-type') ?? '').toLowerCase().startsWith('application/json'))
    return 'type';
  return null;
}

async function readBody(request: Request): Promise<Record<string, unknown> | null> {
  const body = await request.json().catch(() => null);
  return body && typeof body === 'object' && !Array.isArray(body)
    ? (body as Record<string, unknown>)
    : null;
}

const exactKeys = (body: Record<string, unknown>, keys: string[]) =>
  Object.keys(body).length === keys.length && keys.every((key) => key in body);

// --- the four characters that pair a browser page with the app that asked for it ---------------

// 32 symbols without 0, 1, I and O, so that a person can read one out and compare it.
const PAIRING_ALPHABET = '23456789ABCDEFGHJKLMNPQRSTUVWXYZ';

/**
 * Four characters derived from the state (the low five bits of the first four bytes of its
 * SHA-256). The app shows the same four while its request is waiting, and the confirmation page
 * shows them next to the question "is this the sign-in you started?", so a person can tell their
 * own request from one that another program on the same device started. The app (`signin.rs`)
 * computes the same function; both have the same test vectors.
 */
export function pairingCode(state: string): string {
  const digest = createHash('sha256').update(state, 'utf8').digest();
  return [0, 1, 2, 3].map((i) => PAIRING_ALPHABET[digest[i] & 31]).join('');
}

// --- asking for a code: the confirmation page's form ---------------------------------------------

export type FormOutcome =
  | { ok: true; url: string; loopback: boolean }
  | {
      ok: false;
      status: number;
      problem: 'browser' | 'blocked' | 'signed-out' | 'busy' | 'failed';
    };

/**
 * Whether the request is a real, user-activated, top-level form submission of this site, as the
 * browser itself reports it. `null` when it is; otherwise what to tell the person.
 */
export function formSubmission(request: Request, url: URL): 'browser' | 'blocked' | null {
  const header = (name: string) => request.headers.get(name);
  const site = header('sec-fetch-site');
  const mode = header('sec-fetch-mode');
  const user = header('sec-fetch-user');
  const dest = header('sec-fetch-dest');
  // A browser that sends none of them cannot prove anything: fail closed, and say why.
  if (site === null && mode === null && user === null && dest === null) return 'browser';
  if (site !== 'same-origin' || mode !== 'navigate' || user !== '?1' || dest !== 'document')
    return 'blocked';
  if (header('origin') !== url.origin) return 'blocked';
  const type = (header('content-type') ?? '').toLowerCase();
  if (
    !type.startsWith('application/x-www-form-urlencoded') &&
    !type.startsWith('multipart/form-data')
  )
    return 'blocked';
  return null;
}

/** The action of the confirmation page: one code, for the account signed in here, for this request. */
export async function issueForForm(
  { request, url, cookies }: Context,
  parsed: AppSignin
): Promise<FormOutcome> {
  const refusal = formSubmission(request, url);
  if (refusal) return { ok: false, status: refusal === 'browser' ? 400 : 403, problem: refusal };
  const token = await accessToken(cookies, url.protocol === 'https:');
  if (!token) return { ok: false, status: 401, problem: 'signed-out' };
  const key = getConfig().secrets.webServiceKey?.expose();
  let response: Response;
  try {
    response = await fetch(`${engineUrl()}/auth/app-handover`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${token}`,
        ...(key ? { 'x-dynodoc-service-key': key } : {})
      },
      // Exactly the three values the API knows; the app's loopback port stays on this side.
      body: JSON.stringify({
        challenge: parsed.challenge,
        state: parsed.state,
        platform: parsed.platform
      }),
      signal: AbortSignal.timeout(15_000)
    });
  } catch {
    return { ok: false, status: 503, problem: 'failed' };
  }
  if (response.status === 429) return { ok: false, status: 429, problem: 'busy' };
  if (response.status === 401) return { ok: false, status: 401, problem: 'signed-out' };
  const answer: unknown = response.ok ? await response.json().catch(() => null) : null;
  const code = (answer as { code?: unknown } | null)?.code;
  if (!isCode(code)) return { ok: false, status: 502, problem: 'failed' };
  // A desktop app that listens on this computer gets the code there (decision 020); every other
  // app gets the custom-scheme link as before.
  return parsed.redirectPort === undefined
    ? { ok: true, url: appLink(code, parsed.state), loopback: false }
    : { ok: true, url: loopbackLink(parsed.redirectPort, code, parsed.state), loopback: true };
}

// --- trading a code for a session: the app's own window ------------------------------------------

export async function redeem(context: Context): Promise<Response> {
  const refusal = sameOriginJson(context);
  if (refusal === 'origin') return reply({ error: 'Invalid origin' }, 403);
  if (refusal === 'type') return reply({ error: 'Invalid request' }, 415);
  // Only the app's own window (see the top of this file). Everything else gets the one answer.
  const app: Platform | null = appKind(context.request.headers.get('user-agent'));
  const body = await readBody(context.request);
  if (
    !app ||
    !body ||
    !exactKeys(body, ['code', 'state', 'verifier']) ||
    !isCode(body.code) ||
    !isState(body.state) ||
    !isVerifier(body.verifier)
  )
    return reply({ error: REFUSED }, 401);
  const client = clientHint(context.getClientAddress);
  let tokens: Tokens;
  try {
    tokens = (await enginePost('/auth/app-handover/redeem', {
      code: body.code,
      state: body.state,
      verifier: body.verifier,
      app,
      ...(client ? { client } : {})
    })) as Tokens;
  } catch {
    return reply({ error: REFUSED }, 401);
  }
  await saveSession(context.cookies, tokens, context.url.protocol === 'https:');
  return reply({ ok: true }, 200);
}

// --- the visitor's address, for the API's per-address limit --------------------------------------

/**
 * The visitor's address, for the API's per-address limit, only when it tells visitors apart.
 * Behind a reverse proxy that does not forward the address (SvelteKit then sees the proxy's own
 * loopback or private address for everyone) a shared bucket would let one visitor lock every
 * other out, so nothing is sent and only the per-code limit and the edge's own limit apply.
 * To get the real address behind a proxy set ADDRESS_HEADER (and XFF_DEPTH, the number of trusted
 * proxies, never more) for adapter-node.
 */
export function clientHint(getClientAddress?: () => string): string | undefined {
  try {
    const address = getClientAddress?.();
    return address && address.length <= 64 && isInformativeAddress(address) ? address : undefined;
  } catch {
    return undefined;
  }
}

const hex16 = (n: number) => n.toString(16);

/** An IPv6 address as eight 16-bit groups, in any of its spellings (`::`, a dotted tail, a zone id). */
export function ipv6Groups(address: string): number[] | null {
  let text = address.toLowerCase();
  const zone = text.indexOf('%');
  if (zone >= 0) text = text.slice(0, zone);
  if (text === '' || !/^[0-9a-f:.]+$/.test(text) || !text.includes(':')) return null;
  const dotted = /(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(text);
  if (dotted) {
    const [a, b, c, d] = dotted.slice(1).map(Number);
    if ([a, b, c, d].some((part) => part > 255)) return null;
    text = `${text.slice(0, text.length - dotted[0].length)}${hex16((a << 8) | b)}:${hex16((c << 8) | d)}`;
  }
  const halves = text.split('::');
  if (halves.length > 2) return null;
  const groupsOf = (part: string) => (part === '' ? [] : part.split(':'));
  const head = groupsOf(halves[0]);
  const tail = halves.length === 2 ? groupsOf(halves[1]) : [];
  const given = [...head, ...tail];
  if (given.some((g) => !/^[0-9a-f]{1,4}$/.test(g))) return null;
  const missing = 8 - given.length;
  if (halves.length === 1 ? missing !== 0 : missing < 1) return null;
  return [...head, ...Array<string>(halves.length === 2 ? missing : 0).fill('0'), ...tail].map(
    (g) => parseInt(g, 16)
  );
}

function informativeV4(a: number, b: number): boolean {
  return !(
    a === 0 ||
    a === 10 ||
    a === 127 ||
    (a === 100 && b >= 64 && b <= 127) ||
    (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) ||
    a >= 224
  );
}

/**
 * False for loopback, private, link-local, shared (CGNAT), unspecified, multicast and malformed
 * addresses, in whatever spelling (`0:0:0:0:0:0:0:1`, `::ffff:7f00:1`, `fe80::1%eth0`, ...), and
 * for IPv6 forms that carry an IPv4 address (mapped, compatible, NAT64, 6to4) whose IPv4 address
 * is one of those.
 */
export function isInformativeAddress(address: string): boolean {
  const v4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(address);
  if (v4) {
    const [a, b, c, d] = v4.slice(1).map(Number);
    return [a, b, c, d].every((part) => part <= 255) && informativeV4(a, b);
  }
  const g = ipv6Groups(address);
  if (!g) return false;
  const zeros = (from: number, to: number) => g.slice(from, to).every((x) => x === 0);
  const embedded = (group: number) => informativeV4(g[group] >> 8, g[group] & 255);
  if (zeros(0, 7) && (g[7] === 0 || g[7] === 1)) return false; // :: and ::1
  if (zeros(0, 5) && g[5] === 0xffff) return embedded(6); // ::ffff:a.b.c.d (mapped)
  if (zeros(0, 6)) return embedded(6); // ::a.b.c.d (compatible, deprecated)
  if (g[0] === 0x64 && g[1] === 0xff9b && zeros(2, 6)) return embedded(6); // 64:ff9b::/96
  if (g[0] === 0x2002) return embedded(1); // 6to4
  if ((g[0] & 0xffc0) === 0xfe80) return false; // link-local
  if ((g[0] & 0xffc0) === 0xfec0) return false; // site-local (deprecated)
  if ((g[0] & 0xfe00) === 0xfc00) return false; // unique local
  if ((g[0] & 0xff00) === 0xff00) return false; // multicast
  if (g[0] === 0x100 && zeros(1, 4)) return false; // discard-only 100::/64
  return true;
}
