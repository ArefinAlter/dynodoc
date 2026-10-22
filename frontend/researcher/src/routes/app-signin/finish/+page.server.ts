import { appKind } from '$lib/app-signin';
import type { PageServerLoad } from './$types';

// The finish page works only in the Dynodoc app's own window, which names itself in its
// User-Agent (see $lib/server/app-signin.ts). A visitor's ordinary browser, which a crafted link
// can send here, is told to use the app and nothing in the page acts on a fragment.
export const load: PageServerLoad = ({ request }) => ({
  app: appKind(request.headers.get('user-agent')) !== null
});
