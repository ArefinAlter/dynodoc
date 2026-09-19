import { env } from '$env/dynamic/private';
import { json, redirect } from '@sveltejs/kit';
import * as oidc from 'openid-client';
import { activeIdentity, sealProvider, openProvider } from '$lib/server/session';
import type { RequestHandler } from './$types';
let configuration: Promise<oidc.Configuration> | undefined;
function google() {
  if (!env.GOOGLE_CLIENT_ID || !env.GOOGLE_CLIENT_SECRET)
    throw new Error('Google is not configured');
  return (configuration ??= oidc
    .discovery(
      new URL('https://accounts.google.com'),
      env.GOOGLE_CLIENT_ID,
      env.GOOGLE_CLIENT_SECRET
    )
    .catch((e) => {
      configuration = undefined;
      throw e;
    }));
}
const scopes = [
  'https://www.googleapis.com/auth/forms.body.readonly',
  'https://www.googleapis.com/auth/forms.responses.readonly'
];
export const GET: RequestHandler = async ({ params, url, cookies }) => {
  const secure = url.protocol === 'https:',
    identity = await activeIdentity(cookies, secure);
  if (!identity) redirect(303, '/login');
  const options = {
    path: '/integrations/google',
    httpOnly: true,
    sameSite: 'lax' as const,
    secure,
    maxAge: 600
  };
  if (params.action === 'connect') {
    if (!env.GOOGLE_CLIENT_ID || !env.GOOGLE_CLIENT_SECRET)
      return json({ error: 'Google Forms connection is not configured.' }, { status: 503 });
    const state = oidc.randomState(),
      verifier = oidc.randomPKCECodeVerifier();
    cookies.set('forms_pending', await sealProvider({ identity, state, verifier }, 600), options);
    redirect(
      302,
      oidc
        .buildAuthorizationUrl(await google(), {
          redirect_uri: `${url.origin}/integrations/google/callback`,
          scope: scopes.join(' '),
          state,
          code_challenge: await oidc.calculatePKCECodeChallenge(verifier),
          code_challenge_method: 'S256',
          access_type: 'online',
          prompt: 'consent'
        })
        .toString()
    );
  }
  if (params.action === 'callback') {
    const pending = await openProvider(cookies.get('forms_pending'));
    cookies.delete('forms_pending', { path: options.path });
    if (
      !pending ||
      pending.identity !== identity ||
      typeof pending.verifier !== 'string' ||
      typeof pending.state !== 'string' ||
      url.searchParams.get('state') !== pending.state
    )
      redirect(303, '/workspace?forms=expired');
    try {
      const tokens = await oidc.authorizationCodeGrant(await google(), url, {
        pkceCodeVerifier: pending.verifier,
        expectedState: pending.state
      });
      const granted = new Set((tokens.scope || '').split(' '));
      if (!scopes.every((scope) => granted.has(scope))) throw new Error('Missing consent');
      const seconds = Math.min(tokens.expires_in || 3600, 3600);
      cookies.set(
        'dynodoc_forms',
        await sealProvider({ identity, token: tokens.access_token }, seconds),
        { ...options, path: '/api/google-forms', maxAge: seconds }
      );
    } catch {
      redirect(303, '/workspace?forms=failed');
    }
    redirect(303, '/workspace?forms=connected');
  }
  return json({ error: 'Not found' }, { status: 404 });
};
