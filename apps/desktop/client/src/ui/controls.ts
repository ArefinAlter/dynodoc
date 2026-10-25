// Shared controls in T3 Code's manner: 32 px buttons and inputs, pills with a dot and the exact
// state word, settings rows (title, description, control at the right), cards with hairlines.
import { h, cx, type Child, type Handler } from '../dom.ts';
import { icon } from '../icons.ts';
import { initials } from '../lib/format.ts';

export interface ButtonOpts {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'outline';
  size?: 'sm' | 'md';
  icon?: string;
  disabled?: boolean;
  title?: string;
  id?: string;
  onClick?: Handler;
  type?: 'button' | 'submit';
  class?: string;
  pressed?: boolean;
}

export function button(label: Child, o: ButtonOpts = {}): HTMLButtonElement {
  return h(
    'button',
    {
      type: o.type ?? 'button',
      class: cx('btn', `btn-${o.variant ?? 'secondary'}`, o.size === 'sm' && 'btn-sm', o.class),
      disabled: o.disabled,
      title: o.title,
      id: o.id,
      'aria-pressed': o.pressed === undefined ? undefined : String(o.pressed),
      onClick: o.onClick,
    },
    o.icon ? icon(o.icon, 15) : null,
    label === '' ? null : h('span', null, label),
  );
}

export function iconButton(name: string, label: string, onClick?: Handler, o: { disabled?: boolean; active?: boolean; id?: string; class?: string; size?: number } = {}): HTMLButtonElement {
  return h('button', { type: 'button', class: cx('icon-btn', o.active && 'is-active', o.class), 'aria-label': label, title: label, disabled: o.disabled, id: o.id, onClick }, icon(name, o.size ?? 16));
}

export function toggle(on: boolean, onChange: (v: boolean) => void, label: string, o: { disabled?: boolean; id?: string } = {}): HTMLButtonElement {
  return h(
    'button',
    {
      type: 'button',
      role: 'switch',
      class: cx('switch', on && 'is-on'),
      'aria-checked': String(on),
      'aria-label': label,
      disabled: o.disabled,
      id: o.id,
      onClick: () => onChange(!on),
    },
    h('span', { class: 'switch-thumb' }),
  );
}

export function checkbox(checked: boolean | 'mixed', onChange: (v: boolean) => void, label: string, o: { id?: string; disabled?: boolean } = {}): HTMLInputElement {
  return h('input', {
    type: 'checkbox',
    class: 'check',
    checked: checked === true,
    indeterminate: checked === 'mixed',
    'aria-label': label,
    id: o.id,
    disabled: o.disabled,
    onClick: (e: MouseEvent) => e.stopPropagation(),
    onChange: (e: Event) => onChange((e.target as HTMLInputElement).checked),
  });
}

export function select<T extends string>(value: T, options: [T, string][], onChange: (v: T) => void, o: { id?: string; label?: string; disabled?: boolean; class?: string } = {}): HTMLSelectElement {
  return h(
    'select',
    { class: cx('select', o.class), id: o.id, 'aria-label': o.label, disabled: o.disabled, onChange: (e: Event) => onChange((e.target as HTMLSelectElement).value as T) },
    options.map(([v, l]) => h('option', { value: v, selected: v === value }, l)),
  );
}

export function input(value: string, onInput: (v: string) => void, o: { id?: string; placeholder?: string; label?: string; type?: string; maxlength?: number; class?: string; disabled?: boolean; onEnter?: () => void; autofocus?: boolean } = {}): HTMLInputElement {
  return h('input', {
    type: o.type ?? 'text',
    class: cx('input', o.class),
    value,
    id: o.id,
    placeholder: o.placeholder,
    'aria-label': o.label ?? o.placeholder,
    maxlength: o.maxlength,
    disabled: o.disabled,
    spellcheck: 'false',
    autocomplete: 'off',
    'data-autofocus': o.autofocus ? '1' : undefined,
    onInput: (e: Event) => onInput((e.target as HTMLInputElement).value),
    onKeydown: (e: KeyboardEvent) => {
      if (e.key === 'Enter' && o.onEnter) {
        e.preventDefault();
        o.onEnter();
      }
    },
  });
}

export function textarea(value: string, onInput: (v: string) => void, o: { id?: string; placeholder?: string; label?: string; rows?: number; class?: string; maxlength?: number; onKeydown?: Handler } = {}): HTMLTextAreaElement {
  const t = h('textarea', {
    class: cx('textarea', o.class),
    id: o.id,
    placeholder: o.placeholder,
    'aria-label': o.label ?? o.placeholder,
    rows: o.rows ?? 3,
    maxlength: o.maxlength,
    onInput: (e: Event) => onInput((e.target as HTMLTextAreaElement).value),
    onKeydown: o.onKeydown,
  });
  t.value = value;
  return t;
}

export type Tone = 'neutral' | 'primary' | 'success' | 'warning' | 'error' | 'info';

/** A state pill: a dot and the exact word, colour as a second channel. */
export function pill(text: string, tone: Tone = 'neutral', o: { mono?: boolean; title?: string } = {}): HTMLElement {
  return h('span', { class: cx('pill', `tone-${tone}`, o.mono !== false && 'mono'), title: o.title }, h('span', { class: 'pill-dot' }), text);
}

export function badge(text: Child, tone: Tone = 'neutral'): HTMLElement {
  return h('span', { class: cx('badge', `tone-${tone}`) }, text);
}

export function avatar(name: string, agent = false, size: 'sm' | 'md' = 'sm'): HTMLElement {
  return h('span', { class: cx('avatar', `avatar-${size}`, agent && 'is-agent'), title: name, 'aria-hidden': 'true' }, agent ? icon('agent', size === 'sm' ? 12 : 14) : initials(name));
}

export function card(...children: Child[]): HTMLElement {
  return h('div', { class: 'card' }, ...children);
}

/** A settings row: title and description at the left, a control at the right. */
export function row(title: Child, description: Child, control: Child, o: { fixed?: boolean; id?: string; stack?: boolean } = {}): HTMLElement {
  return h(
    'div',
    { class: cx('srow', o.stack && 'srow-stack'), id: o.id },
    h('div', { class: 'srow-text' }, h('div', { class: 'srow-title' }, title, o.fixed ? h('span', { class: 'fixed-tag' }, 'fixed') : null), description ? h('div', { class: 'srow-desc' }, description) : null),
    control ? h('div', { class: 'srow-control' }, control) : null,
  );
}

export function rows(...children: Child[]): HTMLElement {
  return h('div', { class: 'rows' }, ...children);
}

export function section(title: string, description: Child, ...children: Child[]): HTMLElement {
  return h('section', { class: 'ssection' }, h('h3', { class: 'ssection-title' }, title), description ? h('p', { class: 'ssection-desc' }, description) : null, ...children);
}

export function empty(iconName: string, title: string, text: Child, action?: Child): HTMLElement {
  return h('div', { class: 'empty' }, h('div', { class: 'empty-icon' }, icon(iconName, 22)), h('div', { class: 'empty-title' }, title), h('div', { class: 'empty-text' }, text), action ?? null);
}

export function notice(tone: 'info' | 'warning' | 'error' | 'success', ...children: Child[]): HTMLElement {
  return h('div', { class: cx('notice', `notice-${tone}`), role: tone === 'error' ? 'alert' : undefined }, icon(tone === 'success' ? 'check' : tone === 'info' ? 'info' : 'warning', 15), h('div', null, ...children));
}

/** Text from outside Dynodoc's checks (an agent's own description): dashed, labelled. */
export function untrusted(label: string, text: string): HTMLElement {
  return h('div', { class: 'untrusted' }, h('div', { class: 'untrusted-label' }, label), h('div', { class: 'untrusted-text' }, text));
}

export function segmented<T extends string>(value: T, options: [T, string, string?][], onChange: (v: T) => void, label: string): HTMLElement {
  return h(
    'div',
    { class: 'segmented', role: 'radiogroup', 'aria-label': label },
    options.map(([v, l, ic]) =>
      h('button', { type: 'button', role: 'radio', 'aria-checked': String(v === value), class: cx('seg', v === value && 'is-on'), onClick: () => onChange(v) }, ic ? icon(ic, 14) : null, l ? h('span', null, l) : null),
    ),
  );
}

export function tabs<T extends string>(value: T, options: [T, string, number?][], onChange: (v: T) => void, label: string): HTMLElement {
  return h(
    'div',
    { class: 'tabs', role: 'tablist', 'aria-label': label },
    options.map(([v, l, n]) =>
      h('button', { type: 'button', role: 'tab', 'aria-selected': String(v === value), class: cx('tab', v === value && 'is-on'), onClick: () => onChange(v) }, l, n !== undefined ? h('span', { class: 'tab-count' }, String(n)) : null),
    ),
  );
}

export function field(label: string, control: Child, hint?: string): HTMLElement {
  return h('label', { class: 'field' }, h('span', { class: 'field-label' }, label), control, hint ? h('span', { class: 'field-hint' }, hint) : null);
}

export function hash(hex: string, n = 8): HTMLElement {
  return h('code', { class: 'hash', title: hex }, hex.slice(0, n));
}
