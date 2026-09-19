import { test, expect, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
test.skip(
  process.env.PILOT_E2E !== 'true',
  'Requires the isolated authenticated verification stack.'
);
async function login(
  page: Page,
  email = `product-${Date.now()}-${Math.random().toString(36).slice(2)}@example.test`
) {
  await page.goto('/login');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(page.getByRole('button', { name: 'Not now', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Not now', exact: true }).click();
  return email;
}
async function create(page: Page, title: string, kind = 'document') {
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill(title);
  await page.getByRole('combobox').selectOption(kind);
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  await expect(page.getByLabel('Document title')).toHaveValue(title);
}
async function saved(page: Page) {
  await expect(page.locator('.save-state')).toHaveText(/All changes saved|Personal draft saved/, {
    timeout: 20000
  });
}
async function cell(page: Page, address: string, value: string) {
  await page.getByRole('button', { name: new RegExp('^' + address + ':') }).dblclick();
  await page.getByLabel('Edit ' + address, { exact: true }).fill(value);
  await page.keyboard.press('Enter');
  await saved(page);
}
test('published copies stay frozen, private comments stay private, and Trash revokes links', async ({
  page,
  browser
}) => {
  await login(page);
  await create(page, 'Public protocol');
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.fill('The published wording.');
  await saved(page);
  await editor.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Add comment', exact: true }).click();
  await page.getByLabel('Comment', { exact: true }).fill('Private review conversation');
  await page.getByRole('button', { name: 'Post comment', exact: true }).click();
  await expect(page.getByText('Private review conversation', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close panel', exact: true }).click();
  await page.getByRole('button', { name: 'Share', exact: true }).click();
  await page.getByRole('button', { name: 'Create public link', exact: true }).click();
  const link = await page.getByLabel('New public link', { exact: true }).inputValue();
  expect(link).toContain('/view/');
  const guest = await browser.newContext();
  const reader = await guest.newPage();
  try {
    await reader.goto(link);
    await expect(reader.getByRole('textbox', { name: 'Document content' })).toContainText(
      'The published wording.'
    );
    await expect(reader.getByText('Private review conversation')).toHaveCount(0);
    await expect(reader.locator('[contenteditable="true"]')).toHaveCount(0);
    await page.getByRole('button', { name: 'Close panel', exact: true }).click();
    await editor.fill('A later private revision.');
    await saved(page);
    await reader.reload();
    await expect(reader.getByRole('textbox', { name: 'Document content' })).toContainText(
      'The published wording.'
    );
    await page.goto('/workspace');
    page.once('dialog', (d) => d.accept());
    await page.getByRole('button', { name: 'Move to Trash', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Public protocol', exact: true })).toHaveCount(
      0
    );
    expect((await reader.goto(link))?.status()).toBe(404);
    await page.getByRole('button', { name: 'Trash', exact: true }).click();
    await page.getByRole('button', { name: 'Restore document', exact: true }).click();
    expect((await reader.goto(link))?.status()).toBe(404);
    await page.getByRole('button', { name: 'Back to documents', exact: true }).click();
    await page
      .getByRole('link')
      .filter({ has: page.getByRole('heading', { name: 'Public protocol', exact: true }) })
      .click();
    await expect(editor).toContainText('A later private revision.');
  } finally {
    await guest.close();
  }
});
test('document tabs isolate content and LaTeX equations survive a reload', async ({ page }) => {
  await login(page);
  await create(page, 'Tabbed report');
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.fill('Main report text');
  await saved(page);
  page.once('dialog', (d) => d.accept('Methods'));
  await page.getByRole('button', { name: 'Add tab' }).click();
  await editor.fill('Methods text');
  await saved(page);
  await page.getByRole('button', { name: 'Insert LaTeX equation', exact: true }).click();
  await page.getByLabel('Equation source').fill('\\frac{a}{b}');
  await page.getByRole('button', { name: 'Insert equation', exact: true }).click();
  await saved(page);
  await expect(page.locator('.katex').first()).toBeVisible();
  await page.getByRole('button', { name: 'Main document', exact: true }).click();
  await expect(editor).toContainText('Main report text');
  await expect(editor).not.toContainText('Methods text');
  await page.reload();
  await page.getByRole('button', { name: 'Methods', exact: true }).click();
  await expect(editor).toContainText('Methods text');
  await expect(page.locator('.katex').first()).toBeVisible();
  await page.getByRole('button', { name: 'File', exact: true }).click();
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download Overleaf project (.zip)', exact: true }).click();
  expect((await download).suggestedFilename()).toBe('Tabbed report-overleaf.zip');
});
test('spreadsheet context actions preserve formulas through row insertion and sheet rename', async ({
  page
}) => {
  await login(page);
  await create(page, 'Field data', 'spreadsheet');
  await cell(page, 'A1', '7');
  await cell(page, 'B1', '=A1*2');
  await page.getByRole('button', { name: 'A1: 7', exact: true }).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Insert row above', exact: true }).click();
  await saved(page);
  await expect(page.getByRole('button', { name: 'A2: 7', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'B2: 14', exact: true })).toBeVisible();
  page.once('dialog', (d) => d.accept('Data'));
  await page.getByRole('combobox', { name: 'Sheet actions' }).selectOption('rename');
  await saved(page);
  await page.getByRole('button', { name: 'Add sheet', exact: true }).click();
  await cell(page, 'A1', '=Data!A2');
  await expect(page.getByRole('button', { name: 'A1: 7', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Data', exact: true }).click({ button: 'right' });
  page.once('dialog', (d) => d.accept('Field observations'));
  await page.getByRole('menuitem', { name: 'Rename sheet', exact: true }).click();
  await saved(page);
  await page.getByRole('button', { name: 'Sheet 2', exact: true }).click();
  await expect(page.getByRole('button', { name: 'A1: 7', exact: true })).toBeVisible();
  await page.reload();
  await page.getByRole('button', { name: 'Sheet 2', exact: true }).click();
  await expect(page.getByRole('button', { name: 'A1: 7', exact: true })).toBeVisible();
});
test('the tutorial can be reopened and the mobile workspace stays within the viewport', async ({
  page
}) => {
  await login(page);
  await page.getByRole('button', { name: 'Help & tutorials' }).click();
  await expect(
    page.getByRole('heading', { name: 'Bring your research together', exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Write and work with data', exact: true })
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Help & tutorials' }).click();
  await expect(
    page.getByRole('heading', { name: 'Bring your research together', exact: true })
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('link', { name: 'About Dynodoc' })).toHaveAttribute(
    'href',
    'https://dynodoc.online'
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole('button', { name: 'Workspaces', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Manage team workspaces', exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Close navigation', exact: true }).click();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(
    true
  );
});
test('the administrator disables only a confirmed account and can restore it', async ({
  page,
  browser
}) => {
  const member = await browser.newContext();
  const other = await member.newPage();
  try {
    const email = await login(other);
    await login(page, 'admin@example.test');
    await page.goto('/admin');
    await page.getByRole('combobox', { name: 'Records', exact: true }).selectOption('users');
    await page.getByRole('textbox', { name: 'Search', exact: true }).fill(email);
    await page.getByRole('button', { name: 'Search', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Disable account', exact: true })).toHaveCount(1);
    await page.getByRole('button', { name: 'Disable account', exact: true }).click();
    await page.getByRole('dialog').getByRole('textbox').fill(email);
    await page.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Restore', exact: true })).toBeVisible();
    expect((await other.request.get('/api/engine/auth/me')).status()).toBe(404);
    expect((await other.request.get('/api/engine/workspace/index')).status()).toBe(401);
    await page.getByRole('button', { name: 'Restore', exact: true }).click();
    await page.getByRole('dialog').getByRole('textbox').fill(email);
    await page.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Disable account', exact: true })).toBeVisible();
    expect((await other.request.get('/api/engine/workspace/index')).status()).toBe(401);
  } finally {
    await member.close();
  }
});
test('workspace and editor controls pass automated accessibility checks', async ({ page }) => {
  await login(page);
  let result = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
    .analyze();
  expect(result.violations.map((v) => ({ id: v.id, nodes: v.nodes.map((n) => n.target) }))).toEqual(
    []
  );
  await create(page, 'Accessible report');
  result = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
    .analyze();
  expect(result.violations.map((v) => ({ id: v.id, nodes: v.nodes.map((n) => n.target) }))).toEqual(
    []
  );
});

test('organization access reaches a folder document and can be removed centrally', async ({
  page,
  browser
}) => {
  const guest = await browser.newContext();
  const colleague = await guest.newPage();
  try {
    const email = await login(colleague);
    await login(page);
    for (const name of ['Research institute', 'Field team', 'Pilot study']) {
      await page.getByRole('button', { name: 'Manage team workspaces', exact: true }).click();
      await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill(name);
      await page.getByRole('button', { name: 'Create workspace', exact: true }).click();
      await expect(page.getByRole('dialog')).not.toBeVisible();
      await expect(page.getByRole('button', { name, exact: true })).toBeVisible();
    }
    await create(page, 'Inherited protocol');
    const documentUrl = page.url();
    await page.goto('/workspace');
    await page.getByRole('button', { name: 'Research institute', exact: true }).click();
    await page.getByRole('button', { name: 'Manage team workspaces', exact: true }).click();
    await page.getByLabel('Researcher email').fill(email);
    await page.getByRole('combobox', { name: 'Role', exact: true }).selectOption('viewer');
    await page.getByRole('button', { name: 'Update access', exact: true }).click();
    await expect(
      page.getByRole('dialog').locator('.member').filter({ hasText: email })
    ).toBeVisible();
    await colleague.goto(documentUrl);
    await expect(colleague.getByLabel('Document title')).toHaveValue('Inherited protocol');
    await expect(colleague.locator('[contenteditable="true"]')).toHaveCount(0);
    await page.getByLabel('Researcher email').fill(email);
    await page.getByRole('combobox', { name: 'Role', exact: true }).selectOption('remove');
    await page.getByRole('button', { name: 'Update access', exact: true }).click();
    await expect(
      page.getByRole('dialog').locator('.member').filter({ hasText: email })
    ).toHaveCount(0);
    const id = new URL(documentUrl).pathname.split('/').at(-1);
    expect((await colleague.request.get('/api/engine/documents/' + id)).status()).toBe(403);
  } finally {
    await guest.close();
  }
});

test('questionnaire repeats and translations survive XLSForm export', async ({ page }) => {
  const { default: ExcelJS } = await import('exceljs');
  const { readFile } = await import('node:fs/promises');
  await login(page);
  await create(page, 'Household survey', 'questionnaire');
  await page.getByRole('button', { name: 'Add repeating group', exact: true }).click();
  const labels = page.getByLabel('Question label', { exact: true });
  await labels.last().fill('Household members');
  await labels.last().press('Tab');
  await page.getByText('Location & grouping', { exact: true }).last().click();
  await page.getByLabel('Repeat count expression', { exact: true }).fill('3');
  await page.getByLabel('Repeat count expression', { exact: true }).press('Tab');
  await saved(page);
  await page.getByRole('button', { name: 'Add question', exact: true }).click();
  await labels.last().fill('Preferred services');
  await labels.last().press('Tab');
  await page
    .getByRole('combobox', { name: 'Answer type', exact: true })
    .last()
    .selectOption('rank');
  await page.getByRole('button', { name: 'Add option', exact: true }).last().click();
  await page.getByText('Location & grouping', { exact: true }).last().click();
  await page
    .getByRole('combobox', { name: 'Inside', exact: true })
    .last()
    .selectOption({ label: 'Household members' });
  await page
    .getByText('Appearance, translations & advanced XLSForm', { exact: true })
    .last()
    .click();
  await page.getByLabel('appearance', { exact: true }).last().fill('minimal');
  await page.getByLabel('appearance', { exact: true }).last().press('Tab');
  const extra = page
    .getByLabel('Additional XLSForm columns and translations (JSON)', { exact: true })
    .last();
  await extra.fill(
    JSON.stringify({
      'label::English (en)': 'Preferred services',
      'label::Bangla (bn)': 'পছন্দের সেবা'
    })
  );
  await extra.press('Tab');
  await saved(page);
  const wait = page.waitForEvent('download');
  await page.getByRole('button', { name: 'XLSForm', exact: true }).click();
  const download = await wait;
  const book = new ExcelJS.Workbook();
  await book.xlsx.load(await readFile((await download.path())!));
  const text = JSON.stringify(book.getWorksheet('survey')!.getSheetValues());
  expect(text).toContain('begin_repeat');
  expect(text).toContain('end_repeat');
  expect(text).toContain('rank ');
  expect(text).toContain('repeat_count');
  expect(text).toContain('পছন্দের সেবা');
  await page.reload();
  await expect(page.getByLabel('Question label').last()).toHaveValue('Preferred services');
});
