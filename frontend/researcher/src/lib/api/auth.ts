/**
 * Thin typed client for the `/auth/*` routes.
 *
 * These routes predate the utoipa annotations and are NOT in `openapi.yaml`, so they
 * cannot go through the generated openapi-fetch client. This hand-written client is the
 * one documented exception (noted in SETUP.md); the rest of the surface uses the
 * generated client in `./client`.
 *
 * Flow (PoC): `magicLink(email)` returns `magic_link_token` directly in the response
 * (local-dev affordance — production emails it), then `verify(token)` exchanges it for
 * an access + refresh token pair, and `refresh(refreshToken)` rotates the pair.
 */
import { API_BASE_URL } from './config';

export interface MagicLinkResponse {
  magic_link_token: string;
  verify_url: string;
  expires_at: string;
}

export interface TokenResponse {
  access_token: string;
  token_type: string;
  expires_in: number;
  refresh_token: string;
}

export class AuthError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message);
    this.name = 'AuthError';
  }
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE_URL}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body)
  });
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new AuthError(res.status, text || `auth request failed (${res.status})`);
  }
  return (await res.json()) as T;
}

/** Request a magic link. In the PoC the one-time token comes back in the response. */
export function magicLink(email: string, displayName?: string): Promise<MagicLinkResponse> {
  return post<MagicLinkResponse>('/auth/magic-link', {
    email,
    display_name: displayName
  });
}

/** Exchange a magic-link token for an access + refresh token pair. */
export function verify(token: string): Promise<TokenResponse> {
  return post<TokenResponse>('/auth/verify', { token });
}

/** Rotate a refresh token and mint a fresh access token. */
export function refresh(refreshToken: string): Promise<TokenResponse> {
  return post<TokenResponse>('/auth/refresh', { refresh_token: refreshToken });
}
