/**
 * Which pages advertise the web app. The manifest and the Home Screen metas describe the
 * workspace (its start page is /workspace), so they belong on app pages only. The public
 * marketing site lives in the `(site)` route group; on the marketing host /workspace
 * redirects to the workspace host, outside the manifest's scope, so those pages must not
 * offer an install that could never start. An unmatched address (an error page) counts as an
 * app page, as it does for the browser bar colour.
 */
export function isAppRoute(routeId: string | null | undefined): boolean {
  return !(routeId ?? '').startsWith('/(site)');
}
