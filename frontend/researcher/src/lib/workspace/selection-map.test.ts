import { describe, expect, it } from 'vitest';
import { Schema } from '@tiptap/pm/model';
import { mapPositionAcrossReplace } from './selection-map';

const schema = new Schema({
  nodes: {
    doc: { content: 'paragraph+' },
    paragraph: { content: 'text*', group: 'block' },
    text: {}
  }
});
const doc = (...paragraphs: string[]) =>
  schema.node(
    'doc',
    null,
    paragraphs.map((text) => schema.node('paragraph', null, text ? [schema.text(text)] : []))
  );
// Positions: 0 is before the first paragraph, 1 is its start, and each paragraph adds 2.
const caretAfter = (paragraph: number, text: string, offset: number, paragraphs: string[]) =>
  paragraphs.slice(0, paragraph).reduce((sum, p) => sum + p.length + 2, 0) +
  1 +
  Math.min(offset, text.length);

describe('mapPositionAcrossReplace', () => {
  it('keeps a caret in place when the documents are the same', () => {
    const d = doc('One.', 'Two.');
    expect(mapPositionAcrossReplace(d, doc('One.', 'Two.'), 9)).toBe(9);
  });

  it('keeps a caret that is before a remote change', () => {
    const before = doc('One.', 'Two.', 'Three.');
    const after = doc('One.', 'Two.', 'Three. Closing line.');
    const caret = caretAfter(1, 'Two.', 4, ['One.', 'Two.', 'Three.']);
    expect(mapPositionAcrossReplace(before, after, caret)).toBe(caret);
  });

  it('moves a caret that is after a remote insertion by its length', () => {
    const before = doc('One.', 'Two.', 'Three.');
    const after = doc('One. Added.', 'Two.', 'Three.');
    const caret = caretAfter(1, 'Two.', 4, ['One.', 'Two.', 'Three.']);
    expect(mapPositionAcrossReplace(before, after, caret)).toBe(caret + ' Added.'.length);
  });

  it('moves a caret back when text before it was removed', () => {
    const before = doc('One. Extra.', 'Two.');
    const after = doc('One.', 'Two.');
    const caret = caretAfter(1, 'Two.', 2, ['One. Extra.', 'Two.']);
    expect(mapPositionAcrossReplace(before, after, caret)).toBe(caret - ' Extra.'.length);
  });

  it('keeps typing local: a caret in the middle of what the person is writing is not sent to the end', () => {
    // The person has typed "Verifie" into paragraph 2 and the server's copy lacks it.
    const before = doc('One.', 'Two. Verifie', 'Three.');
    const after = doc('One.', 'Two. Verifie', 'Three. Added by someone else.');
    const caret = caretAfter(1, 'Two. Verifie', 12, ['One.', 'Two. Verifie', 'Three.']);
    expect(mapPositionAcrossReplace(before, after, caret)).toBe(caret);
  });

  it('puts a caret inside a replaced range at the end of the replacement, never past the document', () => {
    const before = doc('Alpha beta gamma.');
    const after = doc('Alpha X gamma.');
    const inside = 9;
    const mapped = mapPositionAcrossReplace(before, after, inside);
    expect(mapped).toBeGreaterThanOrEqual(7);
    expect(mapped).toBeLessThanOrEqual(after.content.size);
    expect(mapPositionAcrossReplace(before, doc('A'), 17)).toBeLessThanOrEqual(3);
  });

  it('treats an insertion that could be anywhere in a run of identical text as after the caret', () => {
    const before = doc('aaa');
    const after = doc('aaaaa');
    expect(mapPositionAcrossReplace(before, after, 4)).toBe(4);
    expect(mapPositionAcrossReplace(before, after, 1)).toBe(1);
  });
});
