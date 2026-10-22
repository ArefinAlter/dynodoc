import { readFile } from 'node:fs/promises';
import { adminOverview } from '$lib/server/admin';
import { getConfig } from '$lib/server/config';
import { hostIsStale, needsAttention, type HostStatus } from '$lib/components/admin/health';
import type { PageServerLoad } from './$types';
const local = /^(localhost|127\.0\.0\.1|\[::1\])$/;
export const load: PageServerLoad = async ({ cookies, url, setHeaders }) => {
  const config = getConfig();
  const overview = await adminOverview(cookies, url.protocol === 'https:');
  setHeaders({ 'cache-control': 'private, no-store' });
  let host: HostStatus | null = null;
  if (config.opsStatusPath) {
    try {
      host = JSON.parse(await readFile(config.opsStatusPath, 'utf8'));
    } catch {
      /* Show unavailable, never fabricated healthy. */
    }
  }
  const providers = {
    google: !!(config.googleClientId && config.secrets.googleClientSecret),
    email: !!(config.secrets.resendApiKey && config.emailFrom)
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
      : config.hostedDomain &&
          (url.hostname === config.hostedDomain || url.hostname.endsWith(`.${config.hostedDomain}`))
        ? 'Production'
        : url.hostname
  };
};
