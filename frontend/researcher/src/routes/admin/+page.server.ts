import { env } from '$env/dynamic/private';
import { readFile } from 'node:fs/promises';
import { adminOverview } from '$lib/server/admin';
import type { PageServerLoad } from './$types';
export const load: PageServerLoad = async ({ cookies, url, setHeaders }) => {
  const overview = await adminOverview(cookies, url.protocol === 'https:');
  setHeaders({ 'cache-control': 'private, no-store' });
  let host = null;
  if (env.OPS_STATUS_PATH) {
    try {
      host = JSON.parse(await readFile(env.OPS_STATUS_PATH, 'utf8'));
    } catch {
      /* Show unavailable, never fabricated healthy. */
    }
  }
  return {
    overview,
    host,
    providers: {
      google: !!(env.GOOGLE_CLIENT_ID && env.GOOGLE_CLIENT_SECRET),
      email: !!(env.RESEND_API_KEY && env.EMAIL_FROM)
    }
  };
};
