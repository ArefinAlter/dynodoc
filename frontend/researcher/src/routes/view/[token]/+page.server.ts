import { error } from '@sveltejs/kit';
import { engineUrl } from '$lib/server/session';
import type { DocumentState } from '$lib/api/types';
import type { PageServerLoad } from './$types';
export const load: PageServerLoad = async ({ params, setHeaders }) => {
  setHeaders({
    'cache-control': 'private, no-store',
    'referrer-policy': 'no-referrer',
    'x-robots-tag': 'noindex, nofollow'
  });
  if (!/^[a-f0-9]{64}$/.test(params.token)) error(404, 'This public link is unavailable.');
  const response = await fetch(`${engineUrl()}/public/documents/${params.token}`, {
    signal: AbortSignal.timeout(15000)
  });
  if (!response.ok)
    error(
      response.status === 404 ? 404 : 503,
      'This public link has expired, was revoked, or is temporarily unavailable.'
    );
  return (await response.json()) as { title: string; kind: string; state: DocumentState };
};
