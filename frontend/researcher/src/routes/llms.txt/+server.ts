import type { RequestHandler } from './$types';
import { llmsTxt, isIndexableHost } from '$lib/seo';

const BODY = llmsTxt();

export const GET: RequestHandler = ({ url }) => {
  if (!isIndexableHost(url.hostname)) return new Response('Not found', { status: 404 });
  return new Response(BODY, {
    headers: {
      'content-type': 'text/plain; charset=utf-8',
      'cache-control': 'public, max-age=3600'
    }
  });
};
