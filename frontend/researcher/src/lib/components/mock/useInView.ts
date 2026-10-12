export interface InViewOptions {
  /** Arm the motion at all (default true). */
  animate?: boolean;
  /** Pause when scrolled out and resume when back (for looping motion). Default: play once. */
  repeat?: boolean;
  /** Fraction of the node that must be visible (default 0.3). */
  threshold?: number;
}

/**
 * Svelte action. Marks the node `data-motion="armed"` (components hold their first frame, paused)
 * and flips it to `"play"` when it scrolls into view. Nothing is set on the server, with JS off,
 * or under prefers-reduced-motion, so the markup always renders its final state there.
 */
export function useInView(node: HTMLElement, options: InViewOptions = {}) {
  let io: IntersectionObserver | undefined;
  function stop() {
    io?.disconnect();
    io = undefined;
    delete node.dataset.motion;
  }
  function start({ animate = true, repeat = false, threshold = 0.3 }: InViewOptions) {
    if (
      !animate ||
      typeof IntersectionObserver === 'undefined' ||
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    )
      return;
    node.dataset.motion = 'armed';
    io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            node.dataset.motion = 'play';
            if (!repeat) io?.disconnect();
          } else if (repeat) node.dataset.motion = 'armed';
        }
      },
      { threshold }
    );
    io.observe(node);
  }
  start(options);
  return {
    update(next: InViewOptions) {
      stop();
      start(next);
    },
    destroy: stop
  };
}
