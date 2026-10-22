import { error } from '@sveltejs/kit';
import { currentAddinBundle } from '$lib/server/addins';
import type { RequestHandler } from './$types';

// The add-in files made for this installation: the Office manifests (standard and
// catalog-compatible), the Google packages and SHA256SUMS. They are public on purpose. A
// manifest holds nothing secret (the address, the product name, a support link and ids derived
// from the address), people sideload from a link they were sent, and the Microsoft 365 admin
// center can fetch a manifest from a link. The page that explains how to install them is for
// administrators only (./+page.server.ts). A hosted process answers 404 for every name here.
export const GET: RequestHandler = ({ params }) => {
  const result = currentAddinBundle();
  if (!result.ok)
    return new Response(`${result.reason}\n`, {
      status: 503,
      headers: { 'content-type': 'text/plain; charset=utf-8' }
    });
  const file = result.bundle.files.find((candidate) => candidate.name === params.file);
  if (!file) error(404, 'Not found');
  return new Response(new Uint8Array(file.bytes), {
    headers: {
      'content-type': file.contentType,
      'content-disposition': `attachment; filename="${file.name}"`,
      'x-content-type-options': 'nosniff'
    }
  });
};
