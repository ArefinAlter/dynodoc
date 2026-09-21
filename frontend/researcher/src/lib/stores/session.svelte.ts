/** Sessions are encrypted, HttpOnly cookies; access tokens never enter browser storage. */
import { clearRecovery, setRecoveryPreference } from '$lib/workspace/recovery';
export const session = $state<{ accessToken: null; refreshToken: null; authenticated: boolean }>({
  accessToken: null,
  refreshToken: null,
  authenticated: false
});
export function isAuthenticated() {
  return session.authenticated;
}
export async function loadSession() {
  const res = await fetch('/auth/session');
  session.authenticated = res.ok && (await res.json()).authenticated;
}
export async function clearSession() {
  session.authenticated = false;
  // Disable writes before clearing storage, including writes in other open tabs.
  try {
    setRecoveryPreference(false);
    localStorage.setItem('dynodoc.session-ended', String(Date.now()));
  } catch {
    /* Storage can be unavailable in private browsing. */
  }
  window.dispatchEvent(new Event('dynodoc:signout'));
  await clearRecovery().catch(() => {});
  await fetch('/auth/logout', { method: 'POST' });
}
// Compatibility exports for existing API callers. Cookie sessions handle rotation server-side.
export function setAccessToken(_token: string) {
  void _token;
  session.authenticated = true;
}
export function setTokens(_access: string, _refresh: string) {
  void _access;
  void _refresh;
  session.authenticated = true;
}
