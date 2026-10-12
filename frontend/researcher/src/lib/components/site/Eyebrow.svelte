<script lang="ts">
  import type { Snippet } from 'svelte';
  /** Mono label rendered as `[ LABEL ]`. Brackets are decorative and hidden from readers. */
  let {
    label = '',
    as = 'p',
    accent = false,
    class: className = '',
    children
  }: {
    label?: string;
    as?: 'p' | 'span' | 'div';
    accent?: boolean;
    class?: string;
    children?: Snippet;
  } = $props();
</script>

<svelte:element this={as} class="eyebrow {accent ? 'eyebrow--accent' : ''} {className}"
  ><span aria-hidden="true">[&nbsp;</span>{#if children}{@render children()}{:else}{label}{/if}<span
    aria-hidden="true">&nbsp;]</span
  ></svelte:element
>
