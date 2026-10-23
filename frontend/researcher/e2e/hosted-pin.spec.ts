import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, test } from '@playwright/test';

// Pins what the hosted service sends, as it was before DYNODOC_MODE existed (captured from
// the unchanged build, with the build's nonces normalised). A change here is a change to the
// hosted deployment and needs a reason. Run against the local stack: the production hosts
// differ in what they index, which launch.spec.ts and seo.test.ts cover.
test.beforeEach(({ baseURL }) => {
  test.skip(/dynodoc\.online$/.test(new URL(baseURL!).hostname), 'Local stack only.');
});

const nonce = (value: string) => value.replace(/nonce-[A-Za-z0-9+/=_-]+/g, 'nonce-X');
const PAGE_POLICY =
  "default-src 'none'; frame-src 'self'; worker-src 'self' blob:; connect-src 'self' https://app.dynodoc.online; font-src 'self' data: blob:; img-src 'self' data: blob: https:; manifest-src 'self'; object-src 'none'; script-src 'self' 'nonce-X'; style-src 'self' 'unsafe-inline'; base-uri 'none'; form-action 'self'; frame-ancestors 'self'";
const FALLBACK_POLICY =
  "default-src 'none'; base-uri 'none'; object-src 'none'; form-action 'self'; frame-ancestors 'self'";

const PUBLIC_PAGES = [
  '/',
  '/compatibility',
  '/privacy',
  '/terms',
  '/features',
  '/why-dynodoc',
  '/roadmap',
  '/changelog',
  '/open-source',
  '/brand',
  '/help',
  '/add-ins',
  '/support',
  '/cookies',
  '/security',
  '/subprocessors',
  '/acceptable-use',
  '/accessibility',
  '/dpa',
  '/licenses'
];

test('every page carries the same policy and headers, with the app host in connect-src', async ({
  request
}) => {
  for (const path of ['/', '/privacy', '/login', '/nope']) {
    const response = await request.get(path);
    const headers = response.headers();
    expect(nonce(headers['content-security-policy']), path).toBe(PAGE_POLICY);
    expect(headers['x-frame-options'], path).toBe('SAMEORIGIN');
    expect(headers['x-content-type-options'], path).toBe('nosniff');
    expect(headers['referrer-policy'], path).toBe('strict-origin-when-cross-origin');
  }
  const api = await request.get('/healthz');
  expect(api.headers()['content-security-policy']).toBe(FALLBACK_POLICY);
  const pane = await request.get('/connectors/word');
  expect(pane.headers()['content-security-policy']).toContain(
    "connect-src 'self' https://app.dynodoc.online https://appsforoffice.microsoft.com"
  );
});

test('the public pages are all there, each with its canonical address and index tags', async ({
  request
}) => {
  expect(PUBLIC_PAGES).toHaveLength(20);
  for (const path of PUBLIC_PAGES) {
    const response = await request.get(path);
    expect(response.status(), path).toBe(200);
    const html = await response.text();
    expect(html, path).toContain(`<link rel="canonical" href="https://dynodoc.online${path}"`);
    expect(html, path).toContain(
      '<meta name="robots" content="index, follow, max-image-preview:large, max-snippet:-1, max-video-preview:-1"'
    );
    expect(html, path).toContain(
      '<meta property="og:image" content="https://dynodoc.online/brand/social.png"'
    );
    expect(html, path).toContain('href="https://dynodoc.online/llms.txt"');
  }
});

test('private pages are marked noindex and carry no canonical address', async ({ request }) => {
  for (const path of ['/login', '/workspace', '/projects']) {
    const response = await request.get(path, { maxRedirects: 5 });
    const html = await response.text();
    expect(html, path).toContain('<meta name="robots" content="noindex,nofollow"');
    expect(html, path).not.toContain('rel="canonical"');
  }
});

test('search-engine files and redirects behave as before on a host that is not the public one', async ({
  request
}) => {
  expect(await (await request.get('/robots.txt')).text()).toBe('User-agent: *\nDisallow: /\n');
  for (const path of ['/sitemap.xml', '/llms.txt', '/llms-full.txt'])
    expect((await request.get(path)).status(), path).toBe(404);
  const source = await request.get('/source', { maxRedirects: 0 });
  expect(source.status()).toBe(302);
  expect(source.headers().location).toBe('https://github.com/ArefinAlter/dynodoc-engine');
  for (const path of ['/login', '/healthz'])
    expect((await request.get(path)).headers()['x-robots-tag'], path).toBe('noindex, nofollow');
});

test('page metrics stay on: the four hosted origins may report, nobody else', async ({
  request
}) => {
  const own = [
    [
      'https://dynodoc.online',
      'https://www.dynodoc.online',
      'https://app.dynodoc.online',
      'https://admin.dynodoc.online'
    ]
  ].flat();
  for (const origin of own) {
    const response = await request.fetch('/telemetry', {
      method: 'OPTIONS',
      headers: { origin, 'access-control-request-method': 'POST' }
    });
    expect(response.status(), origin).toBe(204);
    expect(response.headers()['access-control-allow-origin'], origin).toBe(origin);
  }
  for (const origin of ['https://evil.example.test', 'http://dynodoc.online']) {
    const response = await request.fetch('/telemetry', {
      method: 'OPTIONS',
      headers: { origin, 'access-control-request-method': 'POST' }
    });
    expect(response.status(), origin).toBe(403);
  }
  const report = await request.post('/telemetry', {
    headers: { origin: 'https://evil.example.test', 'content-type': 'application/json' },
    data: {}
  });
  expect(report.status()).toBe(403);
});

test('the sign-in page still links the public site and its legal pages', async ({ page }) => {
  await page.goto('/login');
  await expect(page.getByRole('link', { name: /Back to Dynodoc/ })).toHaveAttribute(
    'href',
    'https://dynodoc.online'
  );
  await expect(page.getByRole('link', { name: 'Privacy policy' })).toHaveAttribute(
    'href',
    '/privacy'
  );
  await expect(page.getByRole('link', { name: /Terms/ })).toHaveAttribute('href', '/terms');
  await expect(page.getByText('Continue with Google')).toBeVisible();
  await expect(page.getByLabel('Work email')).toBeVisible();
  await expect(page).toHaveTitle('Sign in · Dynodoc');
});

test('the add-in downloads are the committed files byte for byte, and the per-installation routes do not exist', async ({
  request
}) => {
  const root = resolve(process.cwd(), '..', '..');
  for (const host of ['word', 'excel', 'powerpoint']) {
    const served = await request.get(`/connectors/packages/${host}.xml`);
    expect(served.status(), host).toBe(200);
    expect(
      Buffer.from(await served.body()).equals(
        readFileSync(resolve(root, 'extensions', host, 'manifest.xml'))
      )
    ).toBe(true);
  }
  for (const host of ['docs', 'sheets', 'slides']) {
    const served = await request.get(`/connectors/packages/google-${host}.zip`);
    expect(served.status(), host).toBe(200);
    expect(
      Buffer.from(await served.body()).equals(
        readFileSync(resolve(process.cwd(), 'static/connectors/packages', `google-${host}.zip`))
      )
    ).toBe(true);
  }
  for (const path of [
    '/connectors/addins',
    '/connectors/addins/word.xml',
    '/connectors/addins/word-catalog.xml',
    '/connectors/addins/SHA256SUMS',
    '/office-js/office.js'
  ])
    expect((await request.get(path, { maxRedirects: 0 })).status(), path).toBe(404);
  // The panes name Microsoft's address and carry no local-copy tag.
  const pane = await request.get('/connectors/word');
  expect(pane.headers()['content-security-policy']).toContain(
    'https://appsforoffice.microsoft.com'
  );
  expect(await pane.text()).not.toContain('dynodoc-office-js');
  // The public add-ins page links to the committed files, as it always did.
  const page = await (await request.get('/connectors')).text();
  expect(page).toContain('href="/connectors/packages/word.xml"');
  expect(page).toContain('href="/connectors/packages/google-docs.zip"');
  expect(page).not.toContain('/connectors/addins/');
});

test('the options carry the same four fields', async ({ request }) => {
  const options = await (await request.get('/auth/options')).json();
  expect(Object.keys(options).sort()).toEqual(['destination', 'development', 'email', 'google']);
  expect(options.destination).toBe('/projects');
});
