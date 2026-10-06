import { describe, it, expect, vi } from 'vitest';
import { webcrypto } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runInNewContext } from 'node:vm';
import {
  CaptureSession,
  projectCheckpoint,
  structuredBinding,
  type Checkpoint,
  type Ledger,
  type Host
} from './protocol';
import { exportSidecar, parseSidecar, recoveryLedger } from './sidecar';
import {
  ExcelAdapter,
  PowerPointAdapter,
  cellText,
  cellValue,
  type ExcelRuntime,
  type PowerPointRuntime
} from './office-structured';
vi.stubGlobal('crypto', webcrypto);
const root = '01ARZ3NDEKTSV4RRFFQ69G5FAV',
  group = '01ARZ3NDEKTSV4RRFFQ69G5FAW',
  item = '01ARZ3NDEKTSV4RRFFQ69G5FAX';
function checkpoint(kind = 'spreadsheet'): Checkpoint {
  const node = (
    id: string,
    type: string,
    parent_id: string | null,
    current_fields: Record<string, unknown>
  ) => ({ id, type, parent_id, current_fields, pos: '0', deleted: false });
  return {
    format: 'dynodoc.checkpoint',
    version: 1,
    document_id: '7034d450-77cf-44f0-8eec-63f892643f8e',
    title: 'Native file',
    kind,
    through_seq: 3,
    chain_hash: 'a'.repeat(64),
    state: {
      nodes: {
        [root]: node(root, 'form', null, {}),
        [group]: node(
          group,
          'section',
          root,
          kind === 'spreadsheet'
            ? { kind: 'sheet', label: 'Sheet1', columns: 2 }
            : { kind: 'slide' }
        ),
        [item]: node(
          item,
          'item',
          group,
          kind === 'spreadsheet'
            ? {
                cell_0: '2',
                cell_1: '=A1*2',
                format_0: { type: 'number' },
                format_1: { type: 'number' }
              }
            : { kind: 'text', text: 'Original', style: { bold: true } }
        )
      }
    }
  };
}
function store() {
  let saved: Ledger | null = null;
  return {
    load: async () => saved,
    save: async (value: Ledger) => {
      saved = structuredClone(value);
    }
  };
}
describe('structured capture uses the same durable contract', () => {
  for (const [host, kind] of [
    ['excel', 'spreadsheet'],
    ['google-sheets', 'spreadsheet'],
    ['powerpoint', 'presentation'],
    ['google-slides', 'presentation']
  ] as [Host, string][]) {
    it(`${host}: captures field operations and restores a frozen retry`, async () => {
      const c = checkpoint(kind),
        base = projectCheckpoint(c),
        local = base.map((b, i) => ({
          ...b,
          text: i === 0 ? (kind === 'spreadsheet' ? '7' : 'Edited text') : b.text
        }));
      const session = await CaptureSession.connect(store(), c, base, host);
      await session.capture(local);
      const pending = await session.prepare('Native changes', '', true);
      expect(pending.bundle.changes[0].operation).toEqual({
        type: 'FieldEdited',
        node_id: item,
        field: kind === 'spreadsheet' ? 'cell_0' : 'text',
        value: local[0].text
      });
      const s = parseSidecar(JSON.stringify(exportSidecar(session)));
      expect(recoveryLedger(s, c, host).pending).toEqual(pending);
      expect(await session.prepare('Changed title', 'Changed intent', false)).toEqual(pending);
    });
  }
  it('merges separate cells on the same stable row and stops on the same cell', async () => {
    const c = checkpoint(),
      base = projectCheckpoint(c),
      session = await CaptureSession.connect(store(), c, base, 'excel');
    const local = base.map((b, i) => ({ ...b, text: i === 0 ? '7' : b.text })),
      remote = structuredClone(c);
    remote.through_seq++;
    remote.chain_hash = 'b'.repeat(64);
    remote.state.nodes[item].current_fields.cell_1 = '=A1*3';
    expect(session.plan(remote, local).map((r) => r.text)).toEqual(['7', '=A1*3']);
    remote.state.nodes[item].current_fields.cell_0 = '9';
    expect(session.plan(remote, local)[0].conflict).toBe(true);
  });
  it('refuses formatting, empty-slide structure and editor-kind changes', async () => {
    const c = checkpoint('presentation'),
      base = projectCheckpoint(c),
      session = await CaptureSession.connect(store(), c, base, 'powerpoint');
    const remote = structuredClone(c);
    remote.through_seq++;
    remote.chain_hash = 'b'.repeat(64);
    remote.state.nodes[item].current_fields.style = { bold: false };
    expect(() => session.plan(remote, base)).toThrow('formatting');
    await expect(CaptureSession.connect(store(), c, base, 'excel')).rejects.toThrow('format');
  });
  it('rejects forged sidecar fields, date cells and merged ranges', async () => {
    const c = checkpoint(),
      session = await CaptureSession.connect(store(), c, projectCheckpoint(c), 'excel');
    await session.capture(projectCheckpoint(c).map((b, i) => ({ ...b, text: i ? '=A1*4' : '2' })));
    const s = exportSidecar(session);
    s.ledger.changes[0].operation.field = 'cell_99';
    expect(() => parseSidecar(JSON.stringify(s))).toThrow('Invalid');
    c.state.nodes[item].current_fields.format_0 = { type: 'date' };
    expect(() => projectCheckpoint(c)).toThrow('Date');
    c.state.nodes[item].current_fields.format_0 = {};
    c.state.nodes[group].current_fields.sheet_layout = { merges: [{}] };
    expect(() => projectCheckpoint(c)).toThrow('Merged');
  });
});
describe('native Office adapter boundaries', () => {
  function excel() {
    let values: (string | number | boolean)[][] = [[2, '=A1*2']];
    type Ctx = Parameters<Parameters<ExcelRuntime['run']>[0]>[0];
    type NativeRange = ReturnType<Ctx['workbook']['names']['getItemOrNullObject']>['getRange'];
    const named = new Map<string, ReturnType<NativeRange>>();
    function range(column = 0, count = 2) {
      return {
        get values() {
          return [values[0].slice(column, column + count)];
        },
        set values(next: (string | number | boolean)[][]) {
          next[0].forEach((v, i) => (values[0][column + i] = v));
        },
        get formulas() {
          return [values[0].slice(column, column + count)];
        },
        set formulas(next: (string | number | boolean)[][]) {
          next[0].forEach((v, i) => (values[0][column + i] = v));
        },
        rowIndex: 0,
        columnIndex: column,
        rowCount: 1,
        columnCount: count,
        isNullObject: false,
        worksheet: { name: 'Sheet1', load: () => {} },
        load: () => {},
        getCell: (_r: number, c: number) => range(c, 1)
      };
    }
    const ctx: Ctx = {
      sync: async () => {},
      workbook: {
        worksheets: {
          items: [
            {
              name: 'Sheet1',
              getUsedRangeOrNullObject: () => range(),
              getRangeByIndexes: () => range()
            }
          ],
          load: () => {}
        },
        names: {
          get items() {
            return [...named.keys()].map((name) => ({
              name,
              isNullObject: false,
              load: () => {},
              getRange: () => named.get(name)!
            }));
          },
          load: () => {},
          getItemOrNullObject: (name) => ({
            name,
            isNullObject: !named.has(name),
            load: () => {},
            getRange: () => named.get(name)!
          }),
          add: (name, r) => named.set(name, r)
        }
      }
    };
    const runtime: ExcelRuntime = { run: async (callback) => callback(ctx) };
    return {
      adapter: new ExcelAdapter(runtime),
      named,
      set: (next: (string | number | boolean)[][]) => (values = next),
      values: () => values
    };
  }
  it('binds named rows, captures formulas and writes only changed cells', async () => {
    const x = excel(),
      c = checkpoint(),
      base = projectCheckpoint(c);
    x.adapter.configure(c);
    await x.adapter.bind(base);
    expect(x.named.size).toBe(1);
    x.set([[5, '=A1*2']]);
    const expected = await x.adapter.read();
    expect(expected[0].text).toBe('5');
    const next = expected.map((b, i) => ({ ...b, text: i ? '=A1*3' : b.text }));
    await x.adapter.apply(expected, next);
    expect(x.values()).toEqual([[5, '=A1*3']]);
    x.set([[6, '=A1*3']]);
    await expect(x.adapter.apply(expected, next)).rejects.toThrow('after preview');
  });
  it('preserves foreign or moved row anchors', async () => {
    const x = excel(),
      c = checkpoint();
    x.adapter.configure(c);
    await x.adapter.bind(projectCheckpoint(c));
    x.named.get('dynodoc_' + item)!.worksheet.name = 'AnotherSheet';
    await expect(x.adapter.read()).rejects.toThrow('identity');
    expect(x.named.size).toBe(1);
  });
  it('keeps shape tags stable, refuses re-identification and previews before writes', async () => {
    type Ctx = Parameters<Parameters<PowerPointRuntime['run']>[0]>[0];
    const tags = () => ({
      items: [] as { key: string; value: string }[],
      load: () => {},
      add(key: string, value: string) {
        this.items.push({ key, value });
      }
    });
    const shape = {
        type: 'TextBox',
        tags: tags(),
        textFrame: { textRange: { text: 'Original', load: () => {} } }
      },
      slide = { tags: tags(), shapes: { items: [shape], load: () => {} } };
    const ctx: Ctx = {
        sync: async () => {},
        presentation: { slides: { items: [slide], load: () => {} } }
      },
      runtime: PowerPointRuntime = { run: async (callback) => callback(ctx) },
      adapter = new PowerPointAdapter(runtime),
      c = checkpoint('presentation'),
      base = projectCheckpoint(c);
    adapter.configure(c);
    await adapter.bind(base);
    expect(shape.tags.items[0].value).toBe(item);
    expect(slide.tags.items[0].value).toBe(group);
    const next = base.map((b) => ({ ...b, text: 'Team text' }));
    await adapter.apply(base, next);
    expect(await adapter.read()).toEqual(next);
    shape.textFrame.textRange.text = 'Late typing';
    await expect(adapter.apply(next, base)).rejects.toThrow('after preview');
    shape.tags.items[0].value = root;
    await expect(adapter.bind(base)).rejects.toThrow('identity');
    expect(shape.tags.items[0].value).toBe(root);
  });
  it('preserves scalar types and stops formula-like literal text', () => {
    expect(cellText(true, '', 'boolean')).toBe('true');
    expect(cellValue('false', 'boolean')).toBe(false);
    expect(cellValue('7', 'number')).toBe(7);
    expect(() => cellText('7', '', 'number')).toThrow('type');
    expect(() => cellText('=literal', '', 'text')).toThrow('Literal');
    expect(() => cellText('=A1', '=A1', 'text')).toThrow('formula');
  });
});
describe('Google native adapters (mock Apps Script services)', () => {
  function google(host: string, service: Record<string, unknown>) {
    const properties = new Map<string, string>();
    const props = {
      getProperty: (k: string) => properties.get(k) ?? null,
      setProperty: (k: string, v: string) => properties.set(k, v),
      deleteProperty: (k: string) => properties.delete(k)
    };
    const context: Record<string, unknown> = {
      ...service,
      PropertiesService: { getUserProperties: () => props },
      Utilities: {
        getUuid: () => webcrypto.randomUUID(),
        newBlob: (text: string) => ({ getBytes: () => new TextEncoder().encode(text) })
      },
      LockService: { getUserLock: () => ({ waitLock: () => {}, releaseLock: () => {} }) }
    };
    runInNewContext(
      readFileSync(resolve(process.cwd(), '../../extensions', host, 'Code.gs'), 'utf8'),
      context
    );
    runInNewContext(
      readFileSync(resolve(process.cwd(), '../../extensions', host, 'Common.gs'), 'utf8'),
      context
    );
    return context as Record<string, (...args: unknown[]) => unknown>;
  }
  it('Sheets retains named row identities and refuses late edits', () => {
    const named = new Map<string, ReturnType<typeof range>>();
    const values: (string | number | boolean)[] = [2, '=A1*2'];
    const sheet = {
      getName: () => 'Sheet1',
      getSheetId: () => 1,
      getLastRow: () => 1,
      getLastColumn: () => 2,
      getRange: () => range()
    };
    function range(column = 0) {
      return {
        getSheet: () => sheet,
        getRow: () => 1,
        getColumn: () => 1,
        getNumRows: () => 1,
        getNumColumns: () => 2,
        getMergedRanges: () => [],
        getValues: () => [values.map((v) => (typeof v === 'string' && v.startsWith('=') ? 4 : v))],
        getFormulas: () => [
          values.map((v) => (typeof v === 'string' && v.startsWith('=') ? v : ''))
        ],
        getCell: (_r: number, c: number) => range(c - 1),
        setValue: (v: string | number | boolean) => (values[column] = v),
        setFormula: (v: string) => (values[column] = v)
      };
    }
    const doc = {
      getId: () => 'sheet-id',
      getSheets: () => [sheet],
      getSheetByName: () => sheet,
      getNamedRanges: () => [...named.keys()].map((name) => ({ getName: () => name })),
      getRangeByName: (name: string) => named.get(name) ?? null,
      setNamedRange: (name: string, r: ReturnType<typeof range>) => named.set(name, r)
    };
    const g = google('google-sheets', {
        SpreadsheetApp: { getActiveSpreadsheet: () => doc, flush: () => {} }
      }),
      c = checkpoint(),
      config = structuredBinding(c),
      base = projectCheckpoint(c);
    expect(g.bindDynodoc(base, config, 'dynodoc.sheet-id')).toEqual(base);
    expect(named.size).toBe(1);
    const next = base.map((b, i) => ({ ...b, text: i ? '=A1*3' : b.text }));
    expect(g.applyDynodoc(base, next, config, 'dynodoc.sheet-id')).toEqual(next);
    values[0] = 7;
    expect(() => g.applyDynodoc(next, base, config, 'dynodoc.sheet-id')).toThrow('after preview');
    expect(() => g.readDynodoc(config, 'dynodoc.other')).toThrow('connected');
  });
  it('Slides binds native IDs privately, preserves alt text and rejects copies with changed IDs', () => {
    let text = 'Original\n',
      nativeId = 'shape-1';
    const element = {
      getPageElementType: () => 'SHAPE',
      getObjectId: () => nativeId,
      asShape: () => ({
        getText: () => ({ asString: () => text, setText: (value: string) => (text = value + '\n') })
      })
    };
    const slide = { getObjectId: () => 'slide-1', getPageElements: () => [element] },
      slides = [slide],
      doc = { getId: () => 'deck-id', getSlides: () => slides };
    const g = google('google-slides', { SlidesApp: { getActivePresentation: () => doc } }),
      c = checkpoint('presentation'),
      config = structuredBinding(c),
      base = projectCheckpoint(c);
    expect(g.bindDynodoc(base, config, 'dynodoc.deck-id')).toEqual(base);
    const next = base.map((b) => ({ ...b, text: 'New team text' }));
    expect(g.applyDynodoc(base, next, config, 'dynodoc.deck-id')).toEqual(next);
    nativeId = 'copied-shape';
    expect(() => g.readDynodoc(config, 'dynodoc.deck-id')).toThrow('identities');
    nativeId = 'shape-1';
    slides.push(slide);
    expect(() => g.readDynodoc(config, 'dynodoc.deck-id')).toThrow('Slides');
  });
});
