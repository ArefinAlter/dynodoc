import { describe, it, expect, vi } from 'vitest';
import { webcrypto } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runInNewContext } from 'node:vm';
import { dateText, readExcelDates, writeExcelDates, type DateFunctions } from './date';
import { ExcelAdapter, type ExcelRuntime } from './office-structured';
import {
  CaptureSession,
  projectCheckpoint,
  structuredBinding,
  type Checkpoint,
  type Ledger
} from './protocol';
import { exportSidecar, parseSidecar, recoveryLedger } from './sidecar';
vi.stubGlobal('crypto', webcrypto);
const root = '01ARZ3NDEKTSV4RRFFQ69G5FAV',
  group = '01ARZ3NDEKTSV4RRFFQ69G5FAW',
  row = '01ARZ3NDEKTSV4RRFFQ69G5FAX';
const day = '2026-10-06T00:00:00.000Z';
function checkpoint(): Checkpoint {
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
    title: 'Calendar',
    kind: 'spreadsheet',
    through_seq: 3,
    chain_hash: 'a'.repeat(64),
    state: {
      nodes: {
        [root]: node(root, 'form', null, {}),
        [group]: node(group, 'section', root, { kind: 'sheet', label: 'Calendar', columns: 2 }),
        [row]: node(row, 'item', group, {
          cell_0: day,
          format_0: { type: 'date', numFmt: 'yyyy-mm-dd' },
          cell_1: '2',
          format_1: { type: 'number' }
        })
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
// Model the host function service, including its two possible date systems.
// Production code never uses this serial conversion or a guessed epoch.
function nativeFunctions(epoch: 1900 | 1904) {
  const origin = Date.parse(epoch === 1904 ? '1904-01-01T00:00:00Z' : '1899-12-31T00:00:00Z');
  const serial = (iso: string) => {
    const days = (Date.parse(iso) - origin) / 86400000;
    return days + (epoch === 1900 && days >= 60 ? 1 : 0);
  };
  const parts = (n: number) => {
    if (epoch === 1900 && n === 60) return [1900, 2, 29];
    const d = new Date(origin + (n - (epoch === 1900 && n > 60 ? 1 : 0)) * 86400000);
    return [d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate()];
  };
  const result = (value: number) => ({ value, error: null, load: () => {} });
  const functions: DateFunctions = {
    year: (n) => result(parts(n)[0]),
    month: (n) => result(parts(n)[1]),
    day: (n) => result(parts(n)[2]),
    date: (y, m, d) =>
      result(
        serial(
          `${String(y).padStart(4, '0')}-${String(m).padStart(2, '0')}-${String(d).padStart(2, '0')}T00:00:00.000Z`
        )
      )
  };
  return { functions, serial };
}
function excel(epoch: 1900 | 1904) {
  const f = nativeFunctions(epoch);
  const cells: (string | number | boolean)[] = [f.serial(day), 2];
  let merged = false;
  const writes = vi.fn();
  type Ctx = Parameters<Parameters<ExcelRuntime['run']>[0]>[0];
  type NativeRange = ReturnType<Ctx['workbook']['names']['getItemOrNullObject']>['getRange'];
  const named = new Map<string, ReturnType<NativeRange>>();
  function range(column = 0, count = 2): ReturnType<NativeRange> {
    return {
      get values() {
        return [cells.slice(column, column + count)];
      },
      set values(value) {
        writes();
        value[0].forEach((v, i) => {
          cells[column + i] = v;
        });
      },
      get formulas() {
        return [cells.slice(column, column + count)];
      },
      set formulas(value) {
        writes();
        value[0].forEach((v, i) => {
          cells[column + i] = v;
        });
      },
      rowIndex: 0,
      columnIndex: column,
      rowCount: 1,
      columnCount: count,
      isNullObject: false,
      worksheet: { name: 'Calendar', load: () => {} },
      load: () => {},
      getCell: (_r, c) => range(c, 1),
      getMergedAreasOrNullObject: () => ({ isNullObject: !merged, load: () => {} })
    };
  }
  const ctx: Ctx = {
    sync: async () => {},
    workbook: {
      functions: f.functions,
      worksheets: {
        items: [
          {
            name: 'Calendar',
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
  return {
    adapter: new ExcelAdapter({ run: async (fn) => fn(ctx) }),
    cells,
    writes,
    named,
    merge: () => {
      merged = true;
    },
    ...f
  };
}
function google(zone: string) {
  const named = new Map<string, ReturnType<typeof range>>();
  const cells: (string | number | Date)[] = [];
  let currentZone = zone;
  const formatDate = vi.fn((value: Date, timeZone: string) => {
    const parts = new Intl.DateTimeFormat('en-US', {
      timeZone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      fractionalSecondDigits: 3,
      hourCycle: 'h23'
    }).formatToParts(value);
    const part = (type: string) => parts.find((p) => p.type === type)!.value;
    return `${part('year').padStart(4, '0')}-${part('month')}-${part('day')}T${part('hour')}:${part('minute')}:${part('second')}.${part('fractionalSecond')}`;
  });
  const parseDate = vi.fn((text: string, timeZone: string) => {
    const utc = Date.parse(text + 'Z');
    let instant = utc;
    for (let i = 0; i < 3; i++)
      instant += utc - Date.parse(formatDate(new Date(instant), timeZone) + 'Z');
    return new Date(instant);
  });
  cells.push(parseDate(day.slice(0, -1), zone), 2);
  const writes = vi.fn();
  const sheet = {
    getName: () => 'Calendar',
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
      getValues: () => [cells],
      getFormulas: () => [cells.map((v) => (typeof v === 'string' && v.startsWith('=') ? v : ''))],
      getCell: (_r: number, c: number) => range(c - 1),
      setValue: (value: string | number | Date) => {
        writes();
        cells[column] = value;
      },
      setFormula: (value: string) => {
        writes();
        cells[column] = value;
      }
    };
  }
  const doc = {
    getId: () => 'calendar',
    getSpreadsheetTimeZone: () => currentZone,
    getSheets: () => [sheet],
    getSheetByName: () => sheet,
    getNamedRanges: () => [...named.keys()].map((name) => ({ getName: () => name })),
    getRangeByName: (name: string) => named.get(name) ?? null,
    setNamedRange: (name: string, r: ReturnType<typeof range>) => named.set(name, r)
  };
  const context: Record<string, unknown> = {
    Date,
    SpreadsheetApp: { getActiveSpreadsheet: () => doc, flush: () => {} },
    Utilities: { formatDate, parseDate }
  };
  for (const file of ['Date.gs', 'Common.gs', 'Code.gs'])
    runInNewContext(
      readFileSync(resolve(process.cwd(), '../../extensions/google-sheets', file), 'utf8'),
      context
    );
  return {
    g: context as Record<string, (...args: unknown[]) => unknown>,
    cells,
    writes,
    parseDate,
    formatDate,
    zone: (next: string) => {
      currentZone = next;
    }
  };
}
describe('civil date contract', () => {
  it('accepts real calendar days, blanks and formula source without locale guessing', () => {
    expect(dateText('2024-02-29')).toBe('2024-02-29T00:00:00.000Z');
    expect(dateText('')).toBe('');
    expect(dateText('=DATE(2026,10,6)')).toBe('=DATE(2026,10,6)');
    for (const invalid of [
      '2023-02-29',
      '1900-02-29',
      '1899-12-31',
      '06/10/2026',
      '2026-10-06T01:00:00.000Z',
      '2026-10-06T00:00:00+06:00'
    ])
      expect(() => dateText(invalid)).toThrow();
  });
  for (const epoch of [1900, 1904] as const) {
    it(`round-trips native Excel ${epoch} dates and leaves formulas intact`, async () => {
      const x = excel(epoch),
        c = checkpoint(),
        base = projectCheckpoint(c);
      x.adapter.configure(c);
      await x.adapter.bind(base);
      x.cells[0] = x.serial('2024-02-29T00:00:00.000Z');
      const local = await x.adapter.read();
      expect(local[0].text).toBe('2024-02-29T00:00:00.000Z');
      const next = local.map((b, i) => ({ ...b, text: i ? '3' : '2026-10-07T00:00:00.000Z' }));
      await x.adapter.apply(local, next);
      expect(x.cells).toEqual([x.serial(next[0].text), 3]);
      expect(await x.adapter.read()).toEqual(next);
      const formula = next.map((b, i) => ({ ...b, text: i ? b.text : '=DATE(2026,10,8)' }));
      await x.adapter.apply(next, formula);
      expect(await x.adapter.read()).toEqual(formula);
    });
  }
  it('rejects fictional Excel dates, fractional times and failed native round-trips', async () => {
    const f = nativeFunctions(1900),
      ctx = { workbook: { functions: f.functions }, sync: async () => {} };
    await expect(readExcelDates(ctx, [60])).rejects.toThrow('calendar');
    await expect(readExcelDates(ctx, [f.serial(day) + 0.25])).rejects.toThrow('time');
    await expect(readExcelDates(ctx, ['2026-10-06'])).rejects.toThrow('type');
    f.functions.date = () => ({ value: 1, error: null, load: () => {} });
    await expect(readExcelDates(ctx, [f.serial(day)])).rejects.toThrow('round-trip');
    f.functions.date = () => ({ value: 0, error: '#VALUE!', load: () => {} });
    await expect(writeExcelDates(ctx, [day])).rejects.toThrow('round-trip');
  });
  it('preflights every Excel cell and refuses merges before binding or writing', async () => {
    const x = excel(1900),
      c = checkpoint(),
      base = projectCheckpoint(c);
    x.adapter.configure(c);
    await x.adapter.bind(base);
    const next = base.map((b, i) => ({ ...b, text: i ? '3' : '2026-02-30T00:00:00.000Z' }));
    await expect(x.adapter.apply(base, next)).rejects.toThrow('calendar');
    expect(x.writes).not.toHaveBeenCalled();
    x.merge();
    await expect(x.adapter.apply(base, base)).rejects.toThrow('Merged');
    await expect(x.adapter.bind(base)).rejects.toThrow('Merged');
    expect(x.named.size).toBe(1);
    expect(x.writes).not.toHaveBeenCalled();
  });
  for (const zone of ['Asia/Dhaka', 'America/New_York', 'Pacific/Kiritimati']) {
    it(`Sheets uses ${zone} for reading and writing without changing the calendar day`, () => {
      const x = google(zone),
        c = checkpoint(),
        config = structuredBinding(c),
        base = projectCheckpoint(c);
      expect(x.g.bindDynodoc(base, config, 'dynodoc.calendar')).toEqual(base);
      const next = base.map((b, i) => ({ ...b, text: i ? b.text : '2026-10-07T00:00:00.000Z' }));
      expect(x.g.applyDynodoc(base, next, config, 'dynodoc.calendar')).toEqual(next);
      expect(x.parseDate).toHaveBeenCalledWith(
        '2026-10-07T00:00:00.000',
        zone,
        "yyyy-MM-dd'T'HH:mm:ss.SSS"
      );
      expect(x.cells[0]).toBeInstanceOf(Date);
      expect(x.writes).toHaveBeenCalledTimes(1);
    });
  }
  it('Sheets stops hidden times, invalid dates, skipped local days and late edits before writes', () => {
    const x = google('America/New_York'),
      c = checkpoint(),
      config = structuredBinding(c),
      base = projectCheckpoint(c);
    x.g.bindDynodoc(base, config, 'dynodoc.calendar');
    x.cells[0] = new Date((x.cells[0] as Date).getTime() + 1);
    expect(() => x.g.readDynodoc(config, 'dynodoc.calendar')).toThrow('time');
    x.cells[0] = x.parseDate(day.slice(0, -1), 'America/New_York');
    const invalid = base.map((b, i) => ({ ...b, text: i ? '3' : '2026-02-30T00:00:00.000Z' }));
    expect(() => x.g.applyDynodoc(base, invalid, config, 'dynodoc.calendar')).toThrow('calendar');
    const next = base.map((b, i) => ({ ...b, text: i ? '3' : '2026-10-07T00:00:00.000Z' }));
    x.parseDate.mockReturnValueOnce(new Date((x.cells[0] as Date).getTime() + 3600000));
    expect(() => x.g.applyDynodoc(base, next, config, 'dynodoc.calendar')).toThrow('round-trip');
    x.cells[1] = 4;
    expect(() => x.g.applyDynodoc(base, next, config, 'dynodoc.calendar')).toThrow('after preview');
    expect(x.writes).not.toHaveBeenCalled();
  });
  it('Sheets refuses a timezone change during preflight and keeps the file untouched', () => {
    const x = google('Asia/Dhaka'),
      c = checkpoint(),
      config = structuredBinding(c),
      base = projectCheckpoint(c);
    x.g.bindDynodoc(base, config, 'dynodoc.calendar');
    const normal = x.parseDate.getMockImplementation()!;
    x.parseDate.mockImplementationOnce((text, zone) => {
      x.zone('UTC');
      return normal(text, zone);
    });
    const next = base.map((b, i) => ({ ...b, text: i ? b.text : '2026-10-07T00:00:00.000Z' }));
    expect(() => x.g.applyDynodoc(base, next, config, 'dynodoc.calendar')).toThrow(
      'timezone changed'
    );
    expect(x.writes).not.toHaveBeenCalled();
  });
  it('rechecks both native files after calendar conversion and stops concurrent typing', async () => {
    const c = checkpoint(),
      base = projectCheckpoint(c),
      next = base.map((b, i) => ({ ...b, text: i ? b.text : '2026-10-07T00:00:00.000Z' }));
    const x = excel(1900);
    x.adapter.configure(c);
    await x.adapter.bind(base);
    const nativeDate = x.functions.date;
    x.functions.date = (y, m, d) => {
      if (y === 2026 && m === 10 && d === 7) x.cells[1] = 3;
      return nativeDate(y, m, d);
    };
    await expect(x.adapter.apply(base, next)).rejects.toThrow('after preview');
    expect(x.writes).not.toHaveBeenCalled();
    const g = google('Asia/Dhaka'),
      config = structuredBinding(c);
    g.g.bindDynodoc(base, config, 'dynodoc.calendar');
    const parse = g.parseDate.getMockImplementation()!;
    g.parseDate.mockImplementationOnce((text, zone) => {
      g.cells[1] = 3;
      return parse(text, zone);
    });
    expect(() => g.g.applyDynodoc(base, next, config, 'dynodoc.calendar')).toThrow('after preview');
    expect(g.writes).not.toHaveBeenCalled();
  });
  it('keeps blank date cells distinct from serial zero in a 1904 workbook', async () => {
    const c = checkpoint();
    c.state.nodes[row].current_fields.cell_0 = '';
    const base = projectCheckpoint(c),
      x = excel(1904);
    x.cells[0] = '';
    x.adapter.configure(c);
    await x.adapter.bind(base);
    const next = base.map((b, i) => ({ ...b, text: i ? b.text : '1904-01-01T00:00:00.000Z' }));
    await x.adapter.apply(base, next);
    expect(x.cells[0]).toBe(0);
    expect(await x.adapter.read()).toEqual(next);
    const g = google('Asia/Dhaka'),
      config = structuredBinding(c);
    g.cells[0] = '';
    expect(g.g.bindDynodoc(base, config, 'dynodoc.calendar')).toEqual(base);
  });
  for (const host of ['excel', 'google-sheets'] as const) {
    it(`${host} retains date observations, exact retries and recoverable history`, async () => {
      const c = checkpoint(),
        base = projectCheckpoint(c),
        session = await CaptureSession.connect(store(), c, base, host);
      await session.capture(
        base.map((b, i) => ({ ...b, text: i ? b.text : '2026-10-07T00:00:00.000Z' }))
      );
      const pending = await session.prepare('Date change', '', true);
      expect(pending.bundle.changes[0].operation.value).toBe('2026-10-07T00:00:00.000Z');
      const sidecar = parseSidecar(JSON.stringify(exportSidecar(session)));
      expect(recoveryLedger(sidecar, c, host).pending).toEqual(pending);
      await expect(
        session.capture(
          base.map((b, i) => ({ ...b, text: i ? b.text : '2026-10-08T12:00:00.000Z' }))
        )
      ).rejects.toThrow('time');
      expect(await session.prepare('Retry', '', false)).toEqual(pending);
    });
  }
  it('rejects a forged superseded date observation even when its final state is valid', async () => {
    const c = checkpoint(),
      base = projectCheckpoint(c),
      session = await CaptureSession.connect(store(), c, base, 'excel');
    await session.capture(
      base.map((b, i) => ({ ...b, text: i ? b.text : '2026-10-07T00:00:00.000Z' }))
    );
    await session.capture(
      base.map((b, i) => ({ ...b, text: i ? b.text : '2026-10-08T00:00:00.000Z' }))
    );
    const sidecar = exportSidecar(session);
    sidecar.ledger.changes[0].operation.value = '2026-02-30T00:00:00.000Z';
    expect(() => parseSidecar(JSON.stringify(sidecar))).toThrow('calendar');
  });
  it('previews Excel date observations for a Sheets working copy and stops conflicting dates', async () => {
    const c = checkpoint(),
      base = projectCheckpoint(c),
      session = await CaptureSession.connect(store(), c, base, 'google-sheets');
    const x = excel(1904);
    x.adapter.configure(c);
    await x.adapter.bind(base);
    x.cells[0] = x.serial('2026-10-07T00:00:00.000Z');
    const remote = structuredClone(c);
    remote.through_seq++;
    remote.chain_hash = 'b'.repeat(64);
    remote.state.nodes[row].current_fields.cell_0 = (await x.adapter.read())[0].text;
    const next = session.plan(remote, base).map(({ id, field, text }) => ({ id, field, text }));
    const g = google('Asia/Dhaka'),
      config = structuredBinding(c);
    g.g.bindDynodoc(base, config, 'dynodoc.calendar');
    expect(g.g.applyDynodoc(base, next, config, 'dynodoc.calendar')).toEqual(next);
    await session.acceptPull(remote, next);
    expect(session.ledger.checkpoint.through_seq).toBe(remote.through_seq);
    const later = structuredClone(remote);
    later.through_seq++;
    later.chain_hash = 'c'.repeat(64);
    later.state.nodes[row].current_fields.cell_0 = '2026-10-09T00:00:00.000Z';
    const local = next.map((b, i) => ({ ...b, text: i ? b.text : '2026-10-08T00:00:00.000Z' }));
    expect(session.plan(later, local)[0].conflict).toBe(true);
  });
});
