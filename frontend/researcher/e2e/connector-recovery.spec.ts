import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import {
  CaptureSession,
  projectCheckpoint,
  type Checkpoint,
  type Store
} from '../src/lib/provenance/protocol';
import { exportSidecar } from '../src/lib/provenance/sidecar';
const DOC = '7034d450-77cf-44f0-8eec-63f892643f8e';
const ROOT = '01ARZ3NDEKTSV4RRFFQ69G5FAV',
  BLOCK = '01ARZ3NDEKTSV4RRFFQ69G5FAW';
const checkpoint: Checkpoint = {
  format: 'dynodoc.checkpoint',
  version: 1,
  document_id: DOC,
  title: 'Report',
  kind: 'document',
  through_seq: 3,
  chain_hash: 'a'.repeat(64),
  proposals: [],
  state: {
    nodes: {
      [ROOT]: {
        id: ROOT,
        type: 'form',
        parent_id: null,
        pos: '0',
        deleted: false,
        current_fields: {}
      },
      [BLOCK]: {
        id: BLOCK,
        type: 'item',
        parent_id: ROOT,
        pos: '1',
        deleted: false,
        current_fields: {
          content: { type: 'paragraph', content: [{ type: 'text', text: 'Original' }] }
        }
      }
    }
  }
};
async function recovery(host: 'word' | 'google-docs') {
  const store: Store = { load: async () => null, save: async () => {} };
  const session = await CaptureSession.connect(
    store,
    checkpoint,
    projectCheckpoint(checkpoint),
    host
  );
  await session.capture([{ id: BLOCK, text: 'Edited before rename' }]);
  const push = await session.prepare('Original offline request', 'Keep the original intent', true);
  return {
    sidecar: exportSidecar(session),
    push,
    receipt: {
      id: 'fe257224-bcdd-4261-aa37-778f836fa8e2',
      bundle_id: push.bundle.bundle_id,
      digest: 'b'.repeat(64),
      uploaded_by: DOC,
      submitted: true
    }
  };
}
const wordRuntime = `
window.workingCopyUrl = 'file:///saved-copy.docx';
const paragraph = { text: 'Edited before rename', isListItem: false, contentControls: {items: [], load(){}}, getRange(){return {insertContentControl(){const c={tag:'',title:'',insertText(text){paragraph.text=text;}};paragraph.contentControls.items.push(c);return c;}};} };
window.Word={run: async callback=>callback({document:{body:{paragraphs:{items:[paragraph],load(){}},tables:{items:[],load(){}}}},sync:async()=>{}})};
window.Office={onReady:callback=>callback({host:'Word'}),context:{requirements:{isSetSupported:()=>true},document:{getFilePropertiesAsync:callback=>callback({status:'succeeded',value:{url:window.workingCopyUrl}})}}};
`;
test('Word pane restores an offline queue, retries exactly and stops after Save As', async ({
  page,
  context
}) => {
  const { sidecar, push, receipt } = await recovery('word'),
    uploads: unknown[] = [];
  await context.route('https://appsforoffice.microsoft.com/lib/1/hosted/office.js', (route) =>
    route.fulfill({ contentType: 'text/javascript', body: wordRuntime })
  );
  await context.route(`**/api/connectors/documents/${DOC}/checkpoint*`, (route) =>
    route.fulfill({ json: checkpoint })
  );
  await context.route(`**/api/connectors/documents/${DOC}/bundles`, (route) => {
    uploads.push(route.request().postDataJSON());
    return route.fulfill({
      status: uploads.length === 1 ? 503 : 200,
      json: uploads.length === 1 ? { error: 'Offline transport' } : receipt
    });
  });
  await page.goto('/connectors/word');
  await page.getByLabel('File ID', { exact: true }).fill(DOC);
  await page.getByLabel('File connection key').fill('a'.repeat(64));
  await page.getByLabel('Restore provenance from this editor').setInputFiles({
    name: 'renamed.provenance.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(sidecar))
  });
  await expect(page.getByRole('status')).toContainText('Recovery selected');
  await page.getByRole('button', { name: 'Connect this file' }).click();
  await expect(page.getByRole('status')).toContainText('Provenance restored');
  await expect(page.getByLabel('Enable paragraph capture')).not.toBeChecked();
  await page.getByRole('button', { name: 'Push changes for review' }).click();
  await expect(page.getByRole('alert')).toContainText('Offline transport');
  await page.getByRole('button', { name: 'Push changes for review' }).click();
  await expect(page.getByRole('status')).toContainText('Changes sent for review');
  expect(uploads).toEqual([push, push]);
  await page.evaluate(() => {
    (window as unknown as { workingCopyUrl: string }).workingCopyUrl =
      'file:///different-copy.docx';
  });
  await page.getByRole('button', { name: 'Capture now' }).click();
  await expect(page.getByRole('alert')).toContainText('working copy changed');
  expect(uploads).toHaveLength(2);
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export local provenance' }).click();
  const recovered = JSON.parse(readFileSync((await (await download).path())!, 'utf8'));
  expect(recovered.ledger.awaiting).toEqual(receipt);
  expect(recovered.ledger.changes).toEqual(sidecar.ledger.changes);
  const second = await context.newPage();
  await second.goto('/connectors/word');
  await second.getByLabel('File ID', { exact: true }).fill(DOC);
  await second.getByLabel('File connection key').fill('a'.repeat(64));
  await second.getByLabel('Restore provenance from this editor').setInputFiles({
    name: 'original.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(sidecar))
  });
  await expect(second.getByRole('status')).toContainText('Recovery selected');
  await second.getByRole('button', { name: 'Connect this file' }).click();
  await expect(second.getByRole('alert')).toContainText('already has a saved queue');
});
test('Google Docs sidebar restores and exports the exact queue through its host contract', async ({
  page
}) => {
  const { sidecar, push, receipt } = await recovery('google-docs');
  const html = readFileSync('../../extensions/google-docs/Sidebar.html', 'utf8').replace(
    "<?!= include('Protocol'); ?>",
    readFileSync('../../extensions/google-docs/Protocol.html', 'utf8')
  );
  // Google's host, not the Dynodoc site's CSP, owns Apps Script HTML. Keep this
  // isolated bridge fixture on its own secure origin; native acceptance is separate.
  await page.route('https://google-addon.invalid/', (route) =>
    route.fulfill({ contentType: 'text/html', body: '<!doctype html><html></html>' })
  );
  await page.goto('https://google-addon.invalid/');
  await page.evaluate(
    ({ checkpoint, receipt }) => {
      let success: (value: unknown) => void,
        failure: (error: Error) => void,
        generation: string | null = null,
        ledger: unknown = null;
      const state = window as unknown as { uploads: unknown[]; google: unknown };
      state.uploads = [];
      const methods: Record<string, (...args: unknown[]) => unknown> = {
        connectDynodoc: () => ({ checkpoint, scope: 'dynodoc.copy.tab-a' }),
        loadLedger: () => ({ generation, payload: ledger }),
        historicalCheckpointDynodoc: (seq) => {
          if (seq !== checkpoint.through_seq) throw new Error('Wrong revision');
          return checkpoint;
        },
        bindDynodoc: (blocks) => blocks,
        readDynodoc: () => [{ id: '01ARZ3NDEKTSV4RRFFQ69G5FAW', text: 'Edited before rename' }],
        saveLedger: (payload, expected) => {
          if (expected !== generation) throw new Error('Stale queue');
          ledger = payload;
          generation = crypto.randomUUID();
          return generation;
        },
        pushDynodoc: (body) => {
          state.uploads.push(body);
          if (state.uploads.length === 1) throw new Error('Offline transport');
          return receipt;
        }
      };
      const bridge: unknown = new Proxy(
        {},
        {
          get: (_target, method: string) => {
            if (method === 'withSuccessHandler')
              return (callback: typeof success) => {
                success = callback;
                return bridge;
              };
            if (method === 'withFailureHandler')
              return (callback: typeof failure) => {
                failure = callback;
                return bridge;
              };
            return (...args: unknown[]) => {
              try {
                if (method !== 'connectDynodoc' && args.at(-1) !== 'dynodoc.copy.tab-a')
                  throw new Error('Wrong tab');
                success(methods[method](...args));
              } catch (error) {
                failure(error as Error);
              }
            };
          }
        }
      );
      state.google = { script: { run: bridge } };
    },
    { checkpoint, receipt }
  );
  await page.setContent(html);
  await page.locator('#documentId').fill(DOC);
  await page.locator('#token').fill('a'.repeat(64));
  await page.locator('#recovery').setInputFiles({
    name: 'docs.provenance.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(sidecar))
  });
  await expect(page.getByRole('status')).toContainText('Recovery selected');
  await page.getByRole('button', { name: 'Connect', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('Provenance restored');
  await expect(page.locator('#enabled')).not.toBeChecked();
  await expect(page.locator('#token')).toHaveValue('');
  await page.getByRole('button', { name: 'Push for review' }).click();
  await expect(page.getByRole('status')).toContainText('Offline transport');
  await page.getByRole('button', { name: 'Push for review' }).click();
  await expect(page.getByRole('status')).toContainText('Sent for review');
  expect(await page.evaluate(() => (window as unknown as { uploads: unknown[] }).uploads)).toEqual([
    push,
    push
  ]);
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export local provenance' }).click();
  const recovered = JSON.parse(readFileSync((await (await download).path())!, 'utf8'));
  expect(recovered.ledger.awaiting).toEqual(receipt);
  expect(recovered.ledger.client_id).toBe(sidecar.ledger.client_id);
});
