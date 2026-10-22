import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { startFakeSmtp, type FakeSmtp } from '../src/lib/server/mail/test-helpers/fake-smtp';
import { readMessage } from '../src/lib/server/mail/test-helpers/mime';

// Runs against a self-hosted server that sends its mail through SMTP (settings in
// e2e/selfhosted-smtp.env, on top of selfhosted.env). This file starts the mail server it talks
// to, in this process, on the port the web server was given. The tests need the API and a
// signed-in stack, like the other self-hosted specs; they never use DEV_AUTH.
test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in development stack.');

const settings = Object.fromEntries(
  readFileSync(new URL('./selfhosted-smtp.env', import.meta.url), 'utf8')
    .split(/\r?\n/)
    .filter((line) => line.trim() && !line.startsWith('#'))
    .map((line) => [line.slice(0, line.indexOf('=')), line.slice(line.indexOf('=') + 1)])
);
const brand = readFileSync(new URL('./selfhosted.env', import.meta.url), 'utf8').match(
  /^DYNODOC_BRAND_NAME=(.*)$/m
)![1];

let smtp: FakeSmtp;
let refuseNext = false;
test.beforeAll(async () => {
  smtp = await startFakeSmtp({
    port: Number(settings.SMTP_PORT),
    authMethods: ['PLAIN', 'LOGIN'],
    credentials: { user: settings.SMTP_USER, pass: settings.SMTP_PASSWORD },
    requireAuth: true,
    reply: { rcpt: () => (refuseNext ? '550 5.1.1 No such mailbox here' : undefined) }
  });
});
test.afterAll(() => smtp.close());

/** The message sent to this address, once it has arrived. */
async function received(address: string) {
  await expect
    .poll(() => smtp.messages.filter((message) => message.to.includes(address)).length, {
      message: `a message for ${address}`,
      timeout: 15000
    })
    .toBe(1);
  return smtp.messages.find((message) => message.to.includes(address))!;
}
async function requestLink(page: Page, email: string) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('SMTP tester');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
}

test('the sign-in page offers email, with no development shortcut', async ({ page, request }) => {
  await page.goto('/login');
  await expect(page.getByLabel('Work email')).toBeVisible();
  await expect(page.getByText('Email sign-in is awaiting administrator setup.')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Email me a sign-in link' })).toBeEnabled();
  expect(await (await request.get('/auth/options')).json()).toMatchObject({
    email: true,
    development: false
  });
});

test('a sign-in link arrives by SMTP, on the configured origin, and signs in once', async ({
  page,
  request,
  baseURL
}) => {
  const origin = new URL(baseURL!).origin;
  const email = `smtp-signin-${Date.now()}@example.test`;
  await requestLink(page, email);
  await expect(
    page.getByText('Check your inbox. Your sign-in link is valid for 15 minutes.')
  ).toBeVisible();
  // No shortcut that skips the mail.
  await expect(page.getByRole('button', { name: /Continue in local development/ })).toHaveCount(0);

  const message = await received(email);
  expect(message.authUser).toBe(settings.SMTP_USER);
  expect(message.from).toBe(settings.SMTP_FROM);
  const mail = readMessage(message.data);
  expect(mail.subject).toBe(`Sign in to ${brand}`);
  expect(mail.headers.get('from')).toBe(`${settings.SMTP_FROM_NAME} <${settings.SMTP_FROM}>`);
  expect(mail.headers.get('message-id')).toMatch(/^<[0-9a-f]{32}@acme\.example\.test>$/);
  const link = /https?:\/\/\S+\/auth\/verify\?token=\S+/.exec(mail.text)?.[0] ?? '';
  expect(link.startsWith(`${origin}/auth/verify?token=`), link).toBe(true);
  expect(mail.html).toContain(`href="${link}"`);
  expect(`${mail.text}${mail.html}`).not.toMatch(/dynodoc\.online/);
  expect(mail.html).not.toMatch(/<img|src=|url\(/i);

  // The emailed link leads to a confirmation page, and signing in works.
  await page.goto(link);
  await expect(page.getByText('Your secure sign-in link is ready.')).toBeVisible();
  await page.getByRole('button', { name: 'Continue to workspace' }).click();
  await expect(page).toHaveURL(/\/projects$/);

  // A link works once.
  const token = new URL(link).searchParams.get('token');
  const again = await request.post('/auth/verify', {
    headers: { origin, 'content-type': 'application/json' },
    data: { token }
  });
  expect(again.status()).toBe(401);
});

test('when the mail server refuses, the page says so in plain words and nothing is leaked', async ({
  page
}) => {
  refuseNext = true;
  try {
    await requestLink(page, `smtp-refused-${Date.now()}@example.test`);
    await expect(
      page.getByText(
        'We could not send your sign-in link. Please wait a few minutes and try again.'
      )
    ).toBeVisible();
    const text = (await page.content()).toLowerCase();
    for (const secret of [settings.SMTP_PASSWORD, 'no such mailbox', settings.SMTP_USER])
      expect(text).not.toContain(secret.toLowerCase());
  } finally {
    refuseNext = false;
  }
});

test('an invitation is emailed from the project settings, with a link on the configured origin', async ({
  page,
  baseURL
}) => {
  const origin = new URL(baseURL!).origin;
  const owner = `smtp-owner-${Date.now()}@example.test`;
  await requestLink(page, owner);
  const link = /\S+\/auth\/verify\?token=\S+/.exec(
    readMessage((await received(owner)).data).text
  )![0];
  await page.goto(link);
  await page.getByRole('button', { name: 'Continue to workspace' }).click();
  await expect(page).toHaveURL(/\/projects$/);

  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByLabel('Project name').fill(`Field <b>team</b> ${Date.now()}`);
  await page.getByRole('button', { name: 'Create project' }).click();
  await expect(page.getByRole('button', { name: 'Files', exact: true })).toBeVisible();
  const projectId = page.url().split('/').pop();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const guest = `smtp-guest-${Date.now()}@example.test`;
  await page.getByLabel('Email', { exact: true }).fill(guest);
  await page.getByRole('button', { name: 'Add person' }).click();
  await expect(page.getByRole('status')).toContainText(
    `Access updated. We emailed ${guest} an invitation with a link.`
  );

  const mail = readMessage((await received(guest)).data);
  expect(mail.headers.get('reply-to')).toBe(owner);
  expect(mail.subject).toContain(`with you on ${brand}`);
  expect(mail.text).toContain(`Open it: ${origin}/projects/${projectId}`);
  expect(mail.html).toContain(`href="${origin}/projects/${projectId}"`);
  // The project name is text in the plain part and escaped in the HTML part.
  expect(mail.html).toContain('Field &lt;b&gt;team&lt;/b&gt;');
  expect(mail.html).not.toContain('<b>team</b>');
  expect(`${mail.subject}${mail.text}${mail.html}`).not.toMatch(/dynodoc/i);
  // No Google here, so the message does not offer it.
  expect(mail.text).toContain(`(${guest}): ask for an email sign-in link.`);
  expect(mail.text).not.toContain('Google');
});
