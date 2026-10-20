import { describe, expect, it } from 'vitest';
import { installGuide, isIOS, type InstallEnvironment } from './install';

const agents = {
  chromeWindows:
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36',
  edgeWindows:
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0',
  firefoxWindows:
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:131.0) Gecko/20100101 Firefox/131.0',
  firefoxAndroid: 'Mozilla/5.0 (Android 14; Mobile; rv:131.0) Gecko/131.0 Firefox/131.0',
  safariMac:
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15',
  chromeMac:
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36',
  safariIPhone:
    'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1',
  chromeAndroid:
    'Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36'
};
const env = (over: Partial<InstallEnvironment> & { userAgent: string }): InstallEnvironment => ({
  platform: 'Win32',
  maxTouchPoints: 0,
  standalone: false,
  canPrompt: false,
  secure: true,
  ...over
});

describe('installGuide', () => {
  it('reports an app that is already installed, whatever the browser says', () => {
    expect(installGuide(env({ userAgent: agents.safariIPhone, standalone: true }))).toEqual({
      kind: 'installed'
    });
  });

  it('uses the browser prompt when one is waiting', () => {
    expect(installGuide(env({ userAgent: agents.chromeWindows, canPrompt: true }))).toEqual({
      kind: 'prompt'
    });
    expect(installGuide(env({ userAgent: agents.chromeAndroid, canPrompt: true }))).toEqual({
      kind: 'prompt'
    });
  });

  it('refuses plainly on an insecure page', () => {
    expect(installGuide(env({ userAgent: agents.chromeWindows, secure: false })).kind).toBe(
      'unavailable'
    );
  });

  it('gives Share-menu steps on iPhone and on iPad that asks for the desktop site', () => {
    for (const guide of [
      installGuide(env({ userAgent: agents.safariIPhone, platform: 'iPhone', maxTouchPoints: 5 })),
      installGuide(env({ userAgent: agents.safariMac, platform: 'MacIntel', maxTouchPoints: 5 }))
    ]) {
      expect(guide.kind).toBe('steps');
      expect(JSON.stringify(guide)).toContain('Add to Home Screen');
    }
  });

  it('does not mistake a Mac for an iPad', () => {
    expect(isIOS({ userAgent: agents.safariMac, platform: 'MacIntel', maxTouchPoints: 0 })).toBe(
      false
    );
    const guide = installGuide(env({ userAgent: agents.safariMac, platform: 'MacIntel' }));
    expect(JSON.stringify(guide)).toContain('Add to Dock');
  });

  it('explains Firefox on a computer cannot install, and Firefox on Android can', () => {
    expect(installGuide(env({ userAgent: agents.firefoxWindows })).kind).toBe('unavailable');
    expect(JSON.stringify(installGuide(env({ userAgent: agents.firefoxAndroid })))).toContain(
      'Install'
    );
  });

  it('points Chromium browsers without a prompt at the address bar or the menu', () => {
    for (const userAgent of [agents.chromeWindows, agents.edgeWindows, agents.chromeMac]) {
      const guide = installGuide(env({ userAgent }));
      expect(guide.kind).toBe('steps');
      expect(JSON.stringify(guide)).toContain('address bar');
    }
    expect(JSON.stringify(installGuide(env({ userAgent: agents.chromeAndroid })))).toContain(
      'Add to Home screen'
    );
  });

  it('falls back to a plain sentence for an unknown browser', () => {
    expect(installGuide(env({ userAgent: 'SomeBrowser/1.0' })).kind).toBe('unavailable');
  });
});
