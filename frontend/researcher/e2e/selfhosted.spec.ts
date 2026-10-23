import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

// Runs against a server started with DYNODOC_MODE=selfhosted and the settings in
// e2e/selfhosted.env (playwright.config.ts starts one, or ci-frontend.yml does). The tests that
// sign in need the isolated stack with the API and DEV_AUTH=true, like the hosted ones.
const settings = Object.fromEntries(
  readFileSync(new URL('./selfhosted.env', import.meta.url), 'utf8')
    .split(/\r?\n/)
    .filter((line) => line.trim() && !line.startsWith('#'))
    .map((line) => [line.slice(0, line.indexOf('=')), line.slice(line.indexOf('=') + 1)])
);
const notice = readFileSync(new URL('./fixtures/selfhosted-notice.txt', import.meta.url), 'utf8')
  .trim()
  .split('\n');
const signedIn = process.env.PILOT_E2E === 'true';

const MARKETING = [
  '/privacy',
  '/terms',
  '/cookies',
  '/dpa',
  '/subprocessors',
  '/acceptable-use',
  '/accessibility',
  '/licenses',
  '/security',
  '/support',
  '/help',
  '/features',
  '/why-dynodoc',
  '/add-ins',
  '/compatibility',
  '/roadmap',
  '/changelog',
  '/open-source',
  '/brand',
  '/sitemap.xml',
  '/llms.txt',
  '/llms-full.txt',
  '/source'
];

/** Every request the page makes, as it happens. */
function watch(page: Page) {
  const urls: string[] = [];
  page.on('request', (request) => urls.push(request.url()));
  return urls;
}
const outsiders = (urls: string[], origin: string) =>
  urls.filter((url) => /^https?:/.test(url) && new URL(url).origin !== origin);

test('the marketing pages and search-engine files do not exist: 404, not a redirect', async ({
  request,
  page
}) => {
  for (const path of MARKETING) {
    const response = await request.get(path, { maxRedirects: 0 });
    expect(response.status(), path).toBe(404);
    expect(response.headers().location, path).toBeUndefined();
  }
  const visit = await page.goto('/privacy');
  expect(visit?.status()).toBe(404);
  expect(new URL(page.url()).pathname).toBe('/privacy');
  // The data request a client-side navigation makes is held back as well.
  expect((await request.get('/features/__data.json', { maxRedirects: 0 })).status()).toBe(404);
});

test('/ leads to sign-in, once, with no loop', async ({ request, page }) => {
  const root = await request.get('/', { maxRedirects: 0 });
  expect(root.status()).toBe(303);
  expect(root.headers().location).toBe('/login');
  const navigations: string[] = [];
  page.on('framenavigated', (frame) => {
    if (frame === page.mainFrame()) navigations.push(new URL(frame.url()).pathname);
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Welcome to your workspace/ })).toBeVisible();
  expect(new URL(page.url()).pathname).toBe('/login');
  expect(navigations.length).toBeLessThanOrEqual(2);
  const login = await request.get('/login', { maxRedirects: 0 });
  expect(login.status()).toBe(200);
});

test('nothing is offered to search engines', async ({ request, page }) => {
  for (const path of ['/login', '/robots.txt', '/healthz', '/privacy']) {
    const response = await request.get(path, { maxRedirects: 0 });
    expect(response.headers()['x-robots-tag'], path).toBe('noindex, nofollow');
  }
  const robots = await (await request.get('/robots.txt')).text();
  expect(robots).toBe('User-agent: *\nDisallow: /\n');
  expect(robots).not.toMatch(/sitemap/i);
  await page.goto('/login');
  await expect(page.locator('meta[name="robots"]').first()).toHaveAttribute('content', /noindex/);
  await expect(page.locator('link[rel="canonical"]')).toHaveCount(0);
  await expect(page.locator('meta[property^="og:"]')).toHaveCount(0);
  await expect(page.locator('script[type="application/ld+json"]')).toHaveCount(0);
});

test('the content security policy names this installation and no hosted host', async ({
  request,
  baseURL
}) => {
  const response = await request.get('/login');
  const policy = response.headers()['content-security-policy'];
  expect(policy).toContain(`connect-src 'self' ${new URL(baseURL!).origin}`);
  expect(policy).not.toMatch(/dynodoc\.online/);
  expect(policy).not.toContain('appsforoffice');
  expect(response.headers()['x-frame-options']).toBe('SAMEORIGIN');
});

test('the sign-in page asks nothing of anyone else: no page metrics, no other origin', async ({
  page,
  baseURL
}) => {
  const urls = watch(page);
  await page.goto('/login');
  await expect(page.getByRole('heading', { name: /Welcome to your workspace/ })).toBeVisible();
  // Page metrics start after load and web-vitals reports on hide; give both time to happen.
  await page.waitForTimeout(1500);
  await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await page.goto('/login?again=1');
  await page.waitForTimeout(500);
  expect(urls.filter((url) => new URL(url).pathname === '/telemetry')).toEqual([]);
  expect(outsiders(urls, new URL(baseURL!).origin)).toEqual([]);
  expect(await page.content()).not.toMatch(/dynodoc\.online/);
});

test('the metrics route does nothing, for any origin', async ({ request, baseURL }) => {
  for (const origin of [new URL(baseURL!).origin, 'https://dynodoc.online']) {
    const preflight = await request.fetch('/telemetry', {
      method: 'OPTIONS',
      headers: { origin, 'access-control-request-method': 'POST' }
    });
    expect(preflight.status(), origin).toBe(404);
    const report = await request.post('/telemetry', {
      headers: { origin, 'content-type': 'application/json' },
      data: {
        page_id: 'a21f5eb4-28cf-4f78-a2c0-528cd74c8e17',
        metric: 'LCP',
        page: 'login',
        value: 1
      }
    });
    expect(report.status(), origin).toBe(404);
  }
});

test('the sign-in page carries the organisation’s name, notice and support link', async ({
  page
}) => {
  await page.goto('/login');
  await expect(page).toHaveTitle(new RegExp(`Sign in · ${settings.DYNODOC_BRAND_NAME}`));
  const aside = page.getByRole('complementary', { name: 'About this installation' });
  await expect(aside.getByText(settings.DYNODOC_ORG_NAME)).toBeVisible();
  await expect(aside).toContainText(notice[0]);
  await expect(aside).toContainText(notice[1]);
  // Line breaks in the notice are kept; nothing in it is treated as markup.
  const text = aside.locator('.text');
  expect(await text.evaluate((el) => getComputedStyle(el).whiteSpace)).toBe('pre-line');
  await expect(aside.getByRole('link', { name: 'Contact or support' })).toHaveAttribute(
    'href',
    settings.DYNODOC_SUPPORT_URL
  );
  // Where the hosted service links its privacy policy and terms, there is nothing to follow.
  await expect(page.getByRole('link', { name: /Privacy policy|Terms/ })).toHaveCount(0);
  await expect(page.getByRole('link', { name: /Back to/ })).toHaveCount(0);
});

test('email sign-in is offered, and the page is honest about what is not set up', async ({
  page,
  request
}) => {
  await page.goto('/login');
  await expect(page.getByLabel('Work email')).toBeVisible();
  const options = await (await request.get('/auth/options')).json();
  // Google is not offered unless the operator configured it.
  if (!options.google) await expect(page.getByText('Continue with Google')).toHaveCount(0);
  if (!options.email && !options.development)
    await expect(page.getByText('Email sign-in is awaiting administrator setup.')).toBeVisible();
});

test('start-up stops with one message listing every problem, and prints no secret', async () => {
  test.skip(!existsSync('build/index.js'), 'Needs the production build.');
  const secret = 'change-me-to-a-long-random-value-please';
  const child = spawn(process.execPath, ['build/index.js'], {
    env: {
      PATH: process.env.PATH,
      PORT: '4199',
      HOST: '127.0.0.1',
      DYNODOC_MODE: 'selfhosted',
      SESSION_SECRET: secret,
      DYNODOC_TELEMTRY: 'on'
    }
  });
  let output = '';
  child.stdout.on('data', (data) => (output += data));
  child.stderr.on('data', (data) => (output += data));
  const code = await new Promise<number | null>((resolve) => {
    const timer = setTimeout(() => {
      child.kill();
      resolve(null);
    }, 30000);
    child.on('exit', (status) => {
      clearTimeout(timer);
      resolve(status);
    });
  });
  expect(code, output).not.toBeNull();
  expect(code).not.toBe(0);
  expect(output).toMatch(/invalid configuration \(4 problems\):/);
  for (const problem of [
    'SESSION_SECRET looks like a placeholder',
    'WEB_SERVICE_KEY is not set',
    'ORIGIN is not set',
    'email sign-in is on but no mail transport is configured'
  ])
    expect(output).toContain(problem);
  expect(output).toContain(
    'DYNODOC_TELEMTRY is not a known setting; did you mean DYNODOC_TELEMETRY?'
  );
  expect(output).not.toContain(secret);
  // It never listened.
  await expect(fetch('http://127.0.0.1:4199/healthz')).rejects.toThrow();
});

test.describe('signed in', () => {
  test.skip(!signedIn, 'Requires the isolated signed-in development stack.');
  async function login(page: Page) {
    await page.goto('/login');
    await page.getByLabel('Your name').fill('Self-hosted tester');
    await page.getByLabel('Work email').fill(`selfhosted-${Date.now()}@example.test`);
    await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
    await page.getByRole('button', { name: /Continue in local development/ }).click();
    await expect(page).toHaveURL(/\/projects$/);
  }

  test('signing in and visiting the workspace contact no other origin and send no metrics', async ({
    page,
    baseURL
  }) => {
    const urls = watch(page);
    await login(page);
    await expect(
      page.getByText(`${settings.DYNODOC_BRAND_NAME} · Version control for documents`)
    ).toBeVisible();
    await page.goto('/workspace');
    await expect(
      page.getByRole('link', { name: new RegExp(settings.DYNODOC_BRAND_NAME) })
    ).toBeVisible();
    await page.waitForTimeout(1500);
    await page.goto('/profile');
    await page.waitForTimeout(500);
    expect(outsiders(urls, new URL(baseURL!).origin)).toEqual([]);
    expect(urls.filter((url) => new URL(url).pathname === '/telemetry')).toEqual([]);
    expect(await page.content()).not.toMatch(/dynodoc\.online/);
  });

  test('the workspace footer shows the notice instead of the hosted legal links', async ({
    page
  }) => {
    await login(page);
    const footer = page.locator('footer.repo-footer');
    await expect(footer.getByRole('link', { name: /Privacy|Terms|Supported formats/ })).toHaveCount(
      0
    );
    await footer.getByText(settings.DYNODOC_ORG_NAME).click();
    await expect(footer).toContainText(notice[0]);
    await expect(footer.getByRole('link', { name: 'Contact or support' })).toHaveAttribute(
      'href',
      settings.DYNODOC_SUPPORT_URL
    );
  });

  test('with no mail transport the invitation link is shown to copy, and nothing is emailed', async ({
    page,
    baseURL
  }) => {
    await login(page);
    await page.getByRole('button', { name: 'New project', exact: true }).click();
    await page.getByLabel('Project name').fill(`No mail ${Date.now()}`);
    await page.getByRole('button', { name: 'Create project' }).click();
    await expect(page.getByRole('button', { name: 'Files', exact: true })).toBeVisible();
    const projectId = page.url().split('/').pop();
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    const guest = `nomail-guest-${Date.now()}@example.test`;
    await page.getByLabel('Email', { exact: true }).fill(guest);
    await page.getByRole('button', { name: 'Add person' }).click();
    await expect(page.getByRole('status')).toContainText(
      `Access updated. Email is not set up on this server, so no invitation was sent. Send ${guest} this link yourself: ${new URL(baseURL!).origin}/projects/${projectId}`
    );
  });

  test('/ takes a signed-in person to their projects', async ({ page }) => {
    await login(page);
    await page.goto('/');
    await expect(page).toHaveURL(/\/projects$/);
  });
});

// The add-in files made for this installation (docs/SELF-HOSTED-EDITION.md section 7, P0-7).
// The server is reached on http://localhost, which add-in files accept only because this is a
// test stack (DEV_AUTH=true); a real installation must be on https.
const OFFICE_HOSTS = ['word', 'excel', 'powerpoint'];
const MANIFESTS = OFFICE_HOSTS.flatMap((host) => [`${host}.xml`, `${host}-catalog.xml`]);
const URLS_IN_XML = /https?:\/\/[^\s"<>]+/g;

test.describe('add-in files', () => {
  test('every manifest is public and names this installation and no other address', async ({
    request,
    baseURL
  }) => {
    const origin = new URL(baseURL!).origin;
    const ids = new Set<string>();
    for (const name of MANIFESTS) {
      const response = await request.get(`/connectors/addins/${name}`);
      expect(response.status(), name).toBe(200);
      expect(response.headers()['content-type'], name).toBe('application/xml; charset=utf-8');
      expect(response.headers()['content-disposition'], name).toBe(
        `attachment; filename="${name}"`
      );
      expect(response.headers()['cache-control'], name).toBe('private, no-store');
      expect(response.headers()['x-content-type-options'], name).toBe('nosniff');
      expect(response.headers()['x-robots-tag'], name).toBe('noindex, nofollow');
      const text = await response.text();
      // Every address in the file is on this installation's origin, apart from the XML
      // namespaces and the Office schema addresses that are identifiers, not links.
      const addresses = (text.match(URLS_IN_XML) ?? []).filter(
        (address) => !/schemas\.microsoft\.com|w3\.org/.test(address)
      );
      expect(addresses.length, name).toBeGreaterThanOrEqual(name.includes('catalog') ? 4 : 10);
      for (const address of addresses)
        expect(new URL(address).origin, `${name}: ${address}`).toBe(origin);
      expect(text, name).not.toMatch(/dynodoc\.online/i);
      expect(text).toContain(`DisplayName DefaultValue="${settings.DYNODOC_BRAND_NAME} for `);
      expect(text).toContain(`<ProviderName>${settings.DYNODOC_ORG_NAME}</ProviderName>`);
      ids.add(/<Id>([^<]+)<\/Id>/.exec(text)![1]);
      // The same file the second time: identifiers do not change between fetches.
      expect(await (await request.get(`/connectors/addins/${name}`)).text()).toBe(text);
    }
    expect(ids.size).toBe(MANIFESTS.length);
    const sums = await (await request.get('/connectors/addins/SHA256SUMS')).text();
    for (const name of MANIFESTS) expect(sums).toContain(`  ${name}\n`);
    // A test stack on http cannot have Google packages (Google calls https addresses only).
    expect((await request.get('/connectors/addins/google-docs.zip')).status()).toBe(404);
    expect((await request.get('/connectors/addins/nothing.xml')).status()).toBe(404);
  });

  test('the catalog-compatible manifests have no ribbon part and the others have it', async ({
    request
  }) => {
    for (const host of OFFICE_HOSTS) {
      const standard = await (await request.get(`/connectors/addins/${host}.xml`)).text();
      const catalog = await (await request.get(`/connectors/addins/${host}-catalog.xml`)).text();
      expect(standard).toContain('<VersionOverrides');
      expect(catalog).not.toContain('VersionOverrides');
      expect(catalog).toContain(`/connectors/${host}"`);
    }
  });

  test('the page that explains the files sends a visitor who is not signed in to sign in', async ({
    request
  }) => {
    const response = await request.get('/connectors/addins', { maxRedirects: 0 });
    expect(response.status()).toBe(303);
    expect(response.headers().location).toBe('/login?next=admin');
  });

  test('the public add-ins page links to the files made for this installation', async ({
    page
  }) => {
    await page.goto('/connectors');
    await expect(page.getByRole('link', { name: 'Download the Word add-in' })).toHaveAttribute(
      'href',
      '/connectors/addins/word.xml'
    );
    await expect(page.getByRole('link', { name: 'Download the Excel add-in' })).toHaveAttribute(
      'href',
      '/connectors/addins/excel.xml'
    );
    await expect(page.getByRole('link', { name: 'Download the Docs package' })).toHaveAttribute(
      'href',
      '/connectors/addins/google-docs.zip'
    );
  });

  test.describe('signed in', () => {
    test.skip(!signedIn, 'Requires the isolated signed-in development stack.');
    async function signInAs(page: Page, email: string) {
      await page.goto('/login');
      await page.getByLabel('Your name').fill('Add-in tester');
      await page.getByLabel('Work email').fill(email);
      await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
      await page.getByRole('button', { name: /Continue in local development/ }).click();
      await expect(page).toHaveURL(/\/projects$/);
    }

    test('a person who is not an administrator is refused the page, and the files stay public', async ({
      page,
      request
    }) => {
      await signInAs(page, `member-${Date.now()}@example.test`);
      const response = await page.goto('/connectors/addins');
      expect(response?.status()).toBe(403);
      await expect(page.getByText('Administrator access is required')).toBeVisible();
      await expect(page.getByRole('link', { name: 'word.xml' })).toHaveCount(0);
      expect((await request.get('/connectors/addins/word.xml')).status()).toBe(200);
    });

    test('an administrator gets the files, the install steps and what cannot work offline', async ({
      page,
      baseURL
    }) => {
      await signInAs(page, 'admin@example.test');
      const response = await page.goto('/connectors/addins');
      expect(response?.status()).toBe(200);
      expect(response?.headers()['cache-control']).toBe('private, no-store');
      await expect(
        page.getByRole('heading', { name: `Add-in files for ${settings.DYNODOC_BRAND_NAME}` })
      ).toBeVisible();
      await expect(page.locator('main').getByText(new URL(baseURL!).origin).first()).toBeVisible();
      // Not hidden, and in plain words: the add-ins have not been tried in an installed host.
      await expect(
        page.getByText('Not yet tried in an installed Office or Google editor.')
      ).toBeVisible();
      // What cannot work without internet access, stated plainly.
      const impossible = page.getByRole('heading', {
        name: 'What does not work without internet access, or on a private network'
      });
      await expect(impossible).toBeVisible();
      for (const text of [
        'The Google Docs, Sheets and Slides add-ons.',
        'Office on the web.',
        'Installing from Microsoft AppSource',
        /Office\.js from Microsoft.s servers\./
      ])
        await expect(page.getByText(text).first()).toBeVisible();
      await expect(page.getByRole('cell', { name: /SharePoint app catalog/ })).toBeVisible();
      await expect(page.getByText('No Google packages were made.')).toBeVisible();
      // With no folder set the page says where Office.js comes from.
      await expect(
        page.getByText('Office loads Office.js for each pane from Microsoft')
      ).toBeVisible();
      // The download links work, and give the file the public route gives.
      const link = page.getByRole('link', { name: 'word.xml', exact: true });
      const [download] = await Promise.all([page.waitForEvent('download'), link.click()]);
      expect(download.suggestedFilename()).toBe('word.xml');
      const saved = await download.path();
      const text = readFileSync(saved, 'utf8');
      expect(text).toContain(`<AppDomain>${new URL(baseURL!).origin}</AppDomain>`);
      expect(text).not.toMatch(/dynodoc\.online/i);
      // No accessibility violations on the page.
      const axe = await new AxeBuilder({ page })
        .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
        .analyze();
      expect(axe.violations).toEqual([]);
    });
  });
});
