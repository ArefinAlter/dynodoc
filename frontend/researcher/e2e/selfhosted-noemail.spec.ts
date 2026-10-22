import { expect, test } from '@playwright/test';

// Runs against a self-hosted server started with AUTH_EMAIL_LINK=off (see selfhosted.spec.ts
// for the rest of its settings): email sign-in is not offered and the route refuses.
test('the sign-in page does not offer email, and says what is missing', async ({ page }) => {
  await page.goto('/login');
  await expect(page.getByRole('heading', { name: /Welcome to your workspace/ })).toBeVisible();
  await expect(page.getByLabel('Work email')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Email me a sign-in link' })).toHaveCount(0);
  await expect(page.getByText('or use your email')).toHaveCount(0);
  await expect(
    page.getByText('No sign-in method is turned on. Ask your administrator.')
  ).toBeVisible();
});

test('the options and the email route agree: off', async ({ request, baseURL }) => {
  const options = await (await request.get('/auth/options')).json();
  expect(options).toMatchObject({ email: false, development: false });
  const response = await request.post('/auth/email', {
    headers: { origin: new URL(baseURL!).origin, 'content-type': 'application/json' },
    data: { email: 'olivia@example.test' }
  });
  expect(response.status()).toBe(403);
  expect((await response.json()).error).toMatch(/turned off/);
});
