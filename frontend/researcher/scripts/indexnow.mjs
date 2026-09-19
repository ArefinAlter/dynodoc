#!/usr/bin/env node
// Submit changed URLs to IndexNow, which forwards them to Bing, Yandex, Seznam
// and Naver. Run it after a deploy that changed public page content.
//
//   node scripts/indexnow.mjs                      # everything in the sitemap
//   node scripts/indexnow.mjs /privacy /terms      # just these paths
//   node scripts/indexnow.mjs --dry-run            # print the payload, send nothing
//
// The key is read from the key file in `static/`, never hardcoded here, so the
// submitted key and the published key are the same by construction.

import { readdir, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ORIGIN = process.env.INDEXNOW_ORIGIN ?? 'https://dynodoc.online';
const ENDPOINT = 'https://api.indexnow.org/IndexNow';
const STATIC_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'static');

/**
 * The documented IndexNow response codes; anything else is reported verbatim.
 * @param {number} status
 * @returns {string}
 */
export function describeStatus(status) {
  return (
    /** @type {Record<number, string>} */ ({
      200: 'OK — URLs submitted successfully.',
      202: 'Accepted — URLs received, key validation pending.',
      400: 'Bad request — invalid format.',
      403: 'Forbidden — key not valid (key file missing, or key not in the file).',
      422: 'Unprocessable entity — a URL is not on this host, or the key does not match the schema.',
      429: 'Too many requests — throttled as potential spam.'
    })[status] ?? `Unexpected status ${status}.`
  );
}

/**
 * Find the `<32 hex>.txt` key file in static/ and return its name and contents.
 * @param {string} [staticDir]
 * @returns {Promise<{ key: string, fileName: string }>}
 */
export async function readKey(staticDir = STATIC_DIR) {
  const names = await readdir(staticDir);
  const keyFiles = names.filter((name) => /^[0-9a-f]{32}\.txt$/.test(name));
  if (keyFiles.length !== 1)
    throw new Error(
      `Expected exactly one IndexNow key file in ${staticDir}, found ${keyFiles.length}.`
    );
  const key = (await readFile(join(staticDir, keyFiles[0]), 'utf8')).trim();
  if (`${key}.txt` !== keyFiles[0])
    throw new Error(`Key file ${keyFiles[0]} does not contain its own key (${key}).`);
  return { key, fileName: keyFiles[0] };
}

/**
 * Build the submission body. Every URL must be on the origin's host, or
 * IndexNow rejects the whole batch with 422 — so fail here instead.
 * @param {string} origin
 * @param {string} key
 * @param {string[]} urls
 * @returns {{ host: string, key: string, keyLocation: string, urlList: string[] }}
 */
export function buildPayload(origin, key, urls) {
  const base = origin.replace(/\/$/, '');
  const host = new URL(base).hostname;
  const urlList = urls.map((url) =>
    url.startsWith('http') ? url : `${base}${url.startsWith('/') ? url : `/${url}`}`
  );
  const offHost = urlList.filter((url) => new URL(url).hostname !== host);
  if (offHost.length > 0)
    throw new Error(`IndexNow rejects URLs outside ${host}: ${offHost.join(', ')}`);
  if (urlList.length === 0) throw new Error('IndexNow needs at least one URL.');
  return { host, key, keyLocation: `${base}/${key}.txt`, urlList };
}

/**
 * Read the published sitemap so we submit exactly what we advertise.
 * @param {string} origin
 * @returns {Promise<string[]>}
 */
async function urlsFromSitemap(origin) {
  const response = await fetch(`${origin.replace(/\/$/, '')}/sitemap.xml`);
  if (!response.ok) throw new Error(`Could not read sitemap.xml (${response.status}).`);
  const xml = await response.text();
  return [...xml.matchAll(/<loc>([^<]+)<\/loc>/g)].map((match) => match[1]);
}

async function main() {
  const args = process.argv.slice(2);
  const dryRun = args.includes('--dry-run');
  const paths = args.filter((arg) => !arg.startsWith('--'));

  const { key, fileName } = await readKey();
  const urls = paths.length > 0 ? paths : await urlsFromSitemap(ORIGIN);
  const payload = buildPayload(ORIGIN, key, urls);

  console.log(`Key file: ${ORIGIN}/${fileName}`);
  console.log(`Submitting ${payload.urlList.length} URL(s):`);
  for (const url of payload.urlList) console.log(`  ${url}`);

  if (dryRun) {
    console.log('\n--dry-run: nothing sent.');
    return;
  }

  const response = await fetch(ENDPOINT, {
    method: 'POST',
    headers: { 'content-type': 'application/json; charset=utf-8' },
    body: JSON.stringify(payload)
  });
  console.log(`\n${response.status}: ${describeStatus(response.status)}`);
  if (!response.ok) process.exitCode = 1;
}

// Only run when invoked directly, so the helpers above stay importable in tests.
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) await main();
