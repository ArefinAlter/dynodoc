import type { RequestHandler } from './$types';
export const GET: RequestHandler = () =>
  new Response(
    '<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' +
      ['', '/privacy', '/terms']
        .map((path) => `<url><loc>https://dynodoc.online${path || '/'}</loc></url>`)
        .join('') +
      '</urlset>',
    {
      headers: {
        'content-type': 'application/xml; charset=utf-8',
        'cache-control': 'public, max-age=3600'
      }
    }
  );
