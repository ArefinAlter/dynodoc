import { expect, test } from '@playwright/test';
test('homepage explains the workflow and responds to interaction', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Good research/ })).toBeVisible();
  await page.getByRole('tab', { name: /See what changed/ }).click();
  await expect(page.getByText('Small changes. Full context.')).toBeVisible();
});
test('login offers Google and email without exposing credentials', async ({ page }) => {
  await page.goto('/login');
  await expect(page.getByRole('heading', { name: 'Welcome to your workspace.' })).toBeVisible();
  await expect(page.getByLabel('Work email')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Email me a sign-in link' })).toBeVisible();
});
test('frontend healthz returns ok', async ({ request }) => {
  const res = await request.get('/healthz');
  expect(res.ok()).toBeTruthy();
  expect(await res.json()).toMatchObject({ status: 'ok', service: 'researcher-frontend' });
});
