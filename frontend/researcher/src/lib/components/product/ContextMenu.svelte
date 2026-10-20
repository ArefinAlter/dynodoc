<script lang="ts">
  import { onMount } from 'svelte';
  import type { MenuAction } from '$lib/workspace/context-menu';
  let { x, y, items, onClose }: { x: number; y: number; items: MenuAction[]; onClose: () => void } =
    $props();
  let host: HTMLDivElement;
  let returnFocus: HTMLElement | null = null;
  let left = $state(0),
    top = $state(0);
  onMount(() => {
    returnFocus = document.activeElement as HTMLElement | null;
    left = Math.max(8, Math.min(x, innerWidth - host.offsetWidth - 8));
    top = Math.max(8, Math.min(y, innerHeight - host.offsetHeight - 8));
    host.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  });
  function key(e: KeyboardEvent) {
    const buttons = [...host.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
    let i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) {
      e.preventDefault();
      i =
        e.key === 'Home'
          ? 0
          : e.key === 'End'
            ? buttons.length - 1
            : (i + (e.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length;
      buttons[i]?.focus();
    }
    if (e.key === 'Escape' || e.key === 'Tab') {
      if (e.key === 'Escape') e.preventDefault();
      returnFocus?.focus();
      onClose();
    }
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (!host.contains(e.target as globalThis.Node)) onClose();
  }}
  onresize={onClose}
/>
<div
  class="context-menu"
  bind:this={host}
  role="menu"
  aria-label="Editing actions"
  tabindex="-1"
  style:left={`${left}px`}
  style:top={`${top}px`}
  onkeydown={key}
>
  {#each items as item}<button
      role="menuitem"
      class:separator={item.separator}
      disabled={item.disabled}
      onclick={() => {
        void item.action();
        onClose();
      }}>{item.label}</button
    >{/each}
</div>

<style>
  .context-menu {
    position: fixed;
    z-index: 3000;
    width: 235px;
    max-height: calc(100vh - 16px);
    max-height: calc(100dvh - 16px);
    overflow: auto;
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-strong);
    box-shadow: var(--shadow-menu);
    border-radius: 7px;
    padding: 5px;
    color: var(--color-ink);
  }
  .context-menu button {
    display: block;
    width: 100%;
    border: 0;
    background: none;
    padding: 10px 12px;
    min-height: 36px;
    font: inherit;
    font-size: 14px;
    text-align: left;
    cursor: pointer;
    color: inherit;
    border-radius: 4px;
  }
  .context-menu button:hover,
  .context-menu button:focus-visible {
    background: var(--color-surface-selected);
    outline: 2px solid var(--color-focus);
    outline-offset: -2px;
  }
  .context-menu button:disabled {
    color: var(--color-subtle);
    cursor: default;
  }
  .context-menu .separator {
    border-top: 1px solid var(--color-border);
    border-radius: 0;
    margin-top: 4px;
  }
</style>
