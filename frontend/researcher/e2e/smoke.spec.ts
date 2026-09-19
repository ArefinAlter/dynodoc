import { expect, test } from '@playwright/test';

test('homepage examples demonstrate editing, formulas and selected-change sharing', async ({
  page
}) => {
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: 'Your research, in good company.' })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Try this wording' }).click();
  await expect(
    page.getByText(
      'Tell me about the last time you tried to get care. What was that experience like?'
    )
  ).toBeVisible();
  await page.getByRole('button', { name: 'Spreadsheets', exact: true }).click();
  await page.getByLabel('North district interviews').fill('40');
  await expect(page.getByRole('cell', { name: '93', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Questionnaires', exact: true }).click();
  await page.getByLabel('A group conversation', { exact: true }).check();
  await expect(page.getByLabel('A group conversation', { exact: true })).toBeChecked();
  await page.getByRole('button', { name: /Sam’s draft/ }).click();
  await expect(
    page.getByText('What does getting the care you need look like for you?', { exact: true })
  ).toBeVisible();
  await page.getByRole('checkbox', { name: /Question wording/ }).uncheck();
  await expect(page.getByRole('button', { name: 'Share selected changes' })).toBeDisabled();
  await page.getByRole('checkbox', { name: /Fieldwork note/ }).check();
  await page.getByRole('button', { name: 'Share selected changes' }).click();
  await expect(
    page.getByText(/The fieldwork note is shared. The question stays in your draft./)
  ).toBeVisible();
  await page.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(page.getByRole('checkbox', { name: /Question wording/ })).toBeChecked();
});

test('research workflow and integration explanations reflect the selected tool', async ({
  page
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Prepare the data', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Keep the reasoning close to the dataset.' })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Explore KoboToolbox integration' }).click();
  await expect(
    page.getByRole('heading', { name: 'Give your Kobo project a shared working space.' })
  ).toBeVisible();
  await expect(
    page.getByText(/Repeat imports create new snapshots, without automatic synchronization./)
  ).toBeVisible();
  await page.getByRole('button', { name: 'Explore REDCap integration' }).click();
  await expect(page.getByText(/Dynodoc does not connect to the REDCap API/)).toBeVisible();
});

test('legal pages are linked, readable on mobile and explicitly marked as drafts', async ({
  page
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  for (const [path, title] of [
    ['/privacy', 'Privacy policy'],
    ['/terms', 'Terms & conditions']
  ]) {
    await page.goto(path);
    await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
    await expect(
      page.getByText('Draft prepared 19 September 2026 · Not yet effective')
    ).toBeVisible();
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
  await page
    .getByRole('navigation', { name: 'Main navigation' })
    .getByRole('link', { name: 'Integrations' })
    .click();
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
