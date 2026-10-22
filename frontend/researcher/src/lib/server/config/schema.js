// Web-server configuration: every setting and secret the SvelteKit server reads, validated
// together so a misconfigured deployment gets one error that lists all problems (the API does
// the same in crates/engine-api/src/config.rs). Pure: it takes a lookup function and touches
// the file system only to read `*_FILE` secrets and the notice file, so the server hook, the
// unit tests and scripts/dynodoc-env.mjs share it. No message contains a secret value.
//
// DYNODOC_MODE=hosted (the default) keeps the hosted service exactly as it was: the fixed
// hostnames in ./hosted.js, and a validation that rejects only what could not have worked.
// DYNODOC_MODE=selfhosted turns the hosted-only parts off and requires a complete setup.
import { ALLOW_OPEN_FILES, readBoundedFile, readSecret, Secret } from './secrets.js';
import { HOSTED } from './hosted.js';

/** @typedef {import('./secrets.js').Lookup} Lookup */
/**
 * What the browser may know. Serialisable; carries no secret.
 * @typedef {object} PublicSite
 * @property {'hosted' | 'selfhosted'} mode
 * @property {string} brand product name shown in headings and titles
 * @property {{ organisation: string, text: string, supportUrl: string } | null} notice
 * @property {boolean} indexable hosted only: search engines may be offered the public pages
 * @property {string} homeUrl where a brand link goes
 * @property {string | null} helpUrl where "Full guide" goes, or none
 * @property {string} workspaceUrl origin of the workspace for links from other pages, '' if same origin
 * @property {string | null} themeCookieDomain shared parent domain for the theme cookie, if any
 * @property {{ endpoint: string, hosts: string[] } | null} telemetry null when page metrics are off
 * @property {boolean} emailSignIn false when email sign-in is switched off
 */
/**
 * @typedef {object} WebConfig
 * @property {'hosted' | 'selfhosted'} mode
 * @property {string | undefined} origin ORIGIN
 * @property {string[]} origins ORIGIN plus the alternative origins (self-hosted)
 * @property {string} engineApiUrl
 * @property {boolean} adminSite
 * @property {boolean} devAuth
 * @property {boolean} emailSignIn
 * @property {boolean} marketing the (site) pages exist
 * @property {boolean} indexable hosted only
 * @property {string | undefined} workspaceUrl PUBLIC_WORKSPACE_URL without a trailing slash
 * @property {{ fromHost: string, to: string } | null} rootRedirect
 * @property {string | null} hostedDomain hosted only: hostnames under it are labelled Production in /admin
 * @property {string[]} connectSrc extra `connect-src` origins on every page
 * @property {string} connectorOrigins CONNECTOR_ORIGINS as given (parsed where it is used)
 * @property {{ enabled: boolean, origins: [string, string][] }} telemetry
 * @property {string | undefined} emailFrom
 * @property {string | undefined} googleClientId
 * @property {string | undefined} converterUrl
 * @property {string | undefined} opsStatusPath
 * @property {PublicSite} site
 * @property {{ session?: Secret, webServiceKey?: Secret, resendApiKey?: Secret, googleClientSecret?: Secret, converterKey?: Secret }} secrets
 */
/**
 * @typedef {object} LoadResult
 * @property {boolean} ok no problems
 * @property {string[]} problems each stops startup
 * @property {string[]} warnings reported, never fatal
 * @property {Readonly<WebConfig>} config best effort when not ok; secrets are present only when valid
 */

export const MODES = ['hosted', 'selfhosted'];
export const MIN_SECRET_BYTES = 32;
export const BRAND_MAX = 60;
export const ORG_NAME_MAX = 120;
export const NOTICE_MAX_CHARS = 2000;
export const NOTICE_MAX_LINES = 40;
export const NOTICE_FILE_MAX_BYTES = 16 * 1024;
export const SUPPORT_URL_MAX = 300;
export const ALT_ORIGINS_MAX = 8;

/** Settings only self-hosted mode reads; set in hosted mode they are reported and ignored. */
const SELF_HOSTED_ONLY = [
  'DYNODOC_ALT_ORIGINS',
  'DYNODOC_BRAND_NAME',
  'DYNODOC_ORG_NAME',
  'DYNODOC_ORG_NOTICE',
  'DYNODOC_NOTICE_FILE',
  'DYNODOC_SUPPORT_URL'
];

/** Every DYNODOC_* name some Dynodoc component reads; the typo check compares against these. */
export const KNOWN_DYNODOC_NAMES = [
  'DYNODOC_MODE',
  'DYNODOC_TELEMETRY',
  'DYNODOC_ALLOW_OPEN_SECRET_FILES',
  ...SELF_HOSTED_ONLY,
  'DYNODOC_NAMED_SNAPSHOT_STORAGE',
  'DYNODOC_PERIODIC_CHECKPOINT_STORAGE',
  'DYNODOC_SHARED_DRAFT_BASES',
  'DYNODOC_SHARED_CHECKPOINT_WRITES',
  'DYNODOC_CHECKPOINT_WRITER_LOCK',
  'DYNODOC_REF_SIGNING_KEY_FILE',
  'DYNODOC_KEY_PASSPHRASE_FILE',
  'DYNODOC_KEY',
  'DYNODOC_ORIGIN',
  'DYNODOC_PROJECT',
  'DYNODOC_REMOTE',
  'DYNODOC_WRITE',
  'DYNODOC_HOST',
  'DYNODOC_ID',
  'DYNODOC_TARGET'
];

/** Text that documents show where a secret goes; a value containing one was never generated. */
const PLACEHOLDERS = [
  'changeme',
  'change-me',
  'change_me',
  'replace-with',
  'replace_with',
  'replacewith',
  'placeholder',
  'your-secret',
  'your_secret',
  'your-key',
  'your_key'
];

/**
 * Why a secret is too weak to use, or undefined. A sanity check, not a proof of entropy:
 * it catches short, blank, repeated and documented placeholder values. Never echoes the value.
 * @param {string} value
 */
export function weakSecretReason(value) {
  if (Buffer.byteLength(value, 'utf8') < MIN_SECRET_BYTES)
    return `must be at least ${MIN_SECRET_BYTES} characters`;
  if (value.trim() === '') return 'must not be blank';
  const lower = value.toLowerCase();
  if (PLACEHOLDERS.some((word) => lower.includes(word)) || /<[^>]*>/.test(value))
    return 'looks like a placeholder; generate a random value (openssl rand -hex 32)';
  if (new Set(value).size < 8)
    return 'uses too few distinct characters; generate a random value (openssl rand -hex 32)';
  for (let period = 1; period <= 8; period++) {
    let repeats = true;
    for (let i = period; i < value.length && repeats; i++) repeats = value[i] === value[i - period];
    if (repeats) return 'repeats a short pattern; generate a random value (openssl rand -hex 32)';
  }
  return undefined;
}

/** Edit distance, for the typo warning. @param {string} a @param {string} b */
function distance(a, b) {
  let row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const next = [i];
    for (let j = 1; j <= b.length; j++)
      next[j] = Math.min(row[j] + 1, next[j - 1] + 1, row[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
    row = next;
  }
  return row[b.length];
}

/** The known DYNODOC_* names an unknown one resembles. @param {string} name */
export function likelyTypoOf(name) {
  if (KNOWN_DYNODOC_NAMES.includes(name)) return [];
  const squash = (/** @type {string} */ s) => s.toLowerCase().replace(/[^a-z0-9]/g, '');
  return KNOWN_DYNODOC_NAMES.filter((known) => {
    if (squash(known) === squash(name)) return true;
    const limit = known.length > 20 ? 3 : 2;
    return distance(name, known) <= limit;
  });
}

/** Control characters other than tab and line breaks. */
const hasControl = (/** @type {string} */ text) =>
  [...text].some((ch) => {
    const code = ch.codePointAt(0) ?? 0;
    return (code < 32 && code !== 9 && code !== 10 && code !== 13) || code === 127;
  });
const BIDI = /[‪-‮⁦-⁩]/;
const shown = (/** @type {string} */ v) => (v.length > 40 ? `${v.slice(0, 40)}...` : v);

/**
 * An origin with no path: scheme, host and optional port, as a browser reports it.
 * @param {string} raw
 * @returns {{ origin: string } | { reason: string }}
 */
function parseOrigin(raw) {
  let url;
  try {
    url = new URL(raw);
  } catch {
    return { reason: 'is not a URL' };
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:')
    return { reason: 'must start with https:// or http://' };
  if (url.username || url.password) return { reason: 'must not contain a user name or password' };
  if (url.origin !== raw)
    return {
      reason:
        'must be an origin: scheme, host and optional port, with no path, query or trailing slash'
    };
  return { origin: url.origin };
}

const isLoopback = (/** @type {string} */ origin) =>
  /^https?:\/\/(localhost|127\.\d+\.\d+\.\d+|\[::1\])(:\d+)?$/.test(origin);

/** Plain text with line breaks. @returns {string | undefined} a reason when unusable */
function plainTextReason(/** @type {string} */ text) {
  if (hasControl(text) || BIDI.test(text))
    return 'contains control or direction-override characters';
  return undefined;
}

/**
 * Validate the environment. Always returns a configuration; `ok` says whether it may be used.
 * @param {Record<string, string | undefined>} env the process environment or any stand-in
 * @returns {LoadResult}
 */
export function loadConfig(env) {
  /** @type {Lookup} */
  const lookup = (name) => (Object.hasOwn(env, name) ? env[name] : undefined);
  /** @type {string[]} */ const problems = [];
  /** @type {string[]} */ const warnings = [];
  const get = (/** @type {string} */ name) => {
    const value = lookup(name);
    return value === undefined || value.trim() === '' ? undefined : value;
  };

  // --- Mode ---
  /** @type {'hosted' | 'selfhosted'} */
  let mode = 'hosted';
  const rawMode = get('DYNODOC_MODE');
  if (rawMode !== undefined) {
    const value = rawMode.trim().toLowerCase();
    if (value === 'hosted' || value === 'selfhosted') mode = value;
    else problems.push(`DYNODOC_MODE must be hosted or selfhosted (got "${shown(rawMode)}")`);
  }
  const self = mode === 'selfhosted';

  // --- Secrets ---
  /** @param {string} name */
  const secret = (name) => {
    const found = readSecret(lookup, name);
    if (found.problem) problems.push(found.problem);
    return { secret: found.secret, failed: found.problem !== undefined };
  };
  /** A secret that must be present, long enough and not a placeholder. @param {string} name @param {boolean} required */
  const checked = (name, required) => {
    const { secret: value, failed } = secret(name);
    if (!value) {
      if (!failed) {
        const message = `${name} is not set (at least ${MIN_SECRET_BYTES} characters; or set ${name}_FILE)`;
        if (required) problems.push(message);
        else warnings.push(message);
      }
      return undefined;
    }
    // A weak value is a problem (so startup stops) but stays in the best-effort
    // configuration, which only a development server or a test ever uses.
    const weak = weakSecretReason(value.expose());
    if (weak) problems.push(`${name} ${weak}`);
    return value;
  };
  const session = checked('SESSION_SECRET', true);
  const webServiceKey = checked('WEB_SERVICE_KEY', self);
  if (session && webServiceKey && session.expose() === webServiceKey.expose()) {
    const message =
      'SESSION_SECRET and WEB_SERVICE_KEY have the same value; generate each secret independently';
    (self ? problems : warnings).push(message);
  }
  const resend = secret('RESEND_API_KEY').secret;
  const googleSecret = secret('GOOGLE_CLIENT_SECRET').secret;
  const converterKey = secret('CONVERTER_KEY').secret;
  const emailFrom = get('EMAIL_FROM');
  const googleClientId = get('GOOGLE_CLIENT_ID');
  const converterUrl = get('CONVERTER_URL');
  const pair = (/** @type {unknown} */ a, /** @type {unknown} */ b, /** @type {string} */ what) => {
    if (Boolean(a) !== Boolean(b))
      warnings.push(`${what} need both settings; only one is set, so the feature stays off`);
  };
  pair(resend, emailFrom, 'RESEND_API_KEY and EMAIL_FROM');
  pair(googleSecret, googleClientId, 'GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET');
  pair(converterKey, converterUrl, 'CONVERTER_URL and CONVERTER_KEY');

  // --- Origins ---
  const rawOrigin = get('ORIGIN');
  /** @type {string | undefined} */ let origin;
  if (rawOrigin === undefined) {
    if (self)
      problems.push(
        'ORIGIN is not set (the public origin people use, such as https://dynodoc.example.org)'
      );
  } else {
    const parsed = parseOrigin(rawOrigin.trim());
    if ('origin' in parsed) origin = parsed.origin;
    else (self ? problems : warnings).push(`ORIGIN ${parsed.reason}`);
  }
  /** @type {string[]} */ const origins = origin ? [origin] : [];
  const rawAlt = get('DYNODOC_ALT_ORIGINS');
  if (self && rawAlt !== undefined) {
    const items = rawAlt
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean);
    if (items.length > ALT_ORIGINS_MAX)
      problems.push(`DYNODOC_ALT_ORIGINS lists more than ${ALT_ORIGINS_MAX} origins`);
    for (const item of items.slice(0, ALT_ORIGINS_MAX)) {
      const parsed = parseOrigin(item);
      if ('origin' in parsed) {
        if (!origins.includes(parsed.origin)) origins.push(parsed.origin);
      } else problems.push(`DYNODOC_ALT_ORIGINS entry "${shown(item)}" ${parsed.reason}`);
    }
  }
  if (self) {
    for (const item of origins)
      if (item.startsWith('http:') && !isLoopback(item))
        warnings.push(
          `${item} uses plain HTTP: sign-in cookies are not marked Secure and traffic is readable on the network; terminate TLS in front of Dynodoc`
        );
  }

  const rawEngine = get('ENGINE_API_URL');
  let engineApiUrl = rawEngine ?? 'http://127.0.0.1:8080';
  if (rawEngine !== undefined) {
    try {
      const url = new URL(rawEngine);
      if (url.protocol !== 'https:' && url.protocol !== 'http:')
        problems.push('ENGINE_API_URL must start with http:// or https://');
      else if (url.username || url.password || url.search || url.hash)
        problems.push('ENGINE_API_URL must not contain a user name, password, query or fragment');
      else if (rawEngine.endsWith('/'))
        (self ? problems : warnings).push('ENGINE_API_URL must not end with a slash');
    } catch {
      problems.push('ENGINE_API_URL is not a URL');
      engineApiUrl = 'http://127.0.0.1:8080';
    }
  }

  const rawWorkspace = get('PUBLIC_WORKSPACE_URL');
  /** @type {string | undefined} */ let workspaceUrl;
  if (rawWorkspace !== undefined) {
    const trimmed = rawWorkspace.replace(/\/$/, '');
    if (URL.canParse(trimmed)) {
      workspaceUrl = trimmed;
      if (self)
        warnings.push(
          'PUBLIC_WORKSPACE_URL redirects workspace pages to another host; leave it unset for a single-host installation'
        );
    } else warnings.push('PUBLIC_WORKSPACE_URL is not a URL; no redirect is applied');
  }

  // --- Flags that were always compared with the exact text `true` ---
  for (const name of ['ADMIN_SITE', 'DEV_AUTH']) {
    const value = lookup(name);
    if (value !== undefined && !['', 'true', 'false'].includes(value))
      warnings.push(`${name} should be true or false; "${shown(value)}" counts as false`);
  }
  const adminSite = lookup('ADMIN_SITE') === 'true';
  const devAuth = lookup('DEV_AUTH') === 'true';
  if (self && devAuth)
    warnings.push(
      'DEV_AUTH=true returns sign-in tokens in HTTP responses; use it only on a disposable test stack'
    );

  /** @param {string} name @param {boolean} fallback */
  const onOff = (name, fallback) => {
    const raw = get(name);
    if (raw === undefined) return fallback;
    const value = raw.trim().toLowerCase();
    if (value === 'on') return true;
    if (value === 'off') return false;
    problems.push(`${name} must be on or off (got "${shown(raw)}")`);
    return fallback;
  };
  const telemetryOn = onOff('DYNODOC_TELEMETRY', !self);
  const emailLink = onOff('AUTH_EMAIL_LINK', true);

  // --- Brand and organisation notice (self-hosted) ---
  let brand = 'Dynodoc';
  let organisation = '';
  let noticeText = '';
  let supportUrl = '';
  const rawBrand = get('DYNODOC_BRAND_NAME');
  const rawOrg = get('DYNODOC_ORG_NAME');
  const rawNotice = lookup('DYNODOC_ORG_NOTICE');
  const noticeFile = get('DYNODOC_NOTICE_FILE');
  const rawSupport = get('DYNODOC_SUPPORT_URL');
  if (!self) {
    for (const name of SELF_HOSTED_ONLY)
      if (get(name) !== undefined)
        warnings.push(`${name} is only used when DYNODOC_MODE=selfhosted; it is ignored`);
  } else {
    if (rawBrand !== undefined) {
      const value = rawBrand.trim();
      if (value.length > BRAND_MAX || /[\r\n]/.test(value) || plainTextReason(value))
        problems.push(
          `DYNODOC_BRAND_NAME must be one line of plain text, at most ${BRAND_MAX} characters`
        );
      else brand = value;
    }
    if (rawOrg !== undefined) {
      const value = rawOrg.trim();
      if (value.length > ORG_NAME_MAX || /[\r\n]/.test(value) || plainTextReason(value))
        problems.push(
          `DYNODOC_ORG_NAME must be one line of plain text, at most ${ORG_NAME_MAX} characters`
        );
      else organisation = value;
    }
    /** @type {string | undefined} */ let notice =
      rawNotice !== undefined && rawNotice.trim() !== '' ? rawNotice : undefined;
    if (notice !== undefined && noticeFile !== undefined)
      problems.push('DYNODOC_ORG_NOTICE and DYNODOC_NOTICE_FILE are both set; set exactly one');
    else if (noticeFile !== undefined) {
      const file = readBoundedFile('DYNODOC_NOTICE_FILE', noticeFile, {
        maxBytes: NOTICE_FILE_MAX_BYTES,
        ownerOnly: false,
        allowOpen: true
      });
      if ('problem' in file) problems.push(file.problem);
      else notice = file.text;
    }
    if (notice !== undefined) {
      const value = notice.replace(/\r\n?/g, '\n').replace(/\s+$/, '').replace(/^\n+/, '');
      if (value.length > NOTICE_MAX_CHARS || value.split('\n').length > NOTICE_MAX_LINES)
        problems.push(
          `the organisation notice is longer than ${NOTICE_MAX_CHARS} characters or ${NOTICE_MAX_LINES} lines`
        );
      else if (plainTextReason(value))
        problems.push('the organisation notice contains control or direction-override characters');
      else noticeText = value;
    }
    if (rawSupport !== undefined) {
      const value = rawSupport.trim();
      try {
        const url = new URL(value);
        if (
          value.length > SUPPORT_URL_MAX ||
          !['https:', 'http:', 'mailto:'].includes(url.protocol) ||
          url.username ||
          url.password
        )
          throw new Error('rejected');
        supportUrl = url.href;
      } catch {
        problems.push(
          `DYNODOC_SUPPORT_URL must be an https:, http: or mailto: link of at most ${SUPPORT_URL_MAX} characters, without credentials`
        );
      }
    }
  }

  // --- The reader's allow-file switch and typos ---
  const open = lookup(ALLOW_OPEN_FILES);
  if (open !== undefined && open !== '' && open !== 'true' && open !== 'false')
    warnings.push(`${ALLOW_OPEN_FILES} should be true or false; "${shown(open)}" counts as false`);
  for (const name of Object.keys(env).filter((n) => n.startsWith('DYNODOC_'))) {
    const near = likelyTypoOf(name);
    if (near.length)
      warnings.push(`${name} is not a known setting; did you mean ${near.join(' or ')}?`);
  }

  // --- Derived views ---
  const organisationNotice =
    organisation || noticeText || supportUrl
      ? { organisation, text: noticeText, supportUrl }
      : null;
  const telemetryOrigins = /** @type {[string, string][]} */ (
    self
      ? origins.map((item) => [item, adminSite ? 'admin' : 'workspace'])
      : [...HOSTED.telemetry.origins]
  );
  /** @type {PublicSite} */
  const site = {
    mode,
    brand,
    notice: self ? organisationNotice : null,
    indexable: !self,
    homeUrl: self ? '/' : HOSTED.homeUrl,
    helpUrl: self ? supportUrl || null : HOSTED.helpUrl,
    workspaceUrl: self ? '' : HOSTED.workspaceUrl,
    themeCookieDomain: self ? null : HOSTED.themeCookieDomain,
    telemetry: !telemetryOn
      ? null
      : self
        ? { endpoint: '/telemetry', hosts: origins.map((item) => new URL(item).hostname) }
        : { endpoint: HOSTED.telemetry.endpoint, hosts: [...HOSTED.telemetry.hosts] },
    emailSignIn: emailLink
  };
  /** @type {WebConfig} */
  const config = {
    mode,
    origin,
    origins,
    engineApiUrl,
    adminSite,
    devAuth,
    emailSignIn: emailLink,
    marketing: !self,
    indexable: !self,
    workspaceUrl,
    rootRedirect: self ? null : { ...HOSTED.rootRedirect },
    hostedDomain: self ? null : HOSTED.domain,
    connectSrc: self ? [...origins] : [...HOSTED.connectSrc],
    connectorOrigins: lookup('CONNECTOR_ORIGINS') ?? '',
    telemetry: { enabled: telemetryOn, origins: telemetryOrigins },
    emailFrom,
    googleClientId,
    converterUrl,
    opsStatusPath: get('OPS_STATUS_PATH'),
    site,
    secrets: {
      session,
      webServiceKey,
      resendApiKey: resend,
      googleClientSecret: googleSecret,
      converterKey
    }
  };
  return { ok: problems.length === 0, problems, warnings, config: deepFreeze(config) };
}

/** @template T @param {T} value @returns {Readonly<T>} */
function deepFreeze(value) {
  if (value && typeof value === 'object' && !(value instanceof Secret) && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const inner of Object.values(value)) deepFreeze(inner);
  }
  return value;
}

/** One message for a failed load, in the API's format. @param {string[]} problems */
export function formatProblems(problems) {
  return `invalid configuration (${problems.length} problem${problems.length === 1 ? '' : 's'}):\n  - ${problems.join('\n  - ')}`;
}
