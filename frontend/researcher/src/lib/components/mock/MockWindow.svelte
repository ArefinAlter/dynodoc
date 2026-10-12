<script module lang="ts">
  export type MockVariant =
    | 'browser'
    | 'word'
    | 'excel'
    | 'powerpoint'
    | 'docs'
    | 'sheets'
    | 'slides'
    | 'terminal'
    | 'plain';
  const HOSTS: Partial<Record<MockVariant, string>> = {
    word: 'Word',
    excel: 'Excel',
    powerpoint: 'PowerPoint',
    docs: 'Google Docs',
    sheets: 'Google Sheets',
    slides: 'Google Slides'
  };
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import './mock.css';

  let {
    title = '',
    variant = 'browser',
    label,
    class: className = '',
    children
  }: {
    /** Text in the title bar (file name or address). */
    title?: string;
    /** Frame style; hosts add a thin accent strip and the host name. `plain` has no title bar. */
    variant?: MockVariant;
    /** Accessible name. Omit for decorative use (aria-hidden). */
    label?: string;
    class?: string;
    children?: Snippet;
  } = $props();
  const host = $derived(HOSTS[variant]);
</script>

<div
  class="mock {className}"
  class:has-strip={!!host}
  data-variant={variant}
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
>
  {#if variant !== 'plain'}
    <div class="bar">
      <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
      <span class="title" class:mk-mono={variant === 'terminal' || variant === 'browser'}
        >{title}</span
      >
      {#if host}<span class="host">{host}</span>{/if}
    </div>
  {/if}
  <div class="body">{@render children?.()}</div>
</div>

<style>
  .has-strip::before {
    content: '';
    position: absolute;
    inset: 0 0 auto;
    z-index: 1;
    height: 3px;
    background: var(--mock-host);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 34px;
    padding: 0 12px;
    border-bottom: 1px solid var(--mock-border);
    background: var(--mock-subtle);
    color: var(--mock-muted);
    font-size: 12px;
  }
  .dots {
    display: inline-flex;
    gap: 6px;
    flex: none;
  }
  .dots i {
    display: block;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 1px solid var(--mock-border);
    background: var(--mock-inset);
  }
  .title {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .host {
    flex: none;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.01em;
  }
  .host::before {
    content: '';
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-right: 6px;
    border-radius: 2px;
    background: var(--mock-host);
  }
  .body {
    min-width: 0;
  }
</style>
