import { describe, expect, it } from 'vitest';
import ExcelJS from 'exceljs';
import { strFromU8, unzipSync } from 'fflate';
import type { DocumentState, EventPayload } from '$lib/api/types';
import { applyPayload, emptyState } from '$lib/materializer';
import { cloneState } from './changes';
import { readOrigin } from './file-push';
import { importFile, itemOp, rootOp, rowsForSheet } from './files';
import { writePptx } from './pptx-export';
import { importPptx } from './pptx-import';
import {
  deleteSlideOps,
  editShapeOps,
  initialPresentationOps,
  moveSlideOps,
  newSlideOps,
  readDeck
} from './presentation';
import {
  planSheetPush,
  planSlidePush,
  pushState,
  sheetMatchScore,
  slideMatchScore
} from './structured-push';
import { writeWorkbook } from './xlsx-export';

const DOC = '94a78b6c-eaf0-4d9a-9a52-5f3f0c2a1b11';
const PIXEL =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

/** jsdom's File cannot be read; the importer only needs these members. */
function upload(buffer: ArrayBuffer, name: string) {
  return {
    name,
    size: buffer.byteLength,
    arrayBuffer: async () => buffer.slice(0),
    text: async () => ''
  } as unknown as File;
}
const readXlsx = (buffer: ArrayBuffer, name = 'Scores (v12).xlsx') =>
  importFile(upload(buffer, name));
/** Simulate editing the downloaded workbook in a spreadsheet program and saving it. */
async function editXlsx(buffer: ArrayBuffer, edit: (book: ExcelJS.Workbook) => void) {
  const book = new ExcelJS.Workbook();
  await book.xlsx.load(buffer);
  edit(book);
  return (await book.xlsx.writeBuffer()) as ArrayBuffer;
}
/** Excel rewrites relative formulas when rows move; mimic that for the test column. */
function renumber(sheet: ExcelJS.Worksheet) {
  for (let r = 2; r <= sheet.rowCount; r++)
    if (sheet.getCell(r, 6).formula) sheet.getCell(r, 6).value = { formula: `B${r}*2` };
}
function sheetNames(state: DocumentState, sheetId: string) {
  return rowsForSheet(state, sheetId).map((row) => row.current_fields.cell_0);
}

const PEOPLE = [
  ['Name', 'Score', 'Team', 'Joined', 'Code', 'Double'],
  ['Tariq', '12', 'Blue', '2026-01-05', '007', '=B2*2'],
  ['Sara', '15.50', 'Red', '2026-02-11', '012', '=B3*2'],
  ['Omar', '9', 'Blue', '2026-03-02', '031', '=B4*2'],
  ['Lina', '20', 'Green', '2026-03-17', '044', '=B5*2'],
  ['Mina', '11', 'Red', '2026-04-01', '052', '=B6*2'],
  ['Rafi', '14', 'Green', '2026-04-20', '066', '=B7*2']
];
const COSTS = [
  ['Item', 'Cost', 'Paid'],
  ['Paper', '4.5', 'TRUE'],
  ['Ink', '12', 'false'],
  ['Total', '=SUM(B2:B3)', '']
];
/** Two sheets in the shape the editor stores them, with most supported formatting. */
function workbook(extras = false): DocumentState {
  const state = emptyState();
  applyPayload(state, rootOp('root', 'Scores'));
  const sheet = (id: string, label: string, pos: string, fields: Record<string, unknown>) =>
    applyPayload(state, {
      type: 'NodeCreated',
      node_id: id,
      node_type: 'section',
      parent_id: 'root',
      pos,
      fields: { label, kind: 'sheet', ...fields }
    });
  sheet('scores', 'Scores', '0', {
    columns: 6,
    sheet_layout: { widths: { 0: 140 }, hiddenColumns: [], frozenRows: 1, frozenColumns: 0 }
  });
  PEOPLE.forEach((cells, r) => {
    const fields: Record<string, unknown> = {};
    cells.forEach((v, c) => (fields[`cell_${c}`] = v));
    if (r === 0)
      cells.forEach((_, c) => (fields[`format_${c}`] = { bold: true, background: '#DDEEFF' }));
    else {
      fields.format_1 = { number: 'decimal', decimals: 2 };
      fields.format_3 = { type: 'date' };
      fields.format_4 = { type: 'text', align: 'center' };
    }
    if (r === 3) fields.format_0 = { italic: true, note: 'Joined late' };
    if (r === 4) fields.row_height = 36;
    applyPayload(state, itemOp('scores', r, fields, `s${r}`));
  });
  sheet('costs', 'Costs', '1', {
    columns: 3,
    ...(extras
      ? {
          print_area: 'A1:C4',
          page_setup: { size: 'Letter', landscape: true, margin: 12.7, footer: 'Budget' },
          charts: [
            {
              id: 'costs-chart',
              title: 'Costs',
              type: 'column',
              range: 'A1:B3',
              x: 0,
              y: 120,
              width: 480,
              height: 300
            }
          ],
          sheet_layout: {
            widths: { 1: 90 },
            hiddenColumns: [],
            frozenRows: 0,
            frozenColumns: 0,
            merges: [{ fromRow: 'c3', toRow: 'c3', fromCol: 1, toCol: 2 }]
          }
        }
      : {})
  });
  COSTS.forEach((cells, r) => {
    const fields: Record<string, unknown> = {};
    cells.forEach((v, c) => (fields[`cell_${c}`] = v));
    if (r === 1 || r === 2) fields.format_2 = { type: 'boolean' };
    if (extras && r === 1)
      Object.assign(fields, {
        format_0: { hyperlink: 'https://example.test/paper', color: '#1155CC' },
        format_1: {
          number: 'currency',
          validation: { type: 'decimal', operator: 'between', formulae: [0, 100] },
          conditional: [
            { operator: 'greaterThan', value: 10, background: '#FFC7CE', color: '#9C0006' }
          ]
        }
      });
    applyPayload(state, itemOp('costs', r, fields, `c${r}`));
  });
  if (extras)
    state.nodes.root.current_fields.named_ranges = [
      { name: 'Budget', expression: '=Costs!$B$2:$B$3' }
    ];
  return state;
}

describe('pushing an edited Excel file', () => {
  it('finds no changes in an untouched download, whatever formatting the sheets carry', async () => {
    const base = workbook(true);
    const incoming = await readXlsx(await writeWorkbook(base));
    const plan = planSheetPush(base, incoming);
    expect(plan.changes).toEqual([]);
    expect(plan.ops).toEqual([]);
    expect(plan.unchanged).toBe(PEOPLE.length + COSTS.length);
    expect(sheetMatchScore(base, incoming)).toBe(1);
  });

  it('finds no changes after the file is opened and saved again', async () => {
    const base = workbook();
    const resaved = await editXlsx(await writeWorkbook(base), () => {});
    const plan = planSheetPush(base, await readXlsx(resaved));
    expect(plan.changes).toEqual([]);
    expect(plan.ops).toEqual([]);
  });

  it('turns an edited cell into one edit of that row', async () => {
    const base = workbook();
    const file = await editXlsx(await writeWorkbook(base), (book) => {
      book.getWorksheet('Scores')!.getCell('B3').value = 16;
    });
    const plan = planSheetPush(base, await readXlsx(file));
    expect(plan.changes).toEqual([
      expect.objectContaining({ kind: 'edited', nodeId: 's2', formatting: false })
    ]);
    expect(plan.changes[0].before).toBe(
      'Scores!A3:F3  "Sara | 15.50 | Red | 2026-02-11 | 012 | =B3*2"'
    );
    expect(plan.changes[0].after).toContain('"Sara | 16 | Red');
    expect(plan.ops).toEqual([
      { type: 'FieldEdited', node_id: 's2', field: 'cell_1', value: '16' }
    ]);
    expect(plan.unchanged).toBe(PEOPLE.length + COSTS.length - 1);
  });

  it('adds an inserted row without disturbing the rows around it', async () => {
    const base = workbook();
    const file = await editXlsx(await writeWorkbook(base), (book) => {
      const sheet = book.getWorksheet('Scores')!;
      sheet.spliceRows(4, 0, [
        'Nadia',
        17,
        'Blue',
        new Date('2026-05-02'),
        '071',
        { formula: 'B4*2' }
      ]);
      // Excel moves comments with their cells; exceljs's splice drops them.
      sheet.getCell('A5').note = 'Joined late';
      renumber(sheet);
    });
    const plan = planSheetPush(base, await readXlsx(file));
    expect(plan.changes).toHaveLength(1);
    expect(plan.changes[0]).toMatchObject({ kind: 'added', before: '' });
    expect(plan.changes[0].after).toMatch(/^Scores!A4:F4 {2}"Nadia \| 17 \| Blue/);
    expect(plan.unchanged).toBe(PEOPLE.length + COSTS.length);
    expect(sheetNames(plan.state, 'scores')).toEqual([
      'Name',
      'Tariq',
      'Sara',
      'Nadia',
      'Omar',
      'Lina',
      'Mina',
      'Rafi'
    ]);
    // Rows below keep their identity; only formulas that followed them are rewritten.
    const added = plan.changes[0].nodeId;
    const others = plan.ops.filter((op) => !('node_id' in op) || op.node_id !== added);
    expect(others).toEqual(
      ['s3', 's4', 's5', 's6'].map((id, i) => ({
        type: 'FieldEdited',
        node_id: id,
        field: 'cell_5',
        value: `=B${i + 5}*2`
      }))
    );
    const pos = (id: string) => plan.state.nodes[id].pos;
    expect(pos('s2') < pos(added) && pos(added) < pos('s3')).toBe(true);
    expect(plan.ops.some((op) => op.type === 'NodeMoved')).toBe(false);
  });

  it('keeps an edited row paired when a similar row is inserted just above it', async () => {
    const base = workbook();
    const file = await editXlsx(await writeWorkbook(base), (book) => {
      const sheet = book.getWorksheet('Scores')!;
      sheet.spliceRows(4, 0, ['Nadia', 9, 'Blue', new Date('2026-03-09'), '032']);
      sheet.getCell('A5').note = 'Joined late';
      sheet.getCell('B5').value = 10;
      renumber(sheet);
    });
    const plan = planSheetPush(base, await readXlsx(file));
    expect(plan.changes.map((c) => [c.kind, c.nodeId === 's3'])).toEqual([
      ['added', false],
      ['edited', true]
    ]);
    expect(plan.state.nodes.s3.current_fields).toMatchObject({ cell_0: 'Omar', cell_1: '10' });
  });

  it('removes a deleted row', async () => {
    const base = workbook();
    const file = await editXlsx(await writeWorkbook(base), (book) => {
      const sheet = book.getWorksheet('Scores')!;
      sheet.spliceRows(5, 1);
      renumber(sheet);
    });
    const plan = planSheetPush(base, await readXlsx(file));
    expect(plan.changes).toEqual([
      {
        kind: 'removed',
        nodeId: 's4',
        before: 'Scores!A5:F5  "Lina | 20 | Green | 2026-03-17 | 044 | =B5*2"',
        after: ''
      }
    ]);
    expect(plan.ops.filter((op) => op.type === 'NodeDeleted')).toEqual([
      { type: 'NodeDeleted', node_id: 's4' }
    ]);
    expect(sheetNames(plan.state, 'scores')).not.toContain('Lina');
  });

  it('renames a sheet and adds a new one', async () => {
    const base = workbook();
    const file = await editXlsx(await writeWorkbook(base), (book) => {
      book.getWorksheet('Scores')!.name = 'Results';
      const notes = book.addWorksheet('Notes');
      notes.addRow(['Topic', 'Owner']);
      notes.addRow(['Venue', 'Sara']);
    });
    const plan = planSheetPush(base, await readXlsx(file));
    expect(plan.changes).toEqual([
      {
        kind: 'edited',
        nodeId: 'scores',
        before: 'Sheet “Scores”',
        after: 'Sheet “Results”',
        formatting: false
      },
      expect.objectContaining({ kind: 'added', before: '' })
    ]);
    expect(plan.changes[1].after).toBe('Sheet “Notes” · 2 rows · Notes!A1:B1  "Topic | Owner"');
    expect(plan.unchanged).toBe(PEOPLE.length + COSTS.length);
    const added = plan.state.nodes[plan.changes[1].nodeId];
    expect(added).toMatchObject({ type: 'section', parent_id: 'root' });
    expect(added.current_fields).toMatchObject({ label: 'Notes', kind: 'sheet' });
    expect(added.pos > plan.state.nodes.costs.pos).toBe(true);
    expect(sheetNames(plan.state, added.id)).toEqual(['Topic', 'Venue']);
    // Sheets are created before their rows.
    const created = plan.ops.filter((op) => op.type === 'NodeCreated');
    expect(created[0]).toMatchObject({ node_id: added.id, node_type: 'section' });
  });

  it('recognises a moved row', async () => {
    const base = workbook();
    const file = await editXlsx(await writeWorkbook(base), (book) => {
      const sheet = book.getWorksheet('Scores')!;
      const source = sheet.getRow(4);
      const values = [1, 2, 3, 4, 5, 6].map((c) => source.getCell(c).value);
      const styles = [1, 2, 3, 4, 5, 6].map((c) => ({ ...source.getCell(c).style }));
      const note = source.getCell(1).note;
      sheet.spliceRows(4, 1);
      const row = sheet.getRow(7);
      values.forEach((value, i) => {
        row.getCell(i + 1).value = value;
        row.getCell(i + 1).style = styles[i];
      });
      row.getCell(1).note = note;
      renumber(sheet);
    });
    const plan = planSheetPush(base, await readXlsx(file));
    expect(plan.changes).toEqual([
      {
        kind: 'moved',
        nodeId: 's3',
        before: 'Scores!A4:F4  "Omar | 9 | Blue | 2026-03-02 | 031 | =B4*2"',
        after: 'Scores!A7:F7  "Omar | 9 | Blue | 2026-03-02 | 031 | =B7*2"'
      }
    ]);
    expect(sheetNames(plan.state, 'scores').slice(-2)).toEqual(['Rafi', 'Omar']);
    expect(plan.state.nodes.s3.current_fields.cell_5).toBe('=B7*2');
    expect(plan.ops.filter((op) => op.type === 'NodeMoved')).toEqual([
      {
        type: 'NodeMoved',
        node_id: 's3',
        new_parent_id: 'scores',
        new_pos: plan.state.nodes.s3.pos
      }
    ]);
  });

  it('reports formatting-only edits and keeps unchanged format settings as stored', async () => {
    const base = workbook();
    const file = await editXlsx(await writeWorkbook(base), (book) => {
      const cell = book.getWorksheet('Scores')!.getCell('C2');
      // Loaded cells share style objects; give this one its own, as Excel would.
      cell.style = { ...cell.style, font: { ...cell.font, bold: true } };
    });
    const plan = planSheetPush(base, await readXlsx(file));
    expect(plan.changes).toEqual([
      expect.objectContaining({ kind: 'edited', nodeId: 's1', formatting: true })
    ]);
    expect(plan.changes[0].after).toContain('formatting C2');
    expect(plan.ops).toEqual([
      { type: 'FieldEdited', node_id: 's1', field: 'format_2', value: { bold: true } }
    ]);
  });

  it('compares a teammate’s copy of the original upload with the imported revision', async () => {
    const original = await writeWorkbook(workbook());
    const base = pushState(await readXlsx(original, 'Scores.xlsx'));
    const copy = await editXlsx(original, (book) => {
      book.getWorksheet('Costs')!.getCell('A3').value = 'Toner';
    });
    const incoming = await readXlsx(copy, 'Scores.xlsx');
    expect(sheetMatchScore(base, incoming)).toBeCloseTo(10 / 11);
    const plan = planSheetPush(base, incoming);
    expect(plan.changes).toEqual([expect.objectContaining({ kind: 'edited', formatting: false })]);
    expect(plan.changes[0].after).toBe('Costs!A3:C3  "Toner | 12 | false"');
    expect(plan.ops).toEqual([
      { type: 'FieldEdited', node_id: plan.changes[0].nodeId, field: 'cell_0', value: 'Toner' }
    ]);
  });

  it('treats a completely different workbook as mostly additions and removals', async () => {
    const base = workbook();
    const other = emptyState();
    applyPayload(other, rootOp('other', 'Inventory'));
    applyPayload(other, {
      type: 'NodeCreated',
      node_id: 'inv',
      node_type: 'section',
      parent_id: 'other',
      pos: '0',
      fields: { label: 'Inventory', kind: 'sheet', columns: 4 }
    });
    [
      ['SKU', 'Product', 'Stock', 'Warehouse'],
      ['A-100', 'Hammer', '40', 'North'],
      ['A-101', 'Wrench set', '12', 'North'],
      ['B-220', 'Garden hose', '7', 'South'],
      ['C-310', 'Ladder', '3', 'East']
    ].forEach((cells, r) =>
      applyPayload(
        other,
        itemOp('inv', r, Object.fromEntries(cells.map((v, c) => [`cell_${c}`, v])), `i${r}`)
      )
    );
    const incoming = await readXlsx(await writeWorkbook(other), 'Inventory.xlsx');
    const plan = planSheetPush(base, incoming);
    const count = (kind: string) => plan.changes.filter((c) => c.kind === kind).length;
    expect(count('added') + count('removed')).toBeGreaterThanOrEqual(plan.changes.length - 1);
    expect(count('removed')).toBeGreaterThanOrEqual(PEOPLE.length);
    expect(plan.unchanged).toBe(0);
    expect(sheetMatchScore(base, incoming)).toBe(0);
  });

  it('scores the revision a copy started from above later ones', async () => {
    const v1 = workbook();
    const v2 = cloneState(v1);
    for (const id of ['s1', 's2', 's3', 's4'])
      v2.nodes[id].current_fields = { ...v2.nodes[id].current_fields, cell_2: 'Purple' };
    const copy = await readXlsx(
      await editXlsx(await writeWorkbook(v1), (book) => {
        book.getWorksheet('Scores')!.getCell('A7').value = 'Minah';
      })
    );
    const incoming = pushState(copy, 'spreadsheet');
    expect(sheetMatchScore(v1, incoming)).toBeGreaterThan(sheetMatchScore(v2, incoming));
    expect(sheetMatchScore(v1, incoming)).toBeCloseTo(10 / 11);
  });

  it('records the origin in the workbook properties', async () => {
    const buffer = await writeWorkbook(workbook(), { documentId: DOC, seq: 42 });
    const core = strFromU8(unzipSync(new Uint8Array(buffer))['docProps/core.xml']);
    expect(core).toMatch(
      new RegExp(
        `<dc:subject>dynodoc:document=${DOC};revision=42;exported=[0-9T:.Z-]+</dc:subject>`
      )
    );
    expect(readOrigin(new Uint8Array(buffer))).toMatchObject({ documentId: DOC, seq: 42 });
    // Saving again in a spreadsheet program keeps the standard property.
    const resaved = await editXlsx(buffer, () => {});
    expect(readOrigin(new Uint8Array(resaved))).toMatchObject({ documentId: DOC, seq: 42 });
    expect(readOrigin(new Uint8Array(await writeWorkbook(workbook())))).toEqual({});
  });

  it('refuses a file that is not a workbook', () => {
    const deck = { kind: 'presentation', ops: [], warnings: [], filename: 'x.pptx', rootId: 'r' };
    expect(() => planSheetPush(workbook(), deck as never)).toThrow(/workbook/);
  });
});

function build(ops: EventPayload[], state: DocumentState = emptyState()) {
  for (const op of ops) expect(applyPayload(state, op)).toBe(true);
  return state;
}
/** A four-slide deck with placeholders, notes, a text box, a filled shape and a picture. */
function deck(): DocumentState {
  const state = build(initialPresentationOps('Field study', 'root'));
  for (const [title, body] of [
    ['Method', 'Interviews with twelve households\nTwo survey rounds'],
    ['Findings', 'Trust grows with repeated small favours'],
    ['Next steps', 'Second round of visits in spring']
  ]) {
    const slide = newSlideOps(readDeck(state), 'title-content', undefined, title);
    build(slide.ops, state);
    const shapes = readDeck(state).slides.find((s) => s.id === slide.id)!.shapes;
    build(editShapeOps(readDeck(state).slides.at(-1)!, shapes[1], { text: body }), state);
  }
  const findings = readDeck(state).slides[2];
  build(
    [
      { type: 'FieldEdited', node_id: findings.id, field: 'notes', value: 'Pause for questions' },
      {
        type: 'NodeCreated',
        node_id: 'box',
        node_type: 'item',
        parent_id: findings.id,
        pos: 'x',
        fields: { kind: 'text', text: 'Source: field notes', x: 60, y: 88, w: 30, h: 6, style: {} }
      },
      {
        type: 'NodeCreated',
        node_id: 'badge',
        node_type: 'item',
        parent_id: findings.id,
        pos: 'y',
        fields: { kind: 'shape', text: 'Key result', x: 70, y: 8, w: 22, h: 10, style: {} }
      },
      {
        type: 'NodeCreated',
        node_id: 'photo',
        node_type: 'item',
        parent_id: findings.id,
        pos: 'z',
        fields: {
          kind: 'image',
          text: '',
          x: 70,
          y: 40,
          w: 20,
          h: 25,
          style: {},
          src: PIXEL,
          alt: 'Garden'
        }
      }
    ],
    state
  );
  return state;
}
async function readPptx(state: DocumentState) {
  return importPptx(await writePptx(state, 'Field study'), 'Field study (v9).pptx');
}
const slideId = (state: DocumentState, title: string) =>
  readDeck(state).slides.find((s) => s.label === title)!.id;

describe('pushing an edited PowerPoint file', () => {
  it('finds no changes in an untouched download', async () => {
    const base = deck();
    const incoming = await readPptx(base);
    const plan = planSlidePush(base, incoming);
    expect(plan.changes).toEqual([]);
    expect(plan.ops).toEqual([]);
    expect(plan.unchanged).toBe(11);
    expect(slideMatchScore(base, incoming)).toBe(1);
  });

  it('edits the text of the matching shape and follows a new title', async () => {
    const base = deck();
    const edited = cloneState(base);
    const method = readDeck(edited).slides[1];
    build(
      [
        ...editShapeOps(method, method.shapes[1], {
          text: 'Interviews with fourteen households\nTwo survey rounds'
        }),
        ...editShapeOps(method, method.shapes[0], { text: 'Methods' })
      ],
      edited
    );
    const plan = planSlidePush(base, await readPptx(edited));
    const [title, body] = readDeck(base).slides[1].shapes;
    expect(plan.changes).toEqual([
      {
        kind: 'edited',
        nodeId: title.id,
        before: 'Slide 2 · Title  "Method"',
        after: 'Slide 2 · Title  "Methods"',
        formatting: false
      },
      {
        kind: 'edited',
        nodeId: body.id,
        before: 'Slide 2 · Body  "Interviews with twelve households Two survey rounds"',
        after: 'Slide 2 · Body  "Interviews with fourteen households Two survey rounds"',
        formatting: false
      }
    ]);
    expect(plan.state.nodes[method.id].current_fields.label).toBe('Methods');
    expect(plan.unchanged).toBe(9);
  });

  it('moves a shape without touching its text or style', async () => {
    const base = deck();
    const edited = cloneState(base);
    edited.nodes.badge.current_fields = { ...edited.nodes.badge.current_fields, x: 40, y: 12 };
    const plan = planSlidePush(base, await readPptx(edited));
    expect(plan.changes).toEqual([
      {
        kind: 'edited',
        nodeId: 'badge',
        before: 'Slide 3 · Shape  "Key result" · at 70%, 8% · 22×10%',
        after: 'Slide 3 · Shape  "Key result" · at 40%, 12% · 22×10%',
        formatting: true
      }
    ]);
    expect(plan.ops).toEqual([
      { type: 'FieldEdited', node_id: 'badge', field: 'x', value: 40 },
      { type: 'FieldEdited', node_id: 'badge', field: 'y', value: 12 }
    ]);
  });

  it('adds a slide inserted in the middle', async () => {
    const base = deck();
    const edited = cloneState(base);
    const pilot = newSlideOps(
      readDeck(edited),
      'title-content',
      slideId(edited, 'Method'),
      'Pilot'
    );
    build(pilot.ops, edited);
    const plan = planSlidePush(base, await readPptx(edited));
    expect(plan.changes).toEqual([
      {
        kind: 'added',
        nodeId: expect.any(String),
        before: '',
        after: 'Slide 3  "Pilot" · 2 objects'
      }
    ]);
    expect(plan.unchanged).toBe(11);
    const slides = readDeck(plan.state).slides;
    expect(slides.map((s) => s.label)).toEqual([
      'Field study',
      'Method',
      'Pilot',
      'Findings',
      'Next steps'
    ]);
    expect(slides[2].id).toBe(plan.changes[0].nodeId);
    expect(slides[2].shapes.map((s) => s.role)).toEqual(['title', 'body']);
    // Theme defaults written into the file do not become explicit styles.
    expect(slides[2].shapes[0].style).toEqual({});
    expect(plan.ops.filter((op) => op.type !== 'NodeCreated')).toEqual([]);
  });

  it('removes a deleted slide with its shapes', async () => {
    const base = deck();
    const edited = cloneState(base);
    build(deleteSlideOps(edited, slideId(edited, 'Findings')), edited);
    const plan = planSlidePush(base, await readPptx(edited));
    const findings = readDeck(base).slides[2];
    expect(plan.changes).toEqual([
      expect.objectContaining({ kind: 'removed', nodeId: findings.id, after: '' })
    ]);
    expect(plan.changes[0].before).toBe('Slide 3  "Findings" · 5 objects');
    expect(plan.ops.at(-1)).toEqual({ type: 'NodeDeleted', node_id: findings.id });
    expect(plan.ops).toHaveLength(findings.shapes.length + 1);
  });

  it('recognises reordered slides as moves', async () => {
    const base = deck();
    const edited = cloneState(base);
    build(moveSlideOps(readDeck(edited), slideId(edited, 'Next steps'), -2), edited);
    const plan = planSlidePush(base, await readPptx(edited));
    const next = slideId(base, 'Next steps');
    expect(plan.changes).toEqual([
      {
        kind: 'moved',
        nodeId: next,
        before: 'Slide 4  "Next steps"',
        after: 'Slide 2  "Next steps"',
        formatting: true
      }
    ]);
    expect(readDeck(plan.state).slides.map((s) => s.label)).toEqual([
      'Field study',
      'Next steps',
      'Method',
      'Findings'
    ]);
    expect(plan.ops).toEqual([
      expect.objectContaining({ type: 'NodeMoved', node_id: next, new_parent_id: 'root' })
    ]);
  });

  it('compares a teammate’s copy of an uploaded deck with the imported revision', async () => {
    const base = pushState(importPptx(await writePptx(deck(), 'Field study'), 'Field study.pptx'));
    const edited = deck();
    edited.nodes.badge.current_fields = {
      ...edited.nodes.badge.current_fields,
      text: 'Main result',
      style: { bold: true }
    };
    const plan = planSlidePush(base, await readPptx(edited));
    expect(plan.changes).toEqual([
      expect.objectContaining({
        kind: 'edited',
        before: expect.stringContaining('"Key result"'),
        after: expect.stringContaining('"Main result"'),
        formatting: false
      })
    ]);
    const badge = plan.state.nodes[plan.changes[0].nodeId].current_fields;
    expect(badge).toMatchObject({ text: 'Main result', label: 'Main result' });
    expect(badge.style).toMatchObject({ bold: true, fill: '#c43e1c' });
  });

  it('records the origin in the presentation properties', async () => {
    const bytes = await writePptx(deck(), 'Field study', { documentId: DOC, seq: 7 });
    const core = strFromU8(unzipSync(bytes)['docProps/core.xml']);
    expect(core).toContain(`<dc:subject>dynodoc:document=${DOC};revision=7;exported=`);
    expect(readOrigin(bytes)).toMatchObject({ documentId: DOC, seq: 7 });
    expect(readOrigin(await writePptx(deck(), 'Field study'))).toEqual({});
  });

  it('scores the revision a copy started from above later ones', async () => {
    const v1 = deck();
    const v2 = cloneState(v1);
    for (const slide of readDeck(v2).slides.slice(1))
      build(editShapeOps(slide, slide.shapes[1], { text: 'Rewritten later' }), v2);
    const incoming = await readPptx(v1);
    expect(slideMatchScore(v1, incoming)).toBe(1);
    expect(slideMatchScore(v2, incoming)).toBeCloseTo(0.7);
  });
});
