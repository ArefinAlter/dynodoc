/** A tiny toast queue (runes) for transient error/success notices. */
export type ToastKind = 'error' | 'info' | 'success';

export interface Toast {
  id: number;
  message: string;
  kind: ToastKind;
}

export const toasts = $state<{ items: Toast[] }>({ items: [] });

let nextId = 1;

/** Enqueue a toast; it auto-dismisses after `ttl` ms. */
export function pushToast(message: string, kind: ToastKind = 'info', ttl = 4_000): void {
  const id = nextId++;
  toasts.items.push({ id, message, kind });
  if (typeof setTimeout !== 'undefined') {
    setTimeout(() => dismissToast(id), ttl);
  }
}

export function dismissToast(id: number): void {
  toasts.items = toasts.items.filter((t) => t.id !== id);
}
