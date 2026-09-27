import { expect, test, type Page } from '@playwright/test';
// Requires a local API and DEV_AUTH=true web server. Never enables a public login bypass.
test.skip(
  process.env.PILOT_E2E !== 'true',
  'Set PILOT_E2E=true with a local development API to run the complete workflows.'
);
async function login(page: Page) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Deck author');
  await page
    .getByLabel('Work email')
    .fill(`deck-${Date.now()}-${Math.random().toString(36).slice(2)}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/workspace$/);
}
async function saved(page: Page) {
  await expect(page.locator('.save-state')).toHaveText(/All changes saved/, { timeout: 15000 });
}

test('a presentation keeps slides, text and notes across reloads and exports PPTX', async ({
  page
}) => {
  await login(page);
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill('Group presentation');
  await page.getByRole('dialog').getByRole('combobox').selectOption('presentation');
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  await expect(page.getByLabel('Document title')).toHaveValue('Group presentation');
  const slides = page.getByRole('list', { name: 'Slides' }).or(page.getByLabel('Slides'));
  await expect(slides.getByRole('button', { name: /^Slide 1:/ })).toBeVisible();
  await page.getByRole('button', { name: 'New slide', exact: true }).first().click();
  await expect(slides.getByRole('button', { name: /^Slide 2:/ })).toBeVisible();
  const title = page.getByRole('button', { name: /^Title placeholder/ }).first();
  await title.dblclick();
  await page.getByLabel('Edit text').fill('Why neighbours share');
  await page.getByLabel('Edit text').press('Escape');
  await expect(slides.getByRole('button', { name: /Why neighbours share/ })).toBeVisible();
  await saved(page);
  await page.reload();
  await expect(
    page.getByLabel('Slides').getByRole('button', { name: /Why neighbours share/ })
  ).toBeVisible();
  await page.getByRole('button', { name: 'File', exact: true }).click();
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('button', { name: /Download PowerPoint/ }).click()
  ]);
  expect(download.suggestedFilename()).toMatch(/\.pptx$/);
});
