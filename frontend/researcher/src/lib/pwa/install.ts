/**
 * What to tell someone who wants Dynodoc as an app. Only Chromium browsers (Chrome, Edge,
 * and others on Android and desktop) fire `beforeinstallprompt`; Safari and Firefox never
 * do, so they get plain steps instead. Pure so the wording per browser can be tested.
 */
export interface InstallEnvironment {
  userAgent: string;
  platform: string;
  maxTouchPoints: number;
  /** Already running as an installed app. */
  standalone: boolean;
  /** The browser has handed over an install prompt that can be shown now. */
  canPrompt: boolean;
  /** HTTPS or localhost; browsers refuse to install anything else. */
  secure: boolean;
}

export type InstallGuide =
  | { kind: 'installed' }
  | { kind: 'prompt' }
  | { kind: 'steps'; intro: string; steps: string[] }
  | { kind: 'unavailable'; text: string };

export const isIOS = (env: Pick<InstallEnvironment, 'userAgent' | 'platform' | 'maxTouchPoints'>) =>
  /iPhone|iPad|iPod/.test(env.userAgent) ||
  // iPadOS asks for desktop sites by default and reports itself as a Mac with a touch screen.
  (env.platform === 'MacIntel' && env.maxTouchPoints > 1);

export function installGuide(env: InstallEnvironment): InstallGuide {
  const ua = env.userAgent;
  if (env.standalone) return { kind: 'installed' };
  if (!env.secure)
    return {
      kind: 'unavailable',
      text: 'This page is not on a secure connection, so the browser will not offer to install it.'
    };
  if (env.canPrompt) return { kind: 'prompt' };
  if (isIOS(env))
    return {
      kind: 'steps',
      intro: 'On iPhone and iPad the browser adds the app from its Share menu.',
      steps: [
        'Tap the Share button in the browser toolbar.',
        'Choose Add to Home Screen.',
        'Tap Add. If you do not see the option, open this page in Safari.'
      ]
    };
  if (/Android/.test(ua)) {
    if (/Firefox/.test(ua))
      return {
        kind: 'steps',
        intro: 'Firefox adds the app from its menu.',
        steps: ['Open the Firefox menu.', 'Choose Install.']
      };
    return {
      kind: 'steps',
      intro: 'Your browser did not offer an install button just now. You can still add it by hand.',
      steps: ['Open the browser menu.', 'Choose Install app or Add to Home screen.']
    };
  }
  if (/Firefox/.test(ua))
    return {
      kind: 'unavailable',
      text: 'Firefox on a computer cannot install web apps. Use Chrome, Edge or Safari to install Dynodoc, or keep it open as a pinned tab.'
    };
  if (/Macintosh/.test(ua) && /Safari/.test(ua) && !/Chrome|Chromium|Edg|OPR/.test(ua))
    return {
      kind: 'steps',
      intro: 'Safari 17 and later adds web apps to the Dock.',
      steps: ['Open the File menu.', 'Choose Add to Dock.']
    };
  if (/Chrome|Chromium|Edg|OPR/.test(ua))
    return {
      kind: 'steps',
      intro:
        'Your browser did not offer an install button just now, for example because the app is already installed. You can still add it by hand.',
      steps: [
        'Look for the install icon at the right of the address bar.',
        'Or open the browser menu and choose Install Dynodoc.'
      ]
    };
  return {
    kind: 'unavailable',
    text: 'This browser may not support installing web apps. Look for Install or Add to Home Screen in its menu.'
  };
}
