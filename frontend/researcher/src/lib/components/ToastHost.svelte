<script lang="ts">
  import { toasts, dismissToast } from '$lib/stores/toast.svelte';
</script>

<div class="toast-host" aria-live="polite" aria-atomic="false">
  {#each toasts.items as toast (toast.id)}
    <button type="button" class="toast {toast.kind}" onclick={() => dismissToast(toast.id)}>
      {toast.message}
    </button>
  {/each}
</div>

<style>
  .toast-host {
    position: fixed;
    bottom: calc(1.2rem + env(safe-area-inset-bottom, 0px) + var(--keyboard-inset, 0px));
    right: calc(1.2rem + env(safe-area-inset-right, 0px));
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    max-width: min(28rem, calc(100vw - 2rem));
  }

  .toast {
    text-align: left;
    border: 1px solid var(--color-border);
    border-radius: 0.7rem;
    padding: 0.75rem 0.95rem;
    background: var(--color-surface);
    color: var(--color-ink);
    font-family: var(--font-ui);
    font-size: 0.88rem;
    box-shadow: var(--shadow-menu);
    cursor: pointer;
  }

  .toast.error {
    border-color: color-mix(in srgb, var(--color-terracotta) 55%, var(--color-surface));
    background: color-mix(in srgb, var(--color-terracotta) 8%, var(--color-surface));
    color: var(--color-terracotta);
  }

  .toast.success {
    border-color: color-mix(in srgb, var(--color-suggest) 55%, var(--color-surface));
    background: var(--color-suggest-bg);
    color: var(--color-suggest);
  }
</style>
