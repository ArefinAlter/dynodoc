// The visitor's platform, read in the browser only (never stored or sent), to highlight a download card.

export type DetectedOS = 'windows' | 'linux' | 'mac' | '';

export function detectOS(): DetectedOS {
  if (typeof navigator === 'undefined') return '';
  const nav = navigator as Navigator & { userAgentData?: { platform?: string; mobile?: boolean } };
  const ua = nav.userAgent || '';
  const plat = nav.userAgentData?.platform || nav.platform || '';
  if (nav.userAgentData?.mobile || /android|iphone|ipad|ipod/i.test(ua)) return '';
  if (/win/i.test(plat) || /windows nt/i.test(ua)) return 'windows';
  if (/mac/i.test(plat) || /macintosh|mac os x/i.test(ua)) return 'mac';
  if (/cros/i.test(ua)) return '';
  if (/linux/i.test(plat) || /linux|x11/i.test(ua)) return 'linux';
  return '';
}
