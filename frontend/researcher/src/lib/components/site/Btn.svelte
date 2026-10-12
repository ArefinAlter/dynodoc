<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import { ArrowRight, ArrowUpRight } from '@lucide/svelte';
  /**
   * Rectangular button or link. Renders an <a> when `href` is set, otherwise a <button>.
   * `arrow` adds a trailing arrow that nudges on hover (true = right; external links use
   * the up-right arrow).
   */
  let {
    variant = 'primary',
    size = 'md',
    href,
    arrow = false,
    external = false,
    type = 'button',
    disabled = false,
    class: className = '',
    children,
    // eslint-disable-next-line svelte/valid-compile -- pass-through attributes (aria-*, onclick)
    ...rest
  }: {
    variant?: 'primary' | 'secondary' | 'ghost' | 'link';
    size?: 'sm' | 'md' | 'lg';
    href?: string;
    arrow?: boolean | 'right' | 'up-right';
    external?: boolean;
    type?: 'button' | 'submit' | 'reset';
    disabled?: boolean;
    class?: string;
    children: Snippet;
  } & Omit<HTMLAttributes<HTMLElement>, 'class' | 'children'> = $props();
  const classes = $derived(
    `btn btn--${variant}${size === 'md' ? '' : ` btn--${size}`}${className ? ` ${className}` : ''}`
  );
  const direction = $derived(arrow === 'up-right' || (arrow === true && external) ? 'up' : 'right');
</script>

{#snippet inner()}
  {@render children()}
  {#if arrow}
    {#if direction === 'up'}<ArrowUpRight
        class="btn__arrow btn__arrow--up"
        size={16}
        aria-hidden="true"
      />{:else}<ArrowRight class="btn__arrow" size={16} aria-hidden="true" />{/if}
  {/if}
  {#if external}<span class="sr-only"> (opens in a new tab)</span>{/if}
{/snippet}

{#if href}
  <a
    class={classes}
    {href}
    target={external ? '_blank' : undefined}
    rel={external ? 'noopener noreferrer' : undefined}
    aria-disabled={disabled || undefined}
    {...rest}>{@render inner()}</a
  >
{:else}
  <button class={classes} {type} {disabled} {...rest}>{@render inner()}</button>
{/if}
