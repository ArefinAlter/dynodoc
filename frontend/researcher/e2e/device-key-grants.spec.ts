import { expect, test, type Page } from '@playwright/test';
import { generateKeyPairSync, createPublicKey, sign } from 'node:crypto';
test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in development stack.');

async function login(page: Page, email: string) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Grant tester');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
}
async function api(page: Page, path: string, body?: unknown) {
  return page.evaluate(
    async ({ path, body }) => {
      const r = await fetch('/api/engine' + path, {
        method: body === undefined ? 'GET' : 'POST',
        headers: { 'content-type': 'application/json', 'x-dynodoc-editor': 'office-v5' },
        body: body === undefined ? undefined : JSON.stringify(body)
      });
      const data = await r.json();
      if (!r.ok) throw new Error(JSON.stringify(data));
      return data;
    },
    { path, body }
  );
}

test('A new signing key reaches no project until it is granted one, and a grant can be revoked', async ({
  page
}) => {
  test.setTimeout(90000);
  const stamp = Date.now(),
    projectName = `Grant target ${stamp}`;
  await login(page, `device-grants-${stamp}@example.test`);
  const project = await api(page, '/projects', { name: projectName });
  await page.goto('/profile');
  const section = page.getByRole('region', { name: 'Signing keys' });
  await expect(section.getByText('No signing keys connected.')).toBeVisible();

  // Enrol a synthetic key; the private half never leaves this test process.
  await section.getByText('Connect a local signing key', { exact: true }).click();
  const { privateKey: key } = generateKeyPairSync('ed25519');
  const publicKey = createPublicKey(key)
    .export({ format: 'der', type: 'spki' })
    .subarray(-32)
    .toString('hex');
  await section.getByLabel('Public key', { exact: true }).fill(publicKey);
  const response = page.waitForResponse(
    (r) => r.url().endsWith('/api/engine/devices/challenge') && r.request().method() === 'POST'
  );
  await section.getByRole('button', { name: 'Create enrollment challenge' }).click();
  const challenge = await (await response).json();
  const canonical = JSON.stringify(
    Object.fromEntries(Object.entries(challenge).sort(([a], [b]) => (a < b ? -1 : 1)))
  );
  const signature = sign(
    null,
    Buffer.concat([Buffer.from('dynodoc.device-enrollment\0v1\0'), Buffer.from(canonical)]),
    key
  ).toString('hex');
  await section.getByLabel('Enrollment proof JSON').setInputFiles({
    name: 'device-proof.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify({ challenge, signature }))
  });

  // The default mode is "selected projects", so the key starts with nothing to open.
  await expect(section.getByText('Only projects you grant')).toBeVisible();
  await expect(
    section.getByText("This key can't open any project yet. Grant it a project.")
  ).toBeVisible();
  expect((await api(page, '/devices')).keys[0].project_access).toBe('granted');

  // Grant the project with the default permissions (Read history, Write history).
  await section.getByRole('button', { name: /^Grant a project to key/ }).click();
  const picker = section.getByLabel(/^Project to grant to key/);
  await expect(picker).toBeEnabled();
  await picker.selectOption(project.id);
  await section.getByRole('button', { name: 'Save grant' }).click();
  await expect(section.getByRole('link', { name: projectName, exact: true })).toBeVisible();
  await expect(section.getByText('1 of 50 projects granted')).toBeVisible();
  const granted = (await api(page, '/devices')).keys[0];
  expect(granted.grants).toHaveLength(1);
  expect(granted.grants[0].project_id).toBe(project.id);
  expect(granted.grants[0].scopes).toEqual(['history:read', 'history:write']);

  // Switching to every project needs a second, explicit step.
  await section.getByRole('button', { name: /^Allow all projects for key/ }).click();
  await expect(section.getByText('Allow every project you can access?')).toBeVisible();
  expect((await api(page, '/devices')).keys[0].project_access).toBe('granted');
  await section.getByRole('button', { name: 'Allow all projects', exact: true }).click();
  await expect(section.getByText('All projects you can access')).toBeVisible();
  expect((await api(page, '/devices')).keys[0].project_access).toBe('all');
  await section.getByRole('button', { name: /^Use only granted projects for key/ }).click();
  await expect(section.getByText('Only projects you grant')).toBeVisible();
  // Grants survive a round trip through the broad mode.
  expect((await api(page, '/devices')).keys[0].grants).toHaveLength(1);

  // Revoke the grant; the key can open nothing again.
  await section.getByRole('button', { name: /^Revoke grant for .* on key/ }).click();
  await expect(section.getByText("This key can't open any project yet.")).toBeVisible();
  await expect(section.getByRole('link', { name: projectName, exact: true })).toHaveCount(0);
  const cleared = (await api(page, '/devices')).keys[0];
  expect(cleared.grants).toEqual([]);
  expect(cleared.project_access).toBe('granted');
});

test('The command palette opens the signing keys section', async ({ page }) => {
  await login(page, `device-palette-${Date.now()}@example.test`);
  await page.keyboard.press('Control+k');
  await page.getByRole('combobox', { name: /Search projects, files and actions/ }).fill('signing');
  await page.getByRole('option', { name: /Manage signing keys/ }).click();
  await expect(page).toHaveURL(/\/profile#signing-keys$/);
  await expect(page.getByRole('region', { name: 'Signing keys' })).toBeVisible();
});
