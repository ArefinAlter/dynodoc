import { test, expect, type Page } from '@playwright/test';
import type { MaterializedNode } from '../src/lib/api/types';
import type { JSONContent } from '@tiptap/core';
import AxeBuilder from '@axe-core/playwright';
import { zipSync, strToU8 } from 'fflate';
test.skip(
  process.env.PILOT_E2E !== 'true',
  'Uses the isolated local API and test-only authentication.'
);
async function create(page: Page, kind = 'document') {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Office reviewer');
  await page
    .getByLabel('Work email')
    .fill(`office-${Date.now()}-${Math.random().toString(36).slice(2)}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/workspace$/);
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill('Office workflow');
  await page.getByRole('combobox').selectOption(kind);
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  await expect(page.getByLabel('Document title')).toHaveValue('Office workflow');
}
async function saved(page: Page) {
  await expect(page.locator('.save-state')).toHaveText('All changes saved', { timeout: 30000 });
}
test('Word workflow: direction, tracked text, undo, saved page setup and actual pagination', async ({
  page
}) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await create(page);
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.fill('مرحبا বাংলা — A document with a history.');
  await saved(page);
  await editor.click();
  await page.getByLabel('Text direction').selectOption('rtl');
  await expect(editor.locator('p')).toHaveAttribute('dir', 'rtl');
  await saved(page);
  await page.getByRole('tab', { name: 'Review', exact: true }).click();
  await page.getByRole('button', { name: 'Track text edits', exact: true }).click();
  await editor.click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type(' Proposed.');
  await expect(editor.locator('.review-insert')).toHaveText(' Proposed.');
  await saved(page);
  await page.getByRole('button', { name: 'Reject all text changes' }).click();
  await expect(editor).not.toContainText('Proposed.');
  await saved(page);
  await page.getByRole('button', { name: 'Track text edits', exact: true }).click();
  await editor.click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type(' Undo this.');
  await page.keyboard.press('Control+z');
  await saved(page);
  await expect(editor).not.toContainText('Undo this.');
  await page.getByRole('tab', { name: 'Layout', exact: true }).click();
  await page.getByLabel('Paper size').selectOption('Letter');
  await page.getByLabel('Page header', { exact: true }).fill('A versioned document');
  await page.getByLabel('Page header', { exact: true }).press('Tab');
  await saved(page);
  await page.reload();
  await page.getByRole('tab', { name: 'Layout', exact: true }).click();
  await expect(page.getByLabel('Page header', { exact: true })).toHaveValue('A versioned document');
  await page.getByRole('tab', { name: 'Insert', exact: true }).click();
  await editor.click();
  await page.keyboard.press('Control+End');
  await page.getByRole('button', { name: 'Page break', exact: true }).click();
  await saved(page);
  await page.getByRole('tab', { name: 'Layout', exact: true }).click();
  await page.getByRole('button', { name: 'Preview pages', exact: true }).click();
  const preview = page.frameLocator('iframe[title="Document pages"]');
  await expect(preview.getByRole('button', { name: 'Print / Save as PDF' })).toBeEnabled({
    timeout: 30000
  });
  await expect(preview.locator('.pagedjs_page')).toHaveCount(2);
  expect(
    await preview
      .locator('body')
      .evaluate(() =>
        [...document.fonts].some((f) => f.family === 'Noto Sans Bengali' && f.status === 'loaded')
      )
  ).toBeTruthy();
  await page.getByRole('button', { name: 'Close preview', exact: true }).click();
  expect(errors).toEqual([]);
});
test('Spreadsheet workflow: relative fill, merged cells, frozen panes and keyboard navigation', async ({
  page
}) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await create(page, 'spreadsheet');
  async function edit(address: string, value: string) {
    await page.getByRole('button', { name: new RegExp(`^${address}:`) }).dblclick();
    await page.getByRole('textbox', { name: `Edit ${address}`, exact: true }).fill(value);
    await page.getByRole('textbox', { name: `Edit ${address}`, exact: true }).press('Enter');
  }
  await edit('A1', '2');
  await edit('A2', '3');
  await edit('B1', '=A1*2');
  await saved(page);
  await page.getByLabel('Go to cell or range').fill('B1:B2');
  await page.getByLabel('Go to cell or range').press('Enter');
  await page.getByRole('button', { name: 'Fill down', exact: true }).click();
  await expect(page.getByRole('button', { name: 'B2: 6', exact: true })).toBeVisible();
  await saved(page);
  await page.getByLabel('Go to cell or range').fill('D1:E1');
  await page.getByLabel('Go to cell or range').press('Enter');
  await page.getByRole('button', { name: 'Merge cells', exact: true }).click();
  await expect(page.locator('td[colspan="2"]')).toHaveCount(1);
  await saved(page);
  await page.getByRole('grid').focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('grid')).toHaveAttribute('aria-activedescendant', /-5$/);
  await page.getByRole('button', { name: 'View', exact: true }).click();
  await page.getByLabel('Freeze rows').selectOption('1');
  await page.getByLabel('Freeze columns').selectOption('1');
  await saved(page);
  await page.reload();
  await page.getByRole('button', { name: 'View', exact: true }).click();
  await expect(page.getByLabel('Freeze rows')).toHaveValue('1');
  await expect(page.locator('td[colspan="2"]')).toHaveCount(1);
  await page.getByRole('grid').focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowDown');
  await expect(page.getByLabel('Formula bar')).toHaveValue('=A2*2');
  const result = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze();
  expect(result.violations.map((v) => ({ id: v.id, nodes: v.nodes.map((n) => n.target) }))).toEqual(
    []
  );
  expect(errors).toEqual([]);
});
test('native DOCX imports styled paragraphs in a worker and reports fidelity limits', async ({
  page
}) => {
  await create(page);
  await page.goto('/workspace');
  const body =
    '<w:p><w:pPr><w:bidi/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="336699"/><w:rFonts w:ascii="Arial"/></w:rPr><w:t>العربية বাংলা</w:t></w:r></w:p>';
  const bytes = zipSync({
    'word/document.xml': strToU8(
      `<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${body}</w:body></w:document>`
    )
  });
  await page
    .locator('input[type=file]')
    .first()
    .setInputFiles({
      name: 'styled.docx',
      mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
      buffer: Buffer.from(bytes)
    });
  await expect(page.getByText(/Review conversion before use/)).toBeVisible();
  await page.getByRole('button', { name: 'Import into workspace' }).click();
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await expect(editor.locator('strong')).toContainText('العربية বাংলা');
  await expect(editor.locator('p')).toHaveAttribute('dir', 'rtl');
  await saved(page);
  await page.reload();
  await expect(editor.locator('strong')).toContainText('العربية বাংলা');
});

test('IME input survives a concurrent server edit without overwriting it', async ({ page }) => {
  await create(page);
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.fill('Shared text.');
  await saved(page);
  const id = new URL(page.url()).pathname.split('/').pop();
  const headers = { 'x-dynodoc-editor': 'office-v2', origin: new URL(page.url()).origin };
  const original = await (
    await page.request.get(`/api/engine/documents/${id}`, { headers })
  ).json();
  const block = (Object.values(original.state.nodes) as MaterializedNode[]).find(
    (n: MaterializedNode) => (n.current_fields.content as JSONContent)?.type === 'paragraph'
  ) as MaterializedNode;
  await editor.dispatchEvent('compositionstart', { data: '' });
  await editor.evaluate((el) => {
    el.focus();
    document.execCommand('selectAll');
    document.execCommand('insertText', false, 'বাংলা লেখা');
  });
  const remote = {
    ...(block.current_fields.content as JSONContent),
    content: [{ type: 'text', text: 'A colleague changed this.' }]
  };
  const response = await page.request.post(`/api/engine/documents/${id}/batch`, {
    headers,
    data: {
      base_seq: original.latest_seq,
      ops: [{ type: 'FieldEdited', node_id: block.id, field: 'content', value: remote }]
    }
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  // Hold composition open beyond the autosave interval while the SSE update arrives.
  await page.waitForTimeout(1500);
  await expect(editor).toContainText('বাংলা লেখা');
  await editor.dispatchEvent('compositionend', { data: 'বাংলা লেখা' });
  await expect(page.locator('.save-state')).toHaveText('Changes need attention', {
    timeout: 30000
  });
  await expect(editor).toContainText('বাংলা লেখা');
  const current = await (await page.request.get(`/api/engine/documents/${id}`, { headers })).json();
  expect(JSON.stringify(current.state.nodes[block.id].current_fields.content)).toContain(
    'A colleague changed this.'
  );
});

test('spreadsheet print includes rows outside the virtual viewport', async ({ page }) => {
  await create(page, 'spreadsheet');
  const id = new URL(page.url()).pathname.split('/').pop();
  const headers = { 'x-dynodoc-editor': 'office-v2', origin: new URL(page.url()).origin };
  const detail = await (await page.request.get(`/api/engine/documents/${id}`, { headers })).json();
  const nodes = Object.values(detail.state.nodes) as MaterializedNode[];
  const sheet = nodes.find((n) => n.current_fields.kind === 'sheet');
  const last = nodes
    .filter((n) => n.parent_id === sheet.id)
    .sort((a, b) => a.pos.localeCompare(b.pos))
    .at(-1);
  const response = await page.request.post(`/api/engine/documents/${id}/batch`, {
    headers,
    data: {
      base_seq: detail.latest_seq,
      ops: Array.from({ length: 80 }, (_, i) => ({
        type: 'NodeCreated',
        node_id: `01K00000000000000000${String(i).padStart(6, '0')}`,
        node_type: 'item',
        parent_id: sheet.id,
        pos: last!.pos + 'V' + String(i).padStart(3, '0'),
        fields: { cell_0: i === 79 ? 'Final offscreen row' : `Additional row ${i}` }
      }))
    }
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  await page.reload();
  await expect(page.getByRole('button', { name: /Final offscreen row/ })).toHaveCount(0);
  await page.getByRole('button', { name: 'Print spreadsheet', exact: true }).click();
  const preview = page.frameLocator('iframe[title="Document pages"]');
  await expect(preview.getByRole('button', { name: 'Print / Save as PDF' })).toBeEnabled({
    timeout: 30000
  });
  await expect(preview.getByText('Final offscreen row', { exact: true })).toBeVisible();
});

test('a cell being edited keeps its original save base when a colleague changes it', async ({
  page
}) => {
  await create(page, 'spreadsheet');
  const id = new URL(page.url()).pathname.split('/').pop();
  const headers = { 'x-dynodoc-editor': 'office-v2', origin: new URL(page.url()).origin };
  const detail = await (await page.request.get(`/api/engine/documents/${id}`, { headers })).json();
  const nodes = Object.values(detail.state.nodes) as MaterializedNode[];
  const sheet = nodes.find((n) => n.current_fields.kind === 'sheet')!;
  const row = nodes
    .filter((n) => n.parent_id === sheet.id)
    .sort((a, b) => a.pos.localeCompare(b.pos))[0];
  await page.getByRole('button', { name: /^A1:/ }).dblclick();
  const input = page.getByRole('textbox', { name: 'Edit A1', exact: true });
  await input.fill('My pending value');
  const response = await page.request.post(`/api/engine/documents/${id}/batch`, {
    headers,
    data: {
      base_seq: detail.latest_seq,
      ops: [{ type: 'FieldEdited', node_id: row.id, field: 'cell_0', value: 'Colleague value' }]
    }
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  await page.waitForTimeout(1500);
  await expect(input).toHaveValue('My pending value');
  await input.press('Enter');
  await expect(page.locator('.save-state')).toHaveText('Changes need attention', {
    timeout: 30000
  });
  await expect(
    page.getByRole('button', { name: 'A1: My pending value', exact: true })
  ).toBeVisible();
  const current = await (await page.request.get(`/api/engine/documents/${id}`, { headers })).json();
  expect(current.state.nodes[row.id].current_fields.cell_0).toBe('Colleague value');
});
