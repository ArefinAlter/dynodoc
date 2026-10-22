// The origin an add-in package will point at. It ends up inside XML attributes, JSON, a
// JavaScript string and a Google URL allow-list, so it is accepted only in one canonical
// shape: exactly what a browser reports as `location.origin`. Anything else is refused with a
// reason an administrator can act on, instead of being cleaned up silently.
// Plain JavaScript with no framework imports (the web server, the tests and
// scripts/build-addins.mjs share it).

export const ORIGIN_MAX = 255;

const LOOPBACK = /^(?:localhost|127(?:\.\d{1,3}){3}|\[::1\])$/;

/**
 * @typedef {{ ok: true, origin: string } | { ok: false, reason: string }} OriginResult
 */

/**
 * @param {unknown} raw
 * @param {object} [options]
 * @param {boolean} [options.allowLoopbackHttp] accept http:// for localhost, 127.x.x.x and
 *   [::1] only; a development or test stack, never an installation people use
 * @param {string | null} [options.forbidDomain] refuse this domain and its sub-domains (the
 *   hosted service's, so a self-hosted package can never name it)
 * @returns {OriginResult}
 */
export function parseAddinOrigin(raw, options = {}) {
  /** @param {string} reason @returns {OriginResult} */
  const refuse = (reason) => ({ ok: false, reason });
  if (typeof raw !== 'string' || raw === '') return refuse('is not set');
  if (raw.length > ORIGIN_MAX) return refuse(`is longer than ${ORIGIN_MAX} characters`);
  if (/[^\x21-\x7e]/.test(raw))
    return refuse('contains a space, a control character or a character outside ASCII');
  if (/[\\<>"'`{}|^]/.test(raw))
    return refuse('contains a character that is not part of an origin');
  let url;
  try {
    url = new URL(raw);
  } catch {
    return refuse('is not a URL (write it like https://dynodoc.example.org)');
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:')
    return refuse('must start with https://');
  if (url.username || url.password)
    return refuse('must not contain a user name or password before the host name');
  if (url.origin !== raw)
    return refuse(
      'must be only the scheme, the host name and a non-default port, written the way a browser writes it: lower case, no path, query, fragment, trailing slash or default port, and an internationalised name in its xn-- form'
    );
  if (url.port === '0') return refuse('must not use port 0');
  if (url.hostname.endsWith('.')) return refuse('must not end the host name with a dot');
  if (url.protocol === 'http:' && !(options.allowLoopbackHttp && LOOPBACK.test(url.hostname)))
    return refuse(
      'must use https: Office loads add-ins only over HTTPS (http is accepted for localhost on a test stack only)'
    );
  const domain = options.forbidDomain?.toLowerCase();
  if (domain && (url.hostname === domain || url.hostname.endsWith(`.${domain}`)))
    return refuse(`is an address of the hosted service (${domain}), not of this installation`);
  return { ok: true, origin: url.origin };
}

/**
 * The origin or a RangeError carrying the reason.
 * @param {unknown} raw
 * @param {Parameters<typeof parseAddinOrigin>[1]} [options]
 */
export function requireAddinOrigin(raw, options) {
  const parsed = parseAddinOrigin(raw, options);
  if (!parsed.ok) throw new RangeError(`The origin ${parsed.reason}.`);
  return parsed.origin;
}
