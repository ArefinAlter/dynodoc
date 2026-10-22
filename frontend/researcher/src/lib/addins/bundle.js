// Everything an installation distributes for its add-ins, as one set of files: the Office
// manifests (standard and catalog-compatible), the Google packages and a checksum list. The
// web server serves these files and scripts/build-addins.mjs writes them to a directory, from
// this one function, so what an administrator downloads and what the offline builder writes
// cannot differ. Plain JavaScript with no framework imports.
import { createHash } from 'node:crypto';
import { requireAddinOrigin } from './origin.js';
import {
  OFFICE_KINDS,
  OFFICE_VARIANTS,
  addinGuid,
  manifestFileName,
  manifestVersion,
  renderOfficeManifest
} from './manifest.js';
import {
  GOOGLE_HOSTS,
  googlePackageName,
  googleUnavailableReason,
  renderGooglePackage
} from './google.js';

export const CHECKSUMS_NAME = 'SHA256SUMS';

/**
 * @typedef {object} AddinFile
 * @property {string} name the file name, which is also its last path segment when served
 * @property {'office' | 'google' | 'checksums'} group
 * @property {string} [host] word, excel, powerpoint, google-docs, ...
 * @property {'standard' | 'catalog'} [variant]
 * @property {string} [id] the add-in id (Office manifests)
 * @property {string} contentType
 * @property {Uint8Array} bytes
 * @property {string} sha256 lower-case hex
 */
/**
 * @typedef {object} AddinBundle
 * @property {string} origin
 * @property {string} brand
 * @property {string} appVersion
 * @property {string} manifestVersion
 * @property {string} supportUrl what the manifests carry as the support link
 * @property {{ available: boolean, reason?: string }} google
 * @property {AddinFile[]} files
 */

/** @param {Uint8Array | string} data */
export const sha256 = (data) => createHash('sha256').update(data).digest('hex');

/**
 * The checksum list in the format `sha256sum -c` reads: hex, two spaces, name, sorted by name.
 * @param {{ name: string, sha256: string }[]} files
 */
export function checksumText(files) {
  return [...files]
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .map((file) => `${file.sha256}  ${file.name}\n`)
    .join('');
}

/**
 * @param {object} input
 * @param {string} input.origin
 * @param {string} input.brand
 * @param {string} input.appVersion semantic version, such as 1.0.0
 * @param {string} [input.supportUrl]
 * @param {string} [input.displaySuffix]
 * @param {string} [input.provider]
 * @param {boolean} [input.allowLoopbackHttp]
 * @param {string | null} [input.forbidDomain]
 * @returns {AddinBundle}
 */
export function buildAddinBundle(input) {
  const origin = requireAddinOrigin(input.origin, {
    allowLoopbackHttp: input.allowLoopbackHttp === true,
    forbidDomain: input.forbidDomain ?? null
  });
  const version = manifestVersion(input.appVersion);
  /** @type {AddinFile[]} */ const files = [];
  /** @param {Omit<AddinFile, 'sha256'>} file */
  const add = (file) => files.push({ ...file, sha256: sha256(file.bytes) });
  const encoder = new TextEncoder();
  let supportUrl = '';
  for (const kind of OFFICE_KINDS)
    for (const variant of OFFICE_VARIANTS) {
      const text = renderOfficeManifest({
        origin,
        brand: input.brand,
        kind,
        variant,
        version,
        supportUrl: input.supportUrl,
        displaySuffix: input.displaySuffix,
        provider: input.provider,
        allowLoopbackHttp: input.allowLoopbackHttp,
        forbidDomain: input.forbidDomain
      });
      supportUrl = /<SupportUrl DefaultValue="([^"]*)"/.exec(text)?.[1] ?? supportUrl;
      add({
        name: manifestFileName(kind, variant),
        group: 'office',
        host: kind,
        variant,
        id: addinGuid(origin, kind, variant),
        contentType: 'application/xml; charset=utf-8',
        bytes: encoder.encode(text)
      });
    }
  const reason = googleUnavailableReason(origin);
  if (!reason)
    for (const host of GOOGLE_HOSTS)
      add({
        name: googlePackageName(host),
        group: 'google',
        host,
        contentType: 'application/zip',
        bytes: renderGooglePackage({
          origin,
          brand: input.brand,
          host,
          forbidDomain: input.forbidDomain
        })
      });
  add({
    name: CHECKSUMS_NAME,
    group: 'checksums',
    contentType: 'text/plain; charset=utf-8',
    bytes: encoder.encode(checksumText(files))
  });
  return {
    origin,
    brand: input.brand,
    appVersion: input.appVersion,
    manifestVersion: version,
    supportUrl,
    google: reason ? { available: false, reason } : { available: true },
    files
  };
}
