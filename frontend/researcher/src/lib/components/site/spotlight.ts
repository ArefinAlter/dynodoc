/**
 * Pointer-following glow. Sets `--mx`/`--my` (pixels inside the element) and `--spot`
 * (0 or 1) on the node; the `.spotlight` class in `$lib/styles/site.css` paints the
 * radial glow (and lights up dots when combined with `.dotgrid`).
 *
 * Usage: `<div class="spotlight dotgrid" use:spotlight>`. It does nothing for touch
 * input, and nothing at all under `prefers-reduced-motion: reduce`.
 */
export function spotlight(node: HTMLElement) {
  if (
    typeof window === 'undefined' ||
    window.matchMedia('(prefers-reduced-motion: reduce)').matches ||
    !window.matchMedia('(hover: hover) and (pointer: fine)').matches
  )
    return;
  let frame = 0;
  let x = 0;
  let y = 0;
  const paint = () => {
    frame = 0;
    node.style.setProperty('--mx', `${x}px`);
    node.style.setProperty('--my', `${y}px`);
  };
  const move = (event: PointerEvent) => {
    const box = node.getBoundingClientRect();
    x = event.clientX - box.left;
    y = event.clientY - box.top;
    node.style.setProperty('--spot', '1');
    if (!frame) frame = requestAnimationFrame(paint);
  };
  const leave = () => node.style.setProperty('--spot', '0');
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerleave', leave);
  return {
    destroy() {
      if (frame) cancelAnimationFrame(frame);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerleave', leave);
    }
  };
}
