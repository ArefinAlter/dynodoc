import { expect, test, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in stack.');
test.use({ actionTimeout: 15000 });
async function api(page: Page, path: string, body?: unknown) {
  return page.evaluate(
    async ({ path, body }) => {
      const r = await fetch('/api/engine' + path, {
        method: body === undefined ? 'GET' : 'POST',
        headers: { 'content-type': 'application/json', 'x-dynodoc-editor': 'office-v5' },
        body: body === undefined ? undefined : JSON.stringify(body)
      });
      const result = await r.json();
      if (!r.ok) throw new Error(JSON.stringify(result));
      return result;
    },
    { path, body }
  );
}

test('Projects front supports private profiles, multiple files, draft review, history and separate editors', async ({
  page
}, info) => {
  test.setTimeout(150000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Repository tester');
  await page.getByLabel('Work email').fill(`repository-${Date.now()}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
  await page.getByRole('link', { name: 'Edit profile', exact: true }).click();
  await page.getByLabel('Display name').fill('Morgan Lee');
  await page.getByLabel('Bio').fill('Working on our shared annual report.');
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status')).toHaveText('Profile saved');
  await expect(page.getByText('Project members only', { exact: true })).toBeVisible();
  await page.goto('/projects');
  await expect(page.getByRole('heading', { name: 'Morgan Lee' })).toBeVisible();
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByLabel('Project name').fill('Annual report');
  await page.getByLabel('Description').fill('Documents and decisions for the next annual report.');
  await page.getByRole('button', { name: 'Create project', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Annual report', exact: true })).toBeVisible();
  const projectPath = new URL(page.url()).pathname;
  const upload = page.locator('input[type=file]');
  await upload.setInputFiles({
    name: 'too-large.txt',
    mimeType: 'text/plain',
    buffer: Buffer.alloc(3_000_001)
  });
  await expect(page.getByRole('alert')).toContainText('3 MB');
  expect((await api(page, `${projectPath}/files`)).items).toHaveLength(0);
  for (const [name, text] of [
    ['Report.txt', 'Original report paragraph'],
    ['Budget.csv', 'Item,Amount\nResearch,500']
  ]) {
    await upload.setInputFiles({ name, mimeType: 'text/plain', buffer: Buffer.from(text) });
    await expect(page.getByRole('link', { name: name.split('.')[0], exact: true })).toBeVisible();
  }
  const files = (await api(page, `${projectPath}/files`)).items;
  expect(files).toHaveLength(2);
  await page.screenshot({ path: info.outputPath('project-desktop.png'), fullPage: true });
  expect((await new AxeBuilder({ page }).include('.repo').analyze()).violations).toEqual([]);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: info.outputPath('project-mobile.png'), fullPage: true });
  await page.setViewportSize({ width: 1280, height: 900 });
  const appendix = await api(page, '/documents', { title: 'Appendix' });
  await page.getByText('Move an existing file into this project', { exact: true }).click();
  await page
    .getByRole('combobox', { name: 'Choose a file', exact: true })
    .selectOption(appendix.id);
  await page.getByRole('button', { name: 'Move file', exact: true }).click();
  await expect(page.getByRole('link', { name: 'Appendix', exact: true })).toBeVisible();
  await page.getByRole('link', { name: 'Report', exact: true }).click();
  await expect(page.getByLabel('Read-only file preview')).toContainText(
    'Original report paragraph'
  );
  await expect(page.locator('[contenteditable=true]')).toHaveCount(0);
  await expect(page.getByRole('tablist', { name: 'Document ribbon' })).toBeHidden();
  const filePath = new URL(page.url()).pathname;
  const id = filePath.split('/').pop();
  const original = await api(page, `/documents/${id}`);
  const block = Object.values(original.state.nodes).find(
    (n) => (n as { type: string }).type === 'item'
  ) as { id: string };
  await page.getByRole('button', { name: 'New file draft' }).click();
  await page.getByLabel('Draft name', { exact: true }).fill('Revise report');
  await page.getByRole('button', { name: 'Create draft', exact: true }).click();
  await expect(page).toHaveURL(/draft=/);
  const draft = new URL(page.url()).searchParams.get('draft');
  await api(page, `/documents/${id}/drafts/${draft}`, {
    revision: 0,
    ops: [
      {
        type: 'FieldEdited',
        node_id: block.id,
        field: 'content',
        value: { type: 'paragraph', content: [{ type: 'text', text: 'Updated report paragraph' }] }
      }
    ]
  });
  await page.reload();
  await expect(page.getByLabel('Read-only file preview')).toContainText('Updated report paragraph');
  await page.getByRole('button', { name: 'Compare with main' }).click();
  await expect(page.getByRole('dialog').locator('ins').last()).toContainText('Updated');
  await page.getByRole('dialog').getByRole('button', { name: 'Close review', exact: true }).click();
  await page.getByLabel('Pull request message').fill('Clarify the report introduction');
  await page.getByRole('button', { name: 'Open pull request', exact: true }).click();
  await page.getByRole('button', { name: 'Revise report', exact: true }).click();
  await page.getByRole('button', { name: /^Merge / }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByLabel('File branch')).toHaveValue('');
  await page.getByRole('button', { name: 'File', exact: true }).click();
  await expect(page.getByLabel('Read-only file preview')).toContainText('Updated report paragraph');
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.getByRole('button', { name: 'Recover an earlier revision' }).click();
  await page.getByLabel('Earlier revision', { exact: true }).fill(String(original.latest_seq));
  const native = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download native file', exact: true }).click();
  expect((await native).suggestedFilename()).toMatch(/\.docx$/);
  await page.getByRole('button', { name: 'Preview recovery', exact: true }).click();
  await page
    .getByRole('region', { name: 'Recover earlier content' })
    .getByRole('checkbox')
    .first()
    .check();
  await expect(page.getByRole('button', { name: 'Create recovery draft' })).toBeEnabled();
  await page.getByRole('button', { name: 'Create recovery draft' }).click();
  await expect(page.getByLabel('Read-only file preview')).toContainText(
    'Original report paragraph'
  );
  await page.getByRole('link', { name: 'Web editor', exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`/documents/${id}\\?draft=`));
  await expect(page.getByLabel('Document title')).toHaveValue('Report');
  await page
    .getByRole('navigation', { name: 'Project', exact: true })
    .getByRole('link', { name: 'Report', exact: true })
    .click();
  await expect(page.getByLabel('Read-only file preview')).toContainText('Updated report paragraph');
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.screenshot({ path: info.outputPath('file-dark.png'), fullPage: true });
  expect(errors).toEqual([]);
});
