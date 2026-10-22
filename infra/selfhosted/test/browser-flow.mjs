// The browser half of the bundle's smoke test (Chromium through Playwright), driven by smoke.mjs.
// Everything goes through the bundled proxy over HTTPS with the evaluation certificate (so
// certificate errors are ignored on purpose). Email sign-in end to end: the sign-in link is
// requested in the page and read from the test mail sink.

/** Sign in by email, create a document, see it listed. Returns what later steps need. */
export async function signInAndCreate({ playwright, baseUrl, email, title, readMail, step }) {
  const browser = await playwright.chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, baseURL: baseUrl });
  const page = await context.newPage();
  const problems = [];
  page.on('pageerror', (error) => problems.push(`page error: ${error.message}`));

  await page.goto('/login');
  await page.getByLabel('Your name').fill('Smoke tester');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page
    .getByText('Check your inbox. Your sign-in link is valid for 15 minutes.')
    .waitFor({ timeout: 20000 });
  step('the page says a sign-in link was sent');

  let mail = null;
  for (let i = 0; i < 40 && !mail; i++) {
    mail = (await readMail()).find((m) => m.to.includes(email)) ?? null;
    if (!mail) await new Promise((r) => setTimeout(r, 500));
  }
  if (!mail) throw new Error(`no sign-in mail for ${email} reached the sink`);
  // The message is quoted-printable: undo the soft line breaks and =XX escapes before looking.
  const decoded = mail.data.replace(/=\r?\n/g, '').replace(/=([0-9A-F]{2})/g, (_, h) => String.fromCharCode(parseInt(h, 16)));
  const link = /https?:\/\/[^\s"<>]+\/auth\/verify\?token=[A-Za-z0-9_\-.%]+/.exec(decoded)?.[0];
  if (!link) throw new Error('the sign-in mail has no link');
  step(`the mail arrived at the sink, link on ${new URL(link).origin}`, { link: link.replace(/token=.*/, 'token=...') });
  if (new URL(link).origin !== new URL(baseUrl).origin)
    throw new Error(`the link's origin ${new URL(link).origin} is not ${new URL(baseUrl).origin}`);

  await page.goto(link);
  await page.getByText('Your secure sign-in link is ready.').waitFor({ timeout: 15000 });
  await page.getByRole('button', { name: 'Continue to workspace' }).click();
  await page.waitForURL(/\/projects$/, { timeout: 20000 });
  step('signed in; the workspace opened');

  await page.goto('/workspace');
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill(title);
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  await page.getByLabel('Document title').waitFor({ timeout: 20000 });
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.click();
  await page.keyboard.type('Smoke test paragraph that must survive restarts.');
  await page.locator('.save-state').filter({ hasText: /All changes saved|Personal draft saved/ }).waitFor({ timeout: 20000 });
  step('a document was created and saved');

  await page.goto('/workspace');
  await page.getByText(title).first().waitFor({ timeout: 20000 });
  step('the document is listed in the workspace');
  return { browser, context, page, problems };
}

/** The document is still listed (and its text still there) after something was restarted. */
export async function documentStillThere({ page, title, text }) {
  await page.goto('/workspace');
  await page.getByText(title).first().waitFor({ timeout: 30000 });
  await page.getByText(title).first().click();
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.waitFor({ timeout: 20000 });
  await page.getByText(text).first().waitFor({ timeout: 20000 });
}
