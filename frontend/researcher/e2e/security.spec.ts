import { expect, test } from '@playwright/test';

test('CSP blocks injected scripts and refreshes when crossing the Office pane boundary', async ({
  page
}) => {
  await page.route('https://appsforoffice.microsoft.com/**', (route) =>
    route.fulfill({ contentType: 'text/javascript', body: 'window.Office={onReady:()=>{}};' })
  );
  const first = await page.goto('/connectors');
  const csp = first!.headers()['content-security-policy'];
  expect(csp).toMatch(/script-src[^;]+(?:nonce-|sha256-)/);
  expect(csp).not.toMatch(/script-src[^;]+unsafe-(?:inline|eval)/);
  expect(csp).not.toContain('appsforoffice.microsoft.com');
  await page.evaluate(() => {
    const script = document.createElement('script');
    script.textContent = 'window.injectedScriptRan=true';
    document.head.append(script);
  });
  expect(await page.evaluate(() => 'injectedScriptRan' in window)).toBe(false);
  const paneResponse = page.waitForResponse(
    (r) => r.request().isNavigationRequest() && new URL(r.url()).pathname === '/connectors/word'
  );
  // Exercise Svelte navigation rather than a direct goto: permissions need a new document.
  await page.evaluate(() => {
    const link = document.createElement('a');
    link.href = '/connectors/word';
    link.textContent = 'Open Word pane';
    document.body.append(link);
    link.click();
  });
  const pane = await paneResponse;
  expect(pane.headers()['content-security-policy']).toContain(
    'https://appsforoffice.microsoft.com'
  );
  expect(pane.headers()['x-frame-options']).toBeUndefined();
  await page.waitForURL('**/connectors/word', { waitUntil: 'domcontentloaded' });
  await expect.poll(() => page.evaluate(() => 'Office' in window)).toBe(true);
  const signInResponse = page.waitForResponse(
    (r) => r.request().isNavigationRequest() && new URL(r.url()).pathname === '/login'
  );
  await page.evaluate(() => {
    const link = document.createElement('a');
    link.href = '/login';
    document.body.append(link);
    link.click();
  });
  const signIn = await signInResponse;
  expect(signIn.headers()['content-security-policy']).not.toContain('appsforoffice.microsoft.com');
  expect(signIn.headers()['x-frame-options']).toBe('SAMEORIGIN');
});

test('all pane responses keep scoped policies; redirects, errors and theme remain usable', async ({
  request,
  page
}) => {
  for (const host of ['word', 'excel', 'powerpoint']) {
    const response = await request.get(`/connectors/${host}`);
    expect(response.status()).toBe(200);
    expect(response.headers()['content-security-policy']).toContain('frame-ancestors');
    expect(response.headers()['content-security-policy']).toContain(
      'https://*.officeapps.live.com'
    );
    expect(response.headers()['content-security-policy']).not.toContain('unsafe-eval');
  }
  const redirect = await request.get('/projects', { maxRedirects: 0 });
  expect(redirect.status()).toBe(303);
  expect(redirect.headers()['content-security-policy']).toContain("frame-ancestors 'self'");
  const missing = await request.get('/not-a-real-dynodoc-route');
  expect(missing.status()).toBe(404);
  expect(missing.headers()['content-security-policy']).toContain("object-src 'none'");
  await page.goto('/login');
  await page.evaluate(() => localStorage.setItem('dynodoc.theme', 'dark'));
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('Office manifest command frame loads its SDK and initializer without inline permission', async ({
  page
}) => {
  await page.route('https://appsforoffice.microsoft.com/**', (route) =>
    route.fulfill({
      contentType: 'text/javascript',
      body: 'window.Office={onReady:()=>{window.commandsInitialized=true;}};'
    })
  );
  await page.goto('/connectors/commands.html');
  await expect.poll(() => page.evaluate(() => 'commandsInitialized' in window)).toBe(true);
  await page.evaluate(() => {
    const script = document.createElement('script');
    script.textContent = 'window.commandInjectionRan=true';
    document.head.append(script);
  });
  expect(await page.evaluate(() => 'commandInjectionRan' in window)).toBe(false);
});
