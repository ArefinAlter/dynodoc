import { describe, expect, it } from 'vitest';
import { planPages, PAGE_GAP } from './pagination';

describe('editable page layout', () => {
  it('moves overflowing lines to the next content box without changing positions', () => {
    const lines = Array.from({ length: 9 }, (_, i) => ({
      pos: i * 20 + 1,
      top: i * 100,
      bottom: i * 100 + 90,
      kind: 'line' as const
    }));
    const result = planPages(lines, 600, 50);
    expect(result.pages).toBe(2);
    expect(result.gaps).toEqual([{ pos: 101, height: 101 + PAGE_GAP, kind: 'line' }]);
    expect(lines[5].top).toBe(500);
  });
  it('preserves explicit breaks, including consecutive blank pages', () => {
    const result = planPages(
      [
        { pos: 1, top: 0, bottom: 20, kind: 'block' },
        { pos: 2, top: 20, bottom: 20, kind: 'block', force: true },
        { pos: 3, top: 20, bottom: 20, kind: 'block', force: true }
      ],
      600,
      50
    );
    expect(result.pages).toBe(3);
    expect(result.gaps.map((g) => g.pos)).toEqual([2, 3]);
  });
  it('keeps a heading with the following line', () => {
    const result = planPages(
      [
        { pos: 1, top: 0, bottom: 400, kind: 'block' },
        { pos: 2, top: 450, bottom: 490, kind: 'block', keep: true },
        { pos: 3, top: 495, bottom: 530, kind: 'block' }
      ],
      600,
      50
    );
    expect(result.gaps[0].pos).toBe(2);
  });
  it('reports indivisible oversized content without an unbounded layout loop', () => {
    const result = planPages([{ pos: 0, top: 0, bottom: 1900, kind: 'row' }], 600, 50);
    expect(result.overflow).toBe(true);
    expect(result.pages).toBe(4);
    expect(result.gaps).toEqual([]);
  });
});
