import { error } from '@sveltejs/kit';
import { requireAdministrator } from '$lib/server/admin';
import { currentAddinBundle } from '$lib/server/addins';
import { getConfig } from '$lib/server/config';
import type { PageServerLoad } from './$types';

// The administrators' page for the add-in files: downloads, install steps per channel, and what
// cannot work on a private or offline network. Administrators only (the same rule as /admin):
// the downloads themselves are public (./[file]/+server.ts), but the instructions describe the
// installation's setup, so they are not shown to everyone. A hosted process has no such page.
export const load: PageServerLoad = async ({ cookies, url, setHeaders }) => {
  const config = getConfig();
  if (config.mode !== 'selfhosted') error(404, 'Not found');
  await requireAdministrator(cookies, url.protocol === 'https:');
  setHeaders({ 'cache-control': 'private, no-store' });
  const result = currentAddinBundle();
  return {
    brand: config.site.brand,
    origin: config.origin ?? '',
    officeJsLocal: config.officeJsDir !== null,
    problem: result.ok ? null : result.reason,
    manifestVersion: result.ok ? result.bundle.manifestVersion : '',
    supportUrl: result.ok ? result.bundle.supportUrl : '',
    google: result.ok ? result.bundle.google : { available: false, reason: '' },
    files: result.ok
      ? result.bundle.files.map(({ name, group, host, variant, id, sha256, bytes }) => ({
          name,
          group,
          host: host ?? '',
          variant: variant ?? '',
          id: id ?? '',
          sha256,
          size: bytes.length
        }))
      : []
  };
};
