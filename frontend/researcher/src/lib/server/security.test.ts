import { expect, it } from 'vitest';
import { secureResponse } from './security';

it('preserves generated nonces while restricting Microsoft permissions to exact pane routes', () => {
  for (const path of ['/login', '/connectors/authorize', '/connectors/word/extra', '/projects']) {
    const response = secureResponse(
      new Response('page', {
        headers: {
          'content-security-policy': "script-src 'self' 'nonce-per-response'; object-src 'none'"
        }
      }),
      new URL(path, 'https://app.test')
    );
    expect(response.headers.get('content-security-policy')).toContain("'nonce-per-response'");
    expect(response.headers.get('content-security-policy')).not.toContain('microsoft');
    expect(response.headers.get('content-security-policy')).toContain("frame-ancestors 'self'");
    expect(response.headers.get('x-frame-options')).toBe('SAMEORIGIN');
    expect(response.headers.get('strict-transport-security')).toBe('max-age=31536000');
  }
  for (const host of ['word', 'excel', 'powerpoint']) {
    const response = secureResponse(
      new Response('pane', {
        headers: { 'content-security-policy': "script-src 'self' 'nonce-test'" }
      }),
      new URL(`/connectors/${host}/`, 'https://app.test')
    );
    expect(response.headers.get('content-security-policy')).toContain(
      'https://appsforoffice.microsoft.com'
    );
    expect(response.headers.get('content-security-policy')).toContain(
      'https://*.officeapps.live.com'
    );
    expect(response.headers.has('x-frame-options')).toBe(false);
    expect(response.headers.get('cache-control')).toBe('private, no-store');
    expect(response.headers.get('referrer-policy')).toBe('no-referrer');
  }
});

it('hardens immutable redirects and API errors without adding HSTS on plain HTTP', async () => {
  const redirected = secureResponse(
    Response.redirect('https://app.test/login', 303),
    new URL('https://app.test/auth/logout')
  );
  expect(redirected.status).toBe(303);
  expect(redirected.headers.get('location')).toBe('https://app.test/login');
  expect(redirected.headers.get('cache-control')).toBe('private, no-store');
  const failed = secureResponse(
    new Response('denied', { status: 403 }),
    new URL('http://localhost/api/test')
  );
  expect(failed.status).toBe(403);
  expect(await failed.text()).toBe('denied');
  expect(failed.headers.get('content-security-policy')).toContain("default-src 'none'");
  expect(failed.headers.has('strict-transport-security')).toBe(false);
});

it('keeps the native-app sign-in pages out of caches, referrers, frames and shared windows', () => {
  for (const path of [
    '/app-signin',
    '/app-signin/',
    '/app-signin/finish',
    '/app-signin/finish/',
    '/auth/app-signin/redeem'
  ]) {
    const response = secureResponse(
      new Response('page', {
        headers: { 'content-security-policy': "script-src 'self' 'nonce-n'; connect-src 'self'" }
      }),
      new URL(path, 'https://app.test')
    );
    const page = !path.startsWith('/auth/');
    expect(response.headers.get('cache-control'), path).toBe('private, no-store');
    // The confirmation page's form must send its real Origin, so it shares its address with its own
    // site only; every other sign-in route says nothing about where it was reached from.
    expect(response.headers.get('referrer-policy'), path).toBe(
      path === '/app-signin' ? 'same-origin' : 'no-referrer'
    );
    // The two pages are never framed (a plain link can aim a fragment at a frame) and never share
    // a browsing context group with a window an infected page opened (it could read the code).
    expect(response.headers.get('x-frame-options'), path).toBe(page ? 'DENY' : 'SAMEORIGIN');
    expect(response.headers.get('content-security-policy'), path).toContain(
      page ? "frame-ancestors 'none'" : "frame-ancestors 'self'"
    );
    expect(response.headers.get('content-security-policy'), path).not.toContain(
      page ? "frame-ancestors 'self'" : "frame-ancestors 'none'"
    );
    expect(response.headers.get('cross-origin-opener-policy'), path).toBe(
      page ? 'same-origin' : null
    );
  }
});

it('leaves every other page framable by its own site and in the default window group', () => {
  for (const path of ['/', '/workspace', '/login', '/app-signin-help', '/app-signin/other']) {
    const response = secureResponse(
      new Response('page', { headers: { 'content-security-policy': "script-src 'self'" } }),
      new URL(path, 'https://app.test')
    );
    expect(response.headers.get('x-frame-options'), path).toBe('SAMEORIGIN');
    expect(response.headers.get('content-security-policy'), path).toContain("frame-ancestors 'self'");
    expect(response.headers.has('cross-origin-opener-policy'), path).toBe(false);
  }
});

it('allows configured HTTPS connector origins only for pane connections', () => {
  const response = secureResponse(
    new Response('pane', {
      headers: { 'content-security-policy': "script-src 'self'; connect-src 'self'" }
    }),
    new URL('https://app.test/connectors/word'),
    'https://remote.test/path, javascript:evil, https://user:pass@other.test, https://bad.test; script-src *'
  );
  const policy = response.headers.get('content-security-policy')!;
  expect(policy).toContain("connect-src 'self'");
  expect(policy).toContain('https://remote.test');
  expect(policy).not.toMatch(/other.test|bad.test|javascript|script-src[^;]*remote.test/);
});
