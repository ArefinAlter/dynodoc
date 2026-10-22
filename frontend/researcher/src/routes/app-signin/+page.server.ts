import { error, fail, redirect } from '@sveltejs/kit';
import { appSigninPath, parseAppSignin, PLATFORM_LABEL } from '$lib/app-signin';
import { getConfig } from '$lib/server/config';
import { issueForForm, pairingCode } from '$lib/server/app-signin';
import { accessToken, engineUrl } from '$lib/server/session';
import type { Actions, PageServerLoad } from './$types';

// The page a Dynodoc app opens in the system browser to get signed in (docs/decisions/017).
// It never makes a code by itself: the person must be signed in HERE and must press Continue, which
// submits a real form (see the default action below and `issueForForm`).
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
    // Four characters the app shows too, to tell this request from one another program started.
    pairing: pairingCode(request.state)
  };
};

export const actions: Actions = {
  // Continue. The request is read from the address (validated), never from the form's own fields.
  default: async (event) => {
    if (getConfig().adminSite) error(404, 'Not found');
    const request = parseAppSignin(event.url.searchParams);
    if (!request) return fail(400, { problem: 'invalid' as const });
    const outcome = await issueForForm(event, request);
    if (!outcome.ok) return fail(outcome.status, { problem: outcome.problem });
    return { url: outcome.url };
  }
};
