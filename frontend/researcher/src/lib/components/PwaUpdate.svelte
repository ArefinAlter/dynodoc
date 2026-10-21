<script lang="ts">
  import { pwa } from '$lib/pwa/client.svelte';
</script>

{#if pwa.updateReady && !pwa.updateDeferred}
  <div class="update" role="status">
    <p>A new version of Dynodoc is ready. Updating reloads this page.</p>
    <div class="actions">
      <button type="button" class="primary" onclick={() => pwa.applyUpdate()}>Update now</button>
      <button type="button" onclick={() => pwa.deferUpdate()}>Not now</button>
    </div>
  </div>
{/if}

<style>
  .update {
    position: fixed;
    left: max(12px, env(safe-area-inset-left));
    right: max(12px, env(safe-area-inset-right));
    bottom: calc(12px + env(safe-area-inset-bottom, 0px) + var(--keyboard-inset, 0px));
    z-index: 90;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    max-width: 32rem;
    margin-inline: auto;
    padding: 12px 16px;
    border: 1px solid var(--color-border);
    border-radius: 12px;
    background: var(--color-surface);
    color: var(--color-ink);
    font-family: var(--font-ui);
    font-size: 0.9rem;
    box-shadow: var(--shadow-menu);
  }
  p {
    flex: 1 1 14rem;
    margin: 0;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  button {
    min-height: 44px;
    padding: 0 16px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    cursor: pointer;
  }
  .primary {
    border-color: transparent;
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
  }
  @media print {
    .update {
      display: none;
    }
  }
</style>
