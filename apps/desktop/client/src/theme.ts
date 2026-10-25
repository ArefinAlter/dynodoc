// Applies the Appearance preferences to the document: data attributes the stylesheet reads, and
// the accent as a CSS custom property (set through CSSOM, which the page's policy allows).
import { state } from './state.ts';

export function applyAppearance(): void {
  const p = state.prefs;
  const root = document.documentElement;
  if (p.theme === 'system') root.removeAttribute('data-scheme');
  else root.setAttribute('data-scheme', p.theme);
  root.setAttribute('data-palette', p.palette);
  root.setAttribute('data-density', p.density);
  root.setAttribute('data-font', p.fontSize);
  root.toggleAttribute('data-reduce-motion', p.reduceMotion);
  root.toggleAttribute('data-underline', p.underlineLinks);
  if (p.accent) root.style.setProperty('--accent-custom', p.accent);
  else root.style.removeProperty('--accent-custom');
  root.toggleAttribute('data-accent', Boolean(p.accent));
}

/** The scheme on screen now (system resolved). */
export function scheme(): 'light' | 'dark' {
  const p = state.prefs.theme;
  if (p !== 'system') return p;
  return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function toggleTheme(): void {
  state.prefs.theme = scheme() === 'dark' ? 'light' : 'dark';
  applyAppearance();
}
