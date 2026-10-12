<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { FileText } from '@lucide/svelte';
  type Tab = { key: string; label: string; icon: typeof FileText; count?: string | number | null };
  let {
    tabs,
    current,
    label,
    busy = false,
    onSelect
  }: {
    tabs: readonly Tab[];
    current: string;
    label: string;
    busy?: boolean;
    onSelect: (key: string) => void;
  } = $props();
  let nav = $state<HTMLElement | null>(null),
    bar = $state({ x: 0, w: 0, ready: false });
  function measure() {
    const active = nav?.querySelector<HTMLElement>('[aria-current="page"]');
    bar = active
      ? { x: active.offsetLeft, w: active.offsetWidth, ready: true }
      : { x: 0, w: 0, ready: false };
  }
  $effect(() => {
    void current;
    void tabs.length;
    void tick().then(measure);
  });
  onMount(() => {
    const observer = new ResizeObserver(measure);
    if (nav) observer.observe(nav);
    void document.fonts?.ready.then(measure);
    return () => observer.disconnect();
  });
</script>

<nav class="tabnav" class:ready={bar.ready} aria-label={label} bind:this={nav}>
  {#each tabs as tab (tab.key)}
    <button
      type="button"
      aria-current={current === tab.key ? 'page' : undefined}
      disabled={busy}
      onclick={() => onSelect(tab.key)}
      ><tab.icon
        size={16}
        aria-hidden="true"
      />{tab.label}{#if tab.count !== undefined && tab.count !== null}<span
          class="count"
          aria-hidden="true">{tab.count}</span
        >{/if}</button
    >
  {/each}
  <span
    class="indicator"
    aria-hidden="true"
    style:width="{bar.w}px"
    style:transform="translateX({bar.x}px)"
  ></span>
</nav>

<style>
  .tabnav {
    position: relative;
    display: flex;
    gap: 2px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tabnav::-webkit-scrollbar {
    display: none;
  }
  .tabnav button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 4px;
    padding: 6px 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--rp-fg);
    font: inherit;
    line-height: 20px;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color 0.12s ease;
  }
  .tabnav button :global(svg) {
    color: var(--rp-muted);
  }
  .tabnav button:hover:not([aria-current='page']) {
    background: var(--rp-btn-hover);
  }
  .tabnav button[aria-current='page'] {
    font-weight: 600;
  }
  .tabnav button[aria-current='page'] :global(svg) {
    color: var(--rp-fg);
  }
  .tabnav button:disabled {
    cursor: progress;
  }
  .tabnav:not(.ready) button[aria-current='page'] {
    box-shadow: 0 4px 0 -2px var(--rp-underline);
  }
  .count {
    min-width: 20px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--rp-neutral-bg);
    font-size: 12px;
    font-weight: 500;
    line-height: 18px;
    text-align: center;
  }
  .indicator {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    border-radius: 2px;
    background: var(--rp-underline);
    transition:
      transform 0.25s cubic-bezier(0.3, 0.7, 0.3, 1),
      width 0.25s cubic-bezier(0.3, 0.7, 0.3, 1);
    pointer-events: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .indicator,
    .tabnav button {
      transition: none;
    }
  }
</style>
