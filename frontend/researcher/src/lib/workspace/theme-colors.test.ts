import { afterEach, describe, expect, it } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { TextStyleKit } from '@tiptap/extension-text-style';
import Highlight from '@tiptap/extension-highlight';
import { TableKit } from '@tiptap/extension-table';
import { OfficeParagraph } from './office-extensions';
import { TrackedChange } from './tracked-changes';
import {
  DARK_INK,
  LIGHT_INK,
  LIGHT_PAGES_KEY,
  ThemeInk,
  cellColors,
  contrast,
  isNearBlack,
  luminance,
  parseColor,
  readLightPages,
  readableInk,
  themeInkKey,
  writeLightPages
} from './theme-colors';

describe('parseColor', () => {
  it('reads hex in the forms the editors store', () => {
    expect(parseColor('#fff')).toEqual({ r: 255, g: 255, b: 255, a: 1 });
    expect(parseColor('#107C41')).toEqual({ r: 16, g: 124, b: 65, a: 1 });
    // Spreadsheet ARGB: alpha first.
    expect(parseColor('#80ff0000')).toEqual({ r: 255, g: 0, b: 0, a: 128 / 255 });
  });
  it('reads rgb() and rgba() in comma and space syntax', () => {
    expect(parseColor('rgb(0, 0, 0)')).toEqual({ r: 0, g: 0, b: 0, a: 1 });
    expect(parseColor('rgba(255, 255, 0, 0.5)')).toEqual({ r: 255, g: 255, b: 0, a: 0.5 });
    expect(parseColor('rgb(10 20 30 / 25%)')).toEqual({ r: 10, g: 20, b: 30, a: 0.25 });
    expect(parseColor('rgb(100%, 0%, 0%)')).toEqual({ r: 255, g: 0, b: 0, a: 1 });
    expect(parseColor('black')).toEqual({ r: 0, g: 0, b: 0, a: 1 });
  });
  it('rejects anything else', () => {
    for (const value of ['', 'red', '#12', '#12345', 'rgb(1,2)', 'rgb(a,b,c)', 'url(x)', null, 3])
      expect(parseColor(value)).toBeNull();
  });
});

describe('luminance and contrast', () => {
  it('follows WCAG', () => {
    expect(luminance('#000000')).toBe(0);
    expect(luminance('#ffffff')).toBeCloseTo(1);
    expect(contrast('#000', '#fff')).toBeCloseTo(21);
    expect(contrast('#777777', '#ffffff')).toBeCloseTo(4.48, 1);
    expect(luminance('nope')).toBeNull();
    expect(contrast('#fff', 'nope')).toBeNull();
  });
});

describe('readableInk', () => {
  it('picks dark ink on light fills and light ink on dark fills', () => {
    for (const fill of ['#ffffff', '#fff2a8', '#ffff00', '#c6efce', '#e2f0d9', '#00ff00'])
      expect(readableInk(fill)).toBe(DARK_INK);
    for (const fill of ['#000000', '#107c41', '#000080', '#1f3864', 'rgb(128, 0, 0)'])
      expect(readableInk(fill)).toBe(LIGHT_INK);
    expect(readableInk('not a colour')).toBe(DARK_INK);
  });
  it('always reaches 4.5:1 on the inks it picks for common fills', () => {
    for (const fill of ['#ffffff', '#fff2a8', '#107c41', '#185abd', '#c43e1c', '#808080'])
      expect(contrast(fill, readableInk(fill))).toBeGreaterThanOrEqual(3.9);
  });
});

describe('isNearBlack', () => {
  it('matches black and very dark greys only', () => {
    for (const c of ['#000', '#000000', 'rgb(0, 0, 0)', '#1f1f1f', '#202124', '#262626', '#333333'])
      expect(isNearBlack(c)).toBe(true);
    for (const c of ['#666666', '#000080', '#1f3864', '#800000', '#ff0000', '#ffffff', 'x'])
      expect(isNearBlack(c)).toBe(false);
  });
});

describe('cellColors', () => {
  it('leaves unformatted cells to the interface tokens', () => {
    expect(cellColors({}, false)).toEqual({});
    expect(cellColors({}, true)).toEqual({});
    expect(cellColors({ fill: '', color: '' }, true)).toEqual({});
  });
  it('gives an authored fill readable ink when no text colour is set', () => {
    expect(cellColors({ fill: '#fff2cc' }, true)).toEqual({
      background: '#fff2cc',
      color: DARK_INK
    });
    expect(cellColors({ fill: '#107c41' }, false)).toEqual({
      background: '#107c41',
      color: LIGHT_INK
    });
  });
  it('shows near-black text in the default ink at night only', () => {
    expect(cellColors({ color: '#000000' }, true)).toEqual({});
    expect(cellColors({ color: '#000000' }, false)).toEqual({ color: '#000000' });
    expect(cellColors({ color: 'rgb(32, 33, 36)' }, true)).toEqual({});
  });
  it('keeps authored colours exactly as written', () => {
    expect(cellColors({ color: '#9c0006' }, true)).toEqual({ color: '#9c0006' });
    expect(cellColors({ fill: '#ffc7ce', color: '#9c0006' }, true)).toEqual({
      background: '#ffc7ce',
      color: '#9c0006'
    });
    // Near-black text on an authored fill is the author's choice.
    expect(cellColors({ fill: '#ffff00', color: '#000000' }, true)).toEqual({
      background: '#ffff00',
      color: '#000000'
    });
  });
  it('ignores unreadable and transparent values', () => {
    expect(cellColors({ fill: 'javascript:x', color: 'expression(1)' }, true)).toEqual({});
    expect(cellColors({ fill: 'rgba(0, 0, 0, 0)' }, true)).toEqual({});
  });
});

describe('light pages option', () => {
  afterEach(() => localStorage.removeItem(LIGHT_PAGES_KEY));
  it('persists per browser and defaults off', () => {
    expect(readLightPages()).toBe(false);
    writeLightPages(true);
    expect(localStorage.getItem(LIGHT_PAGES_KEY)).toBe('1');
    expect(readLightPages()).toBe(true);
    writeLightPages(false);
    expect(readLightPages()).toBe(false);
  });
});

describe('ThemeInk', () => {
  let dark = true;
  const editors: Editor[] = [];
  afterEach(() => {
    while (editors.length) editors.pop()!.destroy();
    dark = true;
  });
  function mount(content: string) {
    const editor = new Editor({
      element: document.createElement('div'),
      extensions: [
        StarterKit,
        TextStyleKit,
        Highlight.configure({ multicolor: true }),
        TableKit,
        OfficeParagraph,
        TrackedChange,
        ThemeInk.configure({ active: () => dark })
      ],
      content
    });
    editors.push(editor);
    return editor;
  }
  /** The nearest inline colour that applies to `text` ('' when none is set in the editor). */
  function inkAround(editor: Editor, text: string) {
    const walker = document.createTreeWalker(editor.view.dom, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode())
      if (n.textContent === text) {
        for (let el = n.parentElement; el && el !== editor.view.dom; el = el.parentElement)
          if (el.style.color && el.style.color !== 'inherit') return el.style.color;
        return '';
      }
    throw new Error(`No text “${text}”`);
  }
  const darkInk = 'rgb(31, 35, 40)';
  const pageInk = 'var(--color-page-ink)';

  it('shows black text in the page ink and keeps coloured text', () => {
    const editor = mount(
      '<p><span style="color: #000000">black</span> <span style="color: rgb(0, 0, 0)">rgb</span> <span style="color: #c00000">red</span> plain</p>'
    );
    expect(inkAround(editor, 'black')).toBe(pageInk);
    expect(inkAround(editor, 'rgb')).toBe(pageInk);
    expect(inkAround(editor, 'red')).toBe('rgb(192, 0, 0)');
    expect(inkAround(editor, ' plain')).toBe('');
    // The document itself is untouched.
    expect(JSON.stringify(editor.getJSON())).not.toContain('--color-page-ink');
    expect(editor.getHTML()).not.toContain('--color-page-ink');
  });

  it('gives highlighted text and shaded cells readable ink', () => {
    const editor = mount(
      '<p><mark data-color="#fff2a8" style="background-color: #fff2a8">marked</mark> ' +
        '<mark data-color="#000080" style="background-color: #000080">navy</mark> ' +
        '<mark data-color="#fff2a8"><span style="color: #c00000">red on yellow</span></mark></p>' +
        '<table><tbody><tr><td data-fill="#ffff00"><p>cell</p></td><td><p>plain cell</p></td></tr></tbody></table>'
    );
    expect(DARK_INK).toBe('#1f2328');
    expect(inkAround(editor, 'marked')).toBe(darkInk);
    expect(inkAround(editor, 'navy')).toBe('rgb(243, 245, 248)');
    expect(inkAround(editor, 'red on yellow')).toBe('rgb(192, 0, 0)');
    const cells = editor.view.dom.querySelectorAll('td');
    expect(cells[0].style.color).toBe(darkInk);
    expect(cells[1].style.color).toBe('');
    expect(editor.getHTML()).not.toContain(DARK_INK);
  });

  it('does nothing in day mode and updates when the mode changes', () => {
    dark = false;
    const editor = mount('<p><span style="color: #000000">black</span></p>');
    expect(inkAround(editor, 'black')).toBe('rgb(0, 0, 0)');
    dark = true;
    editor.view.dispatch(editor.state.tr.setMeta(themeInkKey, true));
    expect(inkAround(editor, 'black')).toBe(pageInk);
    dark = false;
    editor.view.dispatch(editor.state.tr.setMeta(themeInkKey, true));
    expect(inkAround(editor, 'black')).toBe('rgb(0, 0, 0)');
  });
});
