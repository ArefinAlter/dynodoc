import { afterEach, describe, expect, it, vi } from 'vitest';
import { copyText } from './clipboard';

afterEach(() => {
  vi.restoreAllMocks();
  Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
});

describe('copyText', () => {
  it('uses the async clipboard when it is available', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    expect(await copyText('hello')).toBe(true);
    expect(writeText).toHaveBeenCalledWith('hello');
  });

  it('falls back to the copy command when the clipboard is missing or refused', async () => {
    const exec = vi.fn().mockReturnValue(true);
    Object.defineProperty(document, 'execCommand', { value: exec, configurable: true });
    for (const clipboard of [
      undefined,
      { writeText: vi.fn().mockRejectedValue(new DOMException('no', 'NotAllowedError')) }
    ]) {
      Object.defineProperty(navigator, 'clipboard', { value: clipboard, configurable: true });
      expect(await copyText('by hand')).toBe(true);
    }
    expect(exec).toHaveBeenCalledTimes(2);
    expect(exec).toHaveBeenCalledWith('copy');
    // The helper leaves nothing behind in the page.
    expect(document.querySelector('textarea')).toBeNull();
  });

  it('reports failure when nothing can copy', async () => {
    Object.defineProperty(document, 'execCommand', {
      value: vi.fn().mockReturnValue(false),
      configurable: true
    });
    expect(await copyText('x')).toBe(false);
    Object.defineProperty(document, 'execCommand', {
      value: vi.fn(() => {
        throw new Error('blocked');
      }),
      configurable: true
    });
    expect(await copyText('x')).toBe(false);
    expect(document.querySelector('textarea')).toBeNull();
  });
});
