import { test, expect } from '@playwright/test';

test('long formatted passages save compact history and replay after reload', async ({ page }) => {
  test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated authenticated stack.');
  await page.goto('/login');
  await page.getByLabel('Work email').fill(`richtext-${Date.now()}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
  await page.goto('/workspace');
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill('Rich-text history verification');
  await page.getByRole('dialog').getByRole('combobox').selectOption('document');
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  const editor = page.getByRole('textbox', { name: 'Document content' });
  const text = 'Research evidence stays with its history. '.repeat(100);
  await editor.fill(text);
  await expect(page.locator('.save-state')).toHaveText('All changes saved', { timeout: 30_000 });
  await editor.focus();
  await page.keyboard.press('Control+a');
  await page.keyboard.press('Control+b');
  await expect(editor.locator('strong')).toContainText('Research evidence');
  await expect(page.locator('.save-state')).toHaveText('All changes saved', { timeout: 30_000 });
  const id = new URL(page.url()).pathname.split('/').pop();
  const oldReader = await page.request.get(`/api/engine/documents/${id}`);
  expect(oldReader.status()).toBe(409);
  expect((await oldReader.json()).error).toContain('reload this tab');
  const response = await page.request.get(`/api/engine/documents/${id}/events?limit=200`, {
    headers: { 'x-dynodoc-editor': 'office-v5' }
  });
  expect(response.ok()).toBeTruthy();
  const events = await response.json();
  expect(events.items.some((e: { type: string }) => e.type === 'RichTextPatched')).toBeTruthy();
  await page.reload();
  await expect(editor.locator('strong')).toContainText('Research evidence');
  await expect(editor).toContainText(text.trim());
});
