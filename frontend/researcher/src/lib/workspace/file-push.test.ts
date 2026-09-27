import { describe, expect, it } from 'vitest';
import { strFromU8, strToU8, unzipSync, zipSync } from 'fflate';
import type { DocumentState } from '$lib/api/types';
import { applyPayload } from '$lib/materializer';
import { writeDocx } from './docx-export';
import { readDocx } from './docx-import';
import { itemOp, rootOp, textOf, type TextBlock } from './files';
import {
  downloadName,
  exportOrder,
  matchScore,
  planPush,
  readOrigin,
  revisionFromName,
  titleFromName,
  wordDiff
} from './file-push';

const ROOT = '01J00000000000000000000000';
const ids = Array.from({ length: 6 }, (_, i) => `01J0000000000000000000000${i + 1}`);
const para = (text: string, marks?: TextBlock['marks']): TextBlock => ({
  type: 'paragraph',
  content: [{ type: 'text', text, ...(marks ? { marks } : {}) }]
});
function baseState(): DocumentState {
  const state: DocumentState = { nodes: {}, comments: [], suggestions: {}, removed_choices: [] };
  applyPayload(state, rootOp(ROOT, 'Group report'));
  const blocks: TextBlock[] = [
    { type: 'heading', attrs: { level: 1 }, content: [{ type: 'text', text: 'Introduction' }] },
    para('Our study looks at how neighbours share tools and time.'),
    para('We interviewed twelve households over three weeks.', [{ type: 'bold' }]),
    {
      type: 'bulletList',
      content: [
        { type: 'listItem', content: [para('Tool library')] },
        { type: 'listItem', content: [para('Shared garden')] }
      ]
    },
    para('Results suggest that trust grows with repeated small favours.'),
    para('Next steps include a second round of visits.')
  ];
  blocks.forEach((block, i) =>
    applyPayload(
      state,
      itemOp(
        ROOT,
        i,
        { label: textOf(block), content: { ...block, attrs: { ...block.attrs, blockId: ids[i] } } },
        ids[i]
      )
    )
  );
  return state;
}
async function bytesOf(blob: Blob) {
  return new Uint8Array(
    await new Promise<ArrayBuffer>((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(r.result as ArrayBuffer);
      r.onerror = reject;
      r.readAsArrayBuffer(blob);
    })
  );
}
/** Simulate editing the downloaded file in Word by rewriting document.xml. */
function editXml(bytes: Uint8Array, edit: (xml: string) => string) {
  const parts = unzipSync(bytes);
  parts['word/document.xml'] = strToU8(edit(strFromU8(parts['word/document.xml'])));
  return zipSync(parts);
}
const paragraphs = (xml: string) => xml.match(/<w:p>[\s\S]*?<\/w:p>|<w:p [\s\S]*?<\/w:p>/g) || [];

describe('pushing an edited Word file', () => {
  it('records origin metadata and the download naming convention', async () => {
    const bytes = await bytesOf(
      await writeDocx(baseState(), 'Group report', {
        documentId: '94a78b6c-eaf0-4d9a-9a52-5f3f0c2a1b11',
        seq: 42
      })
    );
    expect(readOrigin(bytes)).toMatchObject({
      documentId: '94a78b6c-eaf0-4d9a-9a52-5f3f0c2a1b11',
      seq: 42
    });
    expect(downloadName('Group report', 42)).toBe('Group report (v42).docx');
    expect(revisionFromName('Group report (v42).docx')).toBe(42);
    expect(revisionFromName('Group report (v42) (1).docx')).toBe(42);
    expect(revisionFromName('Group_report_v7.docx')).toBe(7);
    expect(revisionFromName('Group report version 12.docx')).toBe(12);
    expect(revisionFromName('Group report.docx')).toBeUndefined();
    expect(titleFromName('Group report (v42) (1).docx')).toBe('Group report');
  });

  it('finds no changes in an untouched download', async () => {
    const base = baseState();
    const bytes = await bytesOf(await writeDocx(base, 'Group report'));
    const incoming = readDocx(bytes);
    expect(incoming.anchors?.filter(Boolean)).toHaveLength(6);
    const plan = planPush(base, incoming, incoming);
    expect(plan.changes).toEqual([]);
    expect(plan.ops).toEqual([]);
    expect(plan.unchanged).toBe(6);
    expect(plan.anchored).toBe(6);
  });

  it('maps edits, insertions, deletions and formatting to stable block IDs', async () => {
    const base = baseState();
    const original = await bytesOf(await writeDocx(base, 'Group report'));
    const roundtrip = readDocx(original);
    const edited = editXml(original, (xml) => {
      const ps = paragraphs(xml);
      expect(ps.at(-1)).toContain('Next steps');
      // Delete the last paragraph, edit two, and add one after the introduction.
      let out = xml.replace(ps.at(-1)!, '').replace('twelve households', 'fourteen households');
      out = out.replace(
        ps[1],
        ps[1] +
          '<w:p><w:r><w:t>Community members asked us to publish a short summary.</w:t></w:r></w:p>'
      );
      return out.replace(
        '<w:t xml:space="preserve">Results',
        '<w:t xml:space="preserve">Early results'
      );
    });
    const plan = planPush(base, readDocx(edited), roundtrip);
    const byKind = (kind: string) => plan.changes.filter((c) => c.kind === kind);
    expect(
      byKind('edited')
        .map((c) => c.nodeId)
        .sort()
    ).toEqual([ids[2], ids[4]].sort());
    expect(byKind('removed').map((c) => c.nodeId)).toEqual([ids[5]]);
    expect(byKind('added')).toHaveLength(1);
    expect(byKind('added')[0].after).toContain('publish a short summary');
    // The bold paragraph keeps its identity and formatting; only the words changed.
    const bold = plan.state.nodes[ids[2]].current_fields.content as TextBlock;
    expect(textOf(bold)).toBe('We interviewed fourteen households over three weeks.');
    expect(JSON.stringify(bold)).toContain('"bold"');
    expect(bold.attrs?.blockId).toBe(ids[2]);
    // The new paragraph sits where it was typed, after the first body paragraph.
    const order = exportOrder(plan.state).map((n) => textOf(n.current_fields.content as TextBlock));
    expect(order.slice(0, 4)).toEqual([
      'Introduction',
      'Our study looks at how neighbours share tools and time.',
      'Community members asked us to publish a short summary.',
      'We interviewed fourteen households over three weeks.'
    ]);
    // Unchanged blocks produce no operations.
    const touched = new Set(plan.ops.map((op) => ('node_id' in op ? op.node_id : '')));
    expect(touched.has(ids[0]) || touched.has(ids[1]) || touched.has(ids[3])).toBe(false);
  });

  it('aligns by content when a file has lost its bookmarks', async () => {
    const base = baseState();
    const original = await bytesOf(await writeDocx(base, 'Group report'));
    const stripped = editXml(original, (xml) =>
      xml
        .replace(/<w:bookmarkStart[^>]*\/>/g, '')
        .replace(/<w:bookmarkEnd[^>]*\/>/g, '')
        .replace('share tools and time', 'share tools, food and time')
    );
    const incoming = readDocx(stripped);
    expect(incoming.anchors?.some(Boolean)).toBe(false);
    const plan = planPush(base, incoming, readDocx(original));
    expect(plan.anchored).toBe(0);
    expect(plan.changes).toEqual([expect.objectContaining({ kind: 'edited', nodeId: ids[1] })]);
  });

  it('compares a teammate’s copy of the original upload with the imported revision', async () => {
    // The original upload was imported directly: blocks came through the same reader.
    const source = await bytesOf(await writeDocx(baseState(), 'Report'));
    const plain = editXml(source, (xml) =>
      xml.replace(/<w:bookmarkStart[^>]*\/>/g, '').replace(/<w:bookmarkEnd[^>]*\/>/g, '')
    );
    const imported = readDocx(plain);
    const state: DocumentState = { nodes: {}, comments: [], suggestions: {}, removed_choices: [] };
    applyPayload(state, rootOp(ROOT, 'Report'));
    imported.blocks.forEach((block, i) =>
      applyPayload(
        state,
        itemOp(
          ROOT,
          i,
          {
            label: textOf(block),
            content: { ...block, attrs: { ...block.attrs, blockId: ids[i] } }
          },
          ids[i]
        )
      )
    );
    const teammate = readDocx(editXml(plain, (xml) => xml.replace('three weeks', 'four weeks')));
    expect(matchScore(state, teammate)).toBe(5);
    const plan = planPush(state, teammate);
    expect(plan.changes).toEqual([expect.objectContaining({ kind: 'edited', nodeId: ids[2] })]);
    expect(plan.unchanged).toBe(5);
  });

  it('exports paragraphs in editor order even when position keys mix letter case', async () => {
    // Keys created by typing Enter: between() yields 'W', 'j', 't' after the first block.
    const state: DocumentState = { nodes: {}, comments: [], suggestions: {}, removed_choices: [] };
    applyPayload(state, rootOp(ROOT, 'Order'));
    ['00000000', 'W', 'j', 't'].forEach((pos, i) =>
      applyPayload(state, {
        ...itemOp(ROOT, i, { label: `Block ${i}`, content: para(`Block ${i}`) }, ids[i]),
        pos
      } as ReturnType<typeof itemOp>)
    );
    const read = readDocx(await bytesOf(await writeDocx(state, 'Order')));
    expect(read.blocks.map(textOf)).toEqual(['Block 0', 'Block 1', 'Block 2', 'Block 3']);
  });

  it('recognises editor-authored paragraphs in a copy without markers', async () => {
    const editorAttrs = {
      dir: 'auto',
      lang: null,
      indent: 0,
      firstLine: 0,
      textAlign: null,
      lineHeight: null,
      spaceAfter: null,
      spaceBefore: null,
      keepWithNext: false,
      pageBreakBefore: false
    };
    const state: DocumentState = { nodes: {}, comments: [], suggestions: {}, removed_choices: [] };
    applyPayload(state, rootOp(ROOT, 'Typed'));
    ['Alpha stays.', 'Beta changes.', 'Gamma stays.'].forEach((text, i) =>
      applyPayload(
        state,
        itemOp(
          ROOT,
          i,
          {
            label: text,
            content: { ...para(text), attrs: { ...editorAttrs, blockId: ids[i] } }
          },
          ids[i]
        )
      )
    );
    const exported = await bytesOf(await writeDocx(state, 'Typed'));
    const plain = readDocx(
      editXml(exported, (xml) =>
        xml
          .replace(/<w:bookmarkStart[^>]*\/>/g, '')
          .replace(/<w:bookmarkEnd[^>]*\/>/g, '')
          .replace('Beta changes.', 'Beta changed.')
      )
    );
    expect(matchScore(state, plain)).toBe(2);
    const plan = planPush(state, plain);
    expect(plan.changes).toEqual([expect.objectContaining({ kind: 'edited', nodeId: ids[1] })]);
  });

  it('shows word-level differences', () => {
    expect(
      wordDiff('We interviewed twelve households', 'We interviewed fourteen households')
    ).toEqual([
      { kind: 'same', text: 'We interviewed ' },
      { kind: 'removed', text: 'twelve' },
      { kind: 'added', text: 'fourteen' },
      { kind: 'same', text: ' households' }
    ]);
  });
});
