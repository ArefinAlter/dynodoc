import { describe, expect, it } from 'vitest';
import { keyboardInset } from './viewport';

const page = { layoutHeight: 800, height: 800, offsetTop: 0, scale: 1 };

describe('keyboardInset', () => {
  it('is 0 when the whole page is visible', () => {
    expect(keyboardInset(page)).toBe(0);
  });

  it('is the covered height when a keyboard shrinks only the visible area', () => {
    expect(keyboardInset({ ...page, height: 480 })).toBe(320);
  });

  it('does not count the part the visible area has scrolled away from the top', () => {
    expect(keyboardInset({ ...page, height: 480, offsetTop: 100 })).toBe(220);
  });

  it('is 0 when the browser resized the page itself, as Chrome on Android does', () => {
    expect(keyboardInset({ layoutHeight: 480, height: 480, offsetTop: 0, scale: 1 })).toBe(0);
  });

  it('ignores a browser toolbar sliding away', () => {
    expect(keyboardInset({ ...page, height: 740 })).toBe(0);
  });

  it('ignores pinch zoom, which also shrinks the visible area', () => {
    expect(keyboardInset({ ...page, height: 400, scale: 2 })).toBe(0);
  });
});
