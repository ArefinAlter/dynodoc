import { env } from '$env/dynamic/private';
import { env as publicEnv } from '$env/dynamic/public';
import { redirect, type Handle } from '@sveltejs/kit';
import { isIndexableHost, isPublicPath } from '$lib/seo';
import { requiresSignIn } from '$lib/auth-next';
import { accessToken } from '$lib/server/session';
export const handle: Handle = async ({ event, resolve }) => {
  const { pathname, search } = event.url;
  if (env.ADMIN_SITE === 'true' && ['/', '/workspace'].includes(pathname)) redirect(303, '/admin');
  if (event.url.hostname === 'app.dynodoc.online' && pathname === '/')
    redirect(303, 'https://dynodoc.online');
  if (env.ADMIN_SITE !== 'true' && (requiresSignIn(pathname) || pathname === '/login')) {
    // The public homepage host has no sessions; send workspace visits to the workspace host.
    const workspace = publicEnv.PUBLIC_WORKSPACE_URL?.replace(/\/$/, '');
    if (workspace && URL.canParse(workspace) && new URL(workspace).host !== event.url.host)
      redirect(303, `${workspace}${pathname}${search}`);
    // Sign in before the workspace renders, then return to the requested page.
    if (
      requiresSignIn(pathname) &&
      !(await accessToken(event.cookies, event.url.protocol === 'https:'))
    )
      redirect(303, `/login?next=${encodeURIComponent(pathname + search)}`);
  }
  const response = await resolve(event);
  // Indexable only on the public host, and only for the public pages and the
  // SEO files (robots, sitemap, llms.txt, the IndexNow key). Everything else —
  // every path on app./admin., and the private routes here — is held back.
  if (!isPublicPath(event.url.pathname) || !isIndexableHost(event.url.hostname))
    response.headers.set('x-robots-tag', 'noindex, nofollow');
  if (event.url.pathname.startsWith('/admin') || event.url.pathname.startsWith('/auth'))
    response.headers.set('cache-control', 'private, no-store');
  response.headers.set(
    'referrer-policy',
    event.url.pathname.startsWith('/view/') ? 'no-referrer' : 'strict-origin-when-cross-origin'
  );
  if (event.url.pathname.startsWith('/integrations/') || event.url.pathname.startsWith('/view/'))
    response.headers.set('cache-control', 'private, no-store');
  response.headers.set('x-content-type-options', 'nosniff');
  return response;
};
