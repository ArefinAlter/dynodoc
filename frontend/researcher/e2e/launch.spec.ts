import { expect, test, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import ExcelJS from 'exceljs';
async function login(page: Page, email: string) {
  await page.goto('/login');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/workspace$/);
}
test('canonical pages, sitemap, structured metadata and icons are available', async ({
  page,
  request
}) => {
  await page.goto('/');
  await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
    'href',
    'https://dynodoc.online/'
  );
  const metadata = JSON.parse(
    (await page.locator('script[type="application/ld+json"]').textContent()) || '{}'
  );
  expect(metadata['@graph'][0].name).toBe('Dynodoc');
  const xml = await (await request.get('/sitemap.xml')).text();
  expect(xml).toContain('https://dynodoc.online/privacy');
  expect(xml).not.toContain('/workspace');
  for (const path of [
    '/favicon.svg',
    '/favicon.ico',
    '/favicon-48.png',
    '/apple-touch-icon.png',
    '/brand/social.png'
  ])
    expect((await request.get(path)).status()).toBe(200);
  const admin = await request.get('/admin', { maxRedirects: 0 });
  expect(admin.status()).toBe(303);
  expect(admin.headers()['x-robots-tag']).toContain('noindex');
});
test('public pages pass automated WCAG A and AA checks', async ({ page }) => {
  for (const path of ['/', '/login', '/privacy', '/terms']) {
    await page.goto(path);
    await page.waitForLoadState('networkidle');
    const result = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'])
      .analyze();
    expect(
      result.violations.map((v) => ({
        id: v.id,
        impact: v.impact,
        nodes: v.nodes.map((n) => n.target)
      })),
      path
    ).toEqual([]);
  }
});
test('administrator sees actual counters while a member is denied', async ({ page, browser }) => {
  test.skip(process.env.PILOT_E2E !== 'true', 'Requires an isolated local test stack');
  await login(page, 'admin@example.test');
  await page.goto('/admin');
  await expect(
    page.getByRole('heading', { name: 'A clear view of your workspace.' })
  ).toBeVisible();
  await expect(page.getByText('document_upload', { exact: true })).toBeVisible();
  const result = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'])
    .analyze();
  expect(result.violations).toEqual([]);
  const context = await browser.newContext();
  try {
    const other = await context.newPage();
    await login(other, `member-${Date.now()}@example.test`);
    const response = await other.goto('/admin');
    expect(response?.status()).toBe(403);
    await expect(other.getByText(/Administrator access is required/)).toBeVisible();
  } finally {
    await context.close();
  }
});
test('respondent preview evaluates relevance, constraints and calculations without saving answers', async ({
  page
}) => {
  test.skip(process.env.PILOT_E2E !== 'true', 'Requires an isolated local test stack');
  await login(page, `preview-${Date.now()}@example.test`);
  const book = new ExcelJS.Workbook();
  book.addWorksheet('survey').addRows([
    ['type', 'name', 'label', 'relevant', 'constraint', 'calculation', 'required'],
    ['integer', 'age', 'Age', '', '. >= 18', '', 'yes'],
    ['text', 'work', 'Work', '${age} >= 18', '', '', 'yes'],
    ['calculate', 'next', 'Next age', '', '', '${age} + 1', '']
  ]);
  await page.locator('input[type="file"]').setInputFiles({
    name: 'logic.xlsx',
    mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    buffer: Buffer.from(await book.xlsx.writeBuffer())
  });
  await page.getByRole('button', { name: 'Import into workspace' }).click();
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await page.getByLabel('Age', { exact: true }).fill('16');
  await expect(page.getByLabel('Work', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('Next age', { exact: true })).toHaveText('17');
  await page.getByRole('button', { name: 'Check answers' }).click();
  await expect(page.getByText('This answer does not meet the validation rule.')).toBeVisible();
  await page.getByLabel('Age', { exact: true }).fill('22');
  await page.getByLabel('Work', { exact: true }).fill('Research');
  await page.getByRole('button', { name: 'Check answers' }).click();
  await expect(page.getByText('These answers pass the supported preview checks.')).toBeVisible();
  await page.reload();
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByLabel('Age', { exact: true })).toHaveValue('');
});
