/**
 * Day and night mode. The preference is `light`, `dark` or `system` (follow the
 * operating system). app.html applies it before first paint; this module changes it
 * and keeps `system` in step with the OS setting. Where the configuration names a shared
 * parent domain (the hosted service), a cookie on it lets the public site, app. and admin.
 * show the same mode.
 */
export type ThemePreference = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = 'dynodoc.theme';
const COOKIE = 'dynodoc_theme';

function readPreference(): ThemePreference {
  if (typeof document === 'undefined') return 'system';
  const attribute = document.documentElement.getAttribute('data-theme-preference');
  return attribute === 'light' || attribute === 'dark' ? attribute : 'system';
}
function systemDark() {
  return typeof window !== 'undefined' && !!window.matchMedia
    ? window.matchMedia('(prefers-color-scheme: dark)').matches
    : false;
}
export function resolveTheme(preference: ThemePreference, osDark = systemDark()): ResolvedTheme {
  return preference === 'dark' || (preference === 'system' && osDark) ? 'dark' : 'light';
}
let sharedDomain: string | null = null;
/** The server configuration names the parent domain that public site, workspace and admin
 *  hosts share (hosted only); with none, the cookie belongs to the current host. */
export function setThemeCookieDomain(domain: string | null) {
  sharedDomain = domain;
}
/** The cookie domain attribute for this host. */
export function cookieDomain(hostname: string) {
  const bare = sharedDomain?.replace(/^\./, '');
  return sharedDomain && bare && (hostname === bare || hostname.endsWith(`.${bare}`))
    ? `; domain=${sharedDomain}`
    : '';
}
export function nextPreference(preference: ThemePreference): ThemePreference {
  return preference === 'system' ? 'light' : preference === 'light' ? 'dark' : 'system';
}

class ThemeState {
  preference = $state<ThemePreference>(readPreference());
  resolved = $state<ResolvedTheme>(resolveTheme(readPreference()));
  #watching = false;

  /** Follow OS changes while the preference is `system`. Call once in the browser. */
  watch() {
    if (this.#watching || typeof window === 'undefined' || !window.matchMedia) return;
    this.#watching = true;
    this.preference = readPreference();
    this.resolved = resolveTheme(this.preference);
    const query = window.matchMedia('(prefers-color-scheme: dark)');
    query.addEventListener('change', () => {
      if (this.preference === 'system') this.#apply();
    });
    // Another tab or host changed the preference.
    window.addEventListener('storage', (event) => {
      if (event.key !== STORAGE_KEY) return;
      const value = event.newValue;
      this.preference = value === 'light' || value === 'dark' ? value : 'system';
      this.#apply();
    });
  }
  set(preference: ThemePreference) {
    this.preference = preference;
    try {
      localStorage.setItem(STORAGE_KEY, preference);
    } catch {
      /* Private windows can refuse storage; the cookie still carries the choice. */
    }
    try {
      const secure = location.protocol === 'https:' ? '; secure' : '';
      document.cookie = `${COOKIE}=${preference}; path=/; max-age=31536000; samesite=lax${cookieDomain(location.hostname)}${secure}`;
    } catch {
      /* The mode still applies to this page. */
    }
    this.#apply();
  }
  cycle() {
    this.set(nextPreference(this.preference));
  }
  #apply() {
    this.resolved = resolveTheme(this.preference);
    const root = document.documentElement;
    root.setAttribute('data-theme', this.resolved);
    root.setAttribute('data-theme-preference', this.preference);
  }
}

export const theme = new ThemeState();
