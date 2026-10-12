import { expect, test } from '@playwright/test';

test('homepage states the product, links the calls to action and carries one JSON-LD graph', async ({
  page
}) => {
  await page.goto('/');
  await expect(
    page.getByRole('heading', {
      level: 1,
      name: 'Version control for the documents you already write.'
    })
  ).toBeVisible();
  const main = page.getByRole('main');
  await expect(main.getByRole('link', { name: 'Open workspace' }).first()).toHaveAttribute(
    'href'
    //projects$/
  );
  await expect(main.getByRole('link', { name: 'Get the add-ins' }).first()).toHaveAttribute(
    'href',
    '/add-ins'
  );
  const more = page.getByRole('navigation', { name: 'More about Dynodoc' });
  for (const [name, href] of [
    ['Features', '/features'],
    ['Add-ins', '/add-ins'],
    ['Security', '/security'],
    ['Why provenance', '/why-dynodoc'],
    ['Roadmap', '/roadmap']
  ])
    await expect(more.getByRole('link', { name, exact: true })).toHaveAttribute('href', href);
  await expect(page.locator('script[type="application/ld+json"]')).toHaveCount(1);
  const graph = JSON.parse(
    (await page.locator('script[type="application/ld+json"]').textContent()) || '{}'
  )['@graph'];
  expect(graph.map((node: { '@type': string }) => node['@type'])).toEqual([
    'Organization',
    'WebSite',
    'SoftwareApplication',
    'FAQPage'
  ]);
  const text = (await main.textContent()) ?? '';
  expect(text).not.toMatch(/questionnaire|XLSForm|KoboToolbox|ODK|REDCap|survey/i);
  for (const section of ['how-it-works', 'editors', 'review', 'history', 'projects', 'faq'])
    await expect(page.locator(`#${section}`)).toHaveCount(1);
});

test('editor tabs switch the add-in preview and support arrow keys', async ({ page }) => {
  await page.goto('/');
  const tablist = page.getByRole('tablist', { name: 'Editor' });
  await tablist.scrollIntoViewIfNeeded();
  const panel = page.getByRole('tabpanel');
  await expect(tablist.getByRole('tab')).toHaveCount(6);
  await expect(tablist.getByRole('tab', { name: 'Word', exact: true })).toHaveAttribute(
    'aria-selected',
    'true'
  );
  await expect(panel.getByRole('img', { name: /pane open in Word,/ })).toBeVisible();
  await tablist.getByRole('tab', { name: 'Excel', exact: true }).click();
  await expect(tablist.getByRole('tab', { name: 'Excel', exact: true })).toHaveAttribute(
    'aria-selected',
    'true'
  );
  await expect(tablist.getByRole('tab', { name: 'Word', exact: true })).toHaveAttribute(
    'aria-selected',
    'false'
  );
  await expect(panel.getByRole('img', { name: /pane open in Excel,/ })).toBeVisible();
  await expect(panel.getByText('Existing cell values, formula sources')).toBeVisible();
  await page.keyboard.press('ArrowRight');
  await expect(tablist.getByRole('tab', { name: 'PowerPoint', exact: true })).toBeFocused();
  await expect(panel.getByRole('img', { name: /pane open in PowerPoint,/ })).toBeVisible();
  await page.keyboard.press('End');
  await expect(tablist.getByRole('tab', { name: 'Google Slides', exact: true })).toHaveAttribute(
    'aria-selected',
    'true'
  );
});

test('FAQ answers open and close, and the page does not overflow on a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  const faq = page.locator('#faq');
  const question = faq.getByText('What does it not do?', { exact: true });
  const answer = faq.getByText(/no live collaborator cursors/);
  await question.scrollIntoViewIfNeeded();
  await expect(answer).toBeHidden();
  await question.click();
  await expect(answer).toBeVisible();
  await question.click();
  await expect(answer).toBeHidden();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true
  );
});

test('legal pages are linked, readable on mobile and explicitly marked as drafts', async ({
  page
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  for (const [path, title] of [
    ['/privacy', 'Privacy policy'],
    ['/terms', 'Terms & conditions'],
    ['/cookies', 'Cookies & local storage'],
    ['/security', 'Security'],
    ['/subprocessors', 'Subprocessors'],
    ['/acceptable-use', 'Acceptable use'],
    ['/accessibility', 'Accessibility statement'],
    ['/dpa', 'Data processing terms (draft)'],
    ['/licenses', 'Licenses']
  ]) {
    await page.goto(path);
    await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
    await expect(page.getByText('Draft revised 12 October 2026 · Not yet effective')).toBeVisible();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
    ).toBe(true);
    const toc = page.getByRole('navigation', { name: 'On this page' });
    const anchors = await toc
      .getByRole('link')
      .evaluateAll((links) => links.map((link) => link.getAttribute('href')));
    for (const anchor of anchors) expect(await page.locator(anchor!).count()).toBe(1);
  }
  await page.goto('/');
  await expect(
    page.getByRole('contentinfo').getByRole('link', { name: 'Privacy policy' })
  ).toBeVisible();
  await expect(
    page.getByRole('contentinfo').getByRole('link', { name: 'Terms & conditions' })
  ).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true
  );
  await page.getByRole('button', { name: 'Open menu', exact: true }).click();
  const main = page.getByRole('navigation', { name: 'Main navigation' });
  await main.getByRole('button', { name: 'Product', exact: true }).click();
  await main.getByRole('link', { name: /^Features/ }).click();
  await expect(page).toHaveURL(/\/features$/);
  // The full-screen menu closes when a link is followed.
  await expect(page.getByRole('button', { name: 'Open menu', exact: true })).toBeVisible();
});

test('login offers Google and email and links to the legal pages', async ({ page }) => {
  await page.goto('/login');
  await expect(page.getByRole('heading', { name: 'Welcome to your workspace.' })).toBeVisible();
  await expect(page.getByLabel('Work email')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Email me a sign-in link' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Privacy policy' })).toHaveAttribute(
    'href',
    '/privacy'
  );
  await expect(page.getByRole('link', { name: 'Terms & conditions' })).toHaveAttribute(
    'href',
    '/terms'
  );
});

test('frontend healthz returns ok', async ({ request }) => {
  const res = await request.get('/healthz');
  expect(res.ok()).toBeTruthy();
  expect(await res.json()).toMatchObject({ status: 'ok', service: 'researcher-frontend' });
});

test('homepage tabs wait for hydration on a slow connection', async ({ page }) => {
  let resumeScripts!: () => void;
  const scriptsReady = new Promise<void>((resolve) => {
    resumeScripts = resolve;
  });
  // Hold hydration bundles, not the parser-blocking prepaint theme initializer.
  let heldScripts = 0;
  await page.route('**/_app/immutable/**/*.js', async (route) => {
    heldScripts += 1;
    await scriptsReady;
    await route.continue();
  });
  try {
    await page.goto('/', { waitUntil: 'domcontentloaded' });
    await expect.poll(() => heldScripts).toBeGreaterThan(0);
    // The server-rendered page is complete and readable; only the tabs wait.
    await expect(
      page.getByRole('heading', { level: 1, name: /Version control for the documents/ })
    ).toBeVisible();
    await expect(page.getByRole('tab', { name: 'Excel', exact: true })).toBeDisabled();
    resumeScripts();
    await expect(page.getByRole('tab', { name: 'Excel', exact: true })).toBeEnabled();
    await page.getByRole('tab', { name: 'Excel', exact: true }).click();
    await expect(page.getByRole('tab', { name: 'Excel', exact: true })).toHaveAttribute(
      'aria-selected',
      'true'
    );
  } finally {
    resumeScripts();
  }
});
