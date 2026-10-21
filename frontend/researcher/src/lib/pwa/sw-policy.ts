/**
 * What the service worker may touch, as plain functions so vitest can check them
 * without a worker. The worker has two jobs only: show a small offline page when a
 * page load fails for lack of a network, and keep content-hashed build files so
 * repeat visits start faster. It never stores a page, an API reply, a sign-in
 * response or anything else a session produced, and it never answers a request it
 * does not recognise: the browser handles those itself, so cookies, redirects,
 * uploads and event streams behave exactly as they do without it.
 */
export const OFFLINE_URL = '/offline.html';
/** Files fetched at install: the offline page and the icons it can show. */
export const SHELL_FILES = [OFFLINE_URL, '/icon-192.png', '/favicon.svg'];

const PREFIX = 'dynodoc-';
export const shellCacheName = (version: string) => `${PREFIX}shell-${version}`;
export const assetCacheName = (version: string) => `${PREFIX}assets-${version}`;
/** Caches left by other versions of this worker, to delete on activate. Other sites' caches stay. */
export function staleCaches(names: string[], version: string): string[] {
  const keep = new Set([shellCacheName(version), assetCacheName(version)]);
  return names.filter((name) => name.startsWith(PREFIX) && !keep.has(name));
}

/**
 * Signed-in areas, API and proxy routes, sign-in, shared links, the Office panes and the
 * telemetry beacon. None of these is ever answered or stored by the worker.
 */
const NEVER_INTERCEPT = [
  '/api',
  '/auth',
  '/admin',
  '/connectors',
  '/integrations',
  '/view',
  '/print',
  '/telemetry',
  '/healthz',
  '/source',
  '/_app/version.json'
];
/** A page or request in one of those areas. The page registers no worker from there either. */
export function isPrivateArea(path: string): boolean {
  return NEVER_INTERCEPT.some((root) => path === root || path.startsWith(`${root}/`));
}
/** Content-hashed build output and the self-hosted fonts: public, identical for everyone. */
const STATIC_PREFIXES = ['/_app/immutable/', '/fonts/'];

export type Plan = 'bypass' | 'navigate' | 'shell' | 'static';

export interface RequestLike {
  method: string;
  url: string;
  /** `navigate` for a page load. */
  mode?: string;
  /** `document` for a top-level page load. */
  destination?: string;
  headers: { get(name: string): string | null };
}

/**
 * `bypass`: do not call respondWith, the browser handles it. `navigate`: a page load,
 * network first, offline page if the network fails, the reply is never stored.
 * `shell`: a file fetched at install. `static`: cache first, filled as files are used.
 */
export function planFor(request: RequestLike, origin: string): Plan {
  if (request.method !== 'GET') return 'bypass';
  let url: URL;
  try {
    url = new URL(request.url);
  } catch {
    return 'bypass';
  }
  if (url.origin !== origin) return 'bypass';
  const header = (name: string) => request.headers.get(name) || '';
  if (header('range') || header('authorization')) return 'bypass';
  if (header('accept').includes('text/event-stream')) return 'bypass';
  const path = url.pathname;
  if (isPrivateArea(path)) return 'bypass';
  if (request.mode === 'navigate')
    return request.destination === 'document' ? 'navigate' : 'bypass';
  if (SHELL_FILES.includes(path)) return 'shell';
  if (STATIC_PREFIXES.some((prefix) => path.startsWith(prefix))) return 'static';
  return 'bypass';
}

export interface ResponseLike {
  status: number;
  type: string;
  headers: { get(name: string): string | null };
}

/**
 * Whether a fetched file may be stored. Only a complete, same-origin, public, non-HTML
 * success. Browsers hide Set-Cookie from scripts, so the header check is a backstop; the
 * routing above is what keeps session responses out.
 */
export function mayStore(response: ResponseLike): boolean {
  if (response.status !== 200 || response.type !== 'basic') return false;
  const header = (name: string) => (response.headers.get(name) || '').toLowerCase();
  if (response.headers.get('set-cookie')) return false;
  if (/(no-store|private)/.test(header('cache-control'))) return false;
  if (header('vary').includes('*')) return false;
  if (header('content-type').includes('text/html')) return false;
  return true;
}

/** Whether a failed page load looks like a missing network rather than a server answer. */
export function offlineFallbackApplies(error: unknown): boolean {
  return error instanceof TypeError || (error instanceof Error && error.name === 'TypeError');
}

/**
 * The offline page is returned with a policy of its own, so a proxy's default policy on
 * static files cannot strip its inline styles. It carries no script and loads nothing.
 */
export const OFFLINE_HEADERS: Record<string, string> = {
  'content-type': 'text/html; charset=utf-8',
  'cache-control': 'no-store',
  'x-content-type-options': 'nosniff',
  'content-security-policy':
    "default-src 'none'; style-src 'unsafe-inline'; img-src data:; form-action 'self'; base-uri 'none'; frame-ancestors 'self'"
};

/** The one message the page may send the worker: apply the waiting update now. */
export function isSkipWaiting(data: unknown): boolean {
  return (
    typeof data === 'object' &&
    data !== null &&
    (data as { type?: unknown }).type === 'SKIP_WAITING'
  );
}
