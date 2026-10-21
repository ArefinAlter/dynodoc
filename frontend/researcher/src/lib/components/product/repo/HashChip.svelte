<script lang="ts">
  import { Copy } from '@lucide/svelte';
  import { pushToast } from '$lib/stores/toast.svelte';
  import { copyText } from '$lib/clipboard';
  import { shortHash } from '$lib/workspace/repo-ui';
  let { hash, label = 'revision hash' }: { hash: string; label?: string } = $props();
  async function copy() {
    if (await copyText(hash)) pushToast(`Copied the ${label}`, 'success', 2200);
    else pushToast(`Copy this ${label}: ${hash}`, 'info', 6000);
  }
</script>

{#if hash}<button type="button" class="hash" title={hash} onclick={() => void copy()}
    ><code>{shortHash(hash)}</code><Copy size={13} aria-hidden="true" /><span class="sr-only"
      >Copy {label}</span
    ></button
  >{/if}

<style>
  .hash {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 1px 7px;
    border: 1px solid var(--rp-border);
    border-radius: 6px;
    background: var(--rp-subtle);
    color: var(--rp-muted);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition:
      background-color 0.12s,
      color 0.12s,
      border-color 0.12s;
  }
  .hash:hover {
    background: var(--rp-btn-hover);
    color: var(--rp-fg);
    border-color: var(--rp-border-strong);
  }
  code {
    font-family: var(--rp-mono);
    font-size: 12px;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
