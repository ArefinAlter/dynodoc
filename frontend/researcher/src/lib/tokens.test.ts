import { describe, expect, it } from 'vitest';
import { color, font } from './tokens';

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
});
