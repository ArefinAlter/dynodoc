<script lang="ts">
  import { onMount } from 'svelte';
  import { Sun, Moon, SunMoon } from '@lucide/svelte';
  import { theme, nextPreference } from '$lib/theme.svelte';

  /** `icon` shows only the icon; `label` adds the current mode as text. */
  let { variant = 'icon', size = 18 }: { variant?: 'icon' | 'label'; size?: number } = $props();
  onMount(() => theme.watch());
  const names = { light: 'Day mode', dark: 'Night mode', system: 'Match system' } as const;
  const title = $derived(
    `${names[theme.preference]}${theme.preference === 'system' ? ` (${theme.resolved === 'dark' ? 'night' : 'day'})` : ''} — switch to ${names[nextPreference(theme.preference)].toLowerCase()}`
  );
</script>

<button
  type="button"
  class="theme-toggle"
  class:labelled={variant === 'label'}
  aria-label={title}
  {title}
  onclick={() => theme.cycle()}
>
  {#if theme.preference === 'system'}<SunMoon
      {size}
      aria-hidden="true"
    />{:else if theme.preference === 'dark'}<Moon {size} aria-hidden="true" />{:else}<Sun
      {size}
      aria-hidden="true"
    />{/if}
  {#if variant === 'label'}<span>{names[theme.preference]}</span>{/if}
</button>

<style>
  .theme-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-width: 36px;
    min-height: 36px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--color-ink-soft);
    cursor: pointer;
    font-size: 0.9rem;
  }
  .theme-toggle:hover {
    background: var(--color-surface-hover);
    color: var(--color-ink-strong);
  }
  .labelled {
    justify-content: flex-start;
    padding: 0 12px;
  }
</style>
