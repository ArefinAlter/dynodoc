import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in stack.');
test.use({ actionTimeout: 15000 });

test('Branches tab explains empty shared history and reads the history refs endpoint', async ({
  page
}) => {
  test.setTimeout(90000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/login');
  await page.getByLabel('Your name').fill('History tester');
  await page.getByLabel('Work email').fill(`history-${Date.now()}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByLabel('Project name').fill('History project');
  await page.getByRole('button', { name: 'Create project', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'History project', exact: true })).toBeVisible();
  const projectPath = new URL(page.url()).pathname;

  // From here on any console error counts, except a failed request that is not a history read
  // (the project page loads optional activity endpoints quietly).
  page.on('console', (message) => {
    if (message.type() !== 'error') return;
    const failedRequest = message.text().startsWith('Failed to load resource');
    if (failedRequest && !message.location().url.includes('/history/')) return;
    errors.push(message.text());
  });
  const refs = page.waitForResponse(
    (response) =>
      response.request().method() === 'GET' &&
      new URL(response.url()).pathname === `/api/engine${projectPath}/history/refs`
  );
  const tab = page
    .getByRole('navigation', { name: 'Project sections' })
    .getByRole('button', { name: 'Branches', exact: true });
  await tab.click();
  expect((await refs).status()).toBe(200);
  await expect(tab).toHaveAttribute('aria-current', 'page');
  await expect(page).toHaveURL(/tab=branches/);

  await expect(page.getByRole('heading', { name: 'No shared history yet' })).toBeVisible();
  await expect(page.locator('.repo-empty')).toContainText('engine-cli sync');
  await expect(page.getByRole('table')).toHaveCount(0);
  expect((await new AxeBuilder({ page }).include('.repo').analyze()).violations).toEqual([]);

  // Other tabs keep their names and the old "not available yet" sentence is gone.
  await page.getByRole('button', { name: 'Your drafts', exact: true }).click();
  await expect(page.getByText('not available yet')).toHaveCount(0);
  await expect(page.getByText('listed under the Branches tab')).toBeVisible();
  expect(errors).toEqual([]);
});
