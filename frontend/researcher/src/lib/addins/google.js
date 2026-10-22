// The Google editor add-on packages (Apps Script projects for Docs, Sheets and Slides),
// generated for one installation (docs/SELF-HOSTED-EDITION.md 7.5, P0-7). The sources are the
// ones the hosted packages are built from, held in google-templates.generated.js with the
// hosted origin replaced by a placeholder; only the origin changes. Nothing is published to
// Google: the result is a ZIP an administrator unpacks into an Apps Script project.
// Plain JavaScript with no framework imports.
import defaultTemplates from './google-templates.generated.js';
import { requireAddinOrigin } from './origin.js';
import { zipStored } from './zip.js';

export const GOOGLE_HOSTS = /** @type {const} */ ([
  'google-docs',
  'google-sheets',
  'google-slides'
]);
/** @typedef {(typeof GOOGLE_HOSTS)[number]} GoogleHost */

const APP_NAMES = /** @type {Record<GoogleHost, string>} */ ({
  'google-docs': 'Docs',
  'google-sheets': 'Sheets',
  'google-slides': 'Slides'
});

/** The order the hosted packages store their files in. */
const ORDER = [
  'Code.gs',
  'Sidebar.html',
  'Protocol.html',
  'Pairing.gs',
  'appsscript.json',
  'Common.gs',
  'Date.gs'
];

/** @typedef {typeof defaultTemplates} GoogleTemplates */

/** @param {unknown} host @returns {host is GoogleHost} */
export const isGoogleHost = (host) => GOOGLE_HOSTS.includes(/** @type {never} */ (host));

/** The package's file name. @param {GoogleHost} host */
export const googlePackageName = (host) => `${host}.zip`;

/**
 * Why a Google package cannot be made for an origin, or undefined. Apps Script fetches only
 * https:// addresses, and fetches them from Google's servers, so a plain-http (local test)
 * origin has no package.
 * @param {string} origin a validated origin
 */
export function googleUnavailableReason(origin) {
  return origin.startsWith('https://')
    ? undefined
    : 'Google editor add-ons can call only https:// addresses, and this installation is not on one.';
}

/**
 * @param {object} input
 * @param {string} input.origin
 * @param {string} input.brand
 * @param {GoogleHost} input.host
 */
function installText({ origin, brand, host }) {
  const app = APP_NAMES[host];
  return `# Install the ${brand} add-on for Google ${app}

This package was made for ${origin}. The add-on sends every request to
${origin}/api/connectors/ and to no other address.

## Before you start

- Google's servers make those requests, not the person's computer. ${origin} must be
  reachable from the public internet. An address that works only inside a company network
  cannot be used with Google editors. On a network with no internet access, the Google
  add-ons do not work at all.
- The add-on keeps its capture queue and the file's connection key in Apps Script user
  properties. Google stores them, so that data is outside your installation.
- Check that the origin above is the one people use to open ${brand}. A package made for a
  different address will not connect.

## Install it for testing (one person)

1. Open https://script.google.com and create a standalone project. Name it "${brand}" so the
   add-on menu reads Extensions > ${brand}.
2. Add every file in this package to the project under the same name. Open Project settings
   and turn on "Show appsscript.json manifest file in editor", then replace the project's
   appsscript.json with the one in this package.
3. Choose Deploy > Test deployments > Editor add-on, pick a file to test with, save, and run
   the test. Google may ask you to allow access to the current file and to external requests.
4. In the file, choose Extensions > ${brand} > Open Dynodoc, then follow the steps in the
   sidebar to sign in and connect the file.

## Install it for an organisation

A Google Workspace administrator can publish the add-on as a private Google Workspace
Marketplace listing. That needs a Google Cloud project and the Marketplace configuration, and
it may need Google's review of the permissions the add-on asks for. Whether it does has not
been checked for this package; ask Google or check at the time. This package does not publish
anything.

## Check the origin

- Code.gs contains the line DYNODOC_ORIGIN with ${origin} between quotes.
- appsscript.json lists ${origin}/api/connectors/ under urlFetchWhitelist.

If you move ${brand} to another address, make a new package; do not edit these two places by
hand.

Installing the package is not acceptance. ACCEPTANCE.md is the worksheet for recording what
was tried in a real Google editor.
`;
}

/**
 * The files of one package, in the order they are stored.
 * @param {object} input
 * @param {string} input.origin an https origin
 * @param {string} input.brand
 * @param {GoogleHost} input.host
 * @param {string | null} [input.forbidDomain]
 * @param {GoogleTemplates} [input.templates] default: the generated templates
 * @returns {{ name: string, data: string }[]}
 */
export function renderGooglePackageFiles({
  origin: raw,
  brand,
  host,
  forbidDomain = null,
  templates = defaultTemplates
}) {
  if (!isGoogleHost(host)) throw new RangeError(`Unknown Google add-on "${String(host)}".`);
  const origin = requireAddinOrigin(raw, { forbidDomain });
  const unavailable = googleUnavailableReason(origin);
  if (unavailable) throw new RangeError(unavailable);
  const own = templates.hosts[host];
  const files = /** @type {Record<string, string>} */ ({ ...templates.common, ...own });
  const out = [];
  for (const name of ORDER) {
    if (!(name in files)) continue;
    out.push({ name, data: files[name].replaceAll(templates.placeholder, origin) });
  }
  const manifest = JSON.parse(out.find((file) => file.name === 'appsscript.json')?.data ?? 'null');
  if (JSON.stringify(manifest?.urlFetchWhitelist) !== JSON.stringify([`${origin}/api/connectors/`]))
    throw new Error('The generated appsscript.json does not allow exactly the connector API.');
  // Docs writes the constant with spaces around the equals sign, Sheets and Slides without.
  const code = out.find((file) => file.name === 'Code.gs')?.data ?? '';
  if (![`DYNODOC_ORIGIN = '${origin}'`, `DYNODOC_ORIGIN='${origin}'`].some((m) => code.includes(m)))
    throw new Error('The generated Code.gs does not carry the origin.');
  if (out.some((file) => /\{\{[A-Z_]+\}\}/.test(file.data)))
    throw new Error('A placeholder was left in the package.');
  out.push({ name: 'INSTALL.md', data: installText({ origin, brand, host }) });
  out.push({ name: 'ACCEPTANCE.md', data: files['ACCEPTANCE.md'] });
  return out;
}

/** The finished ZIP. @param {Parameters<typeof renderGooglePackageFiles>[0]} input */
export const renderGooglePackage = (input) => zipStored(renderGooglePackageFiles(input));
