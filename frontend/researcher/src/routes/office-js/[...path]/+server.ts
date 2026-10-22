import { error } from '@sveltejs/kit';
import { getConfig } from '$lib/server/config';
import { entityTag, readServedFile, resolveOfficeJsFile } from '$lib/server/office-js';
import type { RequestHandler } from './$types';

// The local copy of Office.js and its sibling files, when the installation has one
// (DYNODOC_OFFICE_JS_DIR). Without a directory this route does not exist: 404 for everyone.
// The files are public (Office panes load them without a session) and revalidated on every
// load, so an operator who replaces the copy is not stuck behind a cache.
export const GET: RequestHandler = async ({ params, request }) => {
  const dir = getConfig().officeJsDir;
  if (!dir) error(404, 'Not found');
  const served = await resolveOfficeJsFile(dir, params.path);
  if (!served) error(404, 'Not found');
  const etag = entityTag(served);
  const headers = { etag, 'cache-control': 'no-cache' };
  if (request.headers.get('if-none-match') === etag)
    return new Response(null, { status: 304, headers });
  return new Response(new Uint8Array(await readServedFile(served)), {
    headers: { ...headers, 'content-type': served.type, 'x-content-type-options': 'nosniff' }
  });
};
