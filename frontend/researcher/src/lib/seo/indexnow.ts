// IndexNow lets us tell Bing, Yandex, Seznam and Naver that a page changed
// instead of waiting to be recrawled. Ownership is proved by hosting a file
// named after the key, containing the key, at the root of the site.
//
// The key file lives at `static/<key>.txt` so it is served as a plain static
// asset and stays reachable even if the app fails to render. Submission itself
// is a deploy-time concern and lives in `scripts/indexnow.mjs`; only the key's
// identity is needed here, so that robots.txt and the noindex rules in
// hooks.server.ts can keep the file public.

export const INDEXNOW_KEY = 'ee30dd1d086546a79b0725a154acab9c';

/** Root-relative path of the key file, derived so the two cannot disagree. */
export const INDEXNOW_KEY_PATH = `/${INDEXNOW_KEY}.txt`;
