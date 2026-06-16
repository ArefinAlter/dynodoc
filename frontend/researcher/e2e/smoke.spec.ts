import { expect, test } from '@playwright/test';

test('home page renders the app shell', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'dynodoc' })).toBeVisible();
});

test('frontend healthz returns ok', async ({ request }) => {
  const res = await request.get('/healthz');
  expect(res.ok()).toBeTruthy();
  expect(await res.json()).toMatchObject({ status: 'ok', service: 'researcher-frontend' });
});
