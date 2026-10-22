import { error, redirect } from '@sveltejs/kit';
import { appSigninPath, parseAppSignin, PLATFORM_LABEL } from '$lib/app-signin';
import { getConfig } from '$lib/server/config';
import { accessToken, engineUrl } from '$lib/server/session';
import type { PageServerLoad } from './$types';

// The page a Dynodoc app opens in the system browser to get signed in (docs/decisions/017).
// It never makes a code by itself: the person must be signed in HERE and must press Continue,
// which is a separate same-origin request (POST /auth/app-signin).
export const load: PageServerLoad = async ({ url, cookies }) => {
  const config = getConfig();
  // The public homepage and the operator console host have no app to hand a session to.
  if (config.adminSite) error(404, 'Not found');
  const request = parseAppSignin(url.searchParams);
  if (!request) return { phase: 'invalid' as const };

  // Not signed in (or the session has ended): the usual sign-in, then back to this very request,
  // rebuilt from its validated parts so nothing else rides along. An installation with no way
  // to sign in at all says so here instead of sending the person to a page that cannot help.
  const signIn = () => {
    const methods =
      config.emailSignIn || Boolean(config.googleClientId && config.secrets.googleClientSecret);
    if (!methods) return { phase: 'no-method' as const };
    redirect(303, `/login?next=${encodeURIComponent(appSigninPath(request))}`);
  };
  const token = await accessToken(cookies, url.protocol === 'https:');
  if (!token) return signIn();
  const response = await fetch(`${engineUrl()}/auth/me`, {
    headers: { authorization: `Bearer ${token}` },
    signal: AbortSignal.timeout(10_000)
  }).catch(() => null);
  if (!response?.ok) return signIn();
  const me = (await response.json()) as { email?: unknown };
  if (typeof me.email !== 'string') return signIn();
  return {
    phase: 'ready' as const,
    email: me.email,
    device: PLATFORM_LABEL[request.platform],
    request
  };
};
