// The web server's half of signing a Dynodoc app in (docs/decisions/017). Two POST routes,
// served under /auth/ like the other sign-in routes (so the same edge rate limit and the same
// no-store / no-referrer headers apply):
//
//   POST /auth/app-signin           the person pressed Continue; makes a one-time code for the
//                                   account signed in HERE and returns the link to the app
//   POST /auth/app-signin/redeem    the app's window trades code + state + verifier for a session
//
// Both are same-origin JSON only. Neither reads or writes anything but what is named here, and
// neither puts a code, a verifier or a state in a log, an error message or a URL.
import { json } from '@sveltejs/kit';
import type { Cookies } from '@sveltejs/kit';
import { appLink, asAppSignin, isCode, isState, isVerifier } from '$lib/app-signin';
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

export async function issue(context: Context): Promise<Response> {
  const refusal = sameOriginJson(context);
  if (refusal === 'origin') return reply({ error: 'Invalid origin' }, 403);
  if (refusal === 'type') return reply({ error: 'Invalid request' }, 415);
  const body = await readBody(context.request);
  const parsed =
    body && exactKeys(body, ['challenge', 'state', 'platform']) ? asAppSignin(body) : null;
  if (!parsed) return reply({ error: 'Invalid request' }, 400);
  const token = await accessToken(context.cookies, context.url.protocol === 'https:');
  if (!token) return reply({ error: 'Sign in first, then try again.' }, 401);
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
      body: JSON.stringify(parsed),
      signal: AbortSignal.timeout(15_000)
    });
  } catch {
    return reply({ error: 'The sign-in could not be prepared. Please try again.' }, 503);
  }
  if (response.status === 429)
    return reply({ error: 'Too many sign-in attempts. Wait a minute, then try again.' }, 429);
  if (response.status === 401)
    return reply({ error: 'Your sign-in here has ended. Sign in again, then try again.' }, 401);
  const answer: unknown = response.ok ? await response.json().catch(() => null) : null;
  const code = (answer as { code?: unknown } | null)?.code;
  if (!isCode(code))
    return reply({ error: 'The sign-in could not be prepared. Please try again.' }, 502);
  return reply({ url: appLink(code, parsed.state) }, 200);
}

export async function redeem(context: Context): Promise<Response> {
  const refusal = sameOriginJson(context);
  if (refusal === 'origin') return reply({ error: 'Invalid origin' }, 403);
  if (refusal === 'type') return reply({ error: 'Invalid request' }, 415);
  const body = await readBody(context.request);
  if (
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
      ...(client ? { client } : {})
    })) as Tokens;
  } catch {
    return reply({ error: REFUSED }, 401);
  }
  await saveSession(context.cookies, tokens, context.url.protocol === 'https:');
  return reply({ ok: true }, 200);
}

/**
 * The visitor's address, for the API's per-address limit, only when it tells visitors apart.
 * Behind a reverse proxy that does not forward the address (SvelteKit then sees the proxy's own
 * loopback or private address for everyone) a shared bucket would let one visitor lock every
 * other out, so nothing is sent and only the per-code limit and the edge's own limit apply.
 * To get the real address behind a proxy set ADDRESS_HEADER (and XFF_DEPTH) for adapter-node.
 */
export function clientHint(getClientAddress?: () => string): string | undefined {
  try {
    const address = getClientAddress?.();
    return address && address.length <= 64 && isInformativeAddress(address) ? address : undefined;
  } catch {
    return undefined;
  }
}

/** False for loopback, private, link-local, shared (CGNAT), unspecified and malformed addresses. */
export function isInformativeAddress(address: string): boolean {
  const mapped = /^::ffff:(\d{1,3}(?:\.\d{1,3}){3})$/i.exec(address);
  const text = mapped ? mapped[1] : address;
  const v4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(text);
  if (v4) {
    const [a, b, c, d] = v4.slice(1).map(Number);
    if ([a, b, c, d].some((part) => part > 255)) return false;
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
  if (!/^[0-9a-f:]+$/i.test(text) || !text.includes(':')) return false;
  const lower = text.toLowerCase();
  return !(
    lower === '::' ||
    lower === '::1' ||
    /^fe[89ab]/.test(lower) ||
    /^f[cd]/.test(lower) ||
    lower.startsWith('::ffff:')
  );
}
