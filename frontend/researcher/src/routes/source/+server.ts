import { redirect } from '@sveltejs/kit';
import { ENGINE_REPO_URL } from '$lib/site/operator';

// The workspace and website source is not published, so there is no archive to serve. Old
// links to /source land on the one public repository: the MIT engine, API and CLI.
export function GET() {
  redirect(302, ENGINE_REPO_URL);
}
