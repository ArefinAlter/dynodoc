import { expect, test, type Page } from '@playwright/test';
import { ulid } from 'ulid';
import { readFile } from 'node:fs/promises';
import { unzipSync, strFromU8 } from 'fflate';
test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in stack.');
async function api(page: Page, path: string, body: unknown) {
  return page.evaluate(
    async ({ path, body }) => {
      const r = await fetch('/api/engine' + path, {
        method: 'POST',
        headers: { 'content-type': 'application/json', 'x-dynodoc-editor': 'office-v5' },
        body: JSON.stringify(body)
      });
      if (!r.ok) throw new Error(await r.text());
      return r.json();
    },
    { path, body }
  );
}
for (const kind of ['spreadsheet', 'presentation'])
  test(`${kind} downloads a retained native revision with exact origin`, async ({ page }) => {
    await page.goto('/login');
    await page.getByLabel('Your name').fill('Export tester');
    await page.getByLabel('Work email').fill(`native-${kind}-${Date.now()}@example.test`);
    await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
    await page.getByRole('button', { name: /Continue in local development/ }).click();
    await expect(page).toHaveURL(/\/projects$/);
    await page.goto('/workspace');
    const doc = await api(page, '/documents', { title: 'Earlier native content' });
    await api(page, `/documents/${doc.id}/metadata`, { settings: { kind } });
    const root = ulid(),
      group = ulid(),
      item = ulid(),
      sheet = kind === 'spreadsheet',
      field = sheet ? 'cell_0' : 'text';
    await api(page, `/documents/${doc.id}/batch`, {
      base_seq: 0,
      ops: [
        { type: 'NodeCreated', node_id: root, node_type: 'form', pos: 'a0', fields: {} },
        {
          type: 'NodeCreated',
          node_id: group,
          node_type: 'section',
          parent_id: root,
          pos: 'a0',
          fields: sheet ? { kind: 'sheet', label: 'Sheet1', columns: 2 } : { kind: 'slide' }
        },
        {
          type: 'NodeCreated',
          node_id: item,
          node_type: 'item',
          parent_id: group,
          pos: 'a0',
          fields: sheet
            ? { cell_0: 'Earlier value' }
            : { kind: 'text', text: 'Earlier value', x: 1, y: 1, w: 3, h: 1, style: {} }
        }
      ]
    });
    await api(page, `/documents/${doc.id}/batch`, {
      base_seq: 3,
      ops: [{ type: 'FieldEdited', node_id: item, field, value: 'Later value' }]
    });
    await page.goto(`/documents/${doc.id}`);
    await page.getByRole('button', { name: 'Version history', exact: true }).click();
    const history = page.getByRole('region', { name: 'Recover earlier content' });
    await history.getByLabel('Earlier revision').fill('3');
    const downloading = page.waitForEvent('download');
    await history.getByRole('button', { name: 'Download native file', exact: true }).click();
    const file = await downloading;
    expect(file.suggestedFilename()).toMatch(sheet ? /\(v3\)\.xlsx$/ : /\(v3\)\.pptx$/);
    const zip = unzipSync(await readFile((await file.path())!));
    const content = Object.entries(zip)
      .filter(([name]) => name.endsWith('.xml'))
      .map(([, bytes]) => strFromU8(bytes))
      .join('\n');
    expect(content).toContain('Earlier value');
    expect(content).not.toContain('Later value');
    expect(strFromU8(zip['docProps/core.xml'])).toContain(`dynodoc:document=${doc.id};revision=3;`);
  });
