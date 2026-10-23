import { isRedirect, redirect, type Handle, type ServerInit } from '@sveltejs/kit';
import { isIndexableHost, isPublicPath } from '$lib/seo/site';
import { requiresSignIn } from '$lib/auth-next';
import { accessToken } from '$lib/server/session';
import { secureResponse } from '$lib/server/security';
import { env as publicEnv } from '$env/dynamic/public';
import { getConfig, startupCheck } from '$lib/server/config';
import { isOfficePane } from '$lib/security-policy';
import { OFFICE_JS_LOCAL_PATH, OFFICE_JS_META } from '$lib/connectors/office-js';

// Validate the whole configuration before the server listens (adapter-node waits for it).
export const init: ServerInit = () => startupCheck(publicEnv);

// The hosted service's marketing and search-engine routes. A self-hosted installation does
// not serve them: the pages answer 404 and `/` sends a visitor to sign in or to the workspace.
const marketingRoutes = ['/source', '/sitemap.xml', '/llms.txt', '/llms-full.txt'];
const isMarketing = (id: string | null) =>
  id !== null && (id.startsWith('/(site)') || marketingRoutes.includes(id));
const notFound = () =>
  new Response(
    '<!doctype html><meta charset="utf-8"><title>Not found</title><p>This page does not exist.</p>',
    { status: 404, headers: { 'content-type': 'text/html; charset=utf-8' } }
  );

/** The two pages of the native-app sign-in (decision 017): they belong to the workspace host. */
const isAppSignIn = (pathname: string) =>
  pathname === '/app-signin' || pathname === '/app-signin/finish';

const application: Handle = async ({ event, resolve }) => {
  const config = getConfig();
  event.locals.indexable = config.indexable;
  const { pathname, search } = event.url;
  if (config.adminSite && ['/', '/workspace'].includes(pathname)) redirect(303, '/admin');
  if (
    config.rootRedirect &&
    event.url.hostname === config.rootRedirect.fromHost &&
    pathname === '/'
  )
    redirect(303, config.rootRedirect.to);
  let hidden = false;
  if (!config.marketing && isMarketing(event.route.id)) {
    if (event.route.id === '/(site)')
      redirect(
        303,
        (await accessToken(event.cookies, event.url.protocol === 'https:')) ? '/projects' : '/login'
      );
    hidden = true;
  }
  if (
    !config.adminSite &&
    (requiresSignIn(pathname) || pathname === '/login' || isAppSignIn(pathname))
  ) {
    // The public homepage host has no sessions; send workspace visits (and the sign-in pages of the
    // native apps, which end in a session) to the workspace host.
    const workspace = config.workspaceUrl;
    if (workspace && new URL(workspace).host !== event.url.host)
      redirect(303, `${workspace}${pathname}${search}`);
    // Sign in before the workspace renders, then return to the requested page.
    if (
      requiresSignIn(pathname) &&
      !(await accessToken(event.cookies, event.url.protocol === 'https:'))
    )
      redirect(303, `/login?next=${encodeURIComponent(pathname + search)}`);
  }
  // With a local copy of Office.js the pane pages say so in a meta tag; every other response, and
  // every response of a hosted process, is produced exactly as before.
  const localOffice = config.officeJsDir !== null && isOfficePane(pathname);
  const response = hidden
    ? notFound()
    : localOffice
      ? await resolve(event, {
          transformPageChunk: ({ html }) =>
            html.replace(
              '<head>',
              `<head><meta name="${OFFICE_JS_META}" content="${OFFICE_JS_LOCAL_PATH}" />`
            )
        })
      : await resolve(event);
  // Indexable only on the public host, and only for the public pages and the
  // SEO files (robots, sitemap, llms.txt, the IndexNow key). Everything else —
  // every path on app./admin., and the private routes here — is held back. A
  // self-hosted installation is never indexable.
  if (
    !config.indexable ||
    !isPublicPath(event.url.pathname) ||
    !isIndexableHost(event.url.hostname)
  )
    response.headers.set('x-robots-tag', 'noindex, nofollow');
  if (event.url.pathname.startsWith('/admin') || event.url.pathname.startsWith('/auth'))
    response.headers.set('cache-control', 'private, no-store');
  response.headers.set(
    'referrer-policy',
    event.url.pathname.startsWith('/view/') ? 'no-referrer' : 'strict-origin-when-cross-origin'
  );
  if (event.url.pathname.startsWith('/connectors/')) {
    response.headers.set('referrer-policy', 'no-referrer');
    response.headers.set('cache-control', 'private, no-store');
  }
  if (event.url.pathname.startsWith('/integrations/') || event.url.pathname.startsWith('/view/'))
    response.headers.set('cache-control', 'private, no-store');
  response.headers.set('x-content-type-options', 'nosniff');
  return response;
};

// Include early sign-in/OAuth redirects without buffering bodies or SSE streams.
export const handle: Handle = async (input) => {
  try {
    const config = getConfig();
    return secureResponse(
      await application(input),
      input.event.url,
      config.connectorOrigins,
      config.connectSrc,
      config.officeJsDir === null ? 'cdn' : 'local'
    );
  } catch (error) {
    if (!isRedirect(error)) throw error;
    return secureResponse(
      new Response(null, { status: error.status, headers: { location: error.location } }),
      input.event.url
    );
  }
};
