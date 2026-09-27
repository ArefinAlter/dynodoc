<script lang="ts">
  import type { Bold } from '@lucide/svelte';
  let {
    label,
    text,
    icon: Icon,
    onclick,
    disabled = false,
    active,
    large = false,
    size
  }: {
    /** Accessible name and tooltip. */
    label: string;
    /** Shorter visible caption; defaults to the accessible name. */
    text?: string;
    icon: typeof Bold;
    onclick: () => unknown;
    disabled?: boolean;
    active?: boolean;
    large?: boolean;
    /** icon: compact icon only · small: icon with caption beside it · large: icon above caption. */
    size?: 'icon' | 'small' | 'large';
  } = $props();
  const kind = $derived(size || (large ? 'large' : 'icon'));
</script>

<button
  type="button"
  title={label}
  aria-label={label}
  aria-pressed={active}
  {disabled}
  class={kind}
  onmousedown={(event) => event.preventDefault()}
  {onclick}
>
  <Icon size={kind === 'large' ? 24 : 16} strokeWidth={1.5} />
  {#if kind !== 'icon'}<span>{text || label}</span>{/if}
</button>

<style>
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    padding: 4px;
    border: 1px solid transparent;
    border-radius: 3px;
    color: #303030;
    background: transparent;
    cursor: pointer;
    font:
      12px/1.2 'Segoe UI',
      Arial,
      sans-serif;
  }
  button:hover:not(:disabled),
  button[aria-pressed='true'] {
    background: #e5e5e5;
    border-color: #c6c6c6;
  }
  button:focus-visible {
    outline: 2px solid #185abd;
    outline-offset: 1px;
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  button :global(svg) {
    flex-shrink: 0;
  }
  /* Small: icon and caption on one line, like Word's stacked commands. */
  button.small {
    width: auto;
    height: 26px;
    gap: 6px;
    padding: 3px 7px 3px 5px;
    justify-content: flex-start;
    white-space: nowrap;
  }
  /* Large: every icon sits at the same height, whether the caption has one line or two. */
  button.large {
    flex-direction: column;
    justify-content: flex-start;
    gap: 4px;
    width: auto;
    min-width: 54px;
    max-width: 88px;
    height: 68px;
    padding: 6px 6px 3px;
  }
  button.large span {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    max-width: 80px;
    text-align: center;
  }
</style>
