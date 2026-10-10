import { expect, test, type Page } from '@playwright/test';

test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in stack.');

async function login(page: Page, email: string) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Session tester');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
}

test('signing out revokes a copied cookie while a separate login still works', async ({
  page,
  browser
}) => {
  const email = `logout-${Date.now()}@example.test`;
  await login(page, email);
  const baseURL = new URL(page.url()).origin;
  const copied = await browser.newContext({
    baseURL,
    storageState: await page.context().storageState()
  });
  const separate = await browser.newContext({ baseURL });
  try {
    await login(await separate.newPage(), email);
    expect((await copied.request.get('/auth/session')).ok()).toBe(true);
    await page.getByRole('button', { name: 'Sign out', exact: true }).click();
    await expect(page).toHaveURL(/\/login$/);
    const rejected = await copied.request.get('/auth/session');
    expect(rejected.status()).toBe(401);
    expect((await rejected.json()).authenticated).toBe(false);
    const active = await separate.request.get('/auth/session');
    expect(active.ok()).toBe(true);
    expect((await active.json()).authenticated).toBe(true);
  } finally {
    await copied.close();
    await separate.close();
  }
});
