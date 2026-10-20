<script lang="ts">
  import { onMount } from 'svelte';
  import { Check, Copy } from '@lucide/svelte';
  import { copyText } from '$lib/clipboard';
  /**
   * A command with a copy button. The button copies `code` exactly (the `$` prompt is
   * only decoration) and confirms with "Copied", announced politely to screen readers.
   * The button stays disabled until the page has hydrated.
   */
  let {
    code,
    label = 'command',
    prompt = '$',
    class: className = ''
  }: {
    code: string;
    /** What is being copied; completes the accessible name, e.g. "Copy install command". */
    label?: string;
    prompt?: string;
    class?: string;
  } = $props();
  let ready = $state(false);
  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  onMount(() => {
    ready = true;
    return () => clearTimeout(timer);
  });
  async function copy() {
    const done = await copyText(code);
    if (!done) return;
    copied = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = false), 2000);
  }
</script>

<div class="copycode {className}">
  <pre class="code"><code
      >{#if prompt}<span class="prompt" aria-hidden="true">{prompt} </span>{/if}{code}</code
    ></pre>
  <button type="button" class="copy" disabled={!ready} onclick={copy}>
    {#if copied}<Check size={15} aria-hidden="true" /><span>Copied</span>{:else}<Copy
        size={15}
        aria-hidden="true"
      /><span>Copy<span class="sr-only"> {label}</span></span>{/if}
  </button>
  <span class="sr-only" role="status" aria-live="polite">{copied ? 'Copied to clipboard' : ''}</span
  >
</div>

<style>
  .copycode {
    display: flex;
    align-items: stretch;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius, 6px);
    background: var(--color-surface);
    overflow: hidden;
  }
  .code {
    flex: 1;
    min-width: 0;
    margin: 0;
    padding: 12px 14px;
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.875rem;
    line-height: 1.6;
    color: var(--color-ink);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .code code {
    font: inherit;
  }
  .prompt {
    color: var(--color-muted);
    -webkit-user-select: none;
    user-select: none;
  }
  .copy {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    min-width: 44px;
    padding: 0 14px;
    border: 0;
    border-left: 1px solid var(--color-border);
    background: transparent;
    color: var(--color-ink);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.8125rem;
    cursor: pointer;
    transition: background-color 0.18s;
  }
  .copy:hover:not(:disabled) {
    background: var(--color-surface-hover);
  }
  .copy:disabled {
    color: var(--color-muted);
    cursor: default;
  }
  .copy:focus-visible {
    outline-offset: -3px;
  }
</style>
