/**
 * Phone browsers handle the on-screen keyboard differently. Chrome on Android resizes the
 * page (see `interactive-widget` in app.html); Safari on iPhone and iPad leaves the page as
 * it was and covers its lower part, so anything fixed to the bottom ends up behind the
 * keyboard. This publishes the covered height as `--keyboard-inset` so bottom-anchored
 * layers can sit above it. It is 0 when nothing covers the page, so it changes nothing on
 * desktop.
 */
export interface ViewportMeasure {
  /** The page's layout height (documentElement.clientHeight). */
  layoutHeight: number;
  /** The visible height (visualViewport.height). */
  height: number;
  offsetTop: number;
  /** Pinch zoom; a zoomed page also has a smaller visible area, which is not a keyboard. */
  scale: number;
}

/** Below this the difference is a browser toolbar sliding away, not a keyboard. */
const MIN_KEYBOARD = 100;

export function keyboardInset({ layoutHeight, height, offsetTop, scale }: ViewportMeasure): number {
  if (scale > 1.01) return 0;
  const covered = Math.round(layoutHeight - height - offsetTop);
  return covered >= MIN_KEYBOARD ? covered : 0;
}

/** Start tracking; returns a function that stops it. Call in the browser only. */
export function trackKeyboard(): () => void {
  const viewport = window.visualViewport;
  if (!viewport) return () => {};
  const root = document.documentElement;
  const update = () =>
    root.style.setProperty(
      '--keyboard-inset',
      `${keyboardInset({
        layoutHeight: root.clientHeight,
        height: viewport.height,
        offsetTop: viewport.offsetTop,
        scale: viewport.scale
      })}px`
    );
  viewport.addEventListener('resize', update);
  viewport.addEventListener('scroll', update);
  update();
  return () => {
    viewport.removeEventListener('resize', update);
    viewport.removeEventListener('scroll', update);
    root.style.removeProperty('--keyboard-inset');
  };
}
