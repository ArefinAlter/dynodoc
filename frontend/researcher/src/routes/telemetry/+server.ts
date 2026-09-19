import { env } from '$env/dynamic/private';
import { enginePost } from '$lib/server/session';
import type { RequestHandler } from './$types';
const origins = new Map([
  ['https://dynodoc.online', 'homepage'],
  ['https://www.dynodoc.online', 'homepage'],
  ['https://app.dynodoc.online', 'workspace'],
  ['https://admin.dynodoc.online', 'admin']
]);
export const POST: RequestHandler = async ({ request }) => {
  const origin = request.headers.get('origin') || '';
  const host = origins.get(origin);
  if (!host || !env.WEB_SERVICE_KEY) return new Response(null, { status: 403 });
  if (Number(request.headers.get('content-length') || 0) > 2048)
    return new Response(null, { status: 413 });
  const text = await request.text();
  if (text.length > 2048) return new Response(null, { status: 413 });
  try {
    const input = JSON.parse(text);
    await enginePost('/telemetry', {
      page_id: input.page_id,
      metric: input.metric,
      page: input.page,
      value: input.value,
      host
    });
    return new Response(null, {
      status: 204,
      headers: {
        'access-control-allow-origin': origin,
        'cache-control': 'no-store',
        vary: 'Origin'
      }
    });
  } catch {
    return new Response(null, { status: 400 });
  }
};
