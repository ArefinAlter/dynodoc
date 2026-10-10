import { expect, test, type Page } from '@playwright/test';
import { strFromU8, strToU8, unzipSync, zipSync } from 'fflate';
import { readFileSync } from 'node:fs';
// Requires a local API and DEV_AUTH=true web server. Never enables a public login bypass.
test.skip(
  process.env.PILOT_E2E !== 'true',
  'Set PILOT_E2E=true with a local development API to run the complete workflows.'
);
// Optional: WALKTHROUGH_SHOTS=<folder> saves screenshots used in the user guide, in
// day and night mode.
const shots = process.env.WALKTHROUGH_SHOTS;
async function shot(page: Page, name: string) {
  if (!shots) return;
  await page.screenshot({ path: `${shots}/${name}-day.png` });
  await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'dark'));
  await page.screenshot({ path: `${shots}/${name}-night.png` });
  await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'light'));
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
const DOCX = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
/** A Word file edited elsewhere: Dynodoc's hidden metadata and markers removed. */
function stripped(bytes: Buffer, edit: (xml: string) => string = (x) => x) {
  const parts = unzipSync(new Uint8Array(bytes));
  delete parts['docProps/custom.xml'];
  parts['word/document.xml'] = strToU8(
    edit(strFromU8(parts['word/document.xml']).replace(/<w:bookmark(Start|End)[^>]*\/>/g, ''))
  );
  return Buffer.from(zipSync(parts));
}
/** The same package with completely different paragraphs. */
function different(bytes: Buffer, paragraphs: string[]) {
  return stripped(bytes, (xml) => {
    const start = xml.indexOf('<w:body>') + '<w:body>'.length;
    const section = xml.lastIndexOf('<w:sectPr');
    return (
      xml.slice(0, start) +
      paragraphs.map((p) => `<w:p><w:r><w:t>${p}</w:t></w:r></w:p>`).join('') +
      xml.slice(section)
    );
  });
}
async function openBell(page: Page) {
  // The bell polls every minute; reopening the page asks at once.
  await expect(async () => {
    await page.reload();
    await page.getByRole('button', { name: /^Notifications, \d+ unread/ }).click({ timeout: 3000 });
  }).toPass({ timeout: 45000, intervals: [2000, 3000, 5000] });
  return page.getByRole('dialog', { name: 'Notifications' });
}

const REPORT = [
  'The survey reached 1,240 households across four districts between March and May.',
  'Response rates were highest in Sylhet, where enumerators returned for a second visit.',
  'Household size averaged 4.6 people, and 38 percent of households kept livestock.',
  'Access to clean water improved compared with the previous round of data collection.',
  'We recommend a follow-up study focusing on seasonal migration and school attendance.'
];

test('a renamed copy reaches its owner, who invites the uploader, reviews and merges', async ({
  browser
}) => {
  test.setTimeout(300000);
  const stamp = `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
  const ownerContext = await browser.newContext({
    acceptDownloads: true,
    viewport: { width: 1440, height: 900 }
  });
  const owner = await ownerContext.newPage();
  owner.on('dialog', (d) => void d.accept());
  await login(owner, 'Olivia Owner', `olivia-${stamp}@example.test`);
  await owner.getByRole('button', { name: 'New document', exact: true }).click();
  await owner.getByLabel('Document name', { exact: true }).fill('Group report');
  await owner.getByRole('button', { name: 'Create document', exact: true }).click();
  const editor = owner.getByRole('textbox', { name: 'Document content' });
  await expect(editor).toHaveAttribute('contenteditable', 'true');
  await editor.click();
  // A study code keeps each run's content distinct from earlier runs on the same database.
  for (const [i, paragraph] of [`Study ${stamp} field report.`, ...REPORT].entries()) {
    if (i) await owner.keyboard.press('Enter');
    await owner.keyboard.type(paragraph);
  }
  await saved(owner);
  const documentUrl = owner.url();
  await owner.getByRole('button', { name: 'File', exact: true }).click();
  const [download] = await Promise.all([
    owner.waitForEvent('download'),
    owner.getByRole('button', { name: 'Download Word (.docx)' }).click()
  ]);
  const original = readFileSync((await download.path())!);

  // 1. Tariq, who has no access, uploads a renamed and edited copy from his email.
  const tariqEmail = `tariq-${stamp}@example.test`;
  const tariqContext = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const tariq = await tariqContext.newPage();
  await login(tariq, 'Tariq Rahman', tariqEmail);
  const copy = stripped(original, (xml) =>
    xml.replace('four districts', 'five districts').replace('second visit', 'third visit')
  );
  await tariq.locator('input[type=file]').setInputFiles({
    name: 'report - Tariq.docx',
    mimeType: DOCX,
    buffer: copy
  });
  const importDialog = tariq.getByRole('dialog');
  await expect(importDialog.getByText(/compared with existing documents by content/)).toBeVisible();
  // He can't see the original, so nothing about it is revealed to him.
  await expect(importDialog.getByText('Checking for documents like this file…')).toHaveCount(0, {
    timeout: 15000
  });
  await expect(importDialog.locator('.existing-copy')).toHaveCount(0);
  await shot(tariq, '10-upload-privacy-note');
  await tariq.getByRole('button', { name: 'Import into workspace', exact: true }).click();
  await expect(tariq.getByRole('textbox', { name: 'Document content' })).toContainText(
    'five districts',
    { timeout: 30000 }
  );

  // 2. The owner is told who uploaded which file and how similar it is.
  let inbox = await openBell(owner);
  const notice = inbox.locator('li', { hasText: 'report - Tariq.docx' });
  await expect(notice).toContainText(/Tariq Rahman uploaded .report - Tariq\.docx., which is/);
  await expect(notice).toContainText(/\d+% match/);
  await shot(owner, '11-owner-notified');
  await notice.getByRole('button', { name: 'Invite them to send changes' }).click();
  await expect(notice).toContainText('Invited to send changes');

  // 3. Tariq sends his copy to the original as a change request.
  inbox = await openBell(tariq);
  await shot(tariq, '12-invitation');
  await inbox.getByRole('button', { name: 'Send my copy as changes' }).click();
  await expect(tariq).toHaveURL(new RegExp(documentUrl.split('/').at(-1)!));
  const push = tariq.getByRole('dialog', { name: 'Upload your edited copy' });
  await expect(push.getByText('2 edited')).toBeVisible({ timeout: 30000 });
  await shot(tariq, '13-copy-compared');
  await push.getByRole('button', { name: 'Send for review' }).click();
  await expect(push).toHaveCount(0);

  // 4. The owner requires an approval; as owner she may still merge, which is audited.
  await owner.goto(documentUrl);
  await owner.getByRole('button', { name: 'Share', exact: true }).click();
  await expect(owner.getByText('You are Owner')).toBeVisible();
  await owner.getByLabel('Approvals needed before merging').selectOption('1');
  await owner.getByRole('button', { name: 'Save rules', exact: true }).click();
  await expect(owner.getByText('Review rules saved.')).toBeVisible();
  await expect(owner.getByText(/Tariq Rahman uploaded/)).toBeVisible();
  await shot(owner, '14-people-and-rules');
  await owner.getByRole('button', { name: 'Close panel' }).click();
  await owner.getByRole('button', { name: /^Change requests/ }).click();
  await owner.locator('.request-card').first().click();
  const review = owner.getByRole('dialog').filter({ has: owner.locator('#review-title') });
  await expect(review.getByText('0 of 1 required approval')).toBeVisible({ timeout: 15000 });
  const merge = review.getByRole('button', { name: /Merge without approvals \d+ selected/ });
  await expect(merge).toBeEnabled();
  await shot(owner, '15-review-rules');
  await merge.click();
  await expect(review.getByText('Merged into the team version')).toBeVisible();
  await review.getByRole('button', { name: 'Close review' }).click();
  await expect(editor).toContainText('five districts');
  await owner.getByLabel('Approvals needed before merging').count();

  // 5. A different file under the same name: every replaced paragraph needs a choice.
  await tariq.goto('/workspace');
  await tariq.locator('input[type=file]').setInputFiles({
    name: 'Group report.docx',
    mimeType: DOCX,
    buffer: different(original, [
      'Quarterly budget for the regional office, including travel and equipment.',
      'Staff costs rose by nine percent because two field officers were hired.',
      'Printing and translation costs were lower than planned this quarter.'
    ])
  });
  const match = tariq.getByRole('dialog').locator('.existing-copy');
  await expect(match).toContainText('already exists, but its content is different', {
    timeout: 15000
  });
  await shot(tariq, '16-same-name');
  await match.getByRole('button', { name: 'Upload as changes' }).first().click();
  const secondPush = tariq.getByRole('dialog', { name: 'Upload your edited copy' });
  await expect(secondPush.getByText(/shares only \d+% of its/)).toBeVisible({ timeout: 30000 });
  await secondPush.getByRole('button', { name: 'Send for review' }).click();
  await expect(secondPush).toHaveCount(0);

  await owner.reload();
  await owner.getByRole('button', { name: /^Change requests/ }).click();
  await owner.locator('.request-card', { hasText: 'open' }).first().click();
  const second = owner.getByRole('dialog').filter({ has: owner.locator('#review-title') });
  await expect(second.getByText('This file shares little with this document')).toBeVisible({
    timeout: 15000
  });
  await expect(second.getByText('Replaces a team paragraph').first()).toBeVisible();
  await shot(owner, '17-unrelated-review');
  await second.getByRole('button', { name: 'Keep all team paragraphs' }).click();
  await second.getByRole('button', { name: /Merge without approvals \d+ selected/ }).click();
  await expect(second.getByText(/Merged|Merged into the team version/).first()).toBeVisible();
  await second.getByRole('button', { name: 'Close review' }).click();
  // The team's paragraphs stay; the new material is appended.
  await expect(editor).toContainText('five districts');
  await expect(editor).toContainText('Quarterly budget for the regional office');
  await ownerContext.close();
  await tariqContext.close();
});

test('a protected team version sends editors to drafts', async ({ browser }) => {
  test.setTimeout(120000);
  const stamp = `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
  const ownerContext = await browser.newContext();
  const owner = await ownerContext.newPage();
  await login(owner, 'Olivia Owner', `olivia-p-${stamp}@example.test`);
  await owner.getByRole('button', { name: 'New document', exact: true }).click();
  await owner.getByLabel('Document name', { exact: true }).fill('Protected protocol');
  await owner.getByRole('button', { name: 'Create document', exact: true }).click();
  const editor = owner.getByRole('textbox', { name: 'Document content' });
  await editor.click();
  await owner.keyboard.type('Only reviewed changes reach this protocol.');
  await saved(owner);
  const url = owner.url();
  const editorEmail = `editor-p-${stamp}@example.test`;
  await owner.getByRole('button', { name: 'Share', exact: true }).click();
  await owner.getByLabel('Email address', { exact: true }).fill(editorEmail);
  await owner.getByLabel('Role', { exact: true }).selectOption('editor');
  await owner.getByRole('button', { name: 'Add person', exact: true }).click();
  await expect(owner.getByText(editorEmail, { exact: true })).toBeVisible();
  await owner.getByText('Protect the team version').click();
  await owner.getByRole('button', { name: 'Save rules', exact: true }).click();
  await expect(owner.getByText('Review rules saved.')).toBeVisible();

  const editorContext = await browser.newContext();
  const colleague = await editorContext.newPage();
  await login(colleague, 'Eli Editor', editorEmail);
  await colleague.goto(url);
  await expect(colleague.getByText('The team version is protected.')).toBeVisible();
  await expect(colleague.getByRole('textbox', { name: 'Document content' })).toHaveAttribute(
    'contenteditable',
    'false'
  );
  await colleague.getByRole('button', { name: 'Start a draft', exact: true }).click();
  await expect(colleague.locator('.draft-banner')).toContainText('My working draft');
  await expect(colleague.getByRole('textbox', { name: 'Document content' })).toHaveAttribute(
    'contenteditable',
    'true'
  );
  await ownerContext.close();
  await editorContext.close();
});
