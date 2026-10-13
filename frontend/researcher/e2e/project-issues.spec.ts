import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in stack.');
test.use({ actionTimeout: 15000 });

test('Issues tab opens an issue, takes a comment, closes it, and the project can be starred', async ({
  page
}) => {
  test.setTimeout(90000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Issues tester');
  await page.getByLabel('Work email').fill(`issues-${Date.now()}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByLabel('Project name').fill('Issue tracker');
  await page.getByRole('button', { name: 'Create project', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Issue tracker', exact: true })).toBeVisible();

  const tabs = page.getByRole('navigation', { name: 'Project sections' });
  const issues = tabs.getByRole('button', { name: 'Issues', exact: true });
  await expect(tabs.getByRole('button', { name: 'Pull requests', exact: true })).toBeVisible();
  await issues.click();
  await expect(issues).toHaveAttribute('aria-current', 'page');
  await expect(page).toHaveURL(/tab=issues/);
  await expect(page.getByRole('heading', { name: 'No issues yet' })).toBeVisible();

  // Open an issue and see it on the list.
  await page.getByRole('button', { name: 'New issue' }).first().click();
  await page.getByLabel('Title', { exact: true }).fill('Attach the signed Q3 budget');
  await page.getByLabel('Description (optional)').fill('The board update refers to it.');
  await page.getByRole('button', { name: 'Create issue', exact: true }).click();
  await expect(page.getByRole('heading', { name: /Attach the signed Q3 budget/ })).toBeVisible();
  await page.getByRole('button', { name: 'All issues' }).click();
  await expect(page.getByRole('link', { name: 'Attach the signed Q3 budget' })).toBeVisible();

  // Comment on it, then close it.
  await page.getByRole('link', { name: 'Attach the signed Q3 budget' }).click();
  await page.getByLabel('Add a comment').fill('Uploading it today.');
  await page.getByRole('button', { name: 'Comment', exact: true }).click();
  await expect(page.getByText('Uploading it today.')).toBeVisible();
  await page.getByRole('button', { name: 'Close issue', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Reopen issue', exact: true })).toBeVisible();

  // It moves from Open to Closed.
  await page.getByRole('button', { name: 'All issues' }).click();
  await expect(page.getByRole('heading', { name: 'No open issues' })).toBeVisible();
  await page.getByRole('button', { name: /^Closed/ }).click();
  await expect(page.getByRole('link', { name: 'Attach the signed Q3 budget' })).toBeVisible();
  expect((await new AxeBuilder({ page }).include('.repo').analyze()).violations).toEqual([]);

  // Star and unstar; the answer comes from the server and survives a reload.
  const star = page.getByRole('button', { name: 'Star', exact: true });
  await expect(star).toHaveAttribute('aria-pressed', 'false');
  await star.click();
  const starred = page.getByRole('button', { name: 'Starred', exact: true });
  await expect(starred).toHaveAttribute('aria-pressed', 'true');
  await page.reload();
  await expect(page.getByRole('button', { name: 'Starred', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Starred', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Star', exact: true })).toHaveAttribute(
    'aria-pressed',
    'false'
  );

  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(errors).toEqual([]);
});
