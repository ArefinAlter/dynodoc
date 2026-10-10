import { expect, test, type Page } from '@playwright/test';
// Requires a local API and DEV_AUTH=true web server. Never enables a public login bypass.
test.skip(
  process.env.PILOT_E2E !== 'true',
  'Set PILOT_E2E=true with a local development API to run the complete workflows.'
);
async function login(page: Page) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Steady typist');
  await page
    .getByLabel('Work email')
    .fill(`typist-${Date.now()}-${Math.random().toString(36).slice(2)}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
  await page.goto('/workspace');
}
async function saved(page: Page) {
  await expect(page.locator('.save-state')).toHaveText(/All changes saved|Personal draft saved/, {
    timeout: 15000
  });
}
// Typing continues while autosave (after ~1.3 s) completes; nothing may move the caret.
const sentence = ' The caret must stay put while autosave finishes in the background.';

for (const where of ['team version', 'personal draft']) {
  test(`typing across an autosave keeps every character in order (${where})`, async ({ page }) => {
    await login(page);
    await page.getByRole('button', { name: 'New document', exact: true }).click();
    await page.getByLabel('Document name', { exact: true }).fill(`Typing ${where}`);
    await page.getByRole('button', { name: 'Create document', exact: true }).click();
    const editor = page.getByRole('textbox', { name: 'Document content' });
    await expect(editor).toHaveAttribute('contenteditable', 'true');
    await editor.click();
    await page.keyboard.type('First paragraph stays first.');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Second paragraph.');
    await expect(page.locator('.save-state')).toHaveText(/Unsaved changes|Saving/);
    await saved(page);
    if (where === 'personal draft') {
      await page.getByRole('button', { name: 'Personal drafts', exact: true }).click();
      await page.getByLabel('Draft name').fill('Typing draft');
      await page.getByRole('button', { name: 'Create personal draft', exact: true }).click();
      await expect(page.locator('.draft-banner')).toContainText('Typing draft');
      await page.getByRole('button', { name: 'Close panel' }).click();
    }
    await editor.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type(sentence, { delay: 60 });
    await saved(page);
    await expect(editor.locator('p').nth(1)).toHaveText('Second paragraph.' + sentence);
    await expect(editor.locator('p').first()).toHaveText('First paragraph stays first.');
    await page.reload();
    await expect(
      page.getByRole('textbox', { name: 'Document content' }).locator('p').nth(1)
    ).toHaveText(where === 'personal draft' ? 'Second paragraph.' : 'Second paragraph.' + sentence);
  });
}
