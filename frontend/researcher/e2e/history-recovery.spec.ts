import { expect, test, type Page } from '@playwright/test';
import { ulid } from 'ulid';
import { readFile } from 'node:fs/promises';

test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in development stack.');

async function api(page: Page, path: string, body?: unknown) {
  return page.evaluate(
    async ({ path, body }) => {
      const response = await fetch('/api/engine' + path, {
        method: body === undefined ? 'GET' : 'POST',
        headers: { 'content-type': 'application/json', 'x-dynodoc-editor': 'office-v5' },
        body: body === undefined ? undefined : JSON.stringify(body)
      });
      const result = await response.json();
      if (!response.ok) throw new Error(JSON.stringify(result));
      return result;
    },
    { path, body }
  );
}

test('recover an old deleted block through preview, checkpoint download, draft and review', async ({
  page
}) => {
  test.setTimeout(120000);
  await page.goto('/login');
  await page.getByLabel('Your name').fill('History tester');
  await page.getByLabel('Work email').fill(`history-${Date.now()}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/workspace$/);
  const doc = await api(page, '/documents', { title: 'Historical recovery' });
  await api(page, `/documents/${doc.id}/metadata`, { settings: { kind: 'document' } });
  const root = ulid(),
    old = ulid(),
    live = ulid();
  await api(page, `/documents/${doc.id}/batch`, {
    base_seq: 0,
    ops: [
      { type: 'NodeCreated', node_id: root, node_type: 'form', pos: 'a0', fields: {} },
      {
        type: 'NodeCreated',
        node_id: old,
        node_type: 'item',
        parent_id: root,
        pos: 'a0',
        fields: { label: 'The paragraph we need back' }
      },
      {
        type: 'NodeCreated',
        node_id: live,
        node_type: 'item',
        parent_id: root,
        pos: 'a1',
        fields: { label: 'Original sibling' }
      }
    ]
  });
  await api(page, `/documents/${doc.id}/batch`, {
    base_seq: 3,
    ops: [{ type: 'NodeDeleted', node_id: old }]
  });
  for (let i = 0; i < 55; i++) {
    await api(page, `/documents/${doc.id}/batch`, {
      base_seq: 4 + i,
      ops: [{ type: 'FieldEdited', node_id: live, field: 'label', value: `Later save ${i}` }]
    });
  }
  await page.goto(`/documents/${doc.id}`);
  await page.getByRole('button', { name: 'Version history', exact: true }).click();
  const recovery = page.getByRole('region', { name: 'Recover earlier content' });
  await recovery.getByLabel('Earlier revision').fill('3');
  await recovery.getByRole('button', { name: 'Preview recovery', exact: true }).click();
  await recovery.getByRole('checkbox', { name: /The paragraph we need back/ }).check();
  await expect(recovery.getByRole('checkbox', { name: /Original sibling/ })).not.toBeChecked();
  const downloaded = page.waitForEvent('download');
  await recovery.getByRole('button', { name: 'Download checkpoint JSON' }).click();
  const file = await downloaded;
  const checkpoint = JSON.parse(await readFile((await file.path())!, 'utf8'));
  expect(checkpoint.through_seq).toBe(3);
  expect(checkpoint.state.nodes[old].deleted).toBe(false);
  expect(checkpoint.state.nodes[live].current_fields.label).toBe('Original sibling');
  await recovery.getByRole('button', { name: 'Create recovery draft' }).click();
  await expect(page.getByRole('button', { name: 'Send for review', exact: true })).toBeVisible();
  expect((await api(page, `/documents/${doc.id}/provenance`)).through_seq).toBe(59);
  await page.getByRole('button', { name: 'Send for review', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Draft sent for review');
  const requests = await api(page, `/documents/${doc.id}/change-requests`);
  const request = requests.items.find(
    (item: { source: { kind?: string } }) => item.source.kind === 'history_recovery'
  );
  await page.goto(`/documents/${doc.id}?request=${request.id}`);
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Recovery of content from revision 3');
  await dialog.getByRole('button', { name: 'Merge 1 selected', exact: true }).click();
  await expect
    .poll(async () => (await api(page, `/documents/${doc.id}/provenance`)).through_seq)
    .toBe(60);
  const current = await api(page, `/documents/${doc.id}/provenance`);
  expect(current.state.nodes[old].deleted).toBe(false);
  expect(current.state.nodes[live].current_fields.label).toBe('Later save 54');
  expect((await api(page, `/documents/${doc.id}/verify`)).ok).toBe(true);
});
