import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { projectCheckpoint, type Checkpoint, type Host } from '../src/lib/provenance/protocol';
const DOC = '7034d450-77cf-44f0-8eec-63f892643f8e',
  ROOT = '01ARZ3NDEKTSV4RRFFQ69G5FAV',
  GROUP = '01ARZ3NDEKTSV4RRFFQ69G5FAW',
  ITEM = '01ARZ3NDEKTSV4RRFFQ69G5FAX';
const REQUEST = 'fe257224-bcdd-4261-aa37-778f836fa8e2',
  ACTOR = '6c2d71d5-0aa0-429e-ae13-9e8d4aa253da';
function checkpoint(host: Host): Checkpoint {
  const sheet = host === 'excel' || host === 'google-sheets';
  const node = (
    id: string,
    type: string,
    parent_id: string | null,
    current_fields: Record<string, unknown>
  ) => ({ id, type, parent_id, current_fields, pos: '0', deleted: false });
  return {
    format: 'dynodoc.checkpoint',
    version: 1,
    document_id: DOC,
    title: 'Native file',
    kind: sheet ? 'spreadsheet' : 'presentation',
    through_seq: 3,
    chain_hash: 'a'.repeat(64),
    proposals: [],
    state: {
      nodes: {
        [ROOT]: node(ROOT, 'form', null, {}),
        [GROUP]: node(
          GROUP,
          'section',
          ROOT,
          sheet ? { kind: 'sheet', label: 'Sheet1', columns: 2 } : { kind: 'slide' }
        ),
        [ITEM]: node(
          ITEM,
          'item',
          GROUP,
          sheet
            ? {
                cell_0: '2',
                cell_1: '=A1*2',
                format_0: { type: 'number' },
                format_1: { type: 'number' }
              }
            : { kind: 'text', text: 'Original' }
        )
      }
    }
  };
}
const officeBase = `window.workingCopyUrl='file:///native-copy';window.Office={onReady:callback=>callback({host:HOST}),context:{requirements:{isSetSupported:()=>true},document:{getFilePropertiesAsync:callback=>callback({status:'succeeded',value:{url:window.workingCopyUrl}})}}};`;
const excel = `
let cells=[2,'=A1*2'];const named=new Map();
function range(col=0,count=2){return {get values(){return [cells.slice(col,col+count)]},set values(v){v[0].forEach((x,i)=>cells[col+i]=x)},get formulas(){return [cells.slice(col,col+count)]},set formulas(v){v[0].forEach((x,i)=>cells[col+i]=x)},rowIndex:0,columnIndex:col,rowCount:1,columnCount:count,isNullObject:false,worksheet:{name:'Sheet1',load(){}},load(){},getCell(r,c){return range(c,1)}};}
const sheet={name:'Sheet1',getUsedRangeOrNullObject:()=>range(),getRangeByIndexes:()=>range()};
window.Excel={run:async callback=>callback({sync:async()=>{},workbook:{worksheets:{items:[sheet],load(){}},names:{get items(){return [...named.keys()].map(name=>({name}))},load(){},getItemOrNullObject:name=>({name,isNullObject:!named.has(name),load(){},getRange:()=>named.get(name)}),add:(name,r)=>named.set(name,r)}}})};
window.editNative=()=>cells[0]=7;window.nativeBlocks=()=>cells;`;
const powerpoint = `
const tags=()=>({items:[],load(){},add(key,value){this.items.push({key,value})}});const shape={type:'TextBox',tags:tags(),textFrame:{textRange:{text:'Original',load(){}}}},slide={tags:tags(),shapes:{items:[shape],load(){}}};
window.PowerPoint={run:async callback=>callback({sync:async()=>{},presentation:{slides:{items:[slide],load(){}}}})};
window.editNative=()=>shape.textFrame.textRange.text='Edited';window.nativeBlocks=()=>[shape.textFrame.textRange.text];`;
for (const host of ['excel', 'powerpoint'] as const) {
  test(`${host} production pane retries one bundle and pulls reviewed fields`, async ({
    page,
    context
  }) => {
    const c = checkpoint(host),
      uploads: Record<string, unknown>[] = [];
    let team = c;
    await context.route('https://appsforoffice.microsoft.com/lib/1/hosted/office.js', (r) =>
      r.fulfill({
        contentType: 'text/javascript',
        body:
          `const HOST='${host === 'excel' ? 'Excel' : 'PowerPoint'}';` +
          officeBase +
          (host === 'excel' ? excel : powerpoint)
      })
    );
    await context.route(`**/api/connectors/documents/${DOC}/checkpoint*`, (r) =>
      r.fulfill({ json: team })
    );
    await context.route(`**/api/connectors/documents/${DOC}/bundles`, (r) => {
      const push = r.request().postDataJSON();
      uploads.push(push);
      if (uploads.length === 1)
        return r.fulfill({ status: 503, json: { error: 'Offline transport' } });
      team = structuredClone(c);
      team.through_seq += 2;
      team.chain_hash = 'b'.repeat(64);
      team.proposals = [{ id: REQUEST, status: 'merged' }];
      team.state.nodes[ITEM].current_fields[host === 'excel' ? 'cell_0' : 'text'] =
        host === 'excel' ? '7' : 'Edited';
      if (host === 'excel') team.state.nodes[ITEM].current_fields.cell_1 = '=A1*3';
      return r.fulfill({
        json: {
          id: REQUEST,
          bundle_id: push.bundle.bundle_id,
          digest: 'c'.repeat(64),
          uploaded_by: ACTOR,
          submitted: true
        }
      });
    });
    await page.goto(`/connectors/${host}`);
    await page.getByLabel('File ID', { exact: true }).fill(DOC);
    await page.getByLabel('File connection key').fill('a'.repeat(64));
    await page.getByRole('button', { name: 'Connect this file' }).click();
    await expect(page.getByRole('status')).toContainText('Connected');
    await expect(page.getByLabel('Enable block capture')).not.toBeChecked();
    await page.evaluate(() => (window as unknown as { editNative: () => void }).editNative());
    await page.getByRole('button', { name: 'Push changes for review' }).click();
    await expect(page.getByRole('alert')).toContainText('Offline transport');
    await page.getByLabel('Change request name').fill('Changed after failed transport');
    await page.getByRole('button', { name: 'Push changes for review' }).click();
    await expect(page.getByRole('status')).toContainText('sent for review');
    expect(uploads[1]).toEqual(uploads[0]);
    await page.getByRole('button', { name: 'Preview team changes' }).click();
    await page.getByRole('button', { name: /Apply preview to/ }).click();
    await expect(page.getByRole('status')).toContainText('Team changes applied');
    expect(
      await page.evaluate(() =>
        (window as unknown as { nativeBlocks: () => unknown }).nativeBlocks()
      )
    ).toEqual(host === 'excel' ? [7, '=A1*3'] : ['Edited']);
  });
}
for (const host of ['google-sheets', 'google-slides'] as const) {
  test(`${host} sidebar uses the shared field queue and scoped host bridge`, async ({ page }) => {
    const c = checkpoint(host),
      base = projectCheckpoint(c),
      sheet = host === 'google-sheets';
    await page.route('https://appsforoffice.microsoft.com/lib/1/hosted/office.js', (r) =>
      r.fulfill({
        contentType: 'text/javascript',
        body: "const HOST='Excel';" + officeBase + excel
      })
    );
    await page.goto('/connectors/excel');
    await page.evaluate(
      ({ c, base, host, request, actor }) => {
        let generation: string | null = null,
          ledger: unknown = null,
          blocks = base,
          success: (value: unknown) => void,
          failure: (error: Error) => void;
        const state = window as unknown as {
          google: unknown;
          editNative: () => void;
          uploads: unknown[];
        };
        state.uploads = [];
        state.editNative = () => {
          blocks = blocks.map((b, i) => ({
            ...b,
            text: i === 0 ? (host === 'google-sheets' ? '7' : 'Edited') : b.text
          }));
        };
        const methods: Record<string, (...args: unknown[]) => unknown> = {
          connectDynodoc: () => ({ checkpoint: c, scope: 'dynodoc.native' }),
          loadLedger: () => ({ generation, payload: ledger }),
          saveLedger: (value, expected) => {
            if (expected !== generation) throw new Error('Stale queue');
            ledger = value;
            return (generation = crypto.randomUUID());
          },
          bindDynodoc: (value, config) => {
            if (!(config as { targets: unknown[] }).targets.length)
              throw new Error('Missing block metadata');
            return value;
          },
          readDynodoc: () => blocks,
          pushDynodoc: (body) => {
            state.uploads.push(body);
            if (state.uploads.length === 1) throw new Error('Offline transport');
            return {
              id: request,
              bundle_id: (body as { bundle: { bundle_id: string } }).bundle.bundle_id,
              digest: 'c'.repeat(64),
              uploaded_by: actor,
              submitted: true
            };
          }
        };
        const bridge: unknown = new Proxy(
          {},
          {
            get: (_target, method: string) => {
              if (method === 'withSuccessHandler')
                return (fn: typeof success) => {
                  success = fn;
                  return bridge;
                };
              if (method === 'withFailureHandler')
                return (fn: typeof failure) => {
                  failure = fn;
                  return bridge;
                };
              return (...args: unknown[]) => {
                try {
                  if (method !== 'connectDynodoc' && args.at(-1) !== 'dynodoc.native')
                    throw new Error('Wrong scope');
                  success(methods[method](...args));
                } catch (e) {
                  failure(e as Error);
                }
              };
            }
          }
        );
        state.google = { script: { run: bridge } };
      },
      { c, base, host, request: REQUEST, actor: ACTOR }
    );
    const html = readFileSync(`../../extensions/${host}/Sidebar.html`, 'utf8').replace(
      "<?!= include('Protocol'); ?>",
      readFileSync(`../../extensions/${host}/Protocol.html`, 'utf8')
    );
    await page.setContent(html);
    await page.locator('#documentId').fill(DOC);
    await page.locator('#token').fill('a'.repeat(64));
    await page.getByRole('button', { name: 'Connect', exact: true }).click();
    await expect(page.getByRole('status')).toContainText('Connected');
    await expect(page.locator('#enabled')).not.toBeChecked();
    await page.evaluate(() => (window as unknown as { editNative: () => void }).editNative());
    await page.getByRole('button', { name: 'Push for review' }).click();
    await expect(page.getByRole('status')).toContainText('Offline transport');
    await page.getByRole('button', { name: 'Push for review' }).click();
    await expect(page.getByRole('status')).toContainText('Sent for review');
    const uploads = await page.evaluate(
      () =>
        (
          window as unknown as {
            uploads: {
              bundle: {
                changes: { operation: { field: string; value: unknown } }[];
                capture: { host: string };
              };
            }[];
          }
        ).uploads
    );
    expect(uploads[1]).toEqual(uploads[0]);
    expect(uploads[0].bundle.capture.host).toBe(host);
    expect(uploads[0].bundle.changes[0].operation).toMatchObject({
      field: sheet ? 'cell_0' : 'text',
      value: sheet ? '7' : 'Edited'
    });
    const download = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Export local provenance' }).click();
    const saved = JSON.parse(readFileSync((await (await download).path())!, 'utf8'));
    expect(saved.host).toBe(host);
    expect(saved.ledger.awaiting.id).toBe(REQUEST);
  });
}
