import { json, redirect } from '@sveltejs/kit';
import * as oidc from 'openid-client';
import type { RequestHandler } from './$types';
import {
  saveSession,
  readSession,
  clearSession,
  accessToken,
  engineUrl,
  enginePost
} from '$lib/server/session';
import { safeNext } from '$lib/auth-next';
import { getConfig } from '$lib/server/config';
import { linkOrigin, mailConfigured, sendMail } from '$lib/server/mail';
import { signInEmail } from '$lib/server/mail/templates';

let configuration: Promise<oidc.Configuration> | undefined;
function googleCredentials() {
  const { googleClientId, secrets } = getConfig();
  const secret = secrets.googleClientSecret?.expose();
  return googleClientId && secret ? { id: googleClientId, secret } : null;
}
function google() {
  const credentials = googleCredentials();
  if (!credentials) throw new Error('Google sign-in is not configured');
  return (configuration ??= oidc
    .discovery(new URL('https://accounts.google.com'), credentials.id, credentials.secret)
    .catch((error) => {
      configuration = undefined;
      throw error;
    }));
}
export const GET: RequestHandler = async ({ params, url, cookies }) => {
  const secure = url.protocol === 'https:';
  const config = getConfig();
  if (params.action === 'options')
    return json({
      google: Boolean(googleCredentials()),
      email: config.emailSignIn && mailConfigured(),
      development: config.emailSignIn && config.devAuth,
      destination: config.adminSite ? '/admin' : '/projects'
    });
  if (params.action === 'session') {
    const token = await accessToken(cookies, secure);
    if (!token) return json({ authenticated: false });
    // The Rust authority verifies its PASETO token; only the display identity leaves the server.
    const response = await fetch(`${engineUrl()}/auth/me`, {
      headers: { authorization: `Bearer ${token}` }
    });
    if (!response.ok) return json({ authenticated: false }, { status: response.status });
    const identity = await response.json();
    return json({ authenticated: true, identity_id: identity.identity_id });
  }
  if (params.action === 'google') {
    if (!googleCredentials())
      return json({ error: 'Google sign-in is not configured yet.' }, { status: 503 });
    const state = oidc.randomState(),
      verifier = oidc.randomPKCECodeVerifier();
    cookies.set('oauth_state', state, {
      path: '/auth',
      httpOnly: true,
      secure,
      sameSite: 'lax',
      maxAge: 600
    });
    cookies.set('oauth_verifier', verifier, {
      path: '/auth',
      httpOnly: true,
      secure,
      sameSite: 'lax',
      maxAge: 600
    });
    const next = safeNext(url.searchParams.get('next'));
    if (next)
      cookies.set('oauth_next', next, {
        path: '/auth',
        httpOnly: true,
        secure,
        sameSite: 'lax',
        maxAge: 600
      });
    else cookies.delete('oauth_next', { path: '/auth' });
    redirect(
      302,
      oidc
        .buildAuthorizationUrl(await google(), {
          redirect_uri: `${url.origin}/auth/google/callback`,
          scope: 'openid email profile',
          state,
          code_challenge: await oidc.calculatePKCECodeChallenge(verifier),
          code_challenge_method: 'S256'
        })
        .toString()
    );
  }
  if (params.action === 'google/callback') {
    const code = url.searchParams.get('code'),
      state = url.searchParams.get('state'),
      verifier = cookies.get('oauth_verifier');
    const expected = cookies.get('oauth_state');
    const next = safeNext(cookies.get('oauth_next'));
    cookies.delete('oauth_state', { path: '/auth' });
    cookies.delete('oauth_verifier', { path: '/auth' });
    cookies.delete('oauth_next', { path: '/auth' });
    if (!code || !state || !expected || state !== expected || !verifier)
      return json({ error: 'This sign-in link expired. Please try again.' }, { status: 400 });
    try {
      const tokens = await oidc.authorizationCodeGrant(await google(), url, {
        pkceCodeVerifier: verifier,
        expectedState: state
      });
      const response = await fetch('https://openidconnect.googleapis.com/v1/userinfo', {
        headers: { Authorization: `Bearer ${tokens.access_token}` },
        signal: AbortSignal.timeout(10_000)
      });
      if (!response.ok) throw new Error('Google profile unavailable');
      const user = await response.json();
      if (user.email_verified !== true || typeof user.email !== 'string')
        throw new Error('Email not verified');
      await saveSession(
        cookies,
        await enginePost('/auth/exchange', { email: user.email, display_name: user.name }),
        secure
      );
    } catch {
      return json({ error: 'Google sign-in failed. Please start again.' }, { status: 401 });
    }
    redirect(303, config.adminSite ? '/admin' : next || '/projects');
  }
  if (params.action === 'verify') {
    // Confirmation is POSTed by the page so email link scanners cannot consume it.
    redirect(303, `/login?token=${encodeURIComponent(url.searchParams.get('token') || '')}`);
  }
  return json({ error: 'Not found' }, { status: 404 });
};
export const POST: RequestHandler = async ({ params, url, request, cookies }) => {
  if (request.headers.get('origin') !== url.origin)
    return json({ error: 'Invalid origin' }, { status: 403 });
  if (params.action === 'logout') {
    const session = await readSession(cookies);
    if (session) {
      try {
        // Use the cookie's refresh secret directly, even if its access token expired
        // or a concurrent request rotated it. The engine revokes the whole login.
        await enginePost('/auth/logout', { refresh_token: session.refresh_token });
      } catch {
        return json(
          { error: 'Sign-out could not be completed. Please try again.' },
          { status: 503 }
        );
      }
    }
    clearSession(cookies);
    cookies.delete('dynodoc_forms', { path: '/api/google-forms' });
    cookies.delete('forms_pending', { path: '/integrations/google' });
    return json({ ok: true });
  }
  const body = await request.json().catch(() => null);
  if (!body) return json({ error: 'Invalid request' }, { status: 400 });
  if (params.action === 'verify') {
    try {
      await saveSession(
        cookies,
        await enginePost('/auth/verify', { token: body.token }),
        url.protocol === 'https:'
      );
      return json({ ok: true });
    } catch {
      return json(
        { error: 'This link has expired or has already been used. Request another link.' },
        { status: 401 }
      );
    }
  }
  if (params.action === 'email') {
    const config = getConfig();
    // A link that was already issued can still be confirmed above; asking for a new one stops.
    if (!config.emailSignIn)
      return json({ error: 'Email sign-in is turned off for this installation.' }, { status: 403 });
    const mailOn = mailConfigured();
    const email = String(body.email || '')
      .trim()
      .toLowerCase();
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) || email.length > 254)
      return json({ error: 'Enter a valid email address.' }, { status: 400 });
    if (!mailOn && !config.devAuth)
      return json({ error: 'Email sign-in is not configured yet.' }, { status: 503 });
    try {
      const result = await enginePost('/auth/magic-link', {
        email,
        display_name: body.display_name
      });
      const link = `${linkOrigin(url.origin)}/auth/verify?token=${encodeURIComponent(result.magic_link_token)}`;
      if (config.devAuth && !mailOn) return json({ development_token: result.magic_link_token });
      await sendMail({
        kind: 'signin',
        to: email,
        ...signInEmail({ link, brand: config.site.brand })
      });
      return json({ ok: true });
    } catch {
      return json(
        { error: 'We could not send your sign-in link. Please wait a few minutes and try again.' },
        { status: 503 }
      );
    }
  }
  return json({ error: 'Not found' }, { status: 404 });
};
