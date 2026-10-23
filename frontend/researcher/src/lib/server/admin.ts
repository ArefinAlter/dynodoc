import { error, redirect, type Cookies } from '@sveltejs/kit';
import { accessToken, engineUrl } from './session';
export async function adminOverview(cookies: Cookies, secure: boolean) {
  const token = await accessToken(cookies, secure);
  if (!token) redirect(303, '/login?next=admin');
  const response = await fetch(`${engineUrl()}/admin/overview`, {
    headers: { authorization: `Bearer ${token}` },
    signal: AbortSignal.timeout(10_000)
  });
  if (response.status === 401) redirect(303, '/login?next=admin');
  if (response.status === 403)
    error(
      403,
      'Administrator access is required. Your workspace account does not grant access to operations.'
    );
  if (!response.ok) error(503, 'The operations overview is temporarily unavailable.');
  return response.json();
}

/**
 * Sign-in and administrator check for a page that needs nothing from the overview. The overview
 * counts every event and table, so a page that only needs to know "is this an administrator"
 * asks for the smallest administrator list instead; the rule is the API's own (403 otherwise).
 */
export async function requireAdministrator(cookies: Cookies, secure: boolean) {
  const token = await accessToken(cookies, secure);
  if (!token) redirect(303, '/login?next=admin');
  const response = await fetch(`${engineUrl()}/admin/records?kind=erasures&q=&offset=0`, {
    headers: { authorization: `Bearer ${token}` },
    signal: AbortSignal.timeout(10_000)
  });
  if (response.status === 401) redirect(303, '/login?next=admin');
  if (response.status === 403)
    error(
      403,
      'Administrator access is required. Your workspace account does not grant access to operations.'
    );
  if (!response.ok) error(503, 'Administrator access could not be checked just now.');
}
