import { env } from '$env/dynamic/private';
import { EncryptJWT, jwtDecrypt } from 'jose';
import { createHash } from 'node:crypto';
import type { Cookies } from '@sveltejs/kit';

export const engineUrl = () => env.ENGINE_API_URL || 'http://127.0.0.1:8080';
const cookieName = 'dynodoc_session';
export interface Tokens {
  access_token: string;
  refresh_token: string;
  expires_in: number;
}
interface StoredSession extends Tokens {
  expires_at: number;
}
function key() {
  if (!env.SESSION_SECRET || env.SESSION_SECRET.length < 32)
    throw new Error('SESSION_SECRET must contain at least 32 characters');
  return createHash('sha256').update(env.SESSION_SECRET).digest();
}
export async function saveSession(cookies: Cookies, tokens: Tokens, secure: boolean) {
  const value = await new EncryptJWT({
    ...tokens,
    expires_at: Date.now() + tokens.expires_in * 1000
  })
    .setProtectedHeader({ alg: 'dir', enc: 'A256GCM' })
    .setIssuedAt()
    .setExpirationTime('30d')
    .encrypt(key());
  cookies.set(cookieName, value, {
    path: '/',
    httpOnly: true,
    sameSite: 'lax',
    secure,
    maxAge: 30 * 86400
  });
}
export function clearSession(cookies: Cookies) {
  cookies.delete(cookieName, { path: '/' });
}
export async function readSession(cookies: Cookies): Promise<StoredSession | null> {
  const value = cookies.get(cookieName);
  if (!value) return null;
  try {
    const { payload } = await jwtDecrypt(value, key());
    return payload as unknown as StoredSession;
  } catch {
    return null;
  }
}
const refreshes = new Map<string, Promise<Tokens>>();
export async function accessToken(cookies: Cookies, secure: boolean): Promise<string | null> {
  const session = await readSession(cookies);
  if (!session) return null;
  if (session.expires_at > Date.now() + 60_000) return session.access_token;
  let pending = refreshes.get(session.refresh_token);
  if (!pending) {
    pending = enginePost('/auth/refresh', {
      refresh_token: session.refresh_token
    }) as Promise<Tokens>;
    refreshes.set(session.refresh_token, pending);
    // Keep the successful rotation briefly for simultaneous requests using the old cookie.
    void pending
      .finally(() => setTimeout(() => refreshes.delete(session.refresh_token), 30_000))
      .catch(() => {});
  }
  try {
    const tokens = await pending;
    await saveSession(cookies, tokens, secure);
    return tokens.access_token;
  } catch {
    clearSession(cookies);
    return null;
  }
}
export async function enginePost(path: string, body: unknown) {
  const response = await fetch(engineUrl() + path, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...(env.WEB_SERVICE_KEY ? { 'x-dynodoc-service-key': env.WEB_SERVICE_KEY } : {})
    },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(15_000)
  });
  if (!response.ok) throw new Error('The request could not be completed. Please try again.');
  return response.json();
}

/** Check the active account with the engine before calling a connected provider. */
export async function activeIdentity(cookies: Cookies, secure: boolean): Promise<string | null> {
  const token = await accessToken(cookies, secure);
  if (!token) return null;
  const response = await fetch(`${engineUrl()}/auth/me`, {
    headers: { authorization: `Bearer ${token}` },
    signal: AbortSignal.timeout(10000)
  });
  if (!response.ok) return null;
  return (await response.json()).identity_id as string;
}
export async function sealProvider(value: Record<string, unknown>, seconds: number) {
  return new EncryptJWT(value)
    .setProtectedHeader({ alg: 'dir', enc: 'A256GCM' })
    .setIssuedAt()
    .setExpirationTime(Math.floor(Date.now() / 1000) + seconds)
    .encrypt(key());
}
export async function openProvider(value: string | undefined) {
  if (!value) return null;
  try {
    return (await jwtDecrypt(value, key())).payload;
  } catch {
    return null;
  }
}
