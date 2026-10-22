// The configured extra connect-src origins: added after 'self', for pages only, deduplicated,
// and the Office panes still get their Microsoft sources after them.
import { expect, it } from 'vitest';
import { secureResponse } from './security';

const page = (policy: string, path = '/login', extra: readonly string[] = [], origins = '') =>
  secureResponse(
    new Response('page', { headers: { 'content-security-policy': policy } }),
    new URL(path, 'https://app.test'),
    origins,
    extra
  ).headers.get('content-security-policy');

it('adds the configured origins right after self, and only where a page policy has connect-src', () => {
  expect(
    page("script-src 'self' 'nonce-1'; connect-src 'self'", '/login', ['https://a.test'])
  ).toBe("script-src 'self' 'nonce-1'; connect-src 'self' https://a.test; frame-ancestors 'self'");
  // No connect-src in the policy: nothing is invented.
  expect(page("script-src 'self'", '/login', ['https://a.test'])).toBe(
    "script-src 'self'; frame-ancestors 'self'"
  );
  const bare = secureResponse(new Response('x'), new URL('https://app.test/api'), '', [
    'https://a.test'
  ]);
  expect(bare.headers.get('content-security-policy')).not.toContain('a.test');
});

it('adds nothing when no origin is configured, and drops duplicates', () => {
  expect(page("connect-src 'self'")).toBe("connect-src 'self'; frame-ancestors 'self'");
  expect(page("connect-src 'self' https://a.test", '/login', ['https://a.test'])).toBe(
    "connect-src 'self' https://a.test; frame-ancestors 'self'"
  );
});

it('keeps the order self, configured origins, Office sources, connector origins on a pane', () => {
  const policy = page(
    "script-src 'self'; connect-src 'self'",
    '/connectors/word',
    ['https://app.dynodoc.online'],
    'https://remote.test'
  )!;
  expect(policy).toContain(
    "connect-src 'self' https://app.dynodoc.online https://appsforoffice.microsoft.com https://officeapis.public.onecdn.static.microsoft https://ajax.aspnetcdn.com https://remote.test"
  );
});
