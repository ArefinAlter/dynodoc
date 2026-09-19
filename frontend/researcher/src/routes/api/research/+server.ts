import { json } from '@sveltejs/kit';
import { accessToken } from '$lib/server/session';
import { researchRead, ResearchError } from '$lib/server/research';
import type { ResearchRequest } from '$lib/workspace/connectors';
import type { RequestHandler } from './$types';
let active = 0;
export const POST: RequestHandler = async ({ request, url, cookies }) => {
  if (request.headers.get('origin') !== url.origin)
    return json({ error: 'Invalid origin' }, { status: 403 });
  if (!(await accessToken(cookies, url.protocol === 'https:')))
    return json({ error: 'Please sign in' }, { status: 401 });
  if (active >= 4)
    return json(
      { error: 'Other research imports are running. Try again shortly.' },
      { status: 429 }
    );
  active++;
  try {
    const text = await request.text();
    if (text.length > 8192)
      return json({ error: 'Connection details are too long.' }, { status: 413 });
    const input = JSON.parse(text) as ResearchRequest;
    const result = await researchRead(input);
    return json(result, { headers: { 'cache-control': 'private, no-store' } });
  } catch (error) {
    // Never return connection internals, upstream bodies, or a token in an error.
    return json(
      {
        error:
          error instanceof ResearchError
            ? error.message
            : 'Could not connect. Check the server address, token and project permissions.'
      },
      { status: 422 }
    );
  } finally {
    active--;
  }
};
