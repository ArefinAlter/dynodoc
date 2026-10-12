/**
 * Keyboard access for horizontally scrolling blocks inside rendered content. When a
 * `.table-wrap`, `.table-scroll` or `pre` overflows (for example a wide table on a
 * phone), it gets `tabindex="0"`, `role="group"` and a name taken from the nearest
 * heading, so keyboard users can scroll it. Blocks that do not overflow are left alone.
 *
 * Usage: `<div use:scrollable>…rendered page content…</div>`.
 */
const SELECTOR = '.table-wrap, .table-scroll, pre';

export function scrollable(node: HTMLElement) {
  if (typeof ResizeObserver === 'undefined') return;
  const apply = () => {
    for (const el of node.querySelectorAll<HTMLElement>(SELECTOR)) {
      const overflowing = el.scrollWidth > el.clientWidth + 1;
      if (overflowing && !el.hasAttribute('tabindex')) {
        const heading = el.closest('section')?.querySelector('h2, h3')?.textContent?.trim();
        el.setAttribute('tabindex', '0');
        if (!el.hasAttribute('role')) el.setAttribute('role', 'group');
        if (!el.hasAttribute('aria-label') && !el.hasAttribute('aria-labelledby'))
          el.setAttribute('aria-label', `${heading ? `${heading}: ` : ''}scrollable content`);
        el.dataset.autoScroll = 'true';
      } else if (!overflowing && el.dataset.autoScroll) {
        el.removeAttribute('tabindex');
        el.removeAttribute('role');
        el.removeAttribute('aria-label');
        delete el.dataset.autoScroll;
      }
    }
  };
  const observer = new ResizeObserver(apply);
  observer.observe(node);
  apply();
  return { destroy: () => observer.disconnect() };
}
