import { env } from '$env/dynamic/private';
import { json, redirect } from '@sveltejs/kit';
import * as oidc from 'openid-client';
import type { RequestHandler } from './$types';
import { saveSession, clearSession, accessToken, engineUrl, enginePost } from '$lib/server/session';
import { safeNext } from '$lib/auth-next';

let configuration: Promise<oidc.Configuration> | undefined;
function google() {
  if (!env.GOOGLE_CLIENT_ID || !env.GOOGLE_CLIENT_SECRET)
    throw new Error('Google sign-in is not configured');
  return (configuration ??= oidc
    .discovery(
      new URL('https://accounts.google.com'),
      env.GOOGLE_CLIENT_ID,
      env.GOOGLE_CLIENT_SECRET
    )
    .catch((error) => {
      configuration = undefined;
      throw error;
    }));
}
export const GET: RequestHandler = async ({ params, url, cookies }) => {
  const secure = url.protocol === 'https:';
  if (params.action === 'options')
    return json({
      google: Boolean(env.GOOGLE_CLIENT_ID && env.GOOGLE_CLIENT_SECRET),
      email: Boolean(env.RESEND_API_KEY && env.EMAIL_FROM),
      development: env.DEV_AUTH === 'true',
      destination: env.ADMIN_SITE === 'true' ? '/admin' : '/projects'
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
    if (!env.GOOGLE_CLIENT_ID || !env.GOOGLE_CLIENT_SECRET)
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
    redirect(303, env.ADMIN_SITE === 'true' ? '/admin' : next || '/projects');
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
    const email = String(body.email || '')
      .trim()
      .toLowerCase();
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) || email.length > 254)
      return json({ error: 'Enter a valid email address.' }, { status: 400 });
    if (!env.RESEND_API_KEY && env.DEV_AUTH !== 'true')
      return json({ error: 'Email sign-in is not configured yet.' }, { status: 503 });
    try {
      const result = await enginePost('/auth/magic-link', {
        email,
        display_name: body.display_name
      });
      const link = `${url.origin}/auth/verify?token=${encodeURIComponent(result.magic_link_token)}`;
      if (env.DEV_AUTH === 'true' && !env.RESEND_API_KEY)
        return json({ development_token: result.magic_link_token });
      const delivery = await fetch('https://api.resend.com/emails', {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${env.RESEND_API_KEY}`,
          'content-type': 'application/json'
        },
        body: JSON.stringify({
          from: env.EMAIL_FROM,
          to: [email],
          subject: 'Sign in to Dynodoc',
          text: `Sign in to your research workspace:\n\n${link}\n\nThis link expires in 15 minutes. If you did not request it, you can ignore this message.`
        }),
        signal: AbortSignal.timeout(15_000)
      });
      if (!delivery.ok) throw new Error('Delivery failed');
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
