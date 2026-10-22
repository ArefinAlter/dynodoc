// SvelteKit supplies per-response nonces (or prerendered hashes). Keep that policy
// intact, with narrowly scoped additions for the public Office task panes.
import { isOfficePane } from '$lib/security-policy';
const officeSources = [
  'https://appsforoffice.microsoft.com',
  'https://officeapis.public.onecdn.static.microsoft',
  'https://ajax.aspnetcdn.com'
];
const officeAncestors = [
  "'self'",
  'https://*.officeapps.live.com',
  'https://*.office.com',
  'https://*.office365.com',
  'https://*.microsoft365.com',
  'https://*.cloud.microsoft',
  'https://office.live.com',
  'https://onedrive.live.com',
  'https://*.sharepoint.com'
];
const fallback = "default-src 'none'; base-uri 'none'; object-src 'none'; form-action 'self'";

// `connectSrc` is the configured extra `connect-src` origins for every page (from the
// configuration, never a fixed hostname); `connectorOrigins` applies to the Office panes only.
export function secureResponse(
  response: Response,
  url: URL,
  connectorOrigins = '',
  connectSrc: readonly string[] = []
): Response {
  // Redirect headers can be immutable. Preserve the body/stream without buffering.
  const secured = new Response(response.body, response);
  const headers = secured.headers;
  const pane = isOfficePane(url.pathname);
  const policy = new Map(
    (headers.get('content-security-policy') || fallback)
      .split(';')
      .map((part) => part.trim().split(/\s+/))
      .filter(([key]) => key)
      .map(([key, ...values]) => [key, values])
  );
  if (connectSrc.length && policy.has('connect-src'))
    policy.get('connect-src')!.push(...connectSrc);
  if (pane && policy.has('script-src')) {
    policy.get('script-src')!.push(...officeSources);
    // Operators may explicitly allow another HTTPS Dynodoc installation. Never
    // interpolate raw environment values into policy syntax or widen script-src.
    const origins = connectorOrigins.split(',').flatMap((value) => {
      try {
        const remote = new URL(value.trim());
        return remote.protocol === 'https:' && !remote.username && !remote.password
          ? [remote.origin]
          : [];
      } catch {
        return [];
      }
    });
    policy.set('connect-src', [
      ...(policy.get('connect-src') || ["'self'"]),
      ...officeSources,
      ...origins
    ]);
  }
  policy.set('frame-ancestors', pane ? officeAncestors : ["'self'"]);
  headers.set(
    'content-security-policy',
    [...policy].map(([key, values]) => `${key} ${[...new Set(values)].join(' ')}`).join('; ')
  );
  if (pane) headers.delete('x-frame-options');
  else headers.set('x-frame-options', 'SAMEORIGIN');
  headers.set('x-content-type-options', 'nosniff');
  headers.set(
    'referrer-policy',
    /^\/(auth|connectors|view|app-signin)(\/|$)/.test(url.pathname)
      ? 'no-referrer'
      : 'strict-origin-when-cross-origin'
  );
  if (/^\/(auth|admin|integrations|connectors|view|app-signin)(\/|$)/.test(url.pathname))
    headers.set('cache-control', 'private, no-store');
  if (url.protocol === 'https:') headers.set('strict-transport-security', 'max-age=31536000');
  return secured;
}
