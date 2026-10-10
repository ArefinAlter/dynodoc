import { expect, test, type Page } from '@playwright/test';
import { strFromU8, strToU8, unzipSync, zipSync } from 'fflate';
import { readFileSync } from 'node:fs';
// Requires a local API and DEV_AUTH=true web server. Never enables a public login bypass.
test.skip(
  process.env.PILOT_E2E !== 'true',
  'Set PILOT_E2E=true with a local development API to run the complete workflows.'
);
// Optional: WALKTHROUGH_SHOTS=<folder> saves screenshots used in the user guide.
const shots = process.env.WALKTHROUGH_SHOTS;
async function shot(page: Page, name: string) {
  if (shots) await page.screenshot({ path: `${shots}/${name}.png` });
}
async function login(page: Page, name: string, email: string) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill(name);
  await page.getByLabel('Work email').fill(email);
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
/** Wait until typed text has actually gone through autosave. */
async function typedAndSaved(page: Page) {
  await expect(page.locator('.save-state')).toHaveText(/Unsaved changes|Saving/);
  await saved(page);
}
async function editable(page: Page) {
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await expect(editor).toHaveAttribute('contenteditable', 'true');
  return editor;
}
/** Simulate editing the downloaded file in Word on a laptop. */
function editDocx(bytes: Buffer, edit: (xml: string) => string) {
  const parts = unzipSync(new Uint8Array(bytes));
  parts['word/document.xml'] = strToU8(edit(strFromU8(parts['word/document.xml'])));
  return Buffer.from(zipSync(parts));
}
const DOCX = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';

test('teammates push edited Word files, the owner reviews and merges, and a draft rebases', async ({
  browser
}) => {
  test.setTimeout(180000);
  const stamp = `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
  const tariq = `tariq-${stamp}@example.test`;
  const ownerContext = await browser.newContext({
    acceptDownloads: true,
    viewport: { width: 1440, height: 900 }
  });
  const owner = await ownerContext.newPage();
  await login(owner, 'Olivia Owner', `olivia-${stamp}@example.test`);
  await owner.getByRole('button', { name: 'New document', exact: true }).click();
  await owner.getByLabel('Document name', { exact: true }).fill('Group report');
  await owner.getByRole('button', { name: 'Create document', exact: true }).click();
  const editor = await editable(owner);
  await editor.click();
  await owner.keyboard.type('Introduction to our study.');
  await owner.keyboard.press('Enter');
  await owner.keyboard.type('We interviewed twelve households over three weeks.');
  await owner.keyboard.press('Enter');
  await owner.keyboard.type('Results suggest that trust grows with small favours.');
  await typedAndSaved(owner);
  const documentUrl = owner.url();
  await owner.getByRole('button', { name: 'Share', exact: true }).click();
  await owner.getByLabel('Email address', { exact: true }).fill(tariq);
  await owner.getByRole('button', { name: 'Add person', exact: true }).click();
  await expect(owner.getByText(tariq, { exact: true })).toBeVisible();
  await owner.getByRole('button', { name: 'Close panel' }).click();

  // 1. Download: the file name and hidden metadata record the revision it came from.
  await owner.getByRole('button', { name: 'File', exact: true }).click();
  await shot(owner, '01-file-menu');
  const [download] = await Promise.all([
    owner.waitForEvent('download'),
    owner.getByRole('button', { name: 'Download Word (.docx)' }).click()
  ]);
  expect(download.suggestedFilename()).toMatch(/^Group report \(v\d+\)\.docx$/);
  const original = readFileSync((await download.path())!);

  // 2. Meanwhile the owner keeps editing the team version (paragraph 1).
  await editor.getByText('Introduction to our study.', { exact: true }).click();
  await owner.keyboard.press('End');
  await owner.keyboard.type(' Draft two.');
  await typedAndSaved(owner);

  // 3. Tariq edited his copy of the download in Word: paragraphs 1 and 2, plus a new one.
  const edited = editDocx(original, (xml) => {
    const results = xml.indexOf('Results suggest');
    const close = xml.indexOf('</w:p>', results) + '</w:p>'.length;
    return (
      xml.slice(0, close) +
      '<w:p><w:r><w:t>We will share a summary with every household.</w:t></w:r></w:p>' +
      xml.slice(close)
    )
      .replace('Introduction to our study.', 'Introduction to our neighbourhood study.')
      .replace('twelve households', 'fourteen households');
  });

  // 4. Tariq uploads his edited copy as a change request.
  const tariqContext = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const teammate = await tariqContext.newPage();
  await login(teammate, 'Tariq Rahman', tariq);
  await teammate.goto(documentUrl);
  await teammate.getByRole('button', { name: 'File', exact: true }).click();
  await teammate.getByRole('button', { name: 'Upload edited Word file…' }).click();
  const push = teammate.getByRole('dialog', { name: 'Upload your edited copy' });
  await expect(push).toBeVisible();
  await shot(teammate, '02-upload-dialog');
  await push
    .locator('input[type=file]')
    .setInputFiles({ name: download.suggestedFilename(), mimeType: DOCX, buffer: edited });
  await expect(push.getByText('2 edited')).toBeVisible({ timeout: 20000 });
  await expect(push.getByLabel('This copy started from')).toContainText('The copy you downloaded');
  await expect(push.getByText('1 added')).toBeVisible();
  await expect(push.getByText('0 removed')).toBeVisible();
  await expect(push.getByText(/matched exactly using hidden markers/)).toBeVisible();
  await push.getByLabel('What did you change and why?').fill('Updated numbers after the visit.');
  await shot(teammate, '03-upload-compared');
  await push.getByRole('button', { name: 'Send for review' }).click();
  await expect(push).toHaveCount(0);
  await expect(teammate.getByRole('heading', { name: 'Change requests' })).toBeVisible();
  await expect(teammate.locator('.request-card')).toContainText('Changes from Group report');
  await shot(teammate, '04-request-sent');

  // 5. The owner reviews: one overlap with her own newer edit, the rest combine.
  await owner.getByRole('button', { name: /^Change requests/ }).click();
  await owner.locator('.request-card', { hasText: 'Changes from Group report' }).click();
  const review = owner.getByRole('dialog', { name: 'Changes from Group report' });
  await expect(review.getByText('Overlaps a team change')).toBeVisible({ timeout: 15000 });
  await expect(review.getByText('Updated numbers after the visit.')).toBeVisible();
  await expect(review.locator('ins', { hasText: 'fourteen' })).toBeVisible();
  const merge = review.getByRole('button', { name: /^Merge \d+ selected/ });
  await expect(merge).toBeDisabled();
  await shot(owner, '05-review-overlap');
  await review.getByText(/Use Tariq Rahman.s version/).click();
  await expect(merge).toBeEnabled();
  await merge.click();
  await expect(review.getByText('Merged into the team version')).toBeVisible();
  await shot(owner, '06-merged');
  await review.getByRole('button', { name: 'Close review' }).click();
  await expect(editor).toContainText('Introduction to our neighbourhood study.');
  await expect(editor).toContainText('fourteen households');
  await expect(editor).toContainText('We will share a summary with every household.');
  await expect(owner.locator('.request-card').first()).toContainText('merged');

  // 6. Rebase: Tariq's personal draft falls behind, then replays onto the latest version.
  await teammate.reload();
  await expect(teammate.getByRole('textbox', { name: 'Document content' })).toContainText(
    'fourteen households'
  );
  await teammate.getByRole('button', { name: 'Personal drafts', exact: true }).click();
  await teammate.getByLabel('Draft name').fill('Tariq wording ideas');
  await teammate.getByRole('button', { name: 'Create personal draft', exact: true }).click();
  await expect(teammate.locator('.draft-banner')).toContainText('Tariq wording ideas');
  await teammate.getByRole('button', { name: 'Close panel' }).click();
  const draftEditor = teammate.getByRole('textbox', { name: 'Document content' });
  await expect(draftEditor).toHaveAttribute('contenteditable', 'true');
  await draftEditor.getByText('We will share a summary with every household.').click();
  await teammate.keyboard.press('End');
  await teammate.keyboard.type(' Thanks to all participants.');
  await typedAndSaved(teammate);
  await editor.click();
  await owner.keyboard.press('Control+Home');
  await owner.keyboard.press('ArrowDown');
  await owner.keyboard.press('End');
  await owner.keyboard.type(' Verified in June.');
  await typedAndSaved(owner);
  await expect(teammate.locator('.draft-banner')).toContainText('newer change', {
    timeout: 15000
  });
  await shot(teammate, '07-draft-behind');
  await teammate.getByRole('button', { name: 'Rebase onto latest' }).click();
  const rebase = teammate.getByRole('dialog', { name: 'Rebase onto the latest team version' });
  await expect(rebase.getByText('None of your changes overlap')).toBeVisible();
  await shot(teammate, '08-rebase-dialog');
  await rebase.getByRole('button', { name: 'Rebase draft' }).click();
  await expect(rebase).toHaveCount(0);
  await expect(teammate.locator('.draft-banner')).not.toContainText('newer change');
  await expect(draftEditor).toContainText('Verified in June.');
  await expect(draftEditor).toContainText(
    'We will share a summary with every household. Thanks to all participants.'
  );

  // 7. Tariq asks for review; the owner merges it too, and history verifies.
  await teammate.getByRole('button', { name: 'Personal drafts', exact: true }).click();
  await teammate.getByLabel('Ask your team to review this draft').fill('Closing line.');
  await teammate.getByRole('button', { name: 'Send for review' }).click();
  await expect(teammate.locator('.draft-banner')).toContainText('Sent for review');
  await owner.getByRole('button', { name: /^Change requests/ }).click();
  await owner.getByRole('button', { name: /^Change requests/ }).click();
  await owner.locator('.request-card', { hasText: 'Tariq wording ideas' }).click();
  const second = owner.getByRole('dialog', { name: 'Tariq wording ideas' });
  await second.getByRole('button', { name: /^Merge 1 selected/ }).click();
  await expect(second.getByText('Merged into the team version')).toBeVisible();
  await second.getByRole('button', { name: 'Close review' }).click();
  await expect(editor).toContainText('Thanks to all participants.');
  const documentId = documentUrl.split('/documents/')[1];
  const verification = await owner.request.get(`/api/engine/documents/${documentId}/verify`, {
    headers: { 'x-dynodoc-editor': 'office-v5' }
  });
  expect((await verification.json()).ok).toBe(true);
  await ownerContext.close();
  await tariqContext.close();
});

test('a file without Dynodoc markers is matched by wording against the right revision', async ({
  page
}) => {
  const stamp = `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
  await login(page, 'Mina Marker', `mina-${stamp}@example.test`);
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill('Plain copy');
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  const editor = await editable(page);
  await editor.click();
  for (const line of ['Alpha paragraph stays.', 'Beta paragraph changes.', 'Gamma stays too.']) {
    await page.keyboard.type(line);
    await page.keyboard.press('Enter');
  }
  await typedAndSaved(page);
  await page.getByRole('button', { name: 'File', exact: true }).click();
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('button', { name: 'Download Word (.docx)' }).click()
  ]);
  // Re-saved by another tool: no bookmarks, no metadata, a different file name.
  const parts = unzipSync(new Uint8Array(readFileSync((await download.path())!)));
  delete parts['docProps/custom.xml'];
  parts['word/document.xml'] = strToU8(
    strFromU8(parts['word/document.xml'])
      .replace(/<w:bookmarkStart[^>]*\/>/g, '')
      .replace(/<w:bookmarkEnd[^>]*\/>/g, '')
      .replace('Beta paragraph changes.', 'Beta paragraph changed by email.')
  );
  const plain = Buffer.from(zipSync(parts));
  await page.getByRole('button', { name: 'File', exact: true }).click();
  await page.getByRole('button', { name: 'Upload edited Word file…' }).click();
  const push = page.getByRole('dialog', { name: 'Upload your edited copy' });
  await push
    .locator('input[type=file]')
    .setInputFiles({ name: 'report from email.docx', mimeType: DOCX, buffer: plain });
  await expect(push.getByText('1 edited')).toBeVisible({ timeout: 20000 });
  await expect(push.getByText(/matched by their wording/)).toBeVisible();
  await push.getByRole('button', { name: 'Open as my personal draft' }).click();
  await expect(page.locator('.draft-banner')).toBeVisible();
  await expect(editor).toContainText('Beta paragraph changed by email.');
  await expect(editor).toContainText('Alpha paragraph stays.');
});
