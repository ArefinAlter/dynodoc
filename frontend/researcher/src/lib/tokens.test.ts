import { describe, expect, it } from 'vitest';
import { color, font, fontMono } from './tokens';

describe('design tokens', () => {
  it('exposes the parchment + terracotta palette', () => {
    expect(color.parchment).toBe('#F4EFE6');
    expect(color.terracotta).toBe('#B14B2C');
  });

  it('exposes the three type families', () => {
    expect(font.display).toContain('Fraunces');
    expect(font.body).toContain('Source Serif 4');
    expect(font.ui).toContain('IBM Plex Sans');
  });

  it('exposes editor surface and status colors', () => {
    expect(color.surface).toBe('#FBF8F1');
    expect(color.suggest).toBe('#7B8F50');
    expect(color.warn).toBe('#C18A3F');
  });

  it('exposes the mono family for logic and version labels', () => {
    expect(fontMono).toContain('IBM Plex Mono');
  });
});
