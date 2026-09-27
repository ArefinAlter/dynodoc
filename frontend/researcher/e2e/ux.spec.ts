import { expect, test, type Page } from '@playwright/test';
// Requires a local API and DEV_AUTH=true web server. Never enables a public login bypass.
test.skip(
  process.env.PILOT_E2E !== 'true',
  'Set PILOT_E2E=true with a local development API to run the complete workflows.'
);
async function signIn(page: Page) {
  await page.getByLabel('Your name').fill('Usability check');
  await page
    .getByLabel('Work email')
    .fill(`ux-${Date.now()}-${Math.random().toString(36).slice(2)}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
}
async function create(page: Page, title: string, kind = 'document') {
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill(title);
  await page.getByRole('dialog').getByRole('combobox').selectOption(kind);
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  await expect(page.getByLabel('Document title')).toHaveValue(title);
}

test('signed-out visitors sign in before the workspace appears, then return to it', async ({
  page
}) => {
  const response = await page.goto('/workspace');
  expect(response?.url()).toMatch(/\/login\?next=%2Fworkspace$/);
  await expect(page.getByText('Please sign in')).toHaveCount(0);
  await expect(page.getByText('Sign in to open your workspace.')).toBeVisible();
  await signIn(page);
  await expect(page).toHaveURL(/\/workspace$/);
  await create(page, 'Return path');
  const documentPath = new URL(page.url()).pathname;
  await page.context().clearCookies();
  await page.goto(documentPath);
  await expect(page).toHaveURL(new RegExp(`/login\\?next=${encodeURIComponent(documentPath)}$`));
  await signIn(page);
  await expect(page).toHaveURL(new RegExp(`${documentPath}$`));
  // An open redirect is refused: only workspace paths are honoured.
  await page.context().clearCookies();
  await page.goto('/login?next=https%3A%2F%2Fexample.com');
  await signIn(page);
  await expect(page).toHaveURL(/\/workspace$/);
});

test('the File menu closes on an outside click or Escape, like desktop apps', async ({ page }) => {
  await page.goto('/login');
  await signIn(page);
  await create(page, 'Menu behaviour');
  const file = page.getByRole('button', { name: 'File', exact: true });
  const menu = page.getByRole('menu', { name: 'File' });
  await file.click();
  await expect(menu).toBeVisible();
  // Click an empty spot of the page, well away from the open menu.
  await page.mouse.click(1100, 600);
  await expect(menu).toHaveCount(0);
  await file.click();
  await expect(menu).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
  await expect(file).toBeFocused();
  await file.press('ArrowDown');
  await expect(menu).toBeVisible();
  await expect(menu.getByRole('button').first()).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(menu.getByRole('button').nth(1)).toBeFocused();
  await menu.getByRole('button', { name: 'Original uploads & import report' }).click();
  await expect(menu).toHaveCount(0);
});

test('grid view keeps every document name visible', async ({ page }) => {
  await page.goto('/login');
  await signIn(page);
  const long = 'Community interview guide for the second round of household visits';
  await create(page, long);
  await page.getByRole('link', { name: 'Back to workspace' }).click();
  await page.getByRole('button', { name: 'Grid view' }).click();
  const heading = page.locator('.document-grid:not(.list) h3', { hasText: 'Community interview' });
  await expect(heading).toBeVisible();
  const box = (await heading.boundingBox())!;
  expect(box.width).toBeGreaterThan(150);
  await expect(page.locator('.document-grid:not(.list) .doc-details p').first()).toHaveText(
    'My workspace'
  );
  await page.reload();
  // The chosen layout is remembered in this browser.
  await expect(page.getByRole('button', { name: 'List view' })).toBeVisible();
});

test('ribbon captions stay on one line below their commands', async ({ page }) => {
  await page.goto('/login');
  await signIn(page);
  await create(page, 'Ribbon layout');
  for (const tab of ['Home', 'Insert', 'Layout', 'References', 'Review', 'View']) {
    await page.getByRole('tab', { name: tab, exact: true }).click();
    for (const label of await page.locator('.office-panel .group-label').all()) {
      const box = (await label.boundingBox())!;
      expect(box.height, `${tab}: ${await label.textContent()}`).toBeLessThan(20);
      const group = (await label.locator('xpath=..').boundingBox())!;
      expect(box.width).toBeLessThanOrEqual(group.width + 1);
    }
  }
  await page.getByRole('tab', { name: 'References', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Insert table of contents' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Update table of contents' })).toBeVisible();
});
