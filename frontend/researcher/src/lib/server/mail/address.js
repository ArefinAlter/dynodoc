// Address and header-text rules for outbound mail. Pure and dependency-free: the configuration
// module and scripts/check-mail.mjs load it too.
//
// The accepted address form is deliberately narrow, a documented subset of RFC 5321:
//   local-part@domain, nothing else.
//   - local part: letters, digits and . _ + - ' = &, no leading, trailing or doubled dot, at most
//     64 characters. No quoted strings, comments, display names, angle brackets, groups, source
//     routes, "%" or "!" forms, and no non-ASCII (no SMTPUTF8).
//   - domain: a DNS name of ASCII letters, digits and hyphens in labels of at most 63
//     characters, with at least one dot (an internationalised name must be given in its
//     "xn--" form). No IP literals such as [192.0.2.1].
//   - at most 254 characters in all.
// Legal but rare forms outside this subset are refused, never rewritten.

export const MAX_ADDRESS_LENGTH = 254;
export const MAX_DISPLAY_NAME_LENGTH = 120;
export const MAX_SUBJECT_LENGTH = 300;

const LOCAL = /^[A-Za-z0-9_+'=&-]+(?:\.[A-Za-z0-9_+'=&-]+)*$/;
const LABEL = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/;

/**
 * @param {string} raw
 * @returns {{ address: string } | { reason: string }}
 */
export function parseAddress(raw) {
  if (typeof raw !== 'string') return { reason: 'is not text' };
  // Only spaces are trimmed: a line break or tab at either end is refused, not removed.
  const value = raw.replace(/^ +| +$/g, '');
  if (value === '') return { reason: 'is empty' };
  if (value.length > MAX_ADDRESS_LENGTH)
    return { reason: `is longer than ${MAX_ADDRESS_LENGTH} characters` };
  const at = value.lastIndexOf('@');
  if (at <= 0 || at === value.length - 1 || value.indexOf('@') !== at)
    return { reason: 'must be one local part, an @ and a domain' };
  const local = value.slice(0, at);
  const domain = value.slice(at + 1);
  if (local.length > 64) return { reason: 'has a local part longer than 64 characters' };
  if (!LOCAL.test(local))
    return {
      reason:
        "has a local part with characters outside letters, digits and . _ + - ' = & (quoted, bracketed and other exotic forms are not accepted)"
    };
  const labels = domain.split('.');
  if (labels.length < 2 || !labels.every((label) => LABEL.test(label)))
    return { reason: 'has a domain that is not a plain DNS name such as example.org' };
  if (/^\d+$/.test(labels[labels.length - 1]))
    return { reason: 'has a domain that looks like an IP address' };
  return { address: `${local}@${domain.toLowerCase()}` };
}

// Code points are built with fromCharCode so that this file holds no invisible characters.
const SEPARATORS = String.fromCharCode(0x2028, 0x2029);
const DIRECTION = String.fromCharCode(
  0x202a,
  0x202b,
  0x202c,
  0x202d,
  0x202e,
  0x2066,
  0x2067,
  0x2068,
  0x2069
);
/** Control characters, line and paragraph separators, and bidirectional overrides. */
const FORBIDDEN_IN_HEADER_TEXT = new RegExp(`[\\x00-\\x1f\\x7f-\\x9f${SEPARATORS}${DIRECTION}]`);
/** Control characters other than tab, line feed and carriage return. */
const FORBIDDEN_IN_BODY_TEXT = /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/; // eslint-disable-line no-control-regex

/**
 * Text that goes into a header (a subject, a display name, a user name): one line, no control
 * characters of any kind (CR and LF above all: they would end the header and start another).
 * Returns the reason it is refused, or undefined.
 * @param {unknown} value
 * @param {number} max
 */
export function headerTextProblem(value, max) {
  if (typeof value !== 'string') return 'is not text';
  if (FORBIDDEN_IN_HEADER_TEXT.test(value))
    return 'contains a line break or another control character';
  if (value.length > max) return `is longer than ${max} characters`;
  return undefined;
}

/**
 * Text that goes into a body: line breaks and tabs are fine, other control characters are not.
 * @param {unknown} value
 */
export function bodyTextProblem(value) {
  if (typeof value !== 'string') return 'is not text';
  if (FORBIDDEN_IN_BODY_TEXT.test(value)) return 'contains a control character';
  return undefined;
}

/** The domain part of a validated address. @param {string} address */
export const domainOf = (address) => address.slice(address.lastIndexOf('@') + 1);
