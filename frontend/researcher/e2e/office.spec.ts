import { test, expect, type Page } from '@playwright/test';
import type { MaterializedNode } from '../src/lib/api/types';
import type { JSONContent } from '@tiptap/core';
import AxeBuilder from '@axe-core/playwright';
import { zipSync, strToU8 } from 'fflate';
import { ulid } from 'ulid';
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
  await page.getByRole('dialog').getByRole('combobox').selectOption(kind);
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
  await page.getByRole('tab', { name: 'View', exact: true }).click();
  await page.getByLabel('Freeze rows').selectOption('1');
  await page.getByLabel('Freeze columns').selectOption('1');
  await saved(page);
  await page.reload();
  await page.getByRole('tab', { name: 'View', exact: true }).click();
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
  const headers = { 'x-dynodoc-editor': 'office-v5', origin: new URL(page.url()).origin };
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
  const headers = { 'x-dynodoc-editor': 'office-v5', origin: new URL(page.url()).origin };
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
        node_id: ulid(),
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
  await page.getByRole('tab', { name: 'Page Layout', exact: true }).click();
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
  const headers = { 'x-dynodoc-editor': 'office-v5', origin: new URL(page.url()).origin };
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

test('live pages reflow long editable paragraphs without changing content identity or undo', async ({
  page
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await create(page);
  const editor = page.getByRole('textbox', { name: 'Document content' });
  const text = 'Editable pagination keeps this paragraph intact. বাংলা العربية 🌍. '.repeat(110);
  await editor.fill(text);
  await saved(page);
  await expect.poll(() => page.locator('.page-sheet').count()).toBeGreaterThan(1);
  const blockId = await editor.locator('p').first().getAttribute('data-blockid');
  await expect(editor.locator('.pagination-gap')).not.toHaveCount(0);
  // Every visible text line must stay inside a page's content box, including a
  // paragraph that spans several pages. Empty layout widgets are not document text.
  const overflow = await editor.evaluate((el) => {
    const origin = el.getBoundingClientRect().top;
    const pitch = (297 * 96) / 25.4 + 24;
    const body = ((297 - 2 * 25.4) * 96) / 25.4;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    const bad: { top: number; bottom: number }[] = [];
    let node: Node | null;
    while ((node = walker.nextNode())) {
      if (!node.textContent?.trim()) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      for (const rect of range.getClientRects()) {
        if (!rect.width || !rect.height) continue;
        const top = rect.top - origin;
        const bottom = top + rect.height;
        if (
          bottom % pitch > body + 3 ||
          Math.floor(top / pitch) !== Math.floor((bottom - 1) / pitch)
        )
          bad.push({ top, bottom });
      }
    }
    return bad;
  });
  expect(overflow).toEqual([]);
  await editor.click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type(' END-OF-PAGES');
  await expect(editor).toContainText('END-OF-PAGES');
  await page.keyboard.press('Control+z');
  await expect(editor).not.toContainText('END-OF-PAGES');
  await saved(page);
  await page.screenshot({ path: 'test-results/editable-pages.png', fullPage: true });
  await page.getByRole('tab', { name: 'Layout', exact: true }).click();
  await page.getByLabel('Page margins').selectOption('12.7');
  await saved(page);
  await expect(editor.locator('p').first()).toHaveAttribute('data-blockid', blockId!);
  await page.reload();
  await expect(editor).toContainText('Editable pagination');
  await expect(editor.locator('p').first()).toHaveAttribute('data-blockid', blockId!);
  await editor.fill('One page again.');
  await saved(page);
  await expect(page.locator('.page-sheet')).toHaveCount(1);
  expect(errors).toEqual([]);
});

test('conditional formatting follows formula values and survives save, undo and reload', async ({
  page
}) => {
  await create(page, 'spreadsheet');
  async function edit(address: string, value: string) {
    await page.getByRole('button', { name: new RegExp(`^${address}:`) }).dblclick();
    const input = page.getByRole('textbox', { name: `Edit ${address}`, exact: true });
    await input.fill(value);
    await input.press('Enter');
  }
  await edit('A1', '12');
  await edit('B1', '=A1*2');
  await page.getByLabel('Go to cell or range').fill('B1:B1');
  await page.getByLabel('Go to cell or range').press('Enter');
  await page.getByRole('button', { name: 'Conditional formatting', exact: true }).click();
  await page.getByLabel('Condition value', { exact: true }).fill('20');
  await page.getByRole('button', { name: 'Apply rule', exact: true }).click();
  const cell = page.getByRole('button', { name: /^B1:/ }).locator('..');
  await expect(cell).toHaveCSS('background-color', 'rgb(255, 199, 206)');
  await saved(page);
  await page.reload();
  await expect(cell).toHaveCSS('background-color', 'rgb(255, 199, 206)');
  await edit('A1', '4');
  await expect(cell).toHaveCSS('background-color', 'rgb(255, 255, 255)');
  await saved(page);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(cell).toHaveCSS('background-color', 'rgb(255, 199, 206)');
});

test('editable tables paginate between rows and reload without saving layout rows', async ({
  page
}) => {
  await create(page);
  await page.getByRole('tab', { name: 'Insert', exact: true }).click();
  await page.getByLabel('Table rows', { exact: true }).fill('30');
  await page.getByLabel('Table columns', { exact: true }).fill('3');
  await page.getByRole('button', { name: 'Insert table', exact: true }).click();
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await saved(page);
  await expect.poll(() => page.locator('.page-sheet').count()).toBeGreaterThan(1);
  await expect(editor.locator('tr.pagination-gap')).not.toHaveCount(0);
  await expect(editor.locator('tr:not(.pagination-gap)')).toHaveCount(30);
  await expect(editor.locator('tr.pagination-gap td').first()).toHaveAttribute('colspan', '3');
  const id = new URL(page.url()).pathname.split('/').pop();
  const response = await (
    await page.request.get(`/api/engine/documents/${id}`, {
      headers: { 'x-dynodoc-editor': 'office-v5' }
    })
  ).json();
  const table = (Object.values(response.state.nodes) as MaterializedNode[]).find(
    (n) => (n.current_fields.content as JSONContent)?.type === 'table'
  );
  expect((table?.current_fields.content as JSONContent).content).toHaveLength(30);
  expect(JSON.stringify(table)).not.toContain('pagination-gap');
  await editor.locator('tr:not(.pagination-gap)').last().locator('td').first().click();
  await page.keyboard.type('Last row');
  await saved(page);
  await page.reload();
  await expect(editor).toContainText('Last row');
  await expect(editor.locator('tr:not(.pagination-gap)')).toHaveCount(30);
});

test('Excel analysis: named ranges, chart, pivot refresh and server-enforced protection', async ({
  page
}) => {
  await page.setViewportSize({ width: 1920, height: 1030 });
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await create(page, 'spreadsheet');
  async function select(range: string) {
    await page.getByLabel('Go to cell or range').fill(range);
    await page.getByLabel('Go to cell or range').press('Enter');
  }
  async function edit(address: string, value: string) {
    await page.getByRole('button', { name: new RegExp(`^${address}:`) }).dblclick();
    const input = page.getByRole('textbox', { name: `Edit ${address}`, exact: true });
    await input.fill(value);
    await input.press('Enter');
  }
  for (const [address, value] of [
    ['A1', 'Region'],
    ['B1', 'Sales'],
    ['A2', 'East'],
    ['B2', '10'],
    ['A3', 'West'],
    ['B3', '20'],
    ['A4', 'East'],
    ['B4', '30']
  ])
    await edit(address, value);
  await saved(page);
  await select('A1:B4');
  await page.getByRole('tab', { name: 'Insert', exact: true }).click();
  await page.getByRole('button', { name: 'Insert column chart', exact: true }).click();
  await page.getByLabel('Chart title').fill('Regional sales');
  await page.getByRole('dialog').getByRole('button', { name: 'Insert chart' }).click();
  await expect(page.getByRole('figure', { name: 'Regional sales' })).toBeVisible();
  await saved(page);
  await page.screenshot({ path: 'test-results/excel-workspace.png', fullPage: true });
  await page.getByRole('tab', { name: 'Formulas', exact: true }).click();
  await page.getByRole('button', { name: 'Name Manager', exact: true }).click();
  await page.getByLabel('Name', { exact: true }).fill('SalesTotal');
  await page.getByLabel('Refers to', { exact: true }).fill("=SUM('Sheet 1'!$B$2:$B$4)");
  await page.getByRole('dialog').getByRole('button', { name: 'Apply' }).click();
  await edit('C1', '=SalesTotal');
  await expect(page.getByRole('button', { name: 'C1: 60', exact: true })).toBeVisible();
  await saved(page);
  await select('A1:B4');
  await page.getByRole('tab', { name: 'Insert', exact: true }).click();
  await page.getByRole('button', { name: 'PivotTable', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Create PivotTable', exact: true })
    .click();
  await expect(page.getByRole('button', { name: 'B2: 40', exact: true })).toBeVisible();
  await saved(page);
  await page.locator('.sheet-tabs').getByRole('button', { name: 'Sheet 1', exact: true }).click();
  await edit('B2', '15');
  await saved(page);
  await page
    .locator('.sheet-tabs')
    .getByRole('button', { name: 'PivotTable', exact: true })
    .click();
  await page.getByRole('tab', { name: 'Data', exact: true }).click();
  await page.getByRole('button', { name: 'Refresh PivotTable', exact: true }).click();
  await expect(page.getByRole('button', { name: 'B2: 45', exact: true })).toBeVisible();
  await saved(page);
  await page.getByRole('tab', { name: 'Review', exact: true }).click();
  await page.getByRole('button', { name: 'Protect sheet', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Protect', exact: true }).click();
  await saved(page);
  await expect(page.getByLabel('Formula bar')).toBeDisabled();
  const id = new URL(page.url()).pathname.split('/').pop();
  const headers = { 'x-dynodoc-editor': 'office-v5', origin: new URL(page.url()).origin };
  const data = await (await page.request.get(`/api/engine/documents/${id}`, { headers })).json();
  const sheet = Object.values(data.state.nodes as Record<string, MaterializedNode>).find(
    (n) => n.current_fields.label === 'PivotTable'
  )!;
  const row = Object.values(data.state.nodes as Record<string, MaterializedNode>).find(
    (n) => n.parent_id === sheet.id && !n.deleted
  )!;
  const rejected = await page.request.post(`/api/engine/documents/${id}/batch`, {
    headers,
    data: { ops: [{ type: 'FieldEdited', node_id: row.id, field: 'cell_0', value: 'bypass' }] }
  });
  expect(rejected.status()).toBe(409);
  await page.getByRole('button', { name: 'Unprotect', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Unprotect', exact: true }).click();
  await saved(page);
  await expect(page.getByLabel('Formula bar')).toBeEnabled();
  await page.reload();
  await expect(page.getByRole('button', { name: 'B2: 45', exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test('Word References: note bodies and contents survive save and reload', async ({ page }) => {
  await create(page);
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.fill('Methods');
  await editor.click();
  await page.getByLabel('Paragraph style').selectOption('1');
  await page.keyboard.press('Control+End');
  await page.keyboard.press('Enter');
  await page.keyboard.type('Evidence here.');
  await page.getByRole('tab', { name: 'References', exact: true }).click();
  await page.getByRole('button', { name: 'Insert footnote', exact: true }).click();
  await page.getByLabel('Note text').fill('A real source note বাংলা.');
  await page.getByRole('dialog').getByRole('button', { name: 'Save', exact: true }).click();
  await expect(editor.locator('[data-document-note]')).toHaveCount(1);
  await saved(page);
  await page.getByRole('button', { name: 'Insert table of contents', exact: true }).click();
  await expect(editor.locator('[data-contents]')).toContainText('Methods');
  await saved(page);
  await page.reload();
  await page.getByRole('tab', { name: 'References', exact: true }).click();
  await page.getByRole('button', { name: 'Show notes', exact: true }).click();
  await expect(page.getByRole('complementary', { name: 'Document notes' })).toContainText(
    'A real source note বাংলা.'
  );
  await expect(editor.locator('[data-contents]')).toContainText('Methods');
});

test('Excel data tools split text, remove duplicate rows and sort formulas', async ({ page }) => {
  await create(page, 'spreadsheet');
  async function edit(address: string, value: string) {
    await page.getByRole('button', { name: new RegExp(`^${address}:`) }).dblclick();
    const input = page.getByRole('textbox', { name: `Edit ${address}`, exact: true });
    await input.fill(value);
    await input.press('Enter');
  }
  async function select(range: string) {
    await page.getByLabel('Go to cell or range').fill(range);
    await page.getByLabel('Go to cell or range').press('Enter');
  }
  for (const [a, v] of [
    ['A1', 'Name'],
    ['B1', 'Value'],
    ['C1', 'Double'],
    ['A2', 'West'],
    ['B2', '20'],
    ['C2', '=B2*2'],
    ['A3', 'East'],
    ['B3', '10'],
    ['C3', '=B3*2'],
    ['A4', 'East'],
    ['B4', '10'],
    ['C4', '=B4*2']
  ])
    await edit(a, v);
  await saved(page);
  await select('A1:C4');
  await page.getByRole('tab', { name: 'Data', exact: true }).click();
  await page.getByRole('button', { name: 'Remove Duplicates', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Remove duplicates', exact: true })
    .click();
  await saved(page);
  await expect(page.getByRole('button', { name: 'A4: ', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Custom sort', exact: true }).click();
  await page.getByLabel('My data has headers').check();
  await page.getByRole('dialog').getByRole('button', { name: 'Apply', exact: true }).click();
  await saved(page);
  await expect(page.getByRole('button', { name: 'A2: East', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'C2: 20', exact: true })).toBeVisible();
  await edit('D1', 'Ada,Lovelace');
  await select('D1:D1');
  await page.getByRole('button', { name: 'Text to Columns', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Apply', exact: true }).click();
  await saved(page);
  await expect(page.getByRole('button', { name: 'E1: Lovelace', exact: true })).toBeVisible();
});
