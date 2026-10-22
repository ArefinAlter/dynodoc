import { getConfig } from '$lib/server/config';
import { enginePost } from '$lib/server/session';
import type { RequestHandler } from './$types';
// Which browser origins may report, and the host label stored with each measurement, come
// from the configuration. With page metrics off the route does nothing.
const allowed = () => {
  const { telemetry } = getConfig();
  return telemetry.enabled ? new Map(telemetry.origins) : null;
};
export const OPTIONS: RequestHandler = ({ request }) => {
  const origins = allowed();
  if (!origins) return new Response(null, { status: 404 });
  const origin = request.headers.get('origin') || '';
  if (!origins.has(origin) || request.headers.get('access-control-request-method') !== 'POST')
    return new Response(null, { status: 403 });
  return new Response(null, {
    status: 204,
    headers: {
      'access-control-allow-origin': origin,
      'access-control-allow-methods': 'POST',
      'access-control-allow-headers': 'content-type',
      'access-control-max-age': '600',
      vary: 'Origin'
    }
  });
};
export const POST: RequestHandler = async ({ request }) => {
  const origins = allowed();
  if (!origins) return new Response(null, { status: 404 });
  const origin = request.headers.get('origin') || '';
  const host = origins.get(origin);
  if (!host || !getConfig().secrets.webServiceKey) return new Response(null, { status: 403 });
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
