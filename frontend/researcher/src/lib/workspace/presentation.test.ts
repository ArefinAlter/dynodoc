import { describe, it, expect } from 'vitest';
import { strToU8, zipSync } from 'fflate';
import { emptyState, applyPayload } from '$lib/materializer';
import type { DocumentState, EventPayload } from '$lib/api/types';
import {
  applyLayoutOps,
  arrangeShapeOps,
  deleteSlideOps,
  duplicateSlideOps,
  editShapeOps,
  initialPresentationOps,
  layoutShapes,
  moveSlideOps,
  newSlideOps,
  readDeck
} from './presentation';
import { writePptx } from './pptx-export';
import { importPptx } from './pptx-import';

function build(ops: EventPayload[], state: DocumentState = emptyState()) {
  for (const op of ops) expect(applyPayload(state, op)).toBe(true);
  return state;
}
const PIXEL =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

describe('presentation model', () => {
  it('creates a root and a titled first slide with stable placeholder nodes', () => {
    const ops = initialPresentationOps('Group project', 'root');
    const deck = readDeck(build(ops));
    expect(ops[0]).toMatchObject({ type: 'NodeCreated', node_type: 'form', node_id: 'root' });
    expect(deck.size).toBe('16:9');
    expect(deck.slides).toHaveLength(1);
    const [slide] = deck.slides;
    expect(slide).toMatchObject({ layout: 'title', label: 'Group project', notes: '' });
    expect(slide.shapes.map((s) => s.role)).toEqual(['title', 'subtitle']);
    expect(slide.shapes[0].text).toBe('Group project');
    for (const op of ops.slice(1))
      expect(op.type === 'NodeCreated' && op.node_type).toMatch(/section|item/);
  });

  it('layout presets produce the expected placeholders inside the slide', () => {
    expect(layoutShapes('title').map((s) => s.role)).toEqual(['title', 'subtitle']);
    expect(layoutShapes('title-content').map((s) => s.role)).toEqual(['title', 'body']);
    expect(layoutShapes('section').map((s) => s.role)).toEqual(['title', 'subtitle']);
    expect(layoutShapes('two-content').map((s) => s.role)).toEqual(['title', 'body', 'body']);
    expect(layoutShapes('blank')).toEqual([]);
    for (const layout of ['title', 'title-content', 'section', 'two-content'] as const)
      for (const s of layoutShapes(layout)) {
        expect(s.x).toBeGreaterThanOrEqual(0);
        expect(s.x + s.w).toBeLessThanOrEqual(100);
        expect(s.y + s.h).toBeLessThanOrEqual(100);
      }
    expect(layoutShapes('title-content')[1].style.bullets).toBe(true);
  });

  it('inserts, duplicates and reorders slides with dense keys between neighbours', () => {
    const state = build(initialPresentationOps('Deck', 'root'));
    const first = readDeck(state).slides[0];
    const second = newSlideOps(readDeck(state), 'title-content');
    build(second.ops, state);
    const middle = newSlideOps(readDeck(state), 'blank', first.id);
    build(middle.ops, state);
    let deck = readDeck(state);
    expect(deck.slides.map((s) => s.id)).toEqual([first.id, middle.id, second.id]);
    expect(first.pos < deck.slides[1].pos && deck.slides[1].pos < deck.slides[2].pos).toBe(true);

    const move = moveSlideOps(deck, second.id, -2);
    expect(move).toHaveLength(1);
    expect(move[0]).toMatchObject({ type: 'NodeMoved', node_id: second.id, new_parent_id: 'root' });
    build(move, state);
    deck = readDeck(state);
    expect(deck.slides.map((s) => s.id)).toEqual([second.id, first.id, middle.id]);
    // Untouched slides keep their keys so concurrent edits elsewhere merge cleanly.
    expect(deck.slides[1].pos).toBe(first.pos);
    expect(moveSlideOps(deck, second.id, -1)).toEqual([]);

    const copy = duplicateSlideOps(deck, first.id);
    build(copy.ops, state);
    deck = readDeck(state);
    expect(deck.slides.map((s) => s.id)).toEqual([second.id, first.id, copy.id, middle.id]);
    expect(deck.slides[2].shapes.map((s) => s.text)).toEqual(first.shapes.map((s) => s.text));
    expect(deck.slides[2].shapes.every((s) => !first.shapes.some((f) => f.id === s.id))).toBe(true);
  });

  it('deletes shapes before their slide and emits per-field shape edits', () => {
    const state = build(initialPresentationOps('Deck', 'root'));
    const slide = readDeck(state).slides[0];
    const title = slide.shapes[0];
    const ops = editShapeOps(slide, title, { text: 'New title', x: title.x, y: 12.3456 });
    expect(ops).toEqual([
      { type: 'FieldEdited', node_id: title.id, field: 'text', value: 'New title' },
      { type: 'FieldEdited', node_id: title.id, field: 'y', value: 12.35 },
      { type: 'FieldEdited', node_id: title.id, field: 'label', value: 'New title' },
      { type: 'FieldEdited', node_id: slide.id, field: 'label', value: 'New title' }
    ]);
    build(ops, state);
    const removal = deleteSlideOps(state, slide.id);
    expect(removal.at(-1)).toEqual({ type: 'NodeDeleted', node_id: slide.id });
    expect(removal).toHaveLength(slide.shapes.length + 1);
    build(removal, state);
    expect(readDeck(state).slides).toEqual([]);
  });

  it('switches layouts without discarding typed content and reorders shapes', () => {
    const state = build(initialPresentationOps('Deck', 'root'));
    let slide = readDeck(state).slides[0];
    build(editShapeOps(slide, slide.shapes[1], { text: 'Kept subtitle' }), state);
    slide = readDeck(state).slides[0];
    build(applyLayoutOps(slide, 'two-content'), state);
    slide = readDeck(state).slides[0];
    expect(slide.layout).toBe('two-content');
    expect(slide.shapes.map((s) => s.role)).toEqual(['title', 'body', 'body']);
    expect(slide.shapes[1].text).toBe('Kept subtitle');
    build(applyLayoutOps(slide, 'blank'), state);
    slide = readDeck(state).slides[0];
    expect(slide.shapes.map((s) => s.text)).toEqual(['Deck', 'Kept subtitle']);
    build(arrangeShapeOps(slide, slide.shapes[0].id, 'front'), state);
    expect(readDeck(state).slides[0].shapes.map((s) => s.text)).toEqual(['Kept subtitle', 'Deck']);
  });
});

describe('PPTX round trip', () => {
  it('exports and re-imports slides, text, notes, hidden flags, images and geometry', async () => {
    const state = build(initialPresentationOps('Field study', 'root'));
    let deck = readDeck(state);
    const content = newSlideOps(deck, 'title-content', undefined, 'Findings');
    build(content.ops, state);
    deck = readDeck(state);
    const slide = deck.slides[1];
    build(
      [
        ...editShapeOps(slide, slide.shapes[1], {
          text: 'First point\nSecond point',
          style: { size: 24, bullets: true, bold: true, color: '#aa3300' }
        }),
        { type: 'FieldEdited', node_id: slide.id, field: 'notes', value: 'Say hello\nThen pause' },
        { type: 'FieldEdited', node_id: slide.id, field: 'background', value: '#fff6ec' },
        {
          type: 'NodeCreated',
          node_id: 'image',
          node_type: 'item',
          parent_id: slide.id,
          pos: 'z',
          fields: { kind: 'image', text: '', x: 70, y: 60, w: 20, h: 25, style: {}, src: PIXEL }
        }
      ],
      state
    );
    const hidden = newSlideOps(readDeck(state), 'section', undefined, 'Appendix');
    build(
      [...hidden.ops, { type: 'FieldEdited', node_id: hidden.id, field: 'hidden', value: true }],
      state
    );
    const before = readDeck(state);

    const bytes = await writePptx(state, 'Field study');
    expect(bytes.byteLength).toBeGreaterThan(1000);
    const result = importPptx(bytes, 'Field study.pptx');
    expect(result.kind).toBe('presentation');
    const after = readDeck(build(result.ops));

    expect(after.size).toBe('16:9');
    expect(after.title).toBe('Field study');
    expect(after.slides).toHaveLength(before.slides.length);
    expect(after.slides.map((s) => s.label)).toEqual(['Field study', 'Findings', 'Appendix']);
    expect(after.slides.map((s) => s.hidden)).toEqual([false, false, true]);
    const [, findings, appendix] = after.slides;
    expect(findings.notes).toBe('Say hello\nThen pause');
    expect(findings.background).toBe('#fff6ec');
    const body = findings.shapes.find((s) => s.role === 'body')!;
    expect(body.text).toBe('First point\nSecond point');
    expect(body.style).toMatchObject({ size: 24, bullets: true, bold: true, color: '#aa3300' });
    const image = findings.shapes.find((s) => s.kind === 'image')!;
    expect(image.src?.startsWith('data:image/png;base64,')).toBe(true);
    for (const [a, b] of [
      [before.slides[1].shapes[1], body],
      [before.slides[1].shapes[2], image]
    ])
      for (const key of ['x', 'y', 'w', 'h'] as const)
        expect(Math.abs(a[key] - b[key])).toBeLessThan(0.5);
    expect(appendix.shapes.find((s) => s.role === 'title')?.text).toBe('Appendix');
    expect(after.slides[0].shapes.map((s) => s.role)).toEqual(['title', 'subtitle']);
  });

  it('rejects files that are not presentations', () => {
    expect(() =>
      importPptx(new Uint8Array([80, 75, 5, 6, ...new Array(18).fill(0)]), 'x.pptx')
    ).toThrow();
  });
});

describe('PPTX import from PowerPoint-style files', () => {
  const ns =
    'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"';
  const rels = (items: [string, string, string][]) =>
    `<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">${items
      .map(
        ([id, type, target]) =>
          `<Relationship Id="${id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/${type}" Target="${target}"/>`
      )
      .join('')}</Relationships>`;
  const sp = (name: string, ph: string, body: string, spPr = '') =>
    `<p:sp><p:nvSpPr><p:cNvPr id="2" name="${name}"/><p:cNvSpPr/><p:nvPr>${ph}</p:nvPr></p:nvSpPr><p:spPr>${spPr}</p:spPr><p:txBody>${body}</p:txBody></p:sp>`;
  const xfrm = (x: number, y: number, cx: number, cy: number) =>
    `<a:xfrm><a:off x="${x}" y="${y}"/><a:ext cx="${cx}" cy="${cy}"/></a:xfrm>`;
  const text = (value: string, rPr = '') => `<a:p><a:r>${rPr}<a:t>${value}</a:t></a:r></a:p>`;
  function fixture() {
    const slide = [
      sp('Title 1', '<p:ph type="title"/>', `<a:bodyPr/>${text('Inherited title')}`),
      sp(
        'Content Placeholder 2',
        '<p:ph idx="1"/>',
        `<a:bodyPr><a:normAutofit fontScale="50000"/></a:bodyPr>${text('One')}<a:p><a:pPr lvl="1"/><a:r><a:t>Two</a:t></a:r></a:p>`
      ),
      sp('Footer', '<p:ph type="ftr" idx="11"/>', `<a:bodyPr/>${text('Footer text')}`),
      `<p:grpSp><p:nvGrpSpPr><p:cNvPr id="5" name="Group"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="914400" y="914400"/><a:ext cx="1828800" cy="914400"/><a:chOff x="0" y="0"/><a:chExt cx="3657600" cy="1828800"/></a:xfrm></p:grpSpPr>`,
      sp(
        'Rectangle 5',
        '',
        `<a:bodyPr/>${text('Box', '<a:rPr b="1"/>')}`,
        `${xfrm(0, 0, 3657600, 1828800)}<a:prstGeom prst="rect"/><a:solidFill><a:schemeClr val="accent1"/></a:solidFill>`
      ),
      '</p:grpSp><p:graphicFrame/>'
    ].join('');
    const layout = [
      sp('Title 1', '<p:ph type="title"/>', '<a:bodyPr/>', xfrm(457200, 274638, 8229600, 1143000)),
      sp('Content 2', '<p:ph idx="1"/>', '<a:bodyPr/>', xfrm(457200, 1600200, 8229600, 4525963))
    ].join('');
    const files: Record<string, string> = {
      'ppt/presentation.xml': `<p:presentation ${ns}><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/></p:presentation>`,
      'ppt/_rels/presentation.xml.rels': rels([
        ['rId1', 'slideMaster', 'slideMasters/slideMaster1.xml'],
        ['rId2', 'slide', 'slides/slide1.xml']
      ]),
      'ppt/slides/slide1.xml': `<p:sld ${ns}><p:cSld><p:spTree>${slide}</p:spTree></p:cSld><p:transition/></p:sld>`,
      'ppt/slides/_rels/slide1.xml.rels': rels([
        ['rId1', 'slideLayout', '../slideLayouts/slideLayout1.xml'],
        ['rId2', 'notesSlide', '../notesSlides/notesSlide1.xml']
      ]),
      'ppt/slideLayouts/slideLayout1.xml': `<p:sldLayout ${ns} type="obj"><p:cSld><p:spTree>${layout}</p:spTree></p:cSld></p:sldLayout>`,
      'ppt/slideLayouts/_rels/slideLayout1.xml.rels': rels([
        ['rId1', 'slideMaster', '../slideMasters/slideMaster1.xml']
      ]),
      'ppt/slideMasters/slideMaster1.xml': `<p:sldMaster ${ns}><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="102030"/></a:solidFill></p:bgPr></p:bg><p:spTree/></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2"/><p:txStyles><p:titleStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="4400"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill></a:defRPr></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr><a:buChar char="&#8226;"/><a:defRPr sz="3200"/></a:lvl1pPr></p:bodyStyle><p:otherStyle/></p:txStyles></p:sldMaster>`,
      'ppt/slideMasters/_rels/slideMaster1.xml.rels': rels([
        ['rId1', 'theme', '../theme/theme1.xml']
      ]),
      'ppt/theme/theme1.xml': `<a:theme ${ns}><a:themeElements><a:clrScheme name="Test"><a:dk1><a:srgbClr val="FFFFFF"/></a:dk1><a:lt1><a:srgbClr val="000000"/></a:lt1><a:accent1><a:srgbClr val="4472C4"/></a:accent1></a:clrScheme></a:themeElements></a:theme>`,
      'ppt/notesSlides/notesSlide1.xml': `<p:notes ${ns}><p:cSld><p:spTree>${sp(
        'Notes',
        '<p:ph type="body" idx="1"/>',
        `<a:bodyPr/>${text('Remember the budget')}`
      )}</p:spTree></p:cSld></p:notes>`
    };
    return zipSync(Object.fromEntries(Object.entries(files).map(([k, v]) => [k, strToU8(v)])));
  }

  it('inherits placeholder geometry, styles and colours from layouts, masters and themes', () => {
    const result = importPptx(fixture(), 'Lecture.pptx');
    const deck = readDeck(build(result.ops));
    expect(deck.size).toBe('4:3');
    expect(deck.title).toBe('Lecture');
    expect(deck.slides).toHaveLength(1);
    const [slide] = deck.slides;
    expect(slide).toMatchObject({
      layout: 'title-content',
      background: '#102030',
      notes: 'Remember the budget',
      label: 'Inherited title'
    });
    expect(slide.shapes).toHaveLength(3);
    const [title, body, box] = slide.shapes;
    expect(title).toMatchObject({ role: 'title', text: 'Inherited title', x: 5, w: 90 });
    expect(title.style).toMatchObject({ size: 44, color: '#ffffff', align: 'center' });
    expect(body).toMatchObject({ role: 'body', text: 'One\nTwo' });
    expect(body.style).toMatchObject({ size: 16, bullets: true });
    expect(box).toMatchObject({ kind: 'shape', text: 'Box', x: 10, y: 13.33, w: 20, h: 13.33 });
    expect(box.style).toMatchObject({ fill: '#4472c4', bold: true });
    const warnings = result.warnings.join(' ');
    for (const expected of ['Footers', 'transitions', 'Grouped', 'Nested bullet', 'Tables'])
      expect(warnings).toContain(expected);
  });
});
