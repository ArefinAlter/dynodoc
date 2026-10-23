// Serving the operator's local copy of Office.js (DYNODOC_OFFICE_JS_DIR, docs/SELF-HOSTED-EDITION.md
// 7.3). Nothing of Office.js is in this repository: the licence terms for redistributing it are
// not settled, so the operator supplies the files and the server only hands out what is in that
// one directory, read-only, by file name.
import { readFile, realpath, stat } from 'node:fs/promises';
import { extname, join, sep } from 'node:path';
import { MAX_FILE_BYTES, SERVED_TYPES } from '$lib/addins/office-js-files.js';

export { MAX_FILE_BYTES, SERVED_TYPES };
const MAX_PATH_CHARS = 200;

export interface ServedFile {
  file: string;
  size: number;
  mtimeMs: number;
  type: string;
}

/**
 * The file a request path names inside `dir`, or null. Refuses an empty path, a dot or hidden
 * segment, a backslash, a NUL, a name that does not end in a served extension, a symbolic link
 * that leaves the directory, anything that is not a regular file, and a file over the size limit.
 */
export async function resolveOfficeJsFile(dir: string, path: string): Promise<ServedFile | null> {
  if (!path || path.length > MAX_PATH_CHARS || /[\\\0]/.test(path) || path.startsWith('/'))
    return null;
  const segments = path.split('/');
  if (segments.some((part) => part === '' || part.startsWith('.'))) return null;
  const type = (SERVED_TYPES as Readonly<Record<string, string>>)[extname(path).toLowerCase()];
  if (!type) return null;
  try {
    const root = await realpath(dir);
    const file = await realpath(join(root, ...segments));
    if (!file.startsWith(root + sep)) return null;
    const info = await stat(file);
    if (!info.isFile() || info.size > MAX_FILE_BYTES) return null;
    return { file, size: info.size, mtimeMs: info.mtimeMs, type };
  } catch {
    return null;
  }
}

/** The weak validator for a file: it changes when the operator replaces the file. */
export const entityTag = (served: ServedFile) =>
  `W/"${served.size.toString(16)}-${Math.floor(served.mtimeMs).toString(16)}"`;

export const readServedFile = (served: ServedFile) => readFile(served.file);
