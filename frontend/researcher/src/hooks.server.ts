import { env } from '$env/dynamic/private';
import { redirect, type Handle } from '@sveltejs/kit';
export const handle: Handle = async ({ event, resolve }) => {
  if (env.ADMIN_SITE === 'true' && ['/', '/workspace'].includes(event.url.pathname))
    redirect(303, '/admin');
  const response = await resolve(event);
  const publicPage = ['/', '/privacy', '/terms'].includes(event.url.pathname);
  if (!publicPage || !['dynodoc.online', 'www.dynodoc.online'].includes(event.url.hostname))
    response.headers.set('x-robots-tag', 'noindex, nofollow');
  if (event.url.pathname.startsWith('/admin') || event.url.pathname.startsWith('/auth'))
    response.headers.set('cache-control', 'private, no-store');
  response.headers.set('referrer-policy', 'strict-origin-when-cross-origin');
  response.headers.set('x-content-type-options', 'nosniff');
  return response;
};
