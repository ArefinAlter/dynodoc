<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import { dismissable } from '$lib/actions/dismissable';
  let {
    trigger,
    children,
    ariaLabel,
    title,
    align = 'left',
    open = $bindable(false),
    triggerClass = ''
  }: {
    trigger: Snippet;
    children: Snippet;
    /** Set for icon-only triggers; text triggers are named by their content. */
    ariaLabel?: string;
    title?: string;
    align?: 'left' | 'right';
    open?: boolean;
    triggerClass?: string;
  } = $props();
  let button = $state<HTMLButtonElement | null>(null),
    panel = $state<HTMLElement | null>(null);
  const items = () =>
    panel
      ? [...panel.querySelectorAll<HTMLElement>('[role^="menuitem"]:not([disabled])')]
      : ([] as HTMLElement[]);
  async function toggle() {
    open = !open;
    if (open) {
      await tick();
      items()[0]?.focus();
    }
  }
  function key(event: KeyboardEvent) {
    const list = items();
    if (!list.length) return;
    const at = list.indexOf(document.activeElement as HTMLElement);
    const go = (i: number) => {
      event.preventDefault();
      list[(i + list.length) % list.length].focus();
    };
    if (event.key === 'ArrowDown') go(at + 1);
    else if (event.key === 'ArrowUp') go(at - 1);
    else if (event.key === 'Home') go(0);
    else if (event.key === 'End') go(list.length - 1);
    else if (event.key === 'Tab') open = false;
  }
</script>

<div
  class="menu"
  use:dismissable={{ open, onDismiss: () => (open = false), returnFocus: () => button }}
>
  <button
    bind:this={button}
    type="button"
    class={`menu-trigger ${triggerClass}`}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label={ariaLabel}
    {title}
    onclick={() => void toggle()}
    onkeydown={(e) => {
      if (e.key === 'ArrowDown' && !open) {
        e.preventDefault();
        void toggle();
      }
    }}>{@render trigger()}</button
  >
  {#if open}<div
      class={`menu-panel ${align}`}
      role="menu"
      tabindex="-1"
      aria-label={ariaLabel || title}
      bind:this={panel}
      onkeydown={key}
      onclick={(e) => {
        if ((e.target as HTMLElement).closest('[role^="menuitem"]')) open = false;
      }}
    >
      {@render children()}
    </div>{/if}
</div>

<style>
  .menu {
    position: relative;
    display: inline-block;
  }
  .menu-panel {
    position: absolute;
    top: calc(100% + 6px);
    z-index: 60;
    min-width: 200px;
    max-width: min(320px, calc(100vw - 24px));
    padding: 6px 0;
    border: 1px solid var(--rp-border);
    border-radius: 12px;
    background: var(--rp-overlay);
    color: var(--rp-fg);
    box-shadow: var(--rp-shadow);
    transform-origin: top left;
    animation: pop 0.12s ease-out;
  }
  .menu-panel.left {
    left: 0;
  }
  .menu-panel.right {
    right: 0;
    transform-origin: top right;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.97) translateY(-3px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .menu-panel {
      animation: none;
    }
  }
</style>
