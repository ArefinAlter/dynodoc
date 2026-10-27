// Things above the page: menus (popovers), dialogs, the command palette and toasts. Menus are
// transient elements in their own layer; dialogs and the palette are re-drawn with the app.
import { h, clear, kbd, cx, type Child } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render } from '../state.ts';
import { button } from './controls.ts';

// ---- menus ----------------------------------------------------------------------------------

export interface MenuItem {
  label: string;
  icon?: string;
  shortcut?: string;
  disabled?: boolean;
  /** Shown under the label (why it is disabled, or what it does). */
  note?: string;
  checked?: boolean;
  danger?: boolean;
  separator?: boolean;
  heading?: string;
  /** A link out (opens in the system browser); the menu closes after the click has been handled. */
  href?: string;
  onSelect?: () => void;
}

let menuEl: HTMLElement | null = null;
let menuReturn: HTMLElement | null = null;

export function closeMenu(): void {
  if (menuEl) {
    menuEl.remove();
    menuEl = null;
    document.removeEventListener('pointerdown', outside, true);
    if (menuReturn && document.contains(menuReturn)) menuReturn.focus();
    menuReturn = null;
  }
}

function outside(e: Event): void {
  if (menuEl && !menuEl.contains(e.target as Node)) closeMenu();
}

/** Opens a menu under `anchor`. Arrow keys move, Enter chooses, Escape closes. `extra` goes at the
 *  top (a filter field, for the project and branch lists). */
export function openMenu(anchor: HTMLElement, items: MenuItem[], o: { width?: number; extra?: Child; align?: 'left' | 'right'; className?: string } = {}): void {
  closeMenu();
  const r = anchor.getBoundingClientRect();
  const list = h(
    'div',
    { class: 'menu-list', role: 'menu' },
    items.map((it) => {
      if (it.separator) return h('div', { class: 'menu-sep', role: 'separator' });
      if (it.heading) return h('div', { class: 'menu-heading' }, it.heading);
      if (it.href) {
        return h(
          'a',
          { class: 'menu-item', role: 'menuitem', href: it.href, target: '_blank', rel: 'noopener noreferrer', onClick: () => { setTimeout(closeMenu, 0); } },
          h('span', { class: 'menu-icon' }, it.icon ? icon(it.icon, 14) : null),
          h('span', { class: 'menu-label' }, h('span', null, it.label)),
          icon('external', 12),
        );
      }
      return h(
        'button',
        {
          type: 'button',
          role: it.checked === undefined ? 'menuitem' : 'menuitemcheckbox',
          'aria-checked': it.checked === undefined ? undefined : String(it.checked),
          class: cx('menu-item', it.danger && 'is-danger', it.disabled && 'is-disabled'),
          'aria-disabled': it.disabled ? 'true' : undefined,
          title: it.note,
          onClick: () => {
            if (it.disabled) return;
            closeMenu();
            it.onSelect?.();
          },
        },
        h('span', { class: 'menu-icon' }, it.checked ? icon('check', 14) : it.icon ? icon(it.icon, 14) : null),
        h('span', { class: 'menu-label' }, h('span', null, it.label), it.note ? h('span', { class: 'menu-note' }, it.note) : null),
        it.shortcut ? h('span', { class: 'menu-shortcut' }, it.shortcut) : null,
      );
    }),
  );
  const el = h('div', { class: cx('menu', o.className), css: { width: `${o.width ?? 280}px` } }, o.extra ?? null, list);
  const layer = document.getElementById('menus');
  if (!layer) return;
  layer.appendChild(el);
  const width = o.width ?? 280;
  let left = o.align === 'right' ? r.right - width : r.left;
  left = Math.max(8, Math.min(left, window.innerWidth - width - 8));
  let top = r.bottom + 4;
  const height = el.getBoundingClientRect().height;
  if (top + height > window.innerHeight - 8) top = Math.max(8, r.top - height - 4);
  el.style.setProperty('left', `${left}px`);
  el.style.setProperty('top', `${top}px`);
  menuEl = el;
  menuReturn = anchor;
  document.addEventListener('pointerdown', outside, true);
  el.addEventListener('keydown', (e: KeyboardEvent) => {
    const buttons = Array.from(el.querySelectorAll<HTMLButtonElement>('.menu-item:not(.is-disabled)'));
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      closeMenu();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      buttons[(i + 1) % buttons.length]?.focus();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      buttons[(i - 1 + buttons.length) % buttons.length]?.focus();
    }
  });
  const first = el.querySelector<HTMLElement>('input, .menu-item:not(.is-disabled)');
  first?.focus();
}

// ---- dialogs ---------------------------------------------------------------------------------

export interface DialogSpec {
  title: string;
  /** Rebuilt on every render, so it can show state. */
  body: () => Child;
  actions?: () => Child;
  width?: number;
  onClose?: () => void;
  /** Plain, for consequences (merge, withdraw): no ornament. */
  plain?: boolean;
}

let dialog: DialogSpec | null = null;
let dialogFresh = false;

export function openDialog(spec: DialogSpec): void {
  closeMenu();
  dialog = spec;
  dialogFresh = true;
  render();
}

export function closeDialog(): void {
  const d = dialog;
  dialog = null;
  d?.onClose?.();
  render();
}

export function dialogOpen(): boolean {
  return dialog !== null;
}

/** A confirmation in GitHub Desktop's manner: what will happen, then two buttons; the confirm
 *  button names the action. */
export function confirmDialog(title: string, text: Child, confirmLabel: string, onConfirm: () => void, o: { danger?: boolean } = {}): void {
  openDialog({
    title,
    plain: true,
    width: 480,
    body: () => h('div', { class: 'dialog-text' }, text),
    actions: () => [
      button('Cancel', { onClick: closeDialog }),
      button(confirmLabel, {
        variant: o.danger ? 'danger' : 'primary',
        id: 'dialog-confirm',
        onClick: () => {
          closeDialog();
          onConfirm();
        },
      }),
    ],
  });
}

// ---- palette ---------------------------------------------------------------------------------

export interface Command {
  id: string;
  label: string;
  group: string;
  icon?: string;
  shortcut?: string;
  keywords?: string;
  disabled?: boolean;
  run: () => void;
}

let paletteOpen = false;
let paletteQuery = '';
let paletteIndex = 0;
let commandSource: () => Command[] = () => [];

export function setCommands(fn: () => Command[]): void {
  commandSource = fn;
}

export function openPalette(query = ''): void {
  closeMenu();
  paletteOpen = true;
  paletteQuery = query;
  paletteIndex = 0;
  dialogFresh = true;
  render();
}

export function closePalette(): void {
  paletteOpen = false;
  render();
}

export function paletteIsOpen(): boolean {
  return paletteOpen;
}

function score(c: Command, q: string): number {
  if (!q) return 1;
  const hay = `${c.label} ${c.group} ${c.keywords ?? ''}`.toLowerCase();
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  let s = 0;
  for (const w of words) {
    const i = hay.indexOf(w);
    if (i < 0) return 0;
    s += i === 0 ? 3 : c.label.toLowerCase().includes(w) ? 2 : 1;
  }
  return s;
}

function paletteView(): HTMLElement {
  const list = commandSource()
    .map((c) => ({ c, s: score(c, paletteQuery) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s)
    .slice(0, 60)
    .map((x) => x.c);
  if (paletteIndex >= list.length) paletteIndex = Math.max(0, list.length - 1);
  const groups: Record<string, Command[]> = {};
  for (const c of list) (groups[c.group] ??= []).push(c);
  let n = -1;
  const run = (c: Command) => {
    if (c.disabled) return;
    paletteOpen = false;
    render();
    c.run();
  };
  const inputEl = h('input', {
    class: 'palette-input',
    id: 'palette-input',
    value: paletteQuery,
    placeholder: 'Type a command, a file or a change request',
    'aria-label': 'Command',
    spellcheck: 'false',
    autocomplete: 'off',
    'data-autofocus': '1',
    onInput: (e: Event) => {
      paletteQuery = (e.target as HTMLInputElement).value;
      paletteIndex = 0;
      render();
    },
    onKeydown: (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        paletteIndex = Math.min(list.length - 1, paletteIndex + 1);
        render();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        paletteIndex = Math.max(0, paletteIndex - 1);
        render();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const c = list[paletteIndex];
        if (c) run(c);
      } else if (e.key === 'Escape') {
        e.preventDefault();
        closePalette();
      }
    },
  });
  return h(
    'div',
    { class: 'overlay', onPointerdown: (e: Event) => e.target === e.currentTarget && closePalette() },
    h(
      'div',
      { class: 'palette', role: 'dialog', 'aria-label': 'Command palette' },
      h('div', { class: 'palette-search' }, icon('search', 16), inputEl),
      h(
        'div',
        { class: 'palette-list', 'data-scroll-key': 'palette' },
        list.length === 0 ? h('div', { class: 'palette-empty' }, 'Nothing matches.') : null,
        Object.keys(groups).map((g) => [
          h('div', { class: 'palette-group' }, g),
          groups[g].map((c) => {
            n++;
            const i = n;
            return h(
              'button',
              { type: 'button', class: cx('palette-item', i === paletteIndex && 'is-active', c.disabled && 'is-disabled'), 'aria-disabled': c.disabled ? 'true' : undefined, onClick: () => run(c), onPointermove: () => { if (paletteIndex !== i) { paletteIndex = i; render(); } } },
              h('span', { class: 'palette-icon' }, icon(c.icon ?? 'chevronRight', 15)),
              h('span', { class: 'palette-label' }, c.label),
              c.shortcut ? kbd(c.shortcut) : null,
            );
          }),
        ]),
      ),
      h('div', { class: 'palette-foot' }, kbd('Up'), kbd('Down'), h('span', null, 'to move'), kbd('Enter'), h('span', null, 'to run'), kbd('Esc'), h('span', null, 'to close')),
    ),
  );
}

// ---- the layer -------------------------------------------------------------------------------

function toasts(): HTMLElement {
  return h(
    'div',
    { class: 'toasts', 'aria-live': 'polite' },
    state.ui.toasts.map((t) => h('div', { class: cx('toast', `toast-${t.kind}`) }, h('span', { class: 'toast-dot' }), h('div', null, h('div', { class: 'toast-title' }, t.title), t.text ? h('div', { class: 'toast-text' }, t.text) : null))),
  );
}

function dialogView(d: DialogSpec): HTMLElement {
  const box = h(
    'div',
    { class: cx('dialog', d.plain && 'is-plain'), role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'dialog-title', css: { width: `min(${d.width ?? 620}px, calc(100vw - 32px))` } },
    h('div', { class: 'dialog-head' }, h('h2', { id: 'dialog-title' }, d.title), h('button', { type: 'button', class: 'icon-btn', 'aria-label': 'Close', onClick: closeDialog }, icon('x', 16))),
    h('div', { class: 'dialog-body', 'data-scroll-key': 'dialog' }, d.body()),
    d.actions ? h('div', { class: 'dialog-actions' }, d.actions()) : null,
  );
  box.addEventListener('keydown', (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeDialog();
    }
  });
  return h('div', { class: 'overlay', onPointerdown: (e: Event) => e.target === e.currentTarget && closeDialog() }, box);
}

/** Draws dialogs, the palette and toasts into the layer element. */
export function renderLayer(layer: HTMLElement): void {
  clear(layer);
  if (dialog) layer.appendChild(dialogView(dialog));
  if (paletteOpen) layer.appendChild(paletteView());
  layer.appendChild(toasts());
  if (dialogFresh) {
    dialogFresh = false;
    const target = layer.querySelector<HTMLElement>('[data-autofocus]') ?? layer.querySelector<HTMLElement>('.dialog-body input, .dialog-body textarea, .dialog-body select, #dialog-confirm, .dialog-actions button');
    requestAnimationFrame(() => target?.focus());
  }
}
