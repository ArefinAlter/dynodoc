import { env } from '$env/dynamic/private';
import { readFile } from 'node:fs/promises';
import { adminOverview } from '$lib/server/admin';
import { hostIsStale, needsAttention, type HostStatus } from '$lib/components/admin/health';
import type { PageServerLoad } from './$types';
const local = /^(localhost|127\.0\.0\.1|\[::1\])$/;
export const load: PageServerLoad = async ({ cookies, url, setHeaders }) => {
  const overview = await adminOverview(cookies, url.protocol === 'https:');
  setHeaders({ 'cache-control': 'private, no-store' });
  let host: HostStatus | null = null;
  if (env.OPS_STATUS_PATH) {
    try {
      host = JSON.parse(await readFile(env.OPS_STATUS_PATH, 'utf8'));
    } catch {
      /* Show unavailable, never fabricated healthy. */
    }
  }
  const providers = {
    google: !!(env.GOOGLE_CLIENT_ID && env.GOOGLE_CLIENT_SECRET),
    email: !!(env.RESEND_API_KEY && env.EMAIL_FROM)
  };
  const now = Date.now();
  return {
    overview,
    host,
    providers,
    stale: hostIsStale(host, now),
    attention: needsAttention(host, providers, now),
    // Only names what the request host proves; unknown hosts are shown as they are.
    environment: local.test(url.hostname)
      ? 'Local'
      : /(^|\.)dynodoc\.online$/.test(url.hostname)
        ? 'Production'
        : url.hostname
  };
};
