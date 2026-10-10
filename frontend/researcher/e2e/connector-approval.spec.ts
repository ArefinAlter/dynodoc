import { expect, test, type Page } from '@playwright/test';
import { createHash, randomBytes } from 'node:crypto';
test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in stack.');
async function signIn(page: Page, email: string) {
  await page.getByLabel('Your name').fill('Connector tester');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
}
test('browser approval survives sign-in and returns a revocable file-only connection', async ({
  page,
  context
}) => {
  const email = `pair-${Date.now()}@example.test`;
  await page.goto('/login');
  await signIn(page, email);
  await expect(page).toHaveURL(/\/projects$/);
  await page.goto('/workspace');
  const id = await page.evaluate(async () => {
    const headers = { 'content-type': 'application/json', 'x-dynodoc-editor': 'office-v5' };
    const doc = await (
      await fetch('/api/engine/documents', {
        method: 'POST',
        headers,
        body: JSON.stringify({ title: 'Pair this file' })
      })
    ).json();
    const updated = await fetch(`/api/engine/documents/${doc.id}/metadata`, {
      method: 'POST',
      headers,
      body: JSON.stringify({ settings: { kind: 'document' } })
    });
    if (!updated.ok) throw new Error('Fixture failed');
    return doc.id as string;
  });
  const secret = randomBytes(32).toString('hex'),
    challenge = createHash('sha256').update(secret).digest('hex');
  const url = `/connectors/authorize?host=word&challenge=${challenge}&expires=${Math.floor(Date.now() / 1000) + 600}`;
  await context.clearCookies();
  await page.goto(url);
  await expect(page).toHaveURL(/\/login\?next=/);
  await signIn(page, email);
  await expect(page).toHaveURL(/\/connectors\/authorize/);
  await expect(page.getByRole('main')).toContainText(challenge.slice(0, 12));
  await page.getByLabel('File to connect').selectOption(id);
  await page.getByRole('button', { name: 'Approve connection', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('Connection approved');
  const completion = await page.request.get('/api/connectors/connection', {
    headers: { authorization: `Bearer ${secret}` }
  });
  expect(completion.ok()).toBe(true);
  const connected = await completion.json();
  expect(connected.document_id).toBe(id);
  const hashAttempt = await page.request.get('/api/connectors/connection', {
    headers: { authorization: `Bearer ${challenge}` }
  });
  expect(hashAttempt.status()).toBe(401);
  const revoked = await page.evaluate(
    async ({ id, grant }) => {
      const r = await fetch(`/api/engine/documents/${id}/connectors/${grant}/revoke`, {
        method: 'POST',
        headers: { 'content-type': 'application/json', 'x-dynodoc-editor': 'office-v5' },
        body: '{}'
      });
      return r.status;
    },
    { id, grant: connected.grant_id }
  );
  expect(revoked).toBe(200);
  expect(
    (
      await page.request.get('/api/connectors/connection', {
        headers: { authorization: `Bearer ${secret}` }
      })
    ).status()
  ).toBe(401);
  await page.goto('/connectors');
  expect((await page.request.get('/connectors/packages/word.xml')).ok()).toBe(true);
  expect((await page.request.get('/connectors/packages/google-docs.zip')).ok()).toBe(true);
});
