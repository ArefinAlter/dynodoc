<script lang="ts">
  import type { Snippet } from 'svelte';
  /** Keyboard key(s). `keys={['Ctrl', 'K']}` renders `Ctrl + K`; or pass children for one key. */
  let {
    keys,
    children
  }: {
    keys?: string | string[];
    children?: Snippet;
  } = $props();
  const list = $derived(keys === undefined ? [] : Array.isArray(keys) ? keys : [keys]);
</script>

{#if children}<kbd class="kbd">{@render children()}</kbd>{:else}<span class="keys"
    >{#each list as key, index (index)}{#if index > 0}<span class="plus" aria-hidden="true">+</span
        >{/if}<kbd class="kbd">{key}</kbd>{/each}</span
  >{/if}

<style>
  .keys {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .kbd {
    display: inline-block;
    min-width: 1.6em;
    padding: 0.1em 0.45em;
    border: 1px solid var(--color-border-strong, #b9b39f);
    border-bottom-width: 2px;
    border-radius: 4px;
    background: var(--color-surface-raised, #fff);
    color: var(--color-ink, #1b1a17);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.78em;
    font-weight: 500;
    line-height: 1.5;
    text-align: center;
    white-space: nowrap;
  }
  .plus {
    color: var(--color-muted, #67645a);
    font-size: 0.8em;
  }
</style>
