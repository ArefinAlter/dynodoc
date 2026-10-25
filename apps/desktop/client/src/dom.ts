// A small DOM builder. The client never parses markup from a string: every element is made with
// createElement and every piece of text goes in through textContent, so nothing a document, a
// person or the scripted agent wrote can become markup. Inline style attributes are refused by
// the page's Content-Security-Policy, so styles are classes, or CSSOM properties set with `css`.

export type Child = Node | string | number | null | undefined | false | Child[];
export type Handler = (event: any) => void;
export interface Props {
  [key: string]: unknown;
}

function append(parent: Node, child: Child): void {
  if (child === null || child === undefined || child === false) return;
  if (Array.isArray(child)) {
    for (const c of child) append(parent, c);
    return;
  }
  if (typeof child === 'string' || typeof child === 'number') {
    parent.appendChild(document.createTextNode(String(child)));
    return;
  }
  parent.appendChild(child);
}

function apply(el: Element, props: Props | null | undefined): void {
  if (!props) return;
  for (const key of Object.keys(props)) {
    const value = props[key];
    if (value === undefined || value === null || value === false) continue;
    if (key.startsWith('on') && typeof value === 'function') {
      el.addEventListener(key.slice(2).toLowerCase(), value as EventListener);
    } else if (key === 'class') {
      el.setAttribute('class', String(value));
    } else if (key === 'text') {
      el.textContent = String(value);
    } else if (key === 'css' && typeof value === 'object') {
      const style = (el as HTMLElement).style;
      const map = value as Record<string, string>;
      for (const name of Object.keys(map)) style.setProperty(name, map[name]);
    } else if (key === 'value' || key === 'checked' || key === 'disabled' || key === 'hidden' || key === 'indeterminate' || key === 'selected') {
      (el as unknown as Record<string, unknown>)[key] = value;
    } else if (key === 'ref' && typeof value === 'function') {
      (value as (e: Element) => void)(el);
    } else if (value === true) {
      el.setAttribute(key, '');
    } else {
      el.setAttribute(key, String(value));
    }
  }
}

/** An HTML element. `h('button', { class: 'btn', onClick }, 'Save')`. */
export function h<K extends keyof HTMLElementTagNameMap>(tag: K, props?: Props | null, ...children: Child[]): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  apply(el, props);
  for (const c of children) append(el, c);
  return el;
}

const SVG = 'http://www.w3.org/2000/svg';

/** An SVG element. */
export function s(tag: string, props?: Props | null, ...children: Child[]): SVGElement {
  const el = document.createElementNS(SVG, tag) as SVGElement;
  apply(el, props);
  for (const c of children) append(el, c);
  return el;
}

/** Removes every child of `el`. */
export function clear(el: Element): void {
  while (el.firstChild) el.removeChild(el.firstChild);
}

/** A keyboard combination shown as chips: "Ctrl+Shift+P" becomes three <kbd>. */
export function kbd(combo: string): HTMLElement {
  const parts = combo ? combo.split('+') : [];
  return h('span', { class: 'kbds' }, parts.map((p) => h('kbd', null, p)));
}

export function cx(...names: (string | false | null | undefined)[]): string {
  return names.filter(Boolean).join(' ');
}
