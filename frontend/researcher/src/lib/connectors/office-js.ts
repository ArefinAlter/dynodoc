// Where an Office pane loads Office.js from. By default it is Microsoft's CDN. When the
// installation has a local copy (DYNODOC_OFFICE_JS_DIR), the server adds a meta tag to the pane
// pages naming the same-origin path, and the pane uses that instead. Nothing else about the
// pane changes, and without the tag the pane behaves exactly as before.
export const OFFICE_JS_CDN = 'https://appsforoffice.microsoft.com/lib/1/hosted/office.js';
/** The route that serves the local copy; the directory's files appear below /office-js/. */
export const OFFICE_JS_ROUTE = '/office-js/';
export const OFFICE_JS_LOCAL_PATH = `${OFFICE_JS_ROUTE}office.js`;
export const OFFICE_JS_META = 'dynodoc-office-js';

/** The address a pane should load Office.js from. */
export function officeJsUrl(doc: Pick<Document, 'querySelector'>): string {
  const content = doc.querySelector(`meta[name="${OFFICE_JS_META}"]`)?.getAttribute('content');
  // Only the one same-origin path the server writes is honoured, never an address from a page.
  return content === OFFICE_JS_LOCAL_PATH ? content : OFFICE_JS_CDN;
}
