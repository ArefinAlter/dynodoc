/**
 * Pointer parallax for the hero. Sets `--px` and `--py` (-1 to 1, relative to the node's
 * centre) on the node; children translate by `calc(var(--px) * Npx)` using `transform`
 * only. It does nothing for touch input and nothing under `prefers-reduced-motion`.
 *
 * Usage: `<section use:parallax>`; children read `--px`/`--py`.
 */
export function parallax(node: HTMLElement) {
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
    node.style.setProperty('--px', x.toFixed(3));
    node.style.setProperty('--py', y.toFixed(3));
  };
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(paint);
  };
  const move = (event: PointerEvent) => {
    const box = node.getBoundingClientRect();
    x = Math.max(-1, Math.min(1, ((event.clientX - box.left) / box.width - 0.5) * 2));
    y = Math.max(-1, Math.min(1, ((event.clientY - box.top) / box.height - 0.5) * 2));
    schedule();
  };
  const leave = () => {
    x = 0;
    y = 0;
    schedule();
  };
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
