import { json } from '@sveltejs/kit';
import { engineUrl } from '$lib/server/session';
import type { RequestHandler } from './$types';
/** Dedicated bearer-only gateway. Never exchanges a browser session or service key. */
const proxy: RequestHandler = async ({ request, params, url }) => {
  const path = params.path || '';
  const connection = path === 'connection' && request.method === 'GET';
  if (
    !connection &&
    (!/^documents\/[0-9a-f-]{36}\/(checkpoint|bundles)$/i.test(path) ||
      (request.method === 'GET' && !path.endsWith('/checkpoint')) ||
      (request.method === 'POST' && !path.endsWith('/bundles')))
  )
    return json({ error: 'Not found' }, { status: 404 });
  const auth = request.headers.get('authorization');
  if (!auth || !/^Bearer [0-9a-f]{64}$/.test(auth))
    return json({ error: 'A file connection key is required' }, { status: 401 });
  const values = url.searchParams.getAll('through_seq');
  if (
    [...url.searchParams.keys()].some((key) => key !== 'through_seq') ||
    values.length > 1 ||
    (values.length &&
      (connection ||
        request.method !== 'GET' ||
        !/^\d+$/.test(values[0]) ||
        !Number.isSafeInteger(Number(values[0]))))
  )
    return json({ error: 'Use one nonnegative through_seq for a checkpoint' }, { status: 400 });
  const search = values.length ? `?through_seq=${Number(values[0])}` : '';
  // Apps Script uses server-side UrlFetch. The Word pane is served on this origin.
  const upstream = await fetch(`${engineUrl()}/connector/${path}${search}`, {
    method: request.method,
    headers: { authorization: auth, 'content-type': 'application/json' },
    body: request.method === 'POST' ? await request.arrayBuffer() : undefined,
    signal: request.signal
  });
  return new Response(upstream.body, {
    status: upstream.status,
    headers: {
      'content-type': 'application/json',
      'cache-control': 'private, no-store',
      'x-content-type-options': 'nosniff'
    }
  });
};
export const GET = proxy;
export const POST = proxy;
