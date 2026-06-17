/**
 * Auth session: access + refresh tokens, persisted in `localStorage`.
 *
 * PoC trade-off (documented in SETUP.md): tokens live in `localStorage` so a reload
 * keeps the session. A production build would move the refresh token to an
 * httpOnly cookie; the access token can stay in memory. The API surface this app
 * uses is the generated openapi-fetch client; only the `/auth/*` routes (which
 * predate utoipa and are absent from `openapi.yaml`) go through the thin typed
 * client in `./auth`.
 */
import { browser } from '$app/environment';

const ACCESS_KEY = 'dynodoc.access_token';
const REFRESH_KEY = 'dynodoc.refresh_token';

interface SessionState {
  accessToken: string | null;
  refreshToken: string | null;
}

function read(): SessionState {
  if (!browser) return { accessToken: null, refreshToken: null };
  return {
    accessToken: localStorage.getItem(ACCESS_KEY),
    refreshToken: localStorage.getItem(REFRESH_KEY)
  };
}

const initial = read();

/** Reactive session state (Svelte 5 runes). */
export const session = $state<SessionState>(initial);

/** True when an access token is present (does not validate expiry). */
export function isAuthenticated(): boolean {
  return Boolean(session.accessToken);
}

/** Persist a freshly minted token pair. */
export function setTokens(accessToken: string, refreshToken: string): void {
  session.accessToken = accessToken;
  session.refreshToken = refreshToken;
  if (browser) {
    localStorage.setItem(ACCESS_KEY, accessToken);
    localStorage.setItem(REFRESH_KEY, refreshToken);
  }
}

/** Replace only the access token (refresh keeps the rotated successor). */
export function setAccessToken(accessToken: string): void {
  session.accessToken = accessToken;
  if (browser) localStorage.setItem(ACCESS_KEY, accessToken);
}

/** Clear the session (logout / unrecoverable 401). */
export function clearSession(): void {
  session.accessToken = null;
  session.refreshToken = null;
  if (browser) {
    localStorage.removeItem(ACCESS_KEY);
    localStorage.removeItem(REFRESH_KEY);
  }
}
