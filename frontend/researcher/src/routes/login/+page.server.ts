import { redirect } from '@sveltejs/kit';
import { env } from '$env/dynamic/public';
import type { PageServerLoad } from './$types';

// The marketing host (dynodoc.online) has no sign-in methods; the workspace host (PUBLIC_WORKSPACE_URL) has
// Google and email sign-in. A /login reached on any other host forwards there with its query intact; on the
// workspace host itself the page renders as before.
export const load: PageServerLoad = ({ url }) => {
  const workspace = env.PUBLIC_WORKSPACE_URL;
  if (!workspace) return;
  const target = new URL('/login', workspace);
  if (target.origin === url.origin) return;
  target.search = url.search;
  redirect(307, target.href);
};
