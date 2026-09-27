/**
 * Close a popup menu the way desktop applications do: a click or tap anywhere outside the
 * element, moving keyboard focus elsewhere, or pressing Escape dismisses it.
 * Usage: `<div use:dismissable={{ open, onDismiss: () => (open = false) }}>`.
 */
export interface DismissOptions {
  open: boolean;
  onDismiss: () => void;
  /** Element to focus after Escape, usually the button that opened the menu. */
  returnFocus?: () => HTMLElement | null | undefined;
}
export function dismissable(node: HTMLElement, initial: DismissOptions) {
  let options = initial;
  const outside = (target: EventTarget | null) =>
    target instanceof Node ? !node.contains(target) : true;
  const pointer = (event: PointerEvent) => {
    if (options.open && outside(event.target)) options.onDismiss();
  };
  const focus = (event: FocusEvent) => {
    if (options.open && outside(event.target)) options.onDismiss();
  };
  const key = (event: KeyboardEvent) => {
    if (!options.open || event.key !== 'Escape') return;
    event.preventDefault();
    options.onDismiss();
    options.returnFocus?.()?.focus();
  };
  // Capture phase: controls that stop propagation (editors, grids) still close the menu.
  document.addEventListener('pointerdown', pointer, true);
  document.addEventListener('focusin', focus);
  document.addEventListener('keydown', key);
  return {
    update(next: DismissOptions) {
      options = next;
    },
    destroy() {
      document.removeEventListener('pointerdown', pointer, true);
      document.removeEventListener('focusin', focus);
      document.removeEventListener('keydown', key);
    }
  };
}
