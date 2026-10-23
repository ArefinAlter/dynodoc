import { expect, test } from '@playwright/test';
import { spawnSync } from 'node:child_process';

// Runs against a self-hosted server started with AUTH_EMAIL_LINK=off (see selfhosted.spec.ts
// for the rest of its settings): email sign-in is not offered and the route refuses.
test('the sign-in page does not offer email, and says what is missing', async ({ page }) => {
  await page.goto('/login');
  await expect(page.getByRole('heading', { name: /Welcome to your workspace/ })).toBeVisible();
  await expect(page.getByLabel('Work email')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Email me a sign-in link' })).toHaveCount(0);
  await expect(page.getByText('or use your email')).toHaveCount(0);
  await expect(
    page.getByText('No sign-in method is turned on. Ask your administrator.')
  ).toBeVisible();
});

test('the options and the email route agree: off', async ({ request, baseURL }) => {
  const options = await (await request.get('/auth/options')).json();
  expect(options).toMatchObject({ email: false, development: false });
  const response = await request.post('/auth/email', {
    headers: { origin: new URL(baseURL!).origin, 'content-type': 'application/json' },
    data: { email: 'olivia@example.test' }
  });
  expect(response.status()).toBe(403);
  expect((await response.json()).error).toMatch(/turned off/);
});

test('the native-app sign-in page says there is no way to sign in, instead of sending the person to a page that cannot help', async ({
  page
}) => {
  const challenge = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';
  const state = 'b3JkZXJseS1zdGF0ZS12YWx1ZQ';
  await page.goto(`/app-signin?challenge=${challenge}&state=${state}&platform=android`);
  await expect(page).toHaveURL(/\/app-signin\?/);
  await expect(page.getByRole('heading', { name: 'No sign-in method is turned on' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Continue' })).toHaveCount(0);
});

// --- operator-issued sign-in links (docs/decisions/018-operator-issued-sign-in-links.md) --------
// With email sign-in off nobody can ask for a link; the first administrator comes in with a link the
// operator makes with `engine-cli auth issue-link` and the database OWNER credential. These need the
// isolated stack (the API, a disposable database whose owner URL is MIGRATION_DATABASE_URL) and the
// built engine-cli (ENGINE_CLI, default target/debug/engine-cli at the repository root). The API's
// ADMIN_EMAILS names admin@example.test, as ci-frontend.yml sets it.
const owner = process.env.MIGRATION_DATABASE_URL;
const engineCli = process.env.ENGINE_CLI ?? '../../target/debug/engine-cli';
const operatorReady = process.env.PILOT_E2E === 'true' && Boolean(owner);

function issueLink(origin: string, email: string, extra: string[] = []) {
  const run = spawnSync(
    engineCli,
    [
      'auth',
      'issue-link',
      '--email',
      email,
      '--origin',
      origin,
      '--admin-emails',
      'admin@example.test',
      '--operator',
      'e2e',
      ...extra
    ],
    // The owner credential is given to the tool alone; the logs stay off.
    { env: { ...process.env, MIGRATION_DATABASE_URL: owner, RUST_LOG: 'off' }, encoding: 'utf8' }
  );
  return { status: run.status, link: run.stdout.trim(), stderr: run.stderr };
}

test.describe('operator-issued sign-in link', () => {
  test.skip(!operatorReady, 'Requires the isolated stack and MIGRATION_DATABASE_URL.');

  test('signs the first administrator in with email sign-in off; a replay is refused', async ({
    page,
    browser,
    baseURL
  }) => {
    const origin = new URL(baseURL!).origin;
    // The public request is refused while the operator's link works.
    const refused = await page.request.post('/auth/email', {
      headers: { origin, 'content-type': 'application/json' },
      data: { email: 'admin@example.test' }
    });
    expect(refused.status()).toBe(403);

    const made = issueLink(origin, 'admin@example.test', ['--create', '--role', 'admin']);
    expect(made.status, made.stderr).toBe(0);
    expect(made.link).toMatch(new RegExp(`^${origin}/login\\?token=[0-9a-f]{64}$`));
    expect(made.stderr).not.toContain(new URL(made.link).searchParams.get('token')!);

    await page.goto(made.link);
    await expect(page.getByText('Your secure sign-in link is ready.')).toBeVisible();
    // The page asks for one click before it uses the link (a mail scanner or a preview cannot).
    expect((await page.context().cookies()).some((c) => c.name === 'dynodoc_session')).toBe(false);
    await page.getByRole('button', { name: /Continue to workspace/ }).click();
    await expect(page).toHaveURL(/\/projects/);
    expect((await page.context().cookies()).some((c) => c.name === 'dynodoc_session')).toBe(true);
    const session = await page.request.get('/auth/session');
    expect((await session.json()).authenticated).toBe(true);
    // Administrator because the address is in ADMIN_EMAILS (the link made nobody one).
    expect((await page.request.get('/api/engine/admin/overview')).status()).toBe(200);

    // The native-app sign-in (decision 017) now works from this browser, as after any sign-in.
    const challenge = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';
    const state = 'b3JkZXJseS1zdGF0ZS12YWx1ZQ';
    await page.goto(`/app-signin?challenge=${challenge}&state=${state}&platform=android`);
    await expect(page.getByRole('heading', { name: 'Sign in to the Dynodoc app' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Continue' })).toBeVisible();

    // The same link in another browser: used up, refused, and the words do not offer a request.
    const other = await browser.newContext();
    const second = await other.newPage();
    await second.goto(made.link);
    await second.getByRole('button', { name: /Continue to workspace/ }).click();
    await expect(second.getByRole('alert')).toContainText('Ask your administrator for a new one');
    expect((await other.cookies()).some((c) => c.name === 'dynodoc_session')).toBe(false);
    // The failure is shown, and the link is no longer in the address: not in the history, and a
    // reload sends nothing (it shows the ordinary sign-in page).
    expect(second.url()).not.toContain('token');
    const reSent: string[] = [];
    second.on('request', (request) => {
      if (request.url().includes('/auth/verify')) reSent.push(request.url());
    });
    await second.reload();
    await expect(second.getByText('No sign-in method is turned on')).toBeVisible();
    await expect(second.getByText('Your secure sign-in link is ready.')).toHaveCount(0);
    expect(reSent).toEqual([]);
    expect(second.url()).not.toContain('token');
    await other.close();
  });

  test('an ordinary account signs in with its link and is not an administrator', async ({
    page,
    baseURL
  }) => {
    const origin = new URL(baseURL!).origin;
    const made = issueLink(origin, 'plain.person@example.test', [
      '--create',
      '--confirm-create',
      'plain.person@example.test',
      '--role',
      'user'
    ]);
    expect(made.status, made.stderr).toBe(0);
    await page.goto(made.link);
    await page.getByRole('button', { name: /Continue to workspace/ }).click();
    await expect(page).toHaveURL(/\/projects/);
    // The administration overview is refused for an address that is not in ADMIN_EMAILS.
    expect((await page.request.get('/api/engine/admin/overview')).status()).toBe(403);
  });

  test('an expired link is refused', async ({ page, baseURL }) => {
    test.setTimeout(150_000);
    const origin = new URL(baseURL!).origin;
    const made = issueLink(origin, 'late.person@example.test', [
      '--create',
      '--confirm-create',
      'late.person@example.test',
      '--ttl',
      '60s'
    ]);
    expect(made.status, made.stderr).toBe(0);
    await page.goto(made.link);
    // The shortest life there is; the database clock decides, so wait it out.
    await page.waitForTimeout(62_000);
    await page.getByRole('button', { name: /Continue to workspace/ }).click();
    await expect(page.getByRole('alert')).toContainText('Ask your administrator for a new one');
    expect((await page.context().cookies()).some((c) => c.name === 'dynodoc_session')).toBe(false);
    // The link left the address when the request was made, expired or not.
    expect(page.url()).not.toContain('token');
  });
});
