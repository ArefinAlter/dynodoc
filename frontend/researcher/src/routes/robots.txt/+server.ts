import type { RequestHandler } from './$types';
export const GET: RequestHandler = ({ url }) =>
  new Response(
    url.hostname === 'dynodoc.online' || url.hostname === 'www.dynodoc.online'
      ? 'User-agent: *\nAllow: /\nDisallow: /api/\nDisallow: /auth/\nDisallow: /telemetry\nSitemap: https://dynodoc.online/sitemap.xml\n'
      : 'User-agent: *\nDisallow: /\n',
    {
      headers: {
        'content-type': 'text/plain; charset=utf-8',
        'cache-control': 'public, max-age=3600'
      }
    }
  );
