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

it('keeps the native-app sign-in pages out of caches, referrers and frames', () => {
  for (const path of ['/app-signin', '/app-signin/finish', '/auth/app-signin/redeem']) {
    const response = secureResponse(
      new Response('page', {
        headers: { 'content-security-policy': "script-src 'self' 'nonce-n'; connect-src 'self'" }
      }),
      new URL(path, 'https://app.test')
    );
    expect(response.headers.get('cache-control')).toBe('private, no-store');
    expect(response.headers.get('referrer-policy')).toBe('no-referrer');
    expect(response.headers.get('x-frame-options')).toBe('SAMEORIGIN');
    expect(response.headers.get('content-security-policy')).toContain("frame-ancestors 'self'");
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
