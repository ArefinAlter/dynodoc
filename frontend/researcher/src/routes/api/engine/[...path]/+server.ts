import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { accessToken, engineUrl } from '$lib/server/session';

const proxy: RequestHandler = async ({ request, url, cookies, params }) => {
  if (!params.path || params.path.startsWith('auth') || params.path.includes('..'))
    return json({ error: 'Not found' }, { status: 404 });
  if (!['GET', 'HEAD'].includes(request.method) && request.headers.get('origin') !== url.origin)
    return json({ error: 'Invalid origin' }, { status: 403 });
  const token = await accessToken(cookies, url.protocol === 'https:');
  if (!token) return json({ error: 'Please sign in' }, { status: 401 });
  // New readers are deployed before the engine starts recording compact events.
  // Old tabs must reload instead of silently skipping an unknown event variant.
  const originalDownload =
    request.method === 'GET' && /^documents\/[^/]+\/uploads\/[^/]+$/.test(params.path);
  if (
    params.path.startsWith('documents/') &&
    !originalDownload &&
    request.headers.get('x-dynodoc-editor') !== 'office-v5'
  )
    return json(
      {
        error:
          'Dynodoc has updated its document history. Copy any unsaved text, then reload this tab before continuing.'
      },
      { status: 409 }
    );
  const headers = new Headers({ authorization: `Bearer ${token}` });
  for (const name of ['content-type', 'idempotency-key', 'last-event-id']) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  const upstream = await fetch(`${engineUrl()}/${params.path}${url.search}`, {
    method: request.method,
    headers,
    body: ['GET', 'HEAD'].includes(request.method) ? undefined : await request.arrayBuffer(),
    signal: request.signal
  });
  const outgoing = new Headers({ 'cache-control': 'private, no-store' });
  for (const name of [
    'content-type',
    'retry-after',
    'x-accel-buffering',
    'content-disposition',
    'x-content-type-options'
  ]) {
    const value = upstream.headers.get(name);
    if (value) outgoing.set(name, value);
  }
  return new Response(upstream.body, { status: upstream.status, headers: outgoing });
};
export const GET = proxy;
export const POST = proxy;
export const DELETE = proxy;
export const PATCH = proxy;
