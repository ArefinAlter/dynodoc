<script lang="ts">
  import type { Bold } from '@lucide/svelte';
  let {
    label,
    icon: Icon,
    onclick,
    disabled = false,
    active,
    large = false
  }: {
    label: string;
    icon: typeof Bold;
    onclick: () => unknown;
    disabled?: boolean;
    active?: boolean;
    large?: boolean;
  } = $props();
</script>

<button
  type="button"
  title={label}
  aria-label={label}
  aria-pressed={active}
  {disabled}
  class:large
  onmousedown={(event) => event.preventDefault()}
  {onclick}
>
  <Icon size={large ? 25 : 16} strokeWidth={1.5} />
  {#if large}<span>{label}</span>{/if}
</button>

<style>
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 4px;
    border: 1px solid transparent;
    border-radius: 3px;
    color: #303030;
    background: transparent;
    cursor: pointer;
  }
  button:hover,
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
  button.large {
    flex-direction: column;
    gap: 5px;
    width: auto;
    min-width: 52px;
    height: 62px;
    padding: 5px 8px;
  }
  span {
    font:
      12px/1.15 'Segoe UI',
      Arial,
      sans-serif;
    max-width: 78px;
  }
</style>
