import { expect, test, type Page } from '@playwright/test';
// Requires a local API and DEV_AUTH=true web server. Never enables a public login bypass.
test.skip(
  process.env.PILOT_E2E !== 'true',
  'Set PILOT_E2E=true with a local development API to run the complete workflows.'
);
async function login(page: Page, email?: string) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Pilot researcher');
  await page
    .getByLabel('Work email')
    .fill(email || `browser-${Date.now()}-${Math.random().toString(36).slice(2)}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/workspace$/);
}
async function create(page: Page, title: string, kind = 'document') {
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill(title);
  await page.getByRole('dialog').getByRole('combobox').selectOption(kind);
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  await expect(page.getByLabel('Document title')).toHaveValue(title);
}
async function saved(page: Page) {
  await expect(page.locator('.save-state')).toHaveText(/All changes saved|Personal draft saved/, {
    timeout: 15000
  });
}
test('email sign-in, document editing, sharing, version restore and personal drafts', async ({
  page,
  context
}) => {
  await login(page);
  const cookies = await context.cookies();
  expect(cookies.find((c) => c.name === 'dynodoc_session')?.httpOnly).toBe(true);
  expect(await page.evaluate(() => Object.keys(localStorage))).not.toContain('access_token');
  await create(page, 'Community interview guide');
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.click();
  await page.keyboard.type('What makes a good neighbourhood?');
  await saved(page);
  await page.reload();
  await expect(editor).toContainText('What makes a good neighbourhood?');
  await page.getByRole('button', { name: 'Share', exact: true }).click();
  await page.getByLabel('Email address', { exact: true }).fill('colleague@example.test');
  await page.getByRole('button', { name: 'Add person', exact: true }).click();
  await expect(page.getByText('colleague@example.test', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close panel' }).click();
  await page.getByRole('button', { name: 'Version history', exact: true }).last().click();
  await page.getByLabel('Version name').fill('Before fieldwork');
  await page.getByRole('button', { name: 'Save named version' }).click();
  await expect(page.getByText('Before fieldwork', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close panel' }).click();
  await page.getByRole('button', { name: 'Personal drafts', exact: true }).click();
  await page.getByLabel('Draft name').fill('Alternative wording');
  await page.getByRole('button', { name: 'Create personal draft', exact: true }).click();
  await expect(page.locator('.draft-banner')).toContainText('Alternative wording');
  await page.getByRole('button', { name: 'Close panel' }).click();
  await editor.click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type(' Tell us about your community.');
  await saved(page);
  await page.getByRole('button', { name: 'Review & share changes' }).click();
  await page.getByRole('button', { name: 'Share included changes' }).click();
  await expect(page.locator('.draft-banner')).toHaveCount(0);
  await expect(editor).toContainText('Tell us about your community.');
  await page.getByRole('button', { name: 'Version history', exact: true }).last().click();
  await page.getByRole('button', { name: 'Restore as new changes' }).click();
  await expect(editor).not.toContainText('Tell us about your community.');
  await page.screenshot({ path: '/tmp/dynodoc-editor.png' });
});
test('spreadsheet cells calculate formulas and persist after reload', async ({ page }) => {
  await login(page);
  await create(page, 'Fieldwork tracker', 'spreadsheet');
  async function cell(address: string, value: string) {
    await page.getByRole('button', { name: new RegExp('^' + address + ':') }).dblclick();
    await page.getByLabel('Edit ' + address, { exact: true }).fill(value);
    await page.getByLabel('Edit ' + address, { exact: true }).press('Enter');
  }
  await cell('A1', '12');
  await cell('A2', '8');
  await cell('B1', '=SUM(A1:A2)');
  await expect(page.getByRole('button', { name: 'B1: 20', exact: true })).toBeVisible();
  await saved(page);
  await page.reload();
  await expect(page.getByRole('button', { name: 'B1: 20', exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/dynodoc-sheet.png' });
  await page.getByRole('button', { name: 'A2: 8', exact: true }).click();
  await page.getByRole('button', { name: 'Delete selected row', exact: true }).click();
  await expect(page.getByRole('button', { name: 'B1: 12', exact: true })).toBeVisible();
  await saved(page);
  await page.reload();
  await expect(page.getByRole('button', { name: 'B1: 12', exact: true })).toBeVisible();
});
test('CSV upload creates editable data and retains the original', async ({ page }) => {
  await login(page);
  await page.locator('input[type=file]').setInputFiles({
    name: 'interviews.csv',
    mimeType: 'text/csv',
    buffer: Buffer.from('Site,Interviews\nDhaka,12\nSylhet,8\n')
  });
  await page.getByRole('button', { name: 'Import into workspace', exact: true }).click();
  await expect(page.getByRole('button', { name: 'A1: Site', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'File', exact: true }).click();
  await page.getByRole('button', { name: 'Original uploads & import report' }).click();
  await expect(page.getByRole('link', { name: 'Download original' })).toBeVisible();
});
test('unauthenticated and cross-origin requests cannot access or mutate private data', async ({
  page,
  request
}) => {
  const anonymous = await request.get('/api/engine/documents');
  expect(anonymous.status()).toBe(401);
  await login(page);
  const response = await page.request.post('/api/engine/documents', {
    headers: { origin: 'https://unrelated.example' },
    data: { title: 'Should be refused' }
  });
  expect(response.status()).toBe(403);
});

test('a reviewer proposes a paragraph change and an editor accepts it', async ({
  page,
  browser
}) => {
  await login(page);
  await create(page, 'Review workflow');
  const content = page.getByRole('textbox', { name: 'Document content' });
  await content.click();
  await page.keyboard.type('Original interview question.');
  await saved(page);
  const url = page.url();
  const email = `reviewer-${Date.now()}@example.test`;
  await page.getByRole('button', { name: 'Share', exact: true }).click();
  await page.getByLabel('Email address', { exact: true }).fill(email);
  await page.getByLabel('Role', { exact: true }).selectOption('contributor');
  await page.getByRole('button', { name: 'Add person', exact: true }).click();
  await expect(page.getByText(email, { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close panel' }).click();
  const other = await browser.newContext();
  const reviewer = await other.newPage();
  await login(reviewer, email);
  await reviewer.goto(url);
  const reviewContent = reviewer.getByRole('textbox', { name: 'Document content' });
  await expect(reviewContent).toHaveAttribute('contenteditable', 'false');
  await reviewContent.locator('p').first().click();
  await reviewer.getByRole('button', { name: /^Suggestions/ }).click();
  await reviewer.getByLabel('Suggested paragraph text').fill('A clearer interview question.');
  await reviewer.getByRole('button', { name: 'Propose change' }).click();
  await expect(reviewer.getByText('A clearer interview question.', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: /^Suggestions/ }).click();
  await page.getByRole('button', { name: 'Accept change' }).click();
  await expect(content).toContainText('A clearer interview question.');
  await expect(reviewContent).toContainText('A clearer interview question.');
  await other.close();
});

test('the provided Word, workbook and XLSForm examples import successfully', async ({ page }) => {
  await login(page);
  for (const [name, expected] of [
    ['Interview guide', 'Document content'],
    ['Fieldwork tracker', 'Formula bar'],
    ['Questionnaire', 'Question label']
  ]) {
    await page.goto('/workspace');
    await page.getByRole('button', { name, exact: true }).click();
    await page.getByRole('button', { name: 'Import into workspace' }).click();
    await expect(page.getByLabel(expected).first()).toBeVisible();
  }
});

test('a document can be archived and restored through the workspace', async ({ page }) => {
  await login(page);
  await create(page, 'Archive workflow');
  await page.getByRole('button', { name: 'File', exact: true }).click();
  await page.getByRole('button', { name: 'Move to archive', exact: true }).click();
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(page.getByText('Archive workflow', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Workspace options', exact: true }).click();
  await page.getByRole('button', { name: 'Archived documents', exact: true }).click();
  await page.getByText('Archive workflow', { exact: true }).click();
  await page.getByRole('button', { name: 'File', exact: true }).click();
  await page.getByRole('button', { name: 'Restore to workspace', exact: true }).click();
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(page.getByText('Archive workflow', { exact: true })).toBeVisible();
});

test('research connections reject private servers and import a provider fixture through the GUI', async ({
  page
}) => {
  await login(page);
  const refused = await page.request.post('/api/research', {
    headers: { origin: new URL(page.url()).origin },
    data: {
      provider: 'kobo',
      origin: 'https://127.0.0.1',
      token: 'fixture-private-token',
      action: 'projects'
    }
  });
  expect(refused.status()).toBe(422);
  await page.route('**/api/research', async (route) => {
    const action = route.request().postDataJSON().action;
    await route.fulfill({
      json:
        action === 'projects'
          ? { items: [{ id: 'study1', name: 'Community study' }] }
          : {
              filename: 'kobo-study1-submissions.csv',
              content_type: 'text/csv',
              content_base64: Buffer.from('id,community\n1,Dhaka\n').toString('base64'),
              warnings: ['Responses imported as a snapshot.'],
              source: {
                provider: 'kobo',
                origin: 'https://kf.kobotoolbox.org',
                project: '',
                form: 'study1',
                kind: 'submissions',
                fetched_at: new Date().toISOString()
              }
            }
    });
  });
  await page.getByRole('button', { name: 'Workspace options', exact: true }).click();
  await page.getByRole('button', { name: 'Import from research tool' }).click();
  await page.getByLabel('Kobo API key').fill('fixture-private-token');
  await page.getByRole('button', { name: 'Find questionnaires' }).click();
  await page.getByLabel('What would you like to bring in?').selectOption('submissions');
  await page.getByRole('button', { name: 'Preview import' }).click();
  await expect(page.getByText('Responses imported as a snapshot.')).toBeVisible();
  await page.getByRole('button', { name: 'Import into workspace' }).click();
  await expect(page.getByRole('button', { name: 'B2: Dhaka', exact: true })).toBeVisible();
  const id = page.url().split('/').pop();
  const response = await page.request.get(`/api/engine/documents/${id}`, {
    headers: { 'x-dynodoc-editor': 'office-v5' }
  });
  const detail = await response.json();
  expect(detail.settings.research_source.form).toBe('study1');
  expect(JSON.stringify(detail)).not.toContain('fixture-private-token');
});
