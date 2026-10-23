import AxeBuilder from '@axe-core/playwright';
import http from 'node:http';
import type { AddressInfo, Socket } from 'node:net';
import { expect, test, type Page } from '@playwright/test';
// The installable app: manifest, service worker, offline page and update flow. The signed-in
// checks need the isolated stack (DEV_AUTH); the rest run against any build.
const signedIn = process.env.PILOT_E2E === 'true';
// The WebKit projects block workers for the other specs (playwright.config.ts); these need one.
test.use({ serviceWorkers: 'allow' });
const AXE_TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'];

/** A pass-through proxy in front of the app, so a test can take the "server" away. */
async function startProxy(target: string) {
  const origin = new URL(target);
  const sockets = new Set<Socket>();
  const proxy = {
    mode: 'up' as 'up' | 'unavailable',
    port: 0,
    url: () => `http://127.0.0.1:${proxy.port}`
  };
  const server = http.createServer((req, res) => {
    if (proxy.mode === 'unavailable') {
      res.writeHead(503, { 'content-type': 'text/plain' });
      res.end('Service unavailable');
      return;
    }
    const upstream = http.request(
      {
        host: origin.hostname,
        port: origin.port,
        path: req.url,
        method: req.method,
        headers: { ...req.headers, host: origin.host }
      },
      (reply) => {
        res.writeHead(reply.statusCode || 502, reply.headers);
        reply.pipe(res);
      }
    );
    upstream.on('error', () => res.destroy());
    req.pipe(upstream);
  });
  server.on('connection', (socket) => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
  });
  const start = () =>
    new Promise<void>((resolve) =>
      server.listen(proxy.port, '127.0.0.1', () => {
        proxy.port = (server.address() as AddressInfo).port;
        resolve();
      })
    );
  const stop = () =>
    new Promise<void>((resolve) => {
      for (const socket of sockets) socket.destroy();
      server.close(() => resolve());
    });
  await start();
  return Object.assign(proxy, { start, stop });
}

async function signIn(page: Page) {
  await page.getByLabel('Your name').fill('Install check');
  await page
    .getByLabel('Work email')
    .fill(`pwa-${Date.now()}-${Math.random().toString(36).slice(2)}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
}
/** Open a public page and wait until the worker controls it. */
async function controlled(page: Page, path = '/login') {
  await page.goto(path);
  await page.evaluate(() => navigator.serviceWorker.ready);
  await expect
    .poll(() => page.evaluate(() => !!navigator.serviceWorker.controller), { timeout: 15000 })
    .toBe(true);
}
async function cachedPaths(page: Page) {
  return page.evaluate(async () => {
    const found: { cache: string; path: string; type: string }[] = [];
    for (const name of await caches.keys()) {
      const cache = await caches.open(name);
      for (const request of await cache.keys()) {
        const response = await cache.match(request);
        found.push({
          cache: name,
          path: new URL(request.url).pathname,
          type: response?.headers.get('content-type') || ''
        });
      }
    }
    return found;
  });
}
function pngSize(body: Buffer) {
  expect(body.subarray(1, 4).toString()).toBe('PNG');
  return { width: body.readUInt32BE(16), height: body.readUInt32BE(20) };
}

test('the web app manifest is complete and every icon exists at its declared size @smoke', async ({
  request
}) => {
  const response = await request.get('/site.webmanifest');
  expect(response.status()).toBe(200);
  expect(response.headers()['content-type']).toMatch(/json/);
  const manifest = await response.json();
  expect(manifest).toMatchObject({
    id: '/',
    name: 'Dynodoc',
    short_name: 'Dynodoc',
    scope: '/',
    display: 'standalone',
    theme_color: expect.stringMatching(/^#[0-9a-f]{6}$/i),
    background_color: expect.stringMatching(/^#[0-9a-f]{6}$/i)
  });
  expect(manifest.display_override).toContain('standalone');
  expect(manifest.categories.length).toBeGreaterThan(0);
  expect(manifest.description.length).toBeGreaterThan(20);
  // The start page is inside the scope, and is a signed-in route so a launch lands in the app.
  expect(new URL(manifest.start_url, 'http://x').pathname).toBe('/workspace');
  expect(new URL(manifest.start_url, 'http://x').pathname.startsWith(manifest.scope)).toBe(true);
  expect(manifest.shortcuts.map((s: { name: string }) => s.name)).toEqual([
    'Workspace',
    'Projects'
  ]);
  for (const shortcut of manifest.shortcuts) {
    expect(new URL(shortcut.url, 'http://x').pathname).toMatch(/^\/(workspace|projects)$/);
    for (const icon of shortcut.icons)
      expect((await request.get(icon.src)).status(), icon.src).toBe(200);
  }
  const purposes = manifest.icons.map(
    (i: { sizes: string; purpose: string }) => `${i.sizes} ${i.purpose}`
  );
  expect(purposes).toEqual(
    expect.arrayContaining(['192x192 any', '512x512 any', '512x512 maskable'])
  );
  for (const icon of manifest.icons) {
    const file = await request.get(icon.src);
    expect(file.status(), icon.src).toBe(200);
    expect(file.headers()['content-type']).toBe('image/png');
    const [width, height] = icon.sizes.split('x').map(Number);
    expect(pngSize(await file.body()), icon.src).toEqual({ width, height });
  }
  const apple = await request.get('/apple-touch-icon.png');
  expect(apple.status()).toBe(200);
  expect(pngSize(await apple.body())).toEqual({ width: 180, height: 180 });
});

test('pages link the manifest and carry the install and bar-colour tags @smoke', async ({
  page
}) => {
  // The page policy must let the browser read the manifest, or it can never offer to install.
  await page.addInitScript(() => {
    (window as unknown as { __csp: string[] }).__csp = [];
    document.addEventListener('securitypolicyviolation', (event) =>
      (window as unknown as { __csp: string[] }).__csp.push(
        `${event.violatedDirective} ${event.blockedURI}`
      )
    );
  });
  await page.goto('/login');
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/site.webmanifest');
  await expect(page.locator('meta[name="viewport"]')).toHaveAttribute(
    'content',
    /viewport-fit=cover/
  );
  await expect(page.locator('meta[name="apple-mobile-web-app-capable"]')).toHaveAttribute(
    'content',
    'yes'
  );
  await expect(page.locator('meta[name="mobile-web-app-capable"]')).toHaveAttribute(
    'content',
    'yes'
  );
  await expect(page.locator('meta[name="apple-mobile-web-app-status-bar-style"]')).toHaveCount(1);
  await expect(page.locator('link[rel="apple-touch-icon"]').first()).toHaveAttribute(
    'href',
    /\/apple-touch-icon\.png$/
  );
  const bars = await page
    .locator('meta[name="theme-color"]')
    .evaluateAll((metas) =>
      metas.map((m) => `${m.getAttribute('media')} ${m.getAttribute('content')}`)
    );
  expect(bars).toEqual([
    '(prefers-color-scheme: light) #f8f9fc',
    '(prefers-color-scheme: dark) #0e141b'
  ]);
  await page.waitForLoadState('networkidle');
  expect(await page.evaluate(() => (window as unknown as { __csp: string[] }).__csp)).toEqual([]);
});

const INSTALL_METAS = [
  'application-name',
  'apple-mobile-web-app-title',
  'apple-mobile-web-app-capable',
  'mobile-web-app-capable',
  'apple-mobile-web-app-status-bar-style'
];

test('the public marketing pages do not advertise the web app @smoke', async ({ page }) => {
  // The manifest starts at /workspace, which the marketing host sends to another host, so an
  // install offered from these pages could never start. Their icons stay.
  for (const path of ['/', '/features', '/compatibility', '/privacy', '/open-source']) {
    await page.goto(path);
    await expect(page.getByRole('heading', { level: 1 }).first(), path).toBeVisible();
    await expect(page.locator('link[rel="manifest"]'), path).toHaveCount(0);
    for (const name of INSTALL_METAS)
      await expect(page.locator(`meta[name="${name}"]`), `${path} ${name}`).toHaveCount(0);
    // The site sets one bar colour of its own; the app's pair (light and dark) is not there.
    await expect(page.locator('meta[name="theme-color"]'), path).toHaveCount(1);
    await expect(page.locator('link[rel="apple-touch-icon"]').first(), path).toHaveAttribute(
      'href',
      /\/apple-touch-icon\.png$/
    );
  }
});

test('the sign-in page and the signed-in app advertise the web app @smoke', async ({ page }) => {
  await page.goto('/login');
  await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  for (const name of INSTALL_METAS)
    await expect(page.locator(`meta[name="${name}"]`), name).toHaveCount(1);
  test.skip(!signedIn, 'The signed-in pages need the isolated signed-in development stack.');
  await signIn(page);
  for (const path of ['/projects', '/workspace']) {
    await page.goto(path);
    await expect(page.locator('link[rel="manifest"]'), path).toHaveAttribute(
      'href',
      '/site.webmanifest'
    );
    for (const name of INSTALL_METAS)
      await expect(page.locator(`meta[name="${name}"]`), `${path} ${name}`).toHaveCount(1);
  }
});

test('a service worker registers and controls the page @smoke', async ({ page }) => {
  await controlled(page);
  const registration = await page.evaluate(async () => {
    const r = await navigator.serviceWorker.getRegistration('/');
    return { scope: r?.scope, script: r?.active?.scriptURL, state: r?.active?.state };
  });
  expect(registration.scope).toMatch(/\/$/);
  expect(registration.script).toMatch(/\/service-worker\.js$/);
  expect(registration.state).toBe('activated');
  const stored = await cachedPaths(page);
  expect(stored.map((e) => e.path)).toContain('/offline.html');
});

test('a page load without a network shows the offline page, and it returns when the network does @smoke', async ({
  page,
  context,
  browserName
}) => {
  // Playwright's offline switch also stops the worker's own requests on Chromium only. The
  // test after this one takes the server away instead, which works on every engine.
  test.skip(browserName !== 'chromium', 'setOffline does not reach the worker on this engine.');
  await controlled(page);
  await context.setOffline(true);
  try {
    const response = await page.goto('/features');
    expect(response?.status()).toBe(200);
    await expect(page.getByRole('heading', { name: 'You are offline' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible();
  } finally {
    await context.setOffline(false);
  }
  await page.goto('/features');
  await expect(page.getByRole('heading', { name: 'You are offline' })).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Features', exact: true }).first()).toBeVisible();
});

test('when the server cannot be reached the offline page appears, and a reply from the server never is replaced @smoke', async ({
  page,
  baseURL
}) => {
  const proxy = await startProxy(baseURL!);
  try {
    await controlled(page, `${proxy.url()}/login`);
    // The server answers with an error: it is shown as sent, not hidden behind the offline page.
    proxy.mode = 'unavailable';
    const failing = await page.goto(`${proxy.url()}/features`);
    expect(failing?.status()).toBe(503);
    await expect(page.getByText('Service unavailable')).toBeVisible();
    await expect(page.getByRole('heading', { name: 'You are offline' })).toHaveCount(0);
    // The server cannot be reached at all.
    proxy.mode = 'up';
    await proxy.stop();
    const offline = await page.goto(`${proxy.url()}/features`);
    expect(offline?.status()).toBe(200);
    await expect(page.getByRole('heading', { name: 'You are offline' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible();
    const day = await new AxeBuilder({ page }).withTags(AXE_TAGS).analyze();
    expect(day.violations.map((v) => v.id)).toEqual([]);
    await page.emulateMedia({ colorScheme: 'dark' });
    const night = await new AxeBuilder({ page }).withTags(AXE_TAGS).analyze();
    expect(night.violations.map((v) => v.id)).toEqual([]);
    // It comes back by itself once the server does.
    await proxy.start();
    await page.goto(`${proxy.url()}/features`);
    await expect(page.getByRole('heading', { name: 'You are offline' })).toHaveCount(0);
    await expect(page.getByRole('link', { name: 'Features', exact: true }).first()).toBeVisible();
  } finally {
    await proxy.stop();
  }
});

test('the offline file itself passes accessibility checks in day and night mode @smoke', async ({
  page
}) => {
  for (const colorScheme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme });
    await page.goto('/offline.html');
    await expect(page.getByRole('heading', { name: 'You are offline' })).toBeVisible();
    const result = await new AxeBuilder({ page }).withTags(AXE_TAGS).analyze();
    expect(result.violations.map((v) => v.id)).toEqual([]);
  }
});

test('signing in with the worker active keeps the session cookie, and no signed-in page or API reply is cached @smoke', async ({
  page,
  context
}) => {
  test.skip(!signedIn, 'Requires the isolated signed-in development stack.');
  await controlled(page);
  await signIn(page);
  const cookies = await context.cookies();
  expect(cookies.find((c) => c.name === 'dynodoc_session')?.httpOnly).toBe(true);
  // Visit signed-in pages and make proxied API calls while the worker is in control.
  await page.goto('/workspace');
  await expect(page.getByRole('button', { name: 'New document', exact: true })).toBeVisible();
  expect(await page.evaluate(() => !!navigator.serviceWorker.controller)).toBe(true);
  const reply = await page.evaluate(async () => {
    const r = await fetch('/api/engine/v1/documents', {
      headers: { 'x-dynodoc-editor': 'office-v5' }
    });
    return { status: r.status, setCookie: r.headers.get('set-cookie') };
  });
  expect([200, 404]).toContain(reply.status);
  await page.goto('/projects');
  await page.goto('/profile');
  await expect(page.getByRole('heading', { name: 'Install the app' })).toBeVisible();
  const stored = await cachedPaths(page);
  for (const entry of stored) {
    expect(entry.path, entry.path).toMatch(
      /^\/(offline\.html|icon-192\.png|favicon\.svg|_app\/immutable\/.+|fonts\/.+)$/
    );
    if (entry.path !== '/offline.html') expect(entry.type, entry.path).not.toMatch(/html/);
  }
  const names = stored.map((e) => e.path);
  for (const forbidden of ['/workspace', '/projects', '/profile', '/login', '/api', '/auth'])
    expect(
      names.some((n) => n.startsWith(forbidden)),
      forbidden
    ).toBe(false);
  // The session still works after all of that.
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Install the app' })).toBeVisible();
});

test('the profile offers to install the app, with steps where the browser gives no prompt', async ({
  page
}) => {
  test.skip(!signedIn, 'Requires the isolated signed-in development stack.');
  await page.goto('/login');
  await signIn(page);
  await page.goto('/profile');
  const section = page.getByRole('region', { name: 'Install the app' });
  await expect(section).toBeVisible();
  await expect(section).toContainText(/Install|Add to|Dock|installed|cannot install|menu/);
  await expect(section).toContainText('nothing from your account is stored on the device');
  const result = await new AxeBuilder({ page })
    .include('#install-app')
    .withTags(AXE_TAGS)
    .analyze();
  expect(result.violations.map((v) => v.id)).toEqual([]);
});

test('an update waits until the person accepts it, and never reloads the page by itself @smoke', async ({
  page
}) => {
  await controlled(page, '/features');
  await page.evaluate(() => ((window as unknown as { marker: string }).marker = 'still here'));
  // A different script address counts as a new version of the worker for this scope.
  await page.evaluate(async () => {
    await navigator.serviceWorker.register('/service-worker.js?next-version', { scope: '/' });
  });
  const notice = page.getByRole('status').filter({ hasText: 'A new version of Dynodoc is ready' });
  await expect(notice).toBeVisible({ timeout: 15000 });
  // It is waiting behind the running worker; nothing has switched or reloaded.
  const state = await page.evaluate(async () => {
    const r = await navigator.serviceWorker.getRegistration('/');
    return { waiting: !!r?.waiting, active: r?.active?.scriptURL ?? '' };
  });
  expect(state.waiting).toBe(true);
  expect(state.active).toMatch(/service-worker\.js$/);
  await page.waitForTimeout(1500);
  expect(await page.evaluate(() => (window as unknown as { marker: string }).marker)).toBe(
    'still here'
  );
  const result = await new AxeBuilder({ page })
    .include('[role="status"]')
    .withTags(AXE_TAGS)
    .analyze();
  expect(result.violations.map((v) => v.id)).toEqual([]);
  // "Not now" hides it and still changes nothing.
  await notice.getByRole('button', { name: 'Not now' }).click();
  await expect(notice).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { marker: string }).marker)).toBe(
    'still here'
  );
});

test('accepting an update switches worker and reloads once, only on request @smoke', async ({
  page
}) => {
  await controlled(page, '/features');
  await page.evaluate(() => ((window as unknown as { marker: string }).marker = 'before'));
  await page.evaluate(async () => {
    await navigator.serviceWorker.register('/service-worker.js?accepted-version', { scope: '/' });
  });
  const notice = page.getByRole('status').filter({ hasText: 'A new version of Dynodoc is ready' });
  await expect(notice).toBeVisible({ timeout: 15000 });
  const reloaded = page.waitForEvent('load');
  await notice.getByRole('button', { name: 'Update now' }).click();
  await reloaded;
  await expect(page.getByRole('heading', { level: 1 }).first()).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { marker?: string }).marker)).toBe(
    undefined
  );
  await expect
    .poll(() =>
      page.evaluate(
        async () => (await navigator.serviceWorker.getRegistration('/'))?.active?.scriptURL
      )
    )
    .toMatch(/accepted-version$/);
});
