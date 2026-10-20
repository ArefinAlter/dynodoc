/// <reference types="@sveltejs/kit" />
/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />
// Offline page and a cache of content-hashed build files. The rules live in
// $lib/pwa/sw-policy so they can be tested; see docs/BROWSER-SUPPORT.md.
import { version } from '$service-worker';
import {
  OFFLINE_HEADERS,
  OFFLINE_URL,
  SHELL_FILES,
  assetCacheName,
  isSkipWaiting,
  mayStore,
  offlineFallbackApplies,
  planFor,
  shellCacheName,
  staleCaches
} from '$lib/pwa/sw-policy';

const worker = self as unknown as ServiceWorkerGlobalScope;

worker.addEventListener('install', (event) => {
  // No skipWaiting here: a new worker waits until the person accepts the update or
  // every open window has closed, so an editor is never swapped under unsaved work.
  event.waitUntil(
    caches.open(shellCacheName(version)).then((cache) =>
      // `reload` bypasses the HTTP cache so a new release never precaches an old file.
      cache.addAll(SHELL_FILES.map((file) => new Request(file, { cache: 'reload' })))
    )
  );
});

worker.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      for (const name of staleCaches(await caches.keys(), version)) await caches.delete(name);
      // Covers the page that registered the worker without a reload; it changes no content.
      await worker.clients.claim();
    })()
  );
});

worker.addEventListener('message', (event) => {
  if (isSkipWaiting(event.data)) void worker.skipWaiting();
});

async function offlinePage() {
  const cached = await (await caches.open(shellCacheName(version))).match(OFFLINE_URL);
  if (!cached) return Response.error();
  return new Response(await cached.text(), { status: 200, headers: OFFLINE_HEADERS });
}

async function navigate(request: Request) {
  try {
    // No navigation preload: a bypassed or retried page load must reach the server once,
    // because sign-in links are single use.
    return await fetch(request);
  } catch (error) {
    // Only a missing network gets the offline page. Anything the server answered,
    // including 4xx and 5xx, is passed through untouched.
    if (offlineFallbackApplies(error)) return offlinePage();
    throw error;
  }
}

async function fromCache(request: Request, name: string) {
  const cache = await caches.open(name);
  const hit = await cache.match(request);
  if (hit) return hit;
  const response = await fetch(request);
  if (mayStore(response)) void cache.put(request, response.clone());
  return response;
}

worker.addEventListener('fetch', (event) => {
  const plan = planFor(event.request, worker.location.origin);
  if (plan === 'navigate') event.respondWith(navigate(event.request));
  else if (plan === 'shell') event.respondWith(fromCache(event.request, shellCacheName(version)));
  else if (plan === 'static') event.respondWith(fromCache(event.request, assetCacheName(version)));
});
