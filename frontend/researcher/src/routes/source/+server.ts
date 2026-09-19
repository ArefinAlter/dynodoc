import { env } from '$env/dynamic/private';
import { env as publicEnv } from '$env/dynamic/public';
import { readFile } from 'node:fs/promises';
import { error, redirect } from '@sveltejs/kit';
export async function GET() {
  if (publicEnv.PUBLIC_WORKSPACE_URL) redirect(302, publicEnv.PUBLIC_WORKSPACE_URL + '/source');
  if (!env.SOURCE_ARCHIVE_PATH)
    error(404, 'The source archive is available on the deployed workspace.');
  const source = await readFile(env.SOURCE_ARCHIVE_PATH).catch(() => null);
  if (!source) error(404, 'Source archive unavailable');
  return new Response(source, {
    headers: {
      'content-type': 'application/gzip',
      'content-disposition': 'attachment; filename="dynodoc-source.tar.gz"',
      'cache-control': 'public, max-age=300'
    }
  });
}
