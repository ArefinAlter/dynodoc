import { expect, test } from '@playwright/test';

// These smoke tests exercise the static shell only — they do NOT require a live backend.
// The full E2E flows from the stage doc (login → open → edit → reload, and the
// two-user suggesting flow) need a running API + browsers and are deferred (SETUP.md).

test('home page renders the documents list shell', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Documents' })).toBeVisible();
  await expect(page.getByPlaceholder('New questionnaire title…')).toBeVisible();
});

test('login page renders the magic-link form', async ({ page }) => {
  await page.goto('/login');
  await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
});

test('frontend healthz returns ok', async ({ request }) => {
  const res = await request.get('/healthz');
  expect(res.ok()).toBeTruthy();
  expect(await res.json()).toMatchObject({ status: 'ok', service: 'researcher-frontend' });
});
