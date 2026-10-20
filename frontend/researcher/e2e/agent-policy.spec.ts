import { expect, test, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { generateKeyPairSync, createPublicKey, sign } from 'node:crypto';
test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in development stack.');
test.use({ actionTimeout: 15000 });

async function login(page: Page, email: string) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Agent tester');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
}
async function call(
  page: Page,
  method: string,
  path: string,
  body?: unknown,
  headers: Record<string, string> = {}
) {
  return page.evaluate(
    async ({ method, path, body, headers }) => {
      const r = await fetch('/api/engine' + path, {
        method,
        headers: {
          'x-dynodoc-editor': 'office-v5',
          ...(body === undefined ? {} : { 'content-type': 'application/json' }),
          ...headers
        },
        body: body === undefined ? undefined : JSON.stringify(body)
      });
      const data = await r.json().catch(() => null);
      if (!r.ok) throw new Error(JSON.stringify({ status: r.status, data }));
      return data;
    },
    { method, path, body, headers }
  );
}
const api = (page: Page, path: string, body?: unknown) =>
  call(page, body === undefined ? 'GET' : 'POST', path, body);

test('An owner turns AI agents on, sees a clash with another save, and stops all agents', async ({
  page
}) => {
  test.setTimeout(120000);
  const stamp = Date.now();
  await login(page, `agent-owner-${stamp}@example.test`);
  const project = await api(page, '/projects', { name: `Agent target ${stamp}` });
  await page.goto(`/projects/${project.id}?tab=settings`);
  const section = page.getByRole('region', { name: 'AI agents' });
  await expect(section.getByRole('switch')).toBeVisible();

  // The page says honestly whether this installation serves agents at all.
  const policy = await api(page, `/projects/${project.id}/agent-policy`);
  expect(policy.enabled).toBe(false);
  await expect(
    section.getByText(
      policy.gateway
        ? 'AI agents are turned on in this installation.'
        : 'AI agents are not available on this installation yet.'
    )
  ).toBeVisible();
  await expect(section.getByText('No AI agent activity in the last 7 days.')).toBeVisible();
  expect((await new AxeBuilder({ page }).include('#ai-agents').analyze()).violations).toEqual([]);

  // Turn agents on and tighten the rules; the whole policy is saved with its revision.
  await section.getByRole('switch').check();
  await section.getByLabel('Lowest role that can use an AI agent').selectOption('editor');
  await section.getByLabel(/^Reads per minute/).fill('30');
  await section.getByLabel(/^Data read per day/).fill('5');
  await section.getByLabel('Spreadsheets').uncheck();
  await section.getByRole('button', { name: 'Save AI agent settings' }).click();
  await expect(section.locator('.repo-notice')).toContainText('Saved. AI agents are on');
  let saved = await api(page, `/projects/${project.id}/agent-policy`);
  expect(saved).toMatchObject({
    enabled: true,
    min_role: 'editor',
    kinds: ['document', 'presentation'],
    calls_per_minute: 30,
    read_bytes_per_day: 5_000_000,
    revision: 1
  });

  // Someone else saves first: this page reloads and says so instead of overwriting.
  await call(
    page,
    'PUT',
    `/projects/${project.id}/agent-policy`,
    { enabled: true, min_role: 'manager' },
    { 'if-match': '"1"' }
  );
  await section.getByLabel(/^Reads per minute/).fill('40');
  await section.getByRole('button', { name: 'Save AI agent settings' }).click();
  await expect(section.getByRole('alert')).toContainText('Someone else changed these settings');
  await expect(section.getByLabel('Lowest role that can use an AI agent')).toHaveValue('manager');
  await expect(section.getByLabel(/^Reads per minute/)).toHaveValue('30');
  saved = await api(page, `/projects/${project.id}/agent-policy`);
  expect(saved).toMatchObject({ min_role: 'manager', calls_per_minute: 30, revision: 2 });

  // The stop button explains itself, can be cancelled, and then does what it said.
  await section.getByRole('button', { name: 'Stop all AI agents now' }).click();
  const confirm = section.getByRole('group', { name: 'Stop all AI agents in this project now?' });
  await expect(confirm).toContainText('Leaves people’s own keys untouched.');
  await expect(confirm).toContainText('does not bring the old access back');
  await confirm.getByRole('button', { name: 'Cancel' }).click();
  expect((await api(page, `/projects/${project.id}/agent-policy`)).enabled).toBe(true);
  await section.getByRole('button', { name: 'Stop all AI agents now' }).click();
  await confirm.getByRole('button', { name: 'Yes, stop all AI agents' }).click();
  await expect(section.locator('.repo-notice')).toContainText(
    'AI agents are off for this project.'
  );
  await expect(section.locator('.repo-notice')).toContainText('People’s own keys are untouched.');
  await expect(section.getByRole('switch')).not.toBeChecked();
  saved = await api(page, `/projects/${project.id}/agent-policy`);
  expect(saved.enabled).toBe(false);
  expect(saved.killed_at).toBeTruthy();
});

test('Only an owner or manager sees the AI agent settings', async ({ browser }) => {
  test.setTimeout(120000);
  const ownerContext = await browser.newContext(),
    editorContext = await browser.newContext(),
    stamp = Date.now();
  try {
    const owner = await ownerContext.newPage(),
      editor = await editorContext.newPage();
    await login(owner, `agent-gate-owner-${stamp}@example.test`);
    const project = await api(owner, '/projects', { name: `Agent gate ${stamp}` });
    const email = `agent-gate-editor-${stamp}@example.test`;
    await api(owner, `/spaces/${project.id}/members`, { email, role: 'editor' });
    await login(editor, email);
    await editor.goto(`/projects/${project.id}?tab=settings`);
    const section = editor.getByRole('region', { name: 'AI agents' });
    await expect(section).toContainText(
      'Only a project owner or manager can see or change this project’s AI agent settings.'
    );
    await expect(section.getByRole('switch')).toHaveCount(0);
    await expect(section.getByRole('button')).toHaveCount(0);
  } finally {
    await ownerContext.close();
    await editorContext.close();
  }
});

test('A key enrolled for an AI agent can be given read access and nothing else', async ({
  page
}) => {
  test.setTimeout(120000);
  const stamp = Date.now(),
    projectName = `Agent key target ${stamp}`;
  await login(page, `agent-key-${stamp}@example.test`);
  const project = await api(page, '/projects', { name: projectName });
  await page.goto('/profile');
  const section = page.getByRole('region', { name: 'Signing keys' });
  await section.getByText('Connect a local signing key', { exact: true }).click();

  // "For me" is the default; choosing the agent asks for a name.
  await expect(section.getByRole('radio', { name: /This key is for me/ })).toBeChecked();
  await section.getByRole('radio', { name: /This key is for an AI agent/ }).check();
  await section.getByLabel('Name for this key').fill('Claude on my laptop');
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

  await expect(section.getByRole('status')).toContainText('AI agent key connected');
  await expect(section.getByText('AI agent', { exact: true })).toBeVisible();
  await expect(section.getByText('Claude on my laptop')).toBeVisible();
  const enrolled = (await api(page, '/devices')).keys[0];
  expect(enrolled).toMatchObject({ class: 'agent', label: 'Claude on my laptop' });

  // The only permission on offer is Read documents.
  await section.getByRole('button', { name: /^Grant a project to key/ }).click();
  await section.getByLabel(/^Project to grant to key/).selectOption(project.id);
  await expect(section.getByText('Permission: Read documents')).toBeVisible();
  await expect(section.getByText('Write history')).toHaveCount(0);
  await section.getByRole('button', { name: 'Save grant' }).click();
  await expect(section.getByRole('link', { name: projectName, exact: true })).toBeVisible();
  const granted = (await api(page, '/devices')).keys[0];
  expect(granted.grants).toHaveLength(1);
  expect(granted.grants[0]).toMatchObject({ project_id: project.id, scopes: ['content:read'] });
  expect((await new AxeBuilder({ page }).include('#signing-keys').analyze()).violations).toEqual(
    []
  );
});
