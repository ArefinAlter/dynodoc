<script lang="ts">
  import { Copy, Check } from '@lucide/svelte';
  let { value, label = 'Copy' }: { value: string; label?: string } = $props();
  let copied = $state(false);
  let failed = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  async function copy() {
    clearTimeout(timer);
    try {
      await navigator.clipboard.writeText(value);
      copied = true;
      failed = false;
    } catch {
      copied = false;
      failed = true;
    }
    timer = setTimeout(() => {
      copied = false;
      failed = false;
    }, 1600);
  }
</script>

<span class="copy-wrap"
  ><button
    type="button"
    class="copy"
    class:done={copied}
    aria-label={label}
    title={label}
    onclick={copy}
    >{#if copied}<Check size={13} aria-hidden="true" />{:else}<Copy
        size={13}
        aria-hidden="true"
      />{/if}</button
  ><span class="sr-only" aria-live="polite"
    >{copied ? 'Copied' : failed ? 'Copy is not available' : ''}</span
  ></span
>

<style>
  .copy-wrap {
    display: inline-flex;
    vertical-align: middle;
  }
  .copy {
    display: inline-grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--a-muted);
    cursor: pointer;
    transition:
      background-color 0.12s ease,
      color 0.12s ease,
      transform 0.12s ease;
  }
  .copy:hover {
    background: var(--a-hover);
    color: var(--a-fg);
  }
  .copy:active {
    transform: scale(0.92);
  }
  .copy.done {
    color: var(--a-success);
  }
</style>
