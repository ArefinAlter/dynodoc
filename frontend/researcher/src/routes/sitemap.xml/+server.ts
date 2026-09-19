import type { RequestHandler } from './$types';
import { INDEXABLE_PAGES, canonicalUrl, isIndexableHost } from '$lib/seo';

// Always canonical URLs, whatever host served the request: a sitemap fetched
// from a preview or workspace host must not advertise that host's URLs.
const SITEMAP =
  '<?xml version="1.0" encoding="UTF-8"?>\n' +
  '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
  INDEXABLE_PAGES.map(
    (page) =>
      '  <url>' +
      `<loc>${canonicalUrl(page.path)}</loc>` +
      `<lastmod>${page.lastmod}</lastmod>` +
      `<changefreq>${page.changefreq}</changefreq>` +
      `<priority>${page.priority}</priority>` +
      '</url>'
  ).join('\n') +
  '\n</urlset>\n';

export const GET: RequestHandler = ({ url }) => {
  // Only the public host publishes a sitemap; elsewhere there is nothing to index.
  if (!isIndexableHost(url.hostname)) return new Response('Not found', { status: 404 });
  return new Response(SITEMAP, {
    headers: {
      'content-type': 'application/xml; charset=utf-8',
      'cache-control': 'public, max-age=3600'
    }
  });
};
