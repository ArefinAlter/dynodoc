// The window frame. On Windows and Linux the app's window has no system title bar (the shell creates
// it with decorations off and a shadow): the client's own top bars are the title bar, as in T3 Code.
// They move the window, a double click maximises or restores it, and this file draws the three window
// controls at the right edge and the application menu that replaces the native menu bar.
//
// What this page may ask of the window is a closed list, granted to the bundled client window alone
// by capabilities/window-controls.json: minimise, toggle maximise, close, start dragging (Tauri's own
// drag script, for `data-tauri-drag-region`), read whether it is maximised, set its title. The server's
// pages, which open in a separate window of the system's, can ask for none of it. A test in
// src-tauri/src/policy_tests.rs checks every `plugin:window|...` below against that list.
import { h, s } from '../dom.ts';
import { call, desktopState, inDesktop, openServer } from '../bridge.ts';
import { go, render, state, toast } from '../state.ts';
import { toggleTheme } from '../theme.ts';
import { openMenu, openPalette, type MenuItem } from './layer.ts';

let custom = false;
let controls: HTMLElement | null = null;
let maximizeButton: HTMLButtonElement | null = null;
let lastTitle = '';
let timer: number | undefined;

/** Does this window draw its own title bar and controls? */
export function frameless(): boolean {
  return custom;
}

const root = (): HTMLElement => document.documentElement;

function glyph(kind: 'min' | 'max' | 'restore' | 'close'): SVGElement {
  const path = (d: string) => s('path', { d });
  const children =
    kind === 'min'
      ? [path('M0 5.5h10')]
      : kind === 'max'
        ? [s('rect', { x: 0.5, y: 0.5, width: 9, height: 9, rx: 1 })]
        : kind === 'restore'
          ? [s('rect', { x: 0.5, y: 2.5, width: 7, height: 7, rx: 1 }), path('M2.5 2.5v-1a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1')]
          : [path('M0.5 0.5l9 9M9.5 0.5l-9 9')];
  return s('svg', { viewBox: '0 0 10 10', width: 10, height: 10, fill: 'none', stroke: 'currentColor', 'stroke-width': 1, 'aria-hidden': 'true' }, ...children);
}

function quiet(p: Promise<unknown>): void {
  p.catch(() => undefined);
}

/** Reads whether the window is maximised and draws the matching glyph and the square corners. */
async function syncMaximized(): Promise<void> {
  if (!custom) return;
  let on = false;
  try {
    on = (await call('plugin:window|is_maximized')) === true;
  } catch {
    return;
  }
  root().classList.toggle('window-maximized', on);
  if (maximizeButton) {
    maximizeButton.replaceChildren(glyph(on ? 'restore' : 'max'));
    maximizeButton.setAttribute('aria-label', on ? 'Restore' : 'Maximize');
    maximizeButton.title = on ? 'Restore' : 'Maximize';
  }
}

function build(): HTMLElement {
  maximizeButton = h(
    'button',
    { type: 'button', class: 'max', 'aria-label': 'Maximize', title: 'Maximize', onClick: () => { quiet(call('plugin:window|toggle_maximize').then(syncMaximized)); } },
    glyph('max'),
  );
  return h(
    'div',
    { id: 'frame', class: 'winctl', role: 'group', 'aria-label': 'Window controls' },
    h('button', { type: 'button', class: 'min', 'aria-label': 'Minimize', title: 'Minimize', onClick: () => quiet(call('plugin:window|minimize')) }, glyph('min')),
    maximizeButton,
    h('button', { type: 'button', class: 'close', 'aria-label': 'Close', title: 'Close', onClick: () => quiet(call('plugin:window|close')) }, glyph('close')),
  );
}

function setCustom(on: boolean): void {
  custom = on;
  root().classList.toggle('frameless', on);
  if (on && !controls) {
    controls = document.body.appendChild(build());
  }
  if (controls) controls.hidden = !on;
}

/** Called at boot, before the first paint. The platform tells which frame the shell made (it makes
 *  Windows and Linux windows frameless and keeps the system's frame on macOS); `confirmFrame` then
 *  settles it from the app's own answer. */
export function initFrame(): void {
  if (!inDesktop()) return;
  const ua = navigator.userAgent;
  if (/Android|iPhone|iPad|Mac/.test(ua)) return;
  setCustom(true);
  root().classList.toggle('frame-border', /Linux|X11/.test(ua));
  // Dim the bar while the window is not the active one, as the system's own bars do.
  window.addEventListener('blur', () => root().classList.add('window-blurred'));
  window.addEventListener('focus', () => root().classList.remove('window-blurred'));
  let pending: number | undefined;
  window.addEventListener('resize', () => {
    window.clearTimeout(pending);
    pending = window.setTimeout(() => void syncMaximized(), 60);
  });
  void syncMaximized();
}

/** The app says which frame this window has: remove the controls again where the system draws them. */
export function confirmFrame(frame: string | undefined): void {
  if (frame === 'native' && custom) setCustom(false);
  if (frame === 'custom') void syncMaximized();
}

/** Keeps the window's own title (task bar, Alt+Tab) in step with the view on screen. */
export function syncTitle(title: string): void {
  if (!custom || title === lastTitle) return;
  lastTitle = title;
  window.clearTimeout(timer);
  timer = window.setTimeout(() => quiet(call('plugin:window|set_title', { value: title })), 120);
}

// ---- the application menu ---------------------------------------------------------------------

/** Opens the web workspace: the configured server's own pages, in a window of their own. */
export function openWorkspace(): void {
  openServer()
    .then((address) => {
      if (!address) {
        state.ui.settings.section = 'accounts';
        go('settings');
        toast('info', 'Choose a server first');
      }
    })
    .catch((e) => toast('error', e instanceof Error ? e.message : String(e)));
}

export function reloadClient(): void {
  quiet(call('window_reload'));
}

export function quit(): void {
  quiet(call('plugin:window|close'));
}

export function openDevtools(): void {
  call('devtools_open').catch((e) => toast('error', e instanceof Error ? e.message : String(e)));
}

/** The application menu: what the native File, View and Help menus held, in one place. */
export function openAppMenu(anchor: HTMLElement): void {
  const ds = desktopState();
  const items: MenuItem[] = [
    { label: 'Settings', icon: 'settings', shortcut: 'Ctrl+,', onSelect: () => go('settings') },
    { label: 'Accounts and servers', icon: 'user', onSelect: () => { state.ui.settings.section = 'accounts'; go('settings'); } },
    { label: 'Keyboard shortcuts', icon: 'keyboard', onSelect: () => { state.ui.settings.section = 'keybindings'; go('settings'); } },
    { label: 'Command palette', icon: 'search', shortcut: 'Ctrl+K', onSelect: () => openPalette() },
    { label: 'Light or dark', icon: state.prefs.theme === 'dark' ? 'sun' : 'moon', shortcut: 'Ctrl+Shift+L', onSelect: () => { toggleTheme(); render(); } },
    { label: '', separator: true },
    ds?.server
      ? { label: 'Web workspace', icon: 'globe', note: `${new URL(ds.server).host}, in its own window`, onSelect: openWorkspace }
      : { label: 'Web workspace', icon: 'globe', note: 'Choose a server in Accounts first', disabled: true },
    { label: '', separator: true },
    { label: 'Reload', icon: 'sync', shortcut: 'Ctrl+R', onSelect: reloadClient },
  ];
  if (ds?.devtools) items.push({ label: 'Developer tools', icon: 'advanced', shortcut: 'F12', onSelect: openDevtools });
  items.push(
    { label: '', separator: true },
    { label: 'Dynodoc support', icon: 'info', href: ds?.support },
    { label: 'About Dynodoc', icon: 'info', onSelect: () => { state.ui.settings.section = 'about'; go('settings'); } },
    { label: '', separator: true },
    { label: 'Quit Dynodoc', icon: 'x', shortcut: 'Alt+F4', onSelect: quit },
  );
  openMenu(anchor, items, { width: 300 });
}
