import { describe, expect, it } from 'vitest';
import { color, font, fontMono } from './tokens';

describe('design tokens', () => {
  it('exposes the cream and blue palette', () => {
    expect(color.parchment).toBe('#FAF9F6');
    expect(color.terracotta).toBe('#205BB0');
  });

  it('exposes the sans serif interface families', () => {
    expect(font.display).toContain('sans-serif');
    expect(font.body).toContain('sans-serif');
    expect(font.ui).toContain('sans-serif');
  });

  it('exposes editor surface and status colors', () => {
    expect(color.surface).toBe('#FFFEFA');
    expect(color.suggest).toBe('#107C41');
    expect(color.warn).toBe('#A7792F');
  });

  it('exposes the mono family for logic and version labels', () => {
    expect(fontMono).toContain('monospace');
  });
});
