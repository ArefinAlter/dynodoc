import { expect, test, type Page } from '@playwright/test';

// Runs against a self-hosted server started with DYNODOC_OFFICE_JS_DIR pointing at
// e2e/fixtures/office-js (playwright.config.ts starts one, or ci-frontend.yml does). The folder
// holds a stand-in file, because the real Office.js is not in this repository. The same pane on
// the ordinary self-hosted server (selfhosted.spec.ts) keeps using Microsoft's address.
const MICROSOFT = /appsforoffice\.microsoft\.com|onecdn\.static\.microsoft|ajax\.aspnetcdn\.com/;
const signedIn = process.env.PILOT_E2E === 'true';

test('the pane page names the local copy and its policy lists no Microsoft host', async ({
  request
}) => {
  for (const pane of ['word', 'excel', 'powerpoint']) {
    const response = await request.get(`/connectors/${pane}`);
    expect(response.status(), pane).toBe(200);
    const policy = response.headers()['content-security-policy'];
    expect(policy, pane).not.toMatch(MICROSOFT);
    expect(policy).toMatch(/script-src [^;]*'self'/);
    expect(policy).toContain('frame-ancestors');
    expect(policy).toContain('https://*.officeapps.live.com');
    expect(response.headers()['x-frame-options']).toBeUndefined();
    expect(await response.text()).toContain(
      '<meta name="dynodoc-office-js" content="/office-js/office.js" />'
    );
  }
  // A page that is not a pane is untouched.
  const login = await request.get('/login');
  expect(await login.text()).not.toContain('dynodoc-office-js');
  expect(login.headers()['content-security-policy']).not.toMatch(MICROSOFT);
});

test('the local copy and its sibling files are served, and nothing else is', async ({
  request
}) => {
  const script = await request.get('/office-js/office.js');
  expect(script.status()).toBe(200);
  expect(script.headers()['content-type']).toBe('text/javascript; charset=utf-8');
  expect(script.headers()['cache-control']).toBe('no-cache');
  expect(await script.text()).toContain('__dynodocOfficeJsStandIn');
  const etag = script.headers().etag;
  const again = await request.get('/office-js/office.js', { headers: { 'if-none-match': etag } });
  expect(again.status()).toBe(304);
  expect((await request.get('/office-js/en-us/office_strings.js')).status()).toBe(200);
  for (const path of [
    '/office-js/missing.js',
    '/office-js/.hidden.js',
    '/office-js/en-us',
    '/office-js/en-us/',
    '/office-js/..%2f..%2fselfhosted.env',
    '/office-js/%2e%2e/%2e%2e/package.json',
    '/office-js/office.js%00.png',
    '/office-js/office.js/..%5c..%5cpackage.json'
  ])
    expect((await request.get(path)).status(), path).toBe(404);
});

test('a Word pane loads the local file and not Microsoft’s', async ({ page }) => {
  const requests: string[] = [];
  page.on('request', (request) => requests.push(request.url()));
  await page.goto('/connectors/word');
  // The stand-in reports a host that is not Word, so the pane says so in plain words. The
  // message appears only if the pane loaded and ran the local file.
  await expect(
    page.getByText('Open this task pane in Word with WordApi 1.3 support.')
  ).toBeVisible();
  expect(requests.some((url) => new URL(url).pathname === '/office-js/office.js')).toBe(true);
  expect(requests.filter((url) => MICROSOFT.test(url))).toEqual([]);
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

  test('the administrators page says the installation serves Office.js itself', async ({
    page
  }) => {
    await signInAs(page, 'admin@example.test');
    await page.goto('/connectors/addins');
    await expect(
      page.getByText('This installation serves Office.js from its own server')
    ).toBeVisible();
    await expect(page.getByText('Office loads Office.js for each pane from Microsoft')).toHaveCount(
      0
    );
    // The command page still names Microsoft; the page says so rather than hide it.
    await expect(page.getByText(/still asks Microsoft.s address for/)).toBeVisible();
  });
});
