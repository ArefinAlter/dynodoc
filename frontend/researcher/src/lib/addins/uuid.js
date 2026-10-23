// Name-based UUIDs (RFC 9562, version 5, SHA-1), so an add-in id can be derived instead of
// stored: the same installation origin and add-in always get the same id, and two
// installations get different ones. Plain JavaScript with no framework imports, because the
// web server, the tests and scripts/build-addins.mjs all load it.
import { createHash } from 'node:crypto';

/** Fixed namespace for every Dynodoc add-in id. Changing it changes every derived id. */
export const ADDIN_NAMESPACE = 'fd916f5f-7fd1-41c2-8727-62ba6c31caca';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** @param {unknown} value */
export const isUuid = (value) => typeof value === 'string' && UUID.test(value);

/**
 * @param {string} name any text; it is encoded as UTF-8
 * @param {string} [namespace] a lower-case UUID
 */
export function uuidV5(name, namespace = ADDIN_NAMESPACE) {
  if (!isUuid(namespace)) throw new RangeError('The namespace is not a lower-case UUID.');
  const hash = createHash('sha1')
    .update(Buffer.from(namespace.replaceAll('-', ''), 'hex'))
    .update(Buffer.from(name, 'utf8'))
    .digest();
  hash[6] = (hash[6] & 0x0f) | 0x50;
  hash[8] = (hash[8] & 0x3f) | 0x80;
  const hex = hash.subarray(0, 16).toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
