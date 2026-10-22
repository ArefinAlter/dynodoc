// Which files of an operator-supplied copy of Office.js the server hands out, and how big one
// may be. Shared by the route that serves them ($lib/server/office-js.ts) and by
// scripts/stage-office-js.mjs, which copies exactly these files. Plain JavaScript with no
// framework imports.

/** What may be served, by extension. Pages, SVG and everything else that can run script are not. */
export const SERVED_TYPES = Object.freeze({
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.map': 'application/json; charset=utf-8',
  '.txt': 'text/plain; charset=utf-8',
  '.png': 'image/png',
  '.gif': 'image/gif',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2'
});
export const MAX_FILE_BYTES = 16 * 1024 * 1024;
