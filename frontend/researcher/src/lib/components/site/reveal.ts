/**
 * Reveal on scroll: blocks marked `data-reveal` rise 12px and fade in once, with a
 * short stagger when several arrive together. The visual states live in
 * `$lib/styles/site.css`.
 *
 * Usage: `<main use:reveal>` reveals every `[data-reveal]` inside it;
 * `<div data-reveal use:reveal>` reveals itself. `data-reveal-delay="120"` adds
 * milliseconds to one block. Nothing is hidden when JavaScript is off, motion is
 * reduced, or IntersectionObserver is missing: blocks that are already on screen when
 * the page hydrates are left alone, and only blocks below the fold are held back.
 */
export interface RevealOptions {
  /** Milliseconds between blocks that become visible in the same frame. */
  stagger?: number;
  /** Fraction of a block that must be visible. */
  threshold?: number;
}

const MAX_STAGGER_STEPS = 6;
const TRANSITION_MS = 600;

export function reveal(node: HTMLElement, options: RevealOptions = {}) {
  if (
    typeof window === 'undefined' ||
    !('IntersectionObserver' in window) ||
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  )
    return;
  const stagger = options.stagger ?? 70;
  const targets = node.hasAttribute('data-reveal')
    ? [node]
    : Array.from(node.querySelectorAll<HTMLElement>('[data-reveal]'));
  if (!targets.length) return;
  const timers = new Set<number>();
  const viewport = window.innerHeight;
  const observer = new IntersectionObserver(
    (entries) => {
      let step = 0;
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        const el = entry.target as HTMLElement;
        observer.unobserve(el);
        const extra = Number(el.dataset.revealDelay) || 0;
        const delay = extra + Math.min(step++, MAX_STAGGER_STEPS) * stagger;
        el.style.setProperty('--reveal-delay', `${delay}ms`);
        el.dataset.revealState = 'running';
        const timer = window.setTimeout(
          () => {
            delete el.dataset.revealState;
            el.style.removeProperty('--reveal-delay');
            timers.delete(timer);
          },
          delay + TRANSITION_MS + 80
        );
        timers.add(timer);
      }
    },
    { threshold: options.threshold ?? 0.12, rootMargin: '0px 0px -6% 0px' }
  );
  for (const el of targets) {
    // Content already in the first screen stays as rendered.
    if (el.getBoundingClientRect().top < viewport * 0.92) continue;
    el.dataset.revealState = 'pending';
    observer.observe(el);
  }
  return {
    destroy() {
      observer.disconnect();
      timers.forEach((timer) => window.clearTimeout(timer));
      for (const el of targets) delete el.dataset.revealState;
    }
  };
}
