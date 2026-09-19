import { env } from '$env/dynamic/private';
import { redirect, type Handle } from '@sveltejs/kit';
import { isIndexableHost, isPublicPath } from '$lib/seo';
export const handle: Handle = async ({ event, resolve }) => {
  if (env.ADMIN_SITE === 'true' && ['/', '/workspace'].includes(event.url.pathname))
    redirect(303, '/admin');
  if (event.url.hostname === 'app.dynodoc.online' && event.url.pathname === '/')
    redirect(303, 'https://dynodoc.online');
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
