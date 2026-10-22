// Secret-bearing settings: `NAME`, or `NAME_FILE` naming a file that holds the value.
// The same rules as the API (crates/engine-api/src/secrets.rs), in the order they apply:
// - Only `NAME` set: the value is used exactly as given (no trimming, no checks here).
// - Only `NAME_FILE` set: the file is read, trailing line breaks are removed, and the rest
//   is the value. It must be a regular file of at most MAX_SECRET_FILE_BYTES holding UTF-8
//   text. On Unix a file that group or others can access is refused;
//   DYNODOC_ALLOW_OPEN_SECRET_FILES=true accepts it (Docker and Kubernetes secret mounts
//   often need this).
// - Both set: an error. An empty `NAME` or `NAME_FILE` counts as unset, so a template line
//   such as `WEB_SERVICE_KEY=` cannot collide with `WEB_SERVICE_KEY_FILE`.
// Problems name the variable (and the file path) and never contain a secret value or any
// part of one. This file has no SvelteKit imports: scripts/dynodoc-env.mjs loads it too.
import { closeSync, fstatSync, openSync, readSync } from 'node:fs';

/** Largest accepted secret file. Keys and URLs are far smaller; the bound stops a wrong path
 *  (a log, a device) from being read into memory. */
export const MAX_SECRET_FILE_BYTES = 16 * 1024;

/** Set to `true` to accept a secret file that group or others can read (Unix only). */
export const ALLOW_OPEN_FILES = 'DYNODOC_ALLOW_OPEN_SECRET_FILES';

/** @typedef {(name: string) => string | undefined} Lookup */

/** The text of a secret. Printing, logging or serialising it shows `***`, never the value. */
export class Secret {
  #value;
  /** @param {string} value */
  constructor(value) {
    this.#value = value;
  }
  expose() {
    return this.#value;
  }
  toString() {
    return 'Secret(***)';
  }
  toJSON() {
    return '***';
  }
  [Symbol.for('nodejs.util.inspect.custom')]() {
    return 'Secret(***)';
  }
}

/** True when neither group nor others have any access bit. @param {number} mode */
export function ownerOnly(mode) {
  return (mode & 0o077) === 0;
}

const REASONS = {
  ENOENT: 'no such file or directory',
  EACCES: 'permission denied',
  EPERM: 'operation not permitted',
  EISDIR: 'is a directory',
  ENOTDIR: 'a path component is not a directory'
};
const reason = (/** @type {any} */ error) =>
  REASONS[/** @type {keyof typeof REASONS} */ (error?.code)] ?? String(error?.code ?? 'error');

/**
 * Read a bounded UTF-8 file. Returns the exact text or a one-line problem.
 * @param {string} variable the variable that named the file, for the message
 * @param {string} path
 * @param {{ maxBytes: number, ownerOnly: boolean, allowOpen: boolean, platform?: string }} options
 * @returns {{ text: string } | { problem: string }}
 */
export function readBoundedFile(variable, path, options) {
  const fail = (/** @type {string} */ why) => ({ problem: `${variable} (${path}): ${why}` });
  let fd;
  try {
    fd = openSync(path, 'r');
  } catch (error) {
    return fail(`cannot open: ${reason(error)}`);
  }
  try {
    const stat = fstatSync(fd);
    if (!stat.isFile()) return fail('is not a regular file');
    if ((options.platform ?? process.platform) !== 'win32') {
      if (options.ownerOnly && !options.allowOpen && !ownerOnly(stat.mode)) {
        return fail(
          `is accessible by group or others (mode ${(stat.mode & 0o777).toString(8).padStart(4, '0')}); run chmod 600 on it, or set ${ALLOW_OPEN_FILES}=true where the platform fixes the mode`
        );
      }
    }
    const tooLarge = () => fail(`is larger than ${options.maxBytes} bytes`);
    if (stat.size > options.maxBytes) return tooLarge();
    // Read one byte past the bound, so a file that grows after fstat is still refused.
    const buffer = Buffer.alloc(options.maxBytes + 1);
    let length = 0;
    for (;;) {
      const read = readSync(fd, buffer, length, buffer.length - length, null);
      if (read === 0) break;
      length += read;
      if (length > options.maxBytes) return tooLarge();
    }
    try {
      // `ignoreBOM` keeps a leading byte order mark as content, as the API does.
      return {
        text: new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(
          buffer.subarray(0, length)
        )
      };
    } catch {
      return fail('is not valid UTF-8 text');
    }
  } catch (error) {
    return fail(`cannot read: ${reason(error)}`);
  } finally {
    closeSync(fd);
  }
}

/**
 * Read the secret `name`, or `undefined` when neither `name` nor `name_FILE` is set.
 * @param {Lookup} lookup
 * @param {string} name
 * @returns {{ secret?: Secret, problem?: string }}
 */
export function readSecret(lookup, name) {
  const fileVariable = `${name}_FILE`;
  const plain = lookup(name);
  const path = lookup(fileVariable);
  if (path === undefined || path === '') {
    return plain === undefined || plain === '' ? {} : { secret: new Secret(plain) };
  }
  if (plain !== undefined && plain !== '')
    return { problem: `${name} and ${fileVariable} are both set; set exactly one` };
  const file = readBoundedFile(fileVariable, path, {
    maxBytes: MAX_SECRET_FILE_BYTES,
    ownerOnly: true,
    allowOpen: lookup(ALLOW_OPEN_FILES) === 'true'
  });
  if ('problem' in file) return { problem: file.problem };
  const text = file.text.replace(/[\r\n]+$/, '');
  if (text === '') return { problem: `${fileVariable} (${path}): is empty` };
  return { secret: new Secret(text) };
}
