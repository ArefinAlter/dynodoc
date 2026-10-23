// The add-in files of this installation (docs/SELF-HOSTED-EDITION.md section 7, P0-7): the
// configured origin, product name and support link go into the generator in $lib/addins, and the
// result is kept until the settings change. Self-hosted mode only; a hosted process serves its
// committed files from static/connectors/packages exactly as before and never reaches this code.
import { error } from '@sveltejs/kit';
import { dev } from '$app/environment';
import { version } from '../../../package.json';
import { buildAddinBundle, type AddinBundle } from '$lib/addins/bundle.js';
import { HOSTED } from './config/hosted.js';
import { getConfig } from './config';

export type AddinBundleResult =
  | { ok: true; bundle: AddinBundle }
  | { ok: false; reason: string; origin: string | undefined };

let cache: { key: string; bundle: AddinBundle } | undefined;

/**
 * The bundle for the configured settings, or the reason there is none (the origin cannot be
 * used in an Office manifest). A hosted process gets a 404: these routes do not exist there.
 */
export function currentAddinBundle(): AddinBundleResult {
  const config = getConfig();
  if (config.mode !== 'selfhosted') error(404, 'Not found');
  const origin = config.origin;
  if (!origin)
    return {
      ok: false,
      reason: 'ORIGIN is not set, so the add-in files have no address to use.',
      origin
    };
  const input = {
    origin,
    brand: config.site.brand,
    appVersion: version,
    supportUrl: config.site.notice?.supportUrl || undefined,
    provider: config.site.notice?.organisation || undefined,
    // A development server or a test stack (DEV_AUTH) may be reached on http://localhost; no
    // installation people use may. Office loads add-ins over HTTPS only.
    allowLoopbackHttp: dev || config.devAuth,
    // This installation is not the hosted service; a package naming it is a mistake.
    forbidDomain: HOSTED.domain
  };
  const key = JSON.stringify(input);
  if (cache?.key === key) return { ok: true, bundle: cache.bundle };
  try {
    const bundle = buildAddinBundle(input);
    cache = { key, bundle };
    return { ok: true, bundle };
  } catch (cause) {
    return {
      ok: false,
      reason: `${cause instanceof Error ? cause.message : 'The add-in files could not be made.'} Set ORIGIN to the https address people use to open this installation.`,
      origin
    };
  }
}
